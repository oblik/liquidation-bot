use crate::database::DatabasePool;
use alloy_contract::{ContractInstance, Interface};
use alloy_primitives::{Address, U256};
use alloy_provider::Provider;
use alloy_signer_local::PrivateKeySigner;
use dashmap::DashMap;
use eyre::Result;
use parking_lot::RwLock as SyncRwLock;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::circuit_breaker::{CircuitBreaker, CircuitBreakerState};
use crate::config::{AssetLoadingMethod, BotConfig};
use crate::database;
use crate::events::BotEvent;
use crate::liquidation;
use crate::models::{
    AssetConfig, HardhatArtifact, LiquidationAssetConfig, LiquidationResult, PriceFeed, UserPosition,
};
use crate::monitoring::{discovery, oracle, scanner, websocket};

// Main bot struct with event monitoring capabilities
pub struct LiquidationBot<P> {
    provider: Arc<P>,
    ws_provider: Arc<dyn Provider>,
    signer: PrivateKeySigner,
    pub config: BotConfig,
    pool_contract: ContractInstance<alloy_transport::BoxTransport, Arc<P>>,
    _liquidator_contract: Option<ContractInstance<alloy_transport::BoxTransport, Arc<P>>>,
    db_pool: DatabasePool,
    user_positions: Arc<DashMap<Address, UserPosition>>,
    processing_users: Arc<SyncRwLock<HashSet<Address>>>,
    event_tx: mpsc::UnboundedSender<BotEvent>,
    event_rx: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<BotEvent>>>,
    // High-priority liquidation pipeline
    priority_liquidation_tx: mpsc::UnboundedSender<Address>,
    priority_liquidation_rx: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<Address>>>,
    // Oracle price monitoring
    price_feeds: Arc<DashMap<Address, PriceFeed>>,
    asset_configs: HashMap<Address, AssetConfig>,
    users_by_collateral: Arc<DashMap<Address, HashSet<Address>>>, // asset -> users holding it as collateral
    // Liquidation functionality
    liquidation_assets: HashMap<Address, LiquidationAssetConfig>,
    liquidator_contract_address: Option<Address>,
    // Circuit breaker for extreme market conditions
    circuit_breaker: Arc<CircuitBreaker>,
}

