use alloy_contract::{ContractInstance, Interface};
use alloy_json_abi::JsonAbi;
use alloy_primitives::{Address, U256};
use alloy_provider::{Provider, ProviderBuilder, WsConnect};
use alloy_rpc_types::{Filter, Log};
use alloy_signer_local::PrivateKeySigner;
use alloy_sol_types::{sol, SolEvent};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use eyre::Result;
use futures::StreamExt;
use serde::Deserialize;
use sqlx::{Pool, Row, Sqlite};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

// Define Aave events using sol! macro for type safety
sol! {
    event Borrow(
        address indexed reserve,
        address user,
        address indexed onBehalfOf,
        uint256 amount,
        uint8 interestRateMode,
        uint256 borrowRate,
        uint16 indexed referralCode
    );

    event Repay(
        address indexed reserve,
        address indexed user,
        address indexed repayer,
        uint256 amount,
        bool useATokens
    );

    event Supply(
        address indexed reserve,
        address user,
        address indexed onBehalfOf,
        uint256 amount,
        uint16 indexed referralCode
    );

    event Withdraw(
        address indexed reserve,
        address indexed user,
        address indexed to,
        uint256 amount
    );

    event LiquidationCall(
        address indexed collateralAsset,
        address indexed debtAsset,
        address indexed user,
        uint256 debtToCover,
        uint256 liquidatedCollateralAmount,
        address liquidator,
        bool receiveAToken
    );

    event ReserveDataUpdated(
        address indexed reserve,
        uint256 liquidityRate,
        uint256 stableBorrowRate,
        uint256 variableBorrowRate,
        uint256 liquidityIndex,
        uint256 variableBorrowIndex
    );

    // Chainlink Price Feed events
    event AnswerUpdated(
        int256 indexed current,
        uint256 indexed roundId,
        uint256 updatedAt
    );
}

// Oracle price feed monitoring
#[derive(Debug, Clone)]
struct PriceFeed {
    asset_address: Address,
    feed_address: Address,
    asset_symbol: String,
    last_price: U256,
    last_updated: DateTime<Utc>,
    price_change_threshold: f64, // Percentage change to trigger recalculation
}

#[derive(Debug, Clone)]
struct AssetConfig {
    address: Address,
    symbol: String,
    chainlink_feed: Address,
    price_change_threshold: f64, // e.g., 0.05 for 5% change
}

#[derive(Deserialize)]
struct HardhatArtifact {
    abi: JsonAbi,
}

// User position tracking
#[derive(Debug, Clone)]
struct UserPosition {
    address: Address,
    total_collateral_base: U256,
    total_debt_base: U256,
    available_borrows_base: U256,
    current_liquidation_threshold: U256,
    ltv: U256,
    health_factor: U256,
    last_updated: DateTime<Utc>,
    is_at_risk: bool,
}

// Event types for internal messaging
#[derive(Debug, Clone)]
enum BotEvent {
    UserPositionChanged(Address),
    PriceUpdate(Address, U256, U256), // asset address, old_price, new_price
    LiquidationOpportunity(Address),  // user address
    DatabaseSync(Vec<UserPosition>),
    OraclePriceChanged(Address, U256), // asset address, new price
}

// Configuration struct
#[derive(Debug, Clone)]
struct BotConfig {
    rpc_url: String,
    ws_url: String,
    private_key: String,
    liquidator_contract: Option<Address>,
    min_profit_threshold: U256,
    gas_price_multiplier: u64,
    target_user: Option<Address>,
    database_url: String,
    health_factor_threshold: U256, // Alert if HF below this (e.g., 1.1)
    monitoring_interval_secs: u64,
}

