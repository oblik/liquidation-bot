use alloy_contract::{ContractInstance, Interface};
use alloy_primitives::{Address, U256};
use alloy_provider::Provider;
use alloy_signer_local::PrivateKeySigner;
use dashmap::DashMap;
use eyre::Result;
use parking_lot::RwLock as SyncRwLock;
use sqlx::{Pool, Sqlite};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::config::BotConfig;
use crate::database;
use crate::events::BotEvent;
use crate::liquidation;
use crate::models::{
    AssetConfig, HardhatArtifact, LiquidationAssetConfig, PriceFeed, UserPosition,
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
    db_pool: Pool<Sqlite>,
    user_positions: Arc<DashMap<Address, UserPosition>>,
    processing_users: Arc<SyncRwLock<HashSet<Address>>>,
    event_tx: mpsc::UnboundedSender<BotEvent>,
    event_rx: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<BotEvent>>>,
    // Oracle price monitoring
    price_feeds: Arc<DashMap<Address, PriceFeed>>,
    asset_configs: HashMap<Address, AssetConfig>,
    users_by_collateral: Arc<DashMap<Address, HashSet<Address>>>, // asset -> users holding it as collateral
    // Liquidation functionality
    liquidation_assets: HashMap<Address, LiquidationAssetConfig>,
    liquidator_contract_address: Option<Address>,
}

impl<P> LiquidationBot<P>
where
    P: Provider + 'static,
{
    /// Get a reference to the signer for transaction signing
    pub fn signer(&self) -> &PrivateKeySigner {
        &self.signer
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

        // Initialize asset configurations for Base Sepolia
        let asset_configs = oracle::init_asset_configs();

        // Initialize liquidation asset configurations
        let liquidation_assets = liquidation::init_base_mainnet_assets();

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
            // Oracle price monitoring
            price_feeds: Arc::new(DashMap::new()),
            asset_configs,
            users_by_collateral: Arc::new(DashMap::new()),
            // Liquidation functionality
            liquidation_assets,
            liquidator_contract_address,
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
                    )
                    .await
                    {
                        error!("Failed to update user position for {:?}: {}", user, e);
                    } else {
                        debug!("✅ Completed health check for user: {:?}", user);
                    }
                }
                BotEvent::LiquidationOpportunity(user) => {
                    // Use enhanced liquidation handler if possible, fallback to legacy
                    if let Err(e) = liquidation::handle_liquidation_opportunity(
                        self.provider.clone(),
                        &self.db_pool,
                        user,
                        self.config.min_profit_threshold,
                        self.liquidator_contract_address,
                        Some(self.signer.clone()),
                        &self.pool_contract,
                    )
                    .await
                    {
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

            // Get all users who have this asset as collateral
            if let Some(users) = self.users_by_collateral.get(&asset_address) {
                info!(
                    "📊 Recalculating health factors for {} users affected by {} price change",
                    users.len(),
                    feed.asset_symbol
                );

                // Trigger health factor recalculation for all affected users
                for user in users.iter() {
                    info!("🔍 Triggering health check for user: {:?}", user);
                    let _ = self.event_tx.send(BotEvent::UserPositionChanged(*user));
                }
            } else {
                info!(
                    "⚠️ No users mapped to {} collateral yet, triggering broader health check",
                    feed.asset_symbol
                );

                // Fallback: If we don't have users mapped to this collateral yet,
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
                            let _ = self.event_tx.send(BotEvent::UserPositionChanged(user));
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

        // Start all monitoring services
        tokio::try_join!(
            websocket::start_event_monitoring(
                self.provider.clone(),
                self.ws_provider.clone(),
                &self.config.ws_url,
                self.event_tx.clone(),
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
            scanner::run_periodic_scan(
                self.provider.clone(),
                pool_address,
                self.db_pool.clone(),
                self.event_tx.clone(),
                self.config.clone(),
            ),
            scanner::start_status_reporter(self.db_pool.clone(), self.user_positions.clone(),),
        )?;

        Ok(())
    }

    /// Populate users_by_collateral mapping for all users in the database
    async fn populate_initial_collateral_mapping(&self) -> Result<()> {
        // Get all users from database
        let all_users = match database::get_all_users(&self.db_pool).await {
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
            let _ = self.event_tx.send(BotEvent::UserPositionChanged(user));
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