impl<P> LiquidationBot<P>
where
    P: Provider + 'static,
{
    /// Get a reference to the signer for transaction signing
    pub fn signer(&self) -> &PrivateKeySigner {
        &self.signer
    }

    /// Get circuit breaker status and statistics
    pub fn get_circuit_breaker_status(
        &self,
    ) -> (
        CircuitBreakerState,
        crate::circuit_breaker::CircuitBreakerStats,
    ) {
        (
            self.circuit_breaker.get_state(),
            self.circuit_breaker.get_stats(),
        )
    }

    /// Manually control circuit breaker state (for emergency situations)
    pub async fn disable_circuit_breaker(&self) -> Result<()> {
        self.circuit_breaker.disable().await
    }

    /// Re-enable circuit breaker after manual disable
    pub async fn enable_circuit_breaker(&self) -> Result<()> {
        self.circuit_breaker.enable().await
    }

    /// Run high-priority liquidation processor
    async fn run_liquidation_processor(&self) -> Result<()> {
        info!("🚀 Starting high-priority liquidation processor...");
        
        let mut priority_rx = self.priority_liquidation_rx.lock().await;
        
        while let Some(user_address) = priority_rx.recv().await {
            info!("⚡ Processing priority liquidation for user: {:?}", user_address);
            
            // Check circuit breaker before processing liquidation
            // IMPORTANT: Capture state BEFORE liquidation to avoid TOCTOU bug
            let circuit_breaker_state_before = self.circuit_breaker.get_state();

            if !self.circuit_breaker.is_liquidation_allowed() {
                warn!(
                    "🚫 Priority liquidation blocked by circuit breaker (state: {:?}) for user: {:?}",
                    circuit_breaker_state_before, user_address
                );
                self.circuit_breaker.record_blocked_liquidation();

                // Record the blocked attempt for frequency monitoring
                if let Err(e) = self
                    .circuit_breaker
                    .record_liquidation_attempt(false, None)
                    .await
                {
                    warn!("Failed to record blocked priority liquidation attempt: {}", e);
                }
                continue;
            }

            // Determine if this is a test liquidation based on state BEFORE execution
            let is_test_liquidation = circuit_breaker_state_before
                == crate::circuit_breaker::CircuitBreakerState::HalfOpen;

            // Get current gas price for circuit breaker monitoring
            let current_gas_price = match self.provider.get_gas_price().await {
                Ok(price) => Some(alloy_primitives::U256::from(price)),
                Err(e) => {
                    warn!("Failed to get current gas price for priority liquidation: {}", e);
                    None
                }
            };

            // Execute liquidation first, then record success/failure
            let liquidation_result = liquidation::handle_liquidation_opportunity(
                self.provider.clone(),
                &self.db_pool,
                user_address,
                self.config.min_profit_threshold,
                self.liquidator_contract_address,
                Some(self.signer.clone()),
                &self.pool_contract,
                &self.liquidation_assets,
                &self.config.rpc_url,
            )
            .await;

            let liquidation_succeeded = matches!(liquidation_result, Ok(LiquidationResult::Executed(_)));

            // Handle liquidation failure with fallback
            match &liquidation_result {
                Ok(LiquidationResult::Executed(tx_hash)) => {
                    info!("✅ Priority liquidation executed successfully for user: {:?}, TX: {}", user_address, tx_hash);
                }
                Ok(LiquidationResult::NotNeeded(reason)) => {
                    info!("ℹ️ Priority liquidation not needed for user: {:?}, reason: {:?}", user_address, reason);
                }
                Ok(LiquidationResult::Failed(error)) => {
                    warn!("❌ Priority liquidation failed for user: {:?}, error: {}", user_address, error);
                }
                Err(e) => {
                    error!(
                        "Failed to handle priority liquidation opportunity for {:?}: {}",
                        user_address, e
                    );

                    // Fallback to legacy handler for logging
                    if let Err(legacy_err) = liquidation::handle_liquidation_opportunity_legacy(
                        &self.db_pool,
                        user_address,
                        self.config.min_profit_threshold,
                    )
                    .await
                    {
                        error!("Legacy liquidation handler also failed for priority liquidation: {}", legacy_err);
                    }
                }
            }

            // Record ALL liquidation attempts (both successful and failed) for frequency monitoring
            if let Err(e) = self
                .circuit_breaker
                .record_liquidation_attempt(liquidation_succeeded, current_gas_price)
                .await
            {
                warn!(
                    "Failed to record priority liquidation attempt for circuit breaker: {}",
                    e
                );
            }

            // Record test liquidation if this was a half-open state test (determined before execution)
            if is_test_liquidation && liquidation_succeeded {
                self.circuit_breaker.record_test_liquidation();
                info!(
                    "📊 Recorded successful test liquidation (priority path, state was half-open before attempt) for user: {:?}",
                    user_address
                );
            }
        }
        
        Ok(())
    }

    /// Start periodic circuit breaker status reporting
    async fn run_circuit_breaker_status_reporter(&self) -> Result<()> {
        let circuit_breaker = self.circuit_breaker.clone();
        let mut interval = tokio::time::interval(Duration::from_secs(300)); // Report every 5 minutes

        loop {
            interval.tick().await;
            circuit_breaker.log_status();
        }
    }
    pub async fn new(
        provider: Arc<P>,
        config: BotConfig,
        signer: PrivateKeySigner,
    ) -> Result<Self> {
        // Load ABI of L2Pool from Hardhat artifact
        let artifact_str = include_str!("../abi/L2Pool.json");
        let artifact: HardhatArtifact = serde_json::from_str(artifact_str)?;
        let interface = Interface::new(artifact.abi);

        // Aave V3 Pool address on Base mainnet
        let pool_addr: Address = "0xA238Dd80C259a72e81d7e4664a9801593F98d1c5".parse()?;
        let pool_contract = interface.connect(pool_addr, provider.clone());

        // Try to create WebSocket provider for event monitoring
        let ws_provider = match websocket::try_connect_websocket(&config.ws_url).await {
            Ok(provider) => {
                info!("✅ WebSocket connection established successfully!");
                provider
            }
            Err(e) => {
                info!("⚠️ WebSocket connection failed: {}", e);
                info!("Falling back to HTTP provider for polling mode");
                info!("To enable real-time monitoring, configure WS_URL with a proper WebSocket RPC endpoint");
                provider.clone() as Arc<dyn Provider>
            }
        };

        // Initialize database
        let db_pool = database::init_database(&config.database_url).await?;

        // For now, liquidator contract is optional
        let _liquidator_contract = None;

        // Create event channels for internal communication
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        
        // Create high-priority liquidation channels
        let (priority_liquidation_tx, priority_liquidation_rx) = mpsc::unbounded_channel();

        // Initialize asset configurations for Base Sepolia
        let asset_configs = oracle::init_asset_configs();

        // Initialize circuit breaker
        let circuit_breaker = Arc::new(CircuitBreaker::new(config.clone()));

        // Initialize liquidation asset configurations based on configuration
        let liquidation_assets = match &config.asset_loading_method {
            AssetLoadingMethod::FullyDynamic => {
                info!("🔄 Loading all assets dynamically from Aave protocol...");
                match liquidation::assets::init_assets_from_protocol(&*provider).await {
                    Ok(assets) => {
                        info!(
                            "✅ Successfully loaded {} assets dynamically from Aave protocol",
                            assets.len()
                        );
                        assets
                    }
                    Err(e) => {
                        error!(
                            "❌ Failed to load assets dynamically from Aave protocol: {}",
                            e
                        );
                        error!("🔄 Falling back to hardcoded asset configurations");
                        liquidation::assets::init_base_mainnet_assets()
                    }
                }
            }
            AssetLoadingMethod::FromFile(file_path) => {
                info!("📁 Loading assets from config file: {}", file_path);
                match liquidation::assets::init_assets_from_file(&*provider, file_path).await {
                    Ok(assets) => {
                        info!(
                            "✅ Successfully loaded {} assets from config file",
                            assets.len()
                        );
                        assets
                    }
                    Err(e) => {
                        error!("❌ Failed to load assets from config file: {}", e);
                        error!("🔄 Falling back to hardcoded asset configurations");
                        liquidation::assets::init_base_mainnet_assets()
                    }
                }
            }
            AssetLoadingMethod::Hardcoded => {
                info!("🔧 Using hardcoded asset configurations");
                warn!("⚠️  IMPORTANT: Using hardcoded asset configurations");
                warn!("⚠️  Asset IDs may become incorrect if Aave's reserve list changes!");
                liquidation::assets::init_base_mainnet_assets()
            }
            AssetLoadingMethod::DynamicWithFallback => {
                info!("🔄 Loading assets with dynamic metadata and fallback support...");
                match liquidation::assets::init_base_mainnet_assets_async(&*provider).await {
                    Ok(assets) => {
                        info!("✅ Successfully loaded asset configurations with dynamic data from Aave protocol");
                        assets
                    }
                    Err(e) => {
                        error!("❌ Failed to fetch dynamic asset data from Aave protocol");
                        error!("📋 Error details: {}", e);
                        warn!("🔄 Falling back to hardcoded asset configurations");
                        warn!("⚠️  IMPORTANT: This fallback uses hardcoded asset IDs which may become incorrect");
                        warn!("⚠️  if Aave's reserve list ordering changes over time!");
                        warn!("🔍 To fix this issue:");
                        warn!(
                            "   1. Verify the correct Aave V3 contract addresses for Base mainnet"
                        );
                        warn!("   2. Update BASE_POOL_ADDRESSES_PROVIDER and BASE_UI_POOL_DATA_PROVIDER");
                        warn!("   3. Check if Aave V3 is actually deployed on Base network");
                        liquidation::assets::init_base_mainnet_assets()
                    }
                }
            }
        };

        // Get liquidator contract address from config
        let liquidator_contract_address = config.liquidator_contract;

        if let Some(addr) = liquidator_contract_address {
            info!("✅ Liquidator contract configured at: {:?}", addr);
        } else {
            warn!("⚠️ Liquidator contract not configured - liquidation execution will be disabled");
        }

        info!("✅ Bot initialized with signer for transaction signing capability");

        Ok(Self {
            provider,
            ws_provider,
            signer,
            config,
            pool_contract,
            _liquidator_contract,
            db_pool,
            user_positions: Arc::new(DashMap::new()),
            processing_users: Arc::new(SyncRwLock::new(HashSet::new())),
            event_tx,
            event_rx: Arc::new(tokio::sync::Mutex::new(event_rx)),
            priority_liquidation_tx,
            priority_liquidation_rx: Arc::new(tokio::sync::Mutex::new(priority_liquidation_rx)),
            // Oracle price monitoring
            price_feeds: Arc::new(DashMap::new()),
            asset_configs,
            users_by_collateral: Arc::new(DashMap::new()),
            // Liquidation functionality
            liquidation_assets,
            liquidator_contract_address,
            circuit_breaker,
        })
    }

    async fn run_event_processor(&self) -> Result<()> {
        info!("Starting event processor...");

        let mut event_rx = self.event_rx.lock().await;

        while let Some(event) = event_rx.recv().await {
            match event {
                BotEvent::UserPositionChanged(user) => {
                    debug!(
                        "🔍 Processing UserPositionChanged event for user: {:?}",
                        user
                    );
                    if let Err(e) = scanner::update_user_position(
                        self.provider.clone(),
                        &self.pool_contract,
                        &self.db_pool,
                        self.user_positions.clone(),
                        self.processing_users.clone(),
                        self.event_tx.clone(),
                        self.config.health_factor_threshold,
                        user,
                        Some(self.users_by_collateral.clone()),
                        Some(&self.asset_configs),
                        None, // No priority channel for regular event processing to avoid double-processing
                    )
                    .await
                    {
                        error!("Failed to update user position for {:?}: {}", user, e);
                    } else {
                        debug!("✅ Completed health check for user: {:?}", user);
                    }
                }
                BotEvent::LiquidationOpportunity(user) => {
                    // Check circuit breaker before processing liquidation
                    // IMPORTANT: Capture state BEFORE liquidation to avoid TOCTOU bug
                    let circuit_breaker_state_before = self.circuit_breaker.get_state();

                    if !self.circuit_breaker.is_liquidation_allowed() {
                        warn!(
                            "🚫 Liquidation blocked by circuit breaker (state: {:?}) for user: {:?}",
                            circuit_breaker_state_before, user
                        );
                        self.circuit_breaker.record_blocked_liquidation();

                        // Record the blocked attempt for frequency monitoring
                        if let Err(e) = self
                            .circuit_breaker
                            .record_liquidation_attempt(false, None)
                            .await
                        {
                            warn!("Failed to record blocked liquidation attempt: {}", e);
                        }
                        continue;
                    }

                    info!("🎯 Processing liquidation opportunity for user: {:?}", user);

                    // Determine if this is a test liquidation based on state BEFORE execution
                    let is_test_liquidation = circuit_breaker_state_before
                        == crate::circuit_breaker::CircuitBreakerState::HalfOpen;

                    // Get current gas price for circuit breaker monitoring
                    let current_gas_price = match self.provider.get_gas_price().await {
                        Ok(price) => Some(alloy_primitives::U256::from(price)),
                        Err(e) => {
                            warn!("Failed to get current gas price: {}", e);
                            None
                        }
                    };

                    // Execute liquidation first, then record success/failure
                    let liquidation_result = liquidation::handle_liquidation_opportunity(
                        self.provider.clone(),
                        &self.db_pool,
                        user,
                        self.config.min_profit_threshold,
                        self.liquidator_contract_address,
                        Some(self.signer.clone()),
                        &self.pool_contract,
                        &self.liquidation_assets,
                        &self.config.rpc_url,
                    )
                    .await;

                    let liquidation_succeeded = matches!(liquidation_result, Ok(LiquidationResult::Executed(_)));

                    // Handle liquidation failure with fallback
                    match &liquidation_result {
                        Ok(LiquidationResult::Executed(tx_hash)) => {
                            info!("✅ Liquidation executed successfully for user: {:?}, TX: {}", user, tx_hash);
                        }
                        Ok(LiquidationResult::NotNeeded(reason)) => {
                            info!("ℹ️ Liquidation not needed for user: {:?}, reason: {:?}", user, reason);
                        }
                        Ok(LiquidationResult::Failed(error)) => {
                            warn!("❌ Liquidation failed for user: {:?}, error: {}", user, error);
                        }
                        Err(e) => {
                            error!(
                                "Failed to handle liquidation opportunity for {:?}: {}",
                                user, e
                            );

                            // Fallback to legacy handler for logging
                            if let Err(legacy_err) = liquidation::handle_liquidation_opportunity_legacy(
                                &self.db_pool,
                                user,
                                self.config.min_profit_threshold,
                            )
                            .await
                            {
                                error!("Legacy liquidation handler also failed: {}", legacy_err);
                            }
                        }
                    }

                    // Record ALL liquidation attempts (both successful and failed) for frequency monitoring
                    if let Err(e) = self
                        .circuit_breaker
                        .record_liquidation_attempt(liquidation_succeeded, current_gas_price)
                        .await
                    {
                        warn!(
                            "Failed to record liquidation attempt for circuit breaker: {}",
                            e
                        );
                    }

                    // Record test liquidation if this was a half-open state test (determined before execution)
                    if is_test_liquidation && liquidation_succeeded {
                        self.circuit_breaker.record_test_liquidation();
                        info!(
                            "📊 Recorded successful test liquidation (state was half-open before attempt) for user: {:?}",
                            user
                        );
                    }
                }
                BotEvent::PriceUpdate(asset, _old_price, _new_price) => {
                    debug!("Price update detected for asset: {:?}", asset);
                    // Could trigger a broader scan of users holding this asset
                }
                BotEvent::DatabaseSync(positions) => {
                    debug!("Database sync requested for {} positions", positions.len());
                    for position in positions {
                        if let Err(e) = database::save_user_position(&self.db_pool, &position).await
                        {
                            error!("Failed to sync position for {:?}: {}", position.address, e);
                        }
                    }
                }
                BotEvent::OraclePriceChanged(asset, new_price) => {
                    debug!("Oracle price changed for asset: {:?}", asset);

                    // Record price change for circuit breaker monitoring with current gas price
                    let current_gas_price = match self.provider.get_gas_price().await {
                        Ok(price) => Some(alloy_primitives::U256::from(price)),
                        Err(e) => {
                            warn!("Failed to get current gas price for price update: {}", e);
                            None
                        }
                    };

                    if let Err(e) = self
                        .circuit_breaker
                        .record_price_update(
                            Some(new_price),
                            current_gas_price,
                        )
                        .await
                    {
                        warn!("Failed to record price change for circuit breaker: {}", e);
                    }

                    if let Err(e) = self.handle_oracle_price_change(asset, new_price).await {
                        error!("Error handling oracle price change: {}", e);
                    }
                }
            }
        }

        Ok(())
    }

    async fn handle_oracle_price_change(
        &self,
        asset_address: Address,
        new_price: U256,
    ) -> Result<()> {
        // Update the price feed
        if let Some(mut feed) = self.price_feeds.get_mut(&asset_address) {
            let old_price = feed.last_price;
            feed.last_price = new_price;
            feed.last_updated = chrono::Utc::now();

            info!(
                "🔄 Price changed for {}: {} -> {}",
                feed.asset_symbol, old_price, new_price
            );

            // Since any asset price change can affect user health factors (due to cross-collateral effects),
            // we need to check all users who have ANY collateral, not just users with this specific asset
            let mut users_to_check = HashSet::new();

            // First, collect all users who have this specific asset as collateral
            if let Some(asset_users) = self.users_by_collateral.get(&asset_address) {
                for user in asset_users.iter() {
                    users_to_check.insert(*user);
                }
                info!(
                    "📊 Found {} users directly holding {} as collateral",
                    asset_users.len(),
                    feed.asset_symbol
                );
            }

            // Additionally, collect users from all other tracked collateral assets since price changes
            // can affect health factors through cross-collateral effects
            for entry in self.users_by_collateral.iter() {
                for user in entry.value().iter() {
                    users_to_check.insert(*user);
                }
            }

            if !users_to_check.is_empty() {
                info!(
                    "🔍 Triggering health checks for {} total users affected by {} price change",
                    users_to_check.len(),
                    feed.asset_symbol
                );

                // Trigger health factor recalculation for all affected users
                for user in users_to_check {
                    info!("🔍 Triggering health check for user: {:?}", user);
                    let _ = self.event_tx.send(BotEvent::UserPositionChanged(user));
                }
            } else {
                info!("⚠️ No users mapped to any collateral yet, triggering broader health check");

                // Fallback: If we don't have users mapped to collateral yet,
                // trigger a check of all currently tracked users
                let tracked_user_count = self.user_positions.len();
                if tracked_user_count > 0 {
                    info!("🔍 Checking {} currently tracked users", tracked_user_count);
                    for entry in self.user_positions.iter() {
                        let user = *entry.key();
                        info!("🔍 Triggering health check for tracked user: {:?}", user);
                        let _ = self.event_tx.send(BotEvent::UserPositionChanged(user));
                    }
                } else {
                    info!("ℹ️ No users currently tracked in memory");
                }

                // Also trigger a database scan for at-risk users
                match crate::database::get_at_risk_users(&self.db_pool).await {
                    Ok(at_risk_users) => {
                        info!(
                            "Checking {} at-risk users from database due to price change",
                            at_risk_users.len()
                        );
                        for user in at_risk_users {
                            info!(
                                "🔍 Triggering health check for at-risk user from DB: {:?}",
                                user
                            );
                            let _ = self
                                .event_tx
                                .send(BotEvent::UserPositionChanged(user.address));
                        }
                    }
                    Err(e) => {
                        error!(
                            "Failed to get at-risk users during price change handling: {}",
                            e
                        );
                    }
                }
            }
        }

        Ok(())
    }

    pub async fn run(&self) -> Result<()> {
        info!("🚀 Starting Aave v3 Liquidation Bot with Real-Time WebSocket Monitoring");

        // First, perform initial user discovery to populate the database
        info!("🔍 Performing initial user discovery...");
        let pool_address = *self.pool_contract.address();

        match discovery::discover_initial_users(
            self.provider.clone(),
            pool_address,
            &self.db_pool,
            self.event_tx.clone(),
        )
        .await
        {
            Ok(discovered_users) => {
                info!(
                    "✅ Initial discovery completed. Found {} users to monitor",
                    discovered_users.len()
                );
            }
            Err(e) => {
                warn!("⚠️ Initial user discovery failed: {}. Continuing with event-based monitoring only.", e);
            }
        }

        // Populate users_by_collateral mapping for all users in database
        info!("🗺️ Populating users_by_collateral mapping for all discovered users...");
        if let Err(e) = self.populate_initial_collateral_mapping().await {
            warn!("Failed to populate initial collateral mapping: {}", e);
        }

        // Start all monitoring services including circuit breaker and priority liquidation processor
        tokio::try_join!(
            websocket::start_event_monitoring(
                self.provider.clone(),
                self.ws_provider.clone(),
                &self.config.ws_url,
                self.event_tx.clone(),
                if self.config.ws_fast_path_enabled { Some(self.priority_liquidation_tx.clone()) } else { None },
            ),
            oracle::start_oracle_monitoring(
                self.provider.clone(),
                self.ws_provider.clone(),
                &self.config.ws_url,
                self.event_tx.clone(),
                self.asset_configs.clone(),
                self.price_feeds.clone(),
            ),
            self.run_event_processor(),
            self.run_liquidation_processor(),
            scanner::run_periodic_scan(
                self.provider.clone(),
                pool_address,
                self.db_pool.clone(),
                self.event_tx.clone(),
                self.config.clone(),
                self.asset_configs.clone(),
                self.user_positions.clone(),
                if self.config.ws_fast_path_enabled { Some(self.priority_liquidation_tx.clone()) } else { None },
            ),
            scanner::start_status_reporter(self.db_pool.clone(), self.user_positions.clone(),),
            self.circuit_breaker.run_alert_processor(),
            self.run_circuit_breaker_status_reporter(),
        )?;

        Ok(())
    }

    /// Populate users_by_collateral mapping for all users in the database
    async fn populate_initial_collateral_mapping(&self) -> Result<()> {
        // Get all users from database
        let all_users = match database::get_all_user_positions(&self.db_pool).await {
            Ok(users) => users,
            Err(e) => {
                error!("Failed to get all users from database: {}", e);
                return Ok(()); // Don't fail startup for this
            }
        };

        info!(
            "📋 Populating collateral mapping for {} users from database",
            all_users.len()
        );

        let mut processed_count = 0;

        for user in all_users {
            // Trigger a user position update to populate collateral mapping
            let _ = self
                .event_tx
                .send(BotEvent::UserPositionChanged(user.address));
            processed_count += 1;

            // Add small delay to avoid overwhelming the system
            if processed_count % 10 == 0 {
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            }
        }

        info!(
            "✅ Queued {} users for collateral mapping population",
            processed_count
        );
        Ok(())
    }
}