impl BotConfig {
    fn from_env() -> Result<Self> {
        dotenvy::dotenv().ok();

        let rpc_url = std::env::var("RPC_URL")
            .map_err(|_| eyre::eyre!("RPC_URL environment variable not set"))?;

        // Try to derive WebSocket URL from HTTP URL if not explicitly set
        let ws_url = std::env::var("WS_URL").unwrap_or_else(|_| {
            rpc_url
                .replace("http://", "ws://")
                .replace("https://", "wss://")
        });

        let private_key = std::env::var("PRIVATE_KEY")
            .map_err(|_| eyre::eyre!("PRIVATE_KEY environment variable not set"))?;

        let liquidator_contract = std::env::var("LIQUIDATOR_CONTRACT")
            .ok()
            .and_then(|addr| addr.parse().ok());

        let min_profit_threshold = std::env::var("MIN_PROFIT_THRESHOLD")
            .unwrap_or_else(|_| "5000000000000000000".to_string()) // 5 ETH wei default
            .parse()
            .unwrap_or(U256::from(5000000000000000000u64));

        let gas_price_multiplier = std::env::var("GAS_PRICE_MULTIPLIER")
            .unwrap_or_else(|_| "2".to_string())
            .parse()
            .unwrap_or(2);

        let target_user = std::env::var("TARGET_USER")
            .ok()
            .and_then(|addr| addr.parse().ok());

        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "sqlite:liquidation_bot.db".to_string());

        let health_factor_threshold = std::env::var("HEALTH_FACTOR_THRESHOLD")
            .unwrap_or_else(|_| "1100000000000000000".to_string()) // 1.1 ETH wei default
            .parse()
            .unwrap_or(U256::from(1100000000000000000u64));

        let monitoring_interval_secs = std::env::var("MONITORING_INTERVAL_SECS")
            .unwrap_or_else(|_| "5".to_string())
            .parse()
            .unwrap_or(5);

        Ok(Self {
            rpc_url,
            ws_url,
            private_key,
            liquidator_contract,
            min_profit_threshold,
            gas_price_multiplier,
            target_user,
            database_url,
            health_factor_threshold,
            monitoring_interval_secs,
        })
    }
}

// Main bot struct with event monitoring capabilities
struct LiquidationBot<P> {
    provider: Arc<P>,
    ws_provider: Arc<dyn Provider>,
    config: BotConfig,
    pool_contract: ContractInstance<Arc<P>>,
    liquidator_contract: Option<ContractInstance<Arc<P>>>,
    db_pool: Pool<Sqlite>,
    user_positions: Arc<DashMap<Address, UserPosition>>,
    processing_users: Arc<RwLock<HashSet<Address>>>,
    event_tx: mpsc::UnboundedSender<BotEvent>,
    event_rx: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<BotEvent>>>,
    // Oracle price monitoring
    price_feeds: Arc<DashMap<Address, PriceFeed>>,
    asset_configs: HashMap<Address, AssetConfig>,
    users_by_collateral: Arc<DashMap<Address, HashSet<Address>>>, // asset -> users holding it as collateral
}

impl<P> LiquidationBot<P>
where
    P: Provider + 'static,
{
    async fn new(provider: Arc<P>, config: BotConfig) -> Result<Self> {
        // Load ABI of L2Pool from Hardhat artifact
        let artifact_str = include_str!("../abi/L2Pool.json");
        let artifact: HardhatArtifact = serde_json::from_str(artifact_str)?;
        let interface = Interface::new(artifact.abi);

        // Aave V3 Pool address on Base Sepolia testnet
        let pool_addr: Address = "0x07eA79F68B2B3df564D0A34F8e19D9B1e339814b".parse()?;
        let pool_contract = interface.connect(pool_addr, provider.clone());

        // Try to create WebSocket provider for event monitoring
        let ws_provider = match Self::try_connect_websocket(&config.ws_url).await {
            Ok(provider) => {
                info!("✅ WebSocket connection established successfully!");
                provider
            }
            Err(e) => {
                warn!("⚠️ WebSocket connection failed: {}", e);
                warn!("Falling back to HTTP provider for polling mode");
                warn!("To enable real-time monitoring, configure WS_URL with a proper WebSocket RPC endpoint");
                provider.clone() as Arc<dyn Provider>
            }
        };

        // Initialize database
        let db_pool = Self::init_database(&config.database_url).await?;

        // For now, liquidator contract is optional
        let liquidator_contract = None;

        // Create event channels for internal communication
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        // Initialize asset configurations for Base Sepolia
        let asset_configs = Self::init_asset_configs();

        Ok(Self {
            provider,
            ws_provider,
            config,
            pool_contract,
            liquidator_contract,
            db_pool,
            user_positions: Arc::new(DashMap::new()),
            processing_users: Arc::new(RwLock::new(HashSet::new())),
            event_tx,
            event_rx: Arc::new(tokio::sync::Mutex::new(event_rx)),
            // Oracle price monitoring
            price_feeds: Arc::new(DashMap::new()),
            asset_configs,
            users_by_collateral: Arc::new(DashMap::new()),
        })
    }

    async fn try_connect_websocket(ws_url: &str) -> Result<Arc<dyn Provider>> {
        let ws_connect = WsConnect::new(ws_url.to_string());
        let ws_provider = ProviderBuilder::new().connect_ws(ws_connect).await?;
        Ok(Arc::new(ws_provider))
    }

    async fn init_database(database_url: &str) -> Result<Pool<Sqlite>> {
        let pool = sqlx::SqlitePool::connect(database_url).await?;

        // Create tables if they don't exist
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS user_positions (
                address TEXT PRIMARY KEY,
                total_collateral_base TEXT NOT NULL,
                total_debt_base TEXT NOT NULL,
                available_borrows_base TEXT NOT NULL,
                current_liquidation_threshold TEXT NOT NULL,
                ltv TEXT NOT NULL,
                health_factor TEXT NOT NULL,
                last_updated DATETIME NOT NULL,
                is_at_risk BOOLEAN NOT NULL DEFAULT FALSE
            )
        "#,
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS liquidation_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_address TEXT NOT NULL,
                collateral_asset TEXT NOT NULL,
                debt_asset TEXT NOT NULL,
                debt_covered TEXT NOT NULL,
                collateral_received TEXT NOT NULL,
                profit TEXT NOT NULL,
                tx_hash TEXT,
                block_number INTEGER,
                timestamp DATETIME NOT NULL
            )
        "#,
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS monitoring_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                event_type TEXT NOT NULL,
                user_address TEXT,
                asset_address TEXT,
                health_factor TEXT,
                timestamp DATETIME NOT NULL,
                details TEXT
            )
        "#,
        )
        .execute(&pool)
        .await?;

        // Create table for price data
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS price_feeds (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                asset_address TEXT NOT NULL,
                asset_symbol TEXT NOT NULL,
                price TEXT NOT NULL,
                timestamp DATETIME NOT NULL
            )
        "#,
        )
        .execute(&pool)
        .await?;

        // Create index for price feeds
        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_asset_timestamp 
            ON price_feeds (asset_address, timestamp)
        "#,
        )
        .execute(&pool)
        .await?;

        info!("Database initialized successfully");
        Ok(pool)
    }

    fn init_asset_configs() -> HashMap<Address, AssetConfig> {
        let mut configs = HashMap::new();
        
        // Base Sepolia testnet asset configurations
        // Only including verified working oracle feeds
        
        // WETH - CONFIRMED WORKING ✅
        configs.insert(
            "0x4200000000000000000000000000000000000006".parse().unwrap(),
            AssetConfig {
                address: "0x4200000000000000000000000000000000000006".parse().unwrap(),
                symbol: "WETH".to_string(),
                chainlink_feed: "0x4aDC67696bA383F43DD60A9e78F2C97Fbbfc7cb1".parse().unwrap(), // ETH/USD on Base Sepolia ✅
                price_change_threshold: 0.02, // 2% price change threshold
            },
        );
        
        // Note: USDC/USDT oracle feeds are not available or working on Base Sepolia testnet
        // In production on Base mainnet, you would add:
        // - USDC: Different oracle address  
        // - USDT: Different oracle address
        // - DAI: Different oracle address
        // For now, focusing on working WETH oracle for demonstration
        
        info!("Initialized {} asset configuration(s) for oracle monitoring", configs.len());
        info!("🎯 Active oracle feeds:");
        for config in configs.values() {
            info!("   {} ({}): {}", config.symbol, config.address, config.chainlink_feed);
        }
        
        configs
    }

    async fn start_event_monitoring(&self) -> Result<()> {
        // Check if we're using WebSocket or HTTP fallback
        let using_websocket = self.config.ws_url.starts_with("wss://")
            && !self.config.ws_url.contains("sepolia.base.org");

        if !using_websocket {
            info!("Event monitoring initialized (using HTTP polling mode)");
            warn!("WebSocket event subscriptions skipped - using periodic polling instead");
            warn!(
                "For real-time monitoring, configure WS_URL with a proper WebSocket RPC endpoint"
            );
            return Ok(());
        }

        info!("🚀 Starting real-time WebSocket event monitoring...");

        let pool_address: Address = "0x07eA79F68B2B3df564D0A34F8e19D9B1e339814b".parse()?;

        // Create a general filter for all events from the Aave pool
        let pool_filter = Filter::new().address(pool_address);

        let event_tx = self.event_tx.clone();
        let ws_provider = self.ws_provider.clone();

        tokio::spawn(async move {
            info!("Subscribing to Aave Pool events...");
            let sub = match ws_provider.subscribe_logs(&pool_filter).await {
                Ok(sub) => {
                    info!("✅ Successfully subscribed to Aave Pool events!");
                    sub
                }
                Err(e) => {
                    error!("❌ Failed to subscribe to logs: {}", e);
                    return;
                }
            };

            let mut stream = sub.into_stream();
            info!("🎧 Listening for real-time Aave events...");

            while let Some(log) = stream.next().await {
                if let Err(e) = Self::handle_log_event(log, &event_tx).await {
                    error!("Error handling log event: {}", e);
                }
            }
        });

        info!("✅ WebSocket event subscriptions established");
        Ok(())
    }

    async fn handle_log_event(log: Log, event_tx: &mpsc::UnboundedSender<BotEvent>) -> Result<()> {
        // For now, extract user addresses from log topics manually
        // In Aave events, user addresses are typically in topic[1] or topic[2]
        let topics = log.topics();

        debug!("Processing log event with {} topics", topics.len());

        // Most Aave events have user address in topic[1] (after the event signature)
        if topics.len() >= 2 {
            // Extract the user address from topic[1] (assuming it's an address)
            let user_bytes = topics[1].as_slice();
            if user_bytes.len() >= 20 {
                // Address is the last 20 bytes of the topic
                let addr_bytes = &user_bytes[12..32];
                if let Ok(user_addr) = Address::try_from(addr_bytes) {
                    debug!("Detected event for user: {}", user_addr);
                    let _ = event_tx.send(BotEvent::UserPositionChanged(user_addr));
                }
            }
        }

        // Also check topic[2] for events that might have user there
        if topics.len() >= 3 {
            let user_bytes = topics[2].as_slice();
            if user_bytes.len() >= 20 {
                let addr_bytes = &user_bytes[12..32];
                if let Ok(user_addr) = Address::try_from(addr_bytes) {
                    debug!("Detected additional event for user: {}", user_addr);
                    let _ = event_tx.send(BotEvent::UserPositionChanged(user_addr));
                }
            }
        }

        Ok(())
    }

    async fn check_user_health(&self, user: Address) -> Result<UserPosition> {
        debug!("Checking health factor for user: {:?}", user);

        // Call getUserAccountData
        let args = [alloy_dyn_abi::DynSolValue::Address(user)];
        let call = self.pool_contract.function("getUserAccountData", &args)?;
        let result: Vec<alloy_dyn_abi::DynSolValue> = call.call().await?;

        let parse_u256 = |index: usize| -> Result<U256> {
            if let Some(alloy_dyn_abi::DynSolValue::Uint(value, _)) = result.get(index) {
                Ok(*value)
            } else {
                Err(eyre::eyre!("Failed to parse U256 at index {}", index))
            }
        };

        let total_collateral_base = parse_u256(0)?;
        let total_debt_base = parse_u256(1)?;
        let available_borrows_base = parse_u256(2)?;
        let current_liquidation_threshold = parse_u256(3)?;
        let ltv = parse_u256(4)?;
        let health_factor = parse_u256(5)?;

        let is_liquidatable = health_factor < U256::from(10u128.pow(18));
        let is_at_risk = health_factor < self.config.health_factor_threshold;

        let position = UserPosition {
            address: user,
            total_collateral_base,
            total_debt_base,
            available_borrows_base,
            current_liquidation_threshold,
            ltv,
            health_factor,
            last_updated: Utc::now(),
            is_at_risk,
        };

        if is_liquidatable {
            warn!(
                "🚨 User {:?} is LIQUIDATABLE! Health Factor: {}",
                user, health_factor
            );
        } else if is_at_risk {
            warn!(
                "⚠️  User {:?} is at risk. Health Factor: {}",
                user, health_factor
            );
        } else {
            debug!(
                "✅ User {:?} is healthy. Health Factor: {}",
                user, health_factor
            );
        }

        Ok(position)
    }

    async fn update_user_position(&self, user: Address) -> Result<()> {
        // Prevent duplicate processing
        {
            let mut processing = self.processing_users.write().await;
            if processing.contains(&user) {
                return Ok(());
            }
            processing.insert(user);
        }

        let position = match self.check_user_health(user).await {
            Ok(pos) => pos,
            Err(e) => {
                // Remove from processing set
                self.processing_users.write().await.remove(&user);
                return Err(e);
            }
        };

        // Store in memory cache
        self.user_positions.insert(user, position.clone());

        // Store in database
        if let Err(e) = self.save_user_position(&position).await {
            error!("Failed to save user position to database: {}", e);
        }

        // Check if liquidation opportunity
        if position.health_factor < U256::from(10u128.pow(18)) {
            let _ = self.event_tx.send(BotEvent::LiquidationOpportunity(user));
        }

        // Remove from processing set
        self.processing_users.write().await.remove(&user);

        Ok(())
    }

    async fn save_user_position(&self, position: &UserPosition) -> Result<()> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO user_positions (
                address, total_collateral_base, total_debt_base, available_borrows_base,
                current_liquidation_threshold, ltv, health_factor, last_updated, is_at_risk
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        )
        .bind(position.address.to_string())
        .bind(position.total_collateral_base.to_string())
        .bind(position.total_debt_base.to_string())
        .bind(position.available_borrows_base.to_string())
        .bind(position.current_liquidation_threshold.to_string())
        .bind(position.ltv.to_string())
        .bind(position.health_factor.to_string())
        .bind(position.last_updated)
        .bind(position.is_at_risk)
        .execute(&self.db_pool)
        .await?;

        Ok(())
    }

    async fn log_monitoring_event(
        &self,
        event_type: &str,
        user_address: Option<Address>,
        details: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO monitoring_events (event_type, user_address, timestamp, details)
            VALUES (?, ?, ?, ?)
        "#,
        )
        .bind(event_type)
        .bind(user_address.map(|addr| addr.to_string()))
        .bind(Utc::now())
        .bind(details)
        .execute(&self.db_pool)
        .await?;

        Ok(())
    }

    async fn handle_liquidation_opportunity(&self, user: Address) -> Result<()> {
        info!("🎯 Processing liquidation opportunity for user: {:?}", user);

        // Get current position
        let position = match self.user_positions.get(&user) {
            Some(pos) => pos.clone(),
            None => {
                // Refresh position data
                self.check_user_health(user).await?
            }
        };

        // Log the opportunity
        let details = format!(
            "Health Factor: {}, Debt: {}, Collateral: {}",
            position.health_factor, position.total_debt_base, position.total_collateral_base
        );

        if let Err(e) = self
            .log_monitoring_event("liquidation_opportunity", Some(user), Some(&details))
            .await
        {
            error!("Failed to log liquidation opportunity: {}", e);
        }

        // For now, just log the opportunity
        // In the future, this would call attempt_liquidation
        warn!("Liquidation opportunity detected but not executing (monitoring mode)");
        warn!(
            "User: {:?}, HF: {}, Debt: {}, Collateral: {}",
            user, position.health_factor, position.total_debt_base, position.total_collateral_base
        );

        Ok(())
    }

    async fn run_event_processor(&self) -> Result<()> {
        info!("Starting event processor...");

        let mut event_rx = self.event_rx.lock().await;

        while let Some(event) = event_rx.recv().await {
            match event {
                BotEvent::UserPositionChanged(user) => {
                    if let Err(e) = self.update_user_position(user).await {
                        error!("Failed to update user position for {:?}: {}", user, e);
                    }
                }
                BotEvent::LiquidationOpportunity(user) => {
                    if let Err(e) = self.handle_liquidation_opportunity(user).await {
                        error!(
                            "Failed to handle liquidation opportunity for {:?}: {}",
                            user, e
                        );
                    }
                }
                BotEvent::PriceUpdate(asset, old_price, new_price) => {
                    debug!("Price update detected for asset: {:?}", asset);
                    // Could trigger a broader scan of users holding this asset
                }
                BotEvent::DatabaseSync(positions) => {
                    debug!("Database sync requested for {} positions", positions.len());
                    for position in positions {
                        if let Err(e) = self.save_user_position(&position).await {
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

    async fn run_periodic_scan(&self) -> Result<()> {
        info!("Starting periodic position scan...");

        let mut interval = tokio::time::interval(
            tokio::time::Duration::from_secs(self.config.monitoring_interval_secs * 6), // Slower than event-driven updates
        );

        loop {
            interval.tick().await;

            // Get at-risk users from database
            let at_risk_users = match self.get_at_risk_users().await {
                Ok(users) => users,
                Err(e) => {
                    error!("Failed to get at-risk users: {}", e);
                    continue;
                }
            };

            info!("Scanning {} at-risk users", at_risk_users.len());

            for user_addr in at_risk_users {
                let _ = self.event_tx.send(BotEvent::UserPositionChanged(user_addr));
            }

            // If we have a specific target user, always check them
            if let Some(target_user) = self.config.target_user {
                let _ = self
                    .event_tx
                    .send(BotEvent::UserPositionChanged(target_user));
            }
        }
    }

    async fn get_at_risk_users(&self) -> Result<Vec<Address>> {
        let rows = sqlx::query(
            "SELECT address FROM user_positions WHERE is_at_risk = true OR total_debt_base != '0' ORDER BY health_factor ASC LIMIT 100"
        )
        .fetch_all(&self.db_pool)
        .await?;

        let mut users = Vec::new();
        for row in rows {
            if let Ok(addr_str) = row.try_get::<String, _>("address") {
                if let Ok(addr) = addr_str.parse::<Address>() {
                    users.push(addr);
                }
            }
        }

        Ok(users)
    }

    async fn start_status_reporter(&self) -> Result<()> {
        info!("Starting status reporter...");

        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(300)); // Every 5 minutes

        loop {
            interval.tick().await;

            let position_count = self.user_positions.len();
            let at_risk_count = self
                .user_positions
                .iter()
                .filter(|entry| entry.value().is_at_risk)
                .count();
            let liquidatable_count = self
                .user_positions
                .iter()
                .filter(|entry| entry.value().health_factor < U256::from(10u128.pow(18)))
                .count();

            info!(
                "📊 Status Report: {} positions tracked, {} at risk, {} liquidatable",
                position_count, at_risk_count, liquidatable_count
            );

            if let Err(e) = self
                .log_monitoring_event(
                    "status_report",
                    None,
                    Some(&format!(
                        "positions:{}, at_risk:{}, liquidatable:{}",
                        position_count, at_risk_count, liquidatable_count
                    )),
                )
                .await
            {
                error!("Failed to log status report: {}", e);
            }
        }
    }

    async fn start_oracle_monitoring(&self) -> Result<()> {
        info!("🔮 Starting Chainlink oracle price monitoring...");

        // Check if we're using WebSocket
        let using_websocket = self.config.ws_url.starts_with("wss://")
            && !self.config.ws_url.contains("sepolia.base.org");

        info!("🔧 Oracle monitoring mode decision:");
        info!("   WebSocket URL: {}", self.config.ws_url);
        info!(
            "   Starts with wss://: {}",
            self.config.ws_url.starts_with("wss://")
        );
        info!(
            "   Contains sepolia.base.org: {}",
            self.config.ws_url.contains("sepolia.base.org")
        );
        info!("   Using WebSocket mode: {}", using_websocket);

        if !using_websocket {
            info!("🔄 Oracle monitoring will use periodic polling instead of real-time events");
            return self.start_periodic_price_polling().await;
        }

        info!("📡 Using real-time WebSocket oracle monitoring");
        info!("⚠️ Note: Oracle events may be infrequent on testnet");
        info!("💡 To see active monitoring, you can force polling mode by setting a non-wss:// WS_URL");

        // Also start periodic polling as backup to show activity
        info!("🔄 Starting backup polling to show oracle activity...");
        let _ = self.start_periodic_price_polling().await;

        // Subscribe to AnswerUpdated events from each price feed
        for (asset_address, asset_config) in &self.asset_configs {
            let price_feed = PriceFeed {
                asset_address: *asset_address,
                feed_address: asset_config.chainlink_feed,
                asset_symbol: asset_config.symbol.clone(),
                last_price: U256::ZERO,
                last_updated: Utc::now(),
                price_change_threshold: asset_config.price_change_threshold,
            };

            self.price_feeds.insert(*asset_address, price_feed);

            // Subscribe to AnswerUpdated events for this price feed
            let feed_filter = Filter::new().address(asset_config.chainlink_feed);

            let event_tx = self.event_tx.clone();
            let ws_provider = self.ws_provider.clone();
            let asset_addr = *asset_address;
            let symbol = asset_config.symbol.clone();

            tokio::spawn(async move {
                info!("Subscribing to {} price feed events...", symbol);

                let sub = match ws_provider.subscribe_logs(&feed_filter).await {
                    Ok(sub) => {
                        info!("✅ Successfully subscribed to {} price feed!", symbol);
                        sub
                    }
                    Err(e) => {
                        error!("❌ Failed to subscribe to {} price feed: {}", symbol, e);
                        return;
                    }
                };

                let mut stream = sub.into_stream();
                info!("👂 Listening for {} price updates...", symbol);

                while let Some(log) = stream.next().await {
                    if let Err(e) =
                        Self::handle_price_update_event(log, &event_tx, asset_addr, &symbol).await
                    {
                        error!("Error handling price update for {}: {}", symbol, e);
                    }
                }
            });
        }

        info!("✅ Oracle price monitoring subscriptions established");
        Ok(())
    }

    async fn start_periodic_price_polling(&self) -> Result<()> {
        info!("🔄 Starting periodic price polling (every 30 seconds)...");
        info!(
            "🎯 Monitoring {} assets for price changes",
            self.asset_configs.len()
        );

        for (_, config) in &self.asset_configs {
            info!(
                "📡 Will monitor {}: {} (threshold: {}%)",
                config.symbol,
                config.chainlink_feed,
                config.price_change_threshold * 100.0
            );
        }

        let event_tx = self.event_tx.clone();
        let provider = self.provider.clone();
        let asset_configs = self.asset_configs.clone();
        let price_feeds = self.price_feeds.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));

            loop {
                interval.tick().await;
                info!(
                    "🔍 Polling oracle prices for {} assets...",
                    asset_configs.len()
                );

                for (asset_address, asset_config) in &asset_configs {
                    info!(
                        "📞 Calling {} oracle at {}",
                        asset_config.symbol, asset_config.chainlink_feed
                    );

                    match Self::fetch_price_from_oracle(
                        &provider,
                        asset_config.chainlink_feed,
                        &asset_config.symbol,
                    )
                    .await
                    {
                        Ok(new_price) => {
                            info!("✅ {} price fetched: {}", asset_config.symbol, new_price);

                            // Check if price changed significantly
                            if let Some(mut feed) = price_feeds.get_mut(asset_address) {
                                let old_price = feed.last_price;
                                let price_change = if old_price > U256::ZERO {
                                    let diff = if new_price > old_price {
                                        new_price - old_price
                                    } else {
                                        old_price - new_price
                                    };
                                    // Calculate percentage change
                                    (diff * U256::from(10000)) / old_price // Basis points
                                } else {
                                    U256::MAX // First price update
                                };

                                let threshold_bp = U256::from(
                                    (asset_config.price_change_threshold * 10000.0) as u64,
                                );

                                if price_change > threshold_bp || old_price == U256::ZERO {
                                    feed.last_price = new_price;
                                    feed.last_updated = Utc::now();

                                    let change_pct = if old_price > U256::ZERO {
                                        price_change.as_limbs()[0] as f64 / 100.0
                                    } else {
                                        0.0
                                    };

                                    info!(
                                        "🚨 SIGNIFICANT PRICE CHANGE for {}: {} → {} ({}%)",
                                        asset_config.symbol, old_price, new_price, change_pct
                                    );

                                    let _ = event_tx.send(BotEvent::OraclePriceChanged(
                                        *asset_address,
                                        new_price,
                                    ));
                                } else {
                                    info!(
                                        "📊 {} price stable: {} (change: {}bp, need: {}bp)",
                                        asset_config.symbol,
                                        new_price,
                                        price_change.as_limbs()[0],
                                        threshold_bp.as_limbs()[0]
                                    );
                                }
                            } else {
                                warn!("⚠️ No price feed entry found for {}", asset_config.symbol);
                            }
                        }
                        Err(e) => {
                            error!(
                                "❌ Failed to fetch {} price from {}: {}",
                                asset_config.symbol, asset_config.chainlink_feed, e
                            );
                        }
                    }
                }

                info!("✅ Oracle price polling round completed");
            }
        });

        Ok(())
    }

    async fn fetch_price_from_oracle(
        provider: &Arc<P>,
        feed_address: Address,
        symbol: &str,
    ) -> Result<U256> {
        // Create a simple call to the price feed's latestAnswer() function
        let call_data = alloy_primitives::hex::decode("50d25bcd").unwrap(); // latestAnswer() selector

        let call_request = alloy_rpc_types::TransactionRequest {
            to: Some(feed_address.into()),
            input: alloy_rpc_types::TransactionInput::new(call_data.into()),
            ..Default::default()
        };

        match provider.call(call_request).await {
            Ok(result) => {
                if result.len() >= 32 {
                    let price = U256::from_be_slice(&result[..32]);
                    debug!("Fetched price for {}: {}", symbol, price);
                    Ok(price)
                } else {
                    Err(eyre::eyre!("Invalid price data length for {}", symbol))
                }
            }
            Err(e) => {
                debug!("Failed to fetch price for {}: {}", symbol, e);
                Err(e.into())
            }
        }
    }

    async fn handle_price_update_event(
        log: alloy_rpc_types::Log,
        event_tx: &mpsc::UnboundedSender<BotEvent>,
        asset_address: Address,
        symbol: &str,
    ) -> Result<()> {
        // For now, we'll use a simplified approach since LogData access is complex
        // In a production environment, you would properly decode the AnswerUpdated event
        // For demonstration, we'll just trigger a price change event
        info!(
            "📊 Oracle event detected for {}, triggering price check",
            symbol
        );

        // Send a dummy price change event - in production this would be the actual decoded price
        let placeholder_price = U256::from(1000_00000000u64); // $1000 * 1e8 (Chainlink format)
        let _ = event_tx.send(BotEvent::OraclePriceChanged(
            asset_address,
            placeholder_price,
        ));

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
            feed.last_updated = Utc::now();

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
                    let _ = self.event_tx.send(BotEvent::UserPositionChanged(*user));
                }
            }
        }

        Ok(())
    }

    pub async fn run(&self) -> Result<()> {
        info!("🚀 Starting Aave v3 Liquidation Bot with Real-Time WebSocket Monitoring");

        // Start all monitoring services
        tokio::try_join!(
            self.start_event_monitoring(),
            self.start_oracle_monitoring(),
            self.run_event_processor(),
            self.run_periodic_scan(),
            self.start_status_reporter(),
        )?;

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    info!("Starting Aave v3 Liquidation Bot on Base");

    // Load configuration
    let config = BotConfig::from_env()?;
    info!("Configuration loaded");

    // Parse private key and create signer
    let signer: PrivateKeySigner = config.private_key.parse()?;
    info!("Signer created from private key");

    // Build HTTP provider with signer for transactions
    let url = url::Url::parse(&config.rpc_url)?;
    let provider = ProviderBuilder::new().wallet(signer).connect_http(url);
    let provider = Arc::new(provider);
    info!("HTTP Provider connected to: {}", config.rpc_url);
    info!("WebSocket will connect to: {}", config.ws_url);

    // Create bot instance
    let bot = LiquidationBot::new(provider, config).await?;

    let using_websocket =
        bot.config.ws_url.starts_with("wss://") && !bot.config.ws_url.contains("sepolia.base.org");

    if using_websocket {
        info!("🤖 Liquidation bot initialized with real-time WebSocket monitoring");
    } else {
        info!("🤖 Liquidation bot initialized with polling-based monitoring");
    }

    // Run the bot
    bot.run().await?;

    Ok(())
}
