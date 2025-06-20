use alloy_contract::{ContractInstance, Interface};
use alloy_primitives::{Address, U256};
use alloy_provider::{Provider, ProviderBuilder, WsConnect};
use chrono::Utc;
use dashmap::DashMap;
use eyre::Result;
use sqlx::{Pool, Sqlite};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

use crate::config::BotConfig;
use crate::database::{init_database, log_monitoring_event, save_user_position};
use crate::events::BotEvent;
use crate::liquidation::LiquidationOpportunityHandler;
use crate::models::{HardhatArtifact, UserPosition};
use crate::monitoring::{PeriodicScanner, WebSocketMonitor};

// Main bot struct with event monitoring capabilities
pub struct LiquidationBot<P> {
    provider: Arc<P>,
    ws_provider: Arc<dyn Provider>,
    pub config: BotConfig,
    pool_contract: ContractInstance<Arc<P>>,
    liquidator_contract: Option<ContractInstance<Arc<P>>>,
    db_pool: Pool<Sqlite>,
    user_positions: Arc<DashMap<Address, UserPosition>>,
    processing_users: Arc<RwLock<HashSet<Address>>>,
    event_tx: mpsc::UnboundedSender<BotEvent>,
    event_rx: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<BotEvent>>>,
    liquidation_handler: LiquidationOpportunityHandler,
}

impl<P> LiquidationBot<P>
where
    P: Provider + 'static,
{
    pub async fn new(provider: Arc<P>, config: BotConfig) -> Result<Self> {
        // Load ABI of L2Pool from Hardhat artifact
        let artifact_str = include_str!("../abi/L2Pool.json");
        let artifact: HardhatArtifact = serde_json::from_str(artifact_str)?;
        let interface = Interface::new(artifact.abi);

        // Aave V3 Pool address on Base mainnet
        let pool_addr: Address = "0xA238Dd80C259a72e81d7e4664a9801593F98d1c5".parse()?;
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
        let db_pool = init_database(&config.database_url).await?;

        // For now, liquidator contract is optional
        let liquidator_contract = None;

        // Create event channels for internal communication
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        // Initialize shared user positions cache
        let user_positions = Arc::new(DashMap::new());

        // Initialize liquidation handler
        let liquidation_handler = LiquidationOpportunityHandler::new(
            db_pool.clone(),
            user_positions.clone(),
        );

        Ok(Self {
            provider,
            ws_provider,
            config,
            pool_contract,
            liquidator_contract,
            db_pool,
            user_positions,
            processing_users: Arc::new(RwLock::new(HashSet::new())),
            event_tx,
            event_rx: Arc::new(tokio::sync::Mutex::new(event_rx)),
            liquidation_handler,
        })
    }

    async fn try_connect_websocket(ws_url: &str) -> Result<Arc<dyn Provider>> {
        let ws_connect = WsConnect::new(ws_url.to_string());
        let ws_provider = ProviderBuilder::new().connect_ws(ws_connect).await?;
        Ok(Arc::new(ws_provider))
    }

    pub async fn check_user_health(&self, user: Address) -> Result<UserPosition> {
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
        if let Err(e) = save_user_position(&self.db_pool, &position).await {
            error!("Failed to save user position to database: {}", e);
        }

        // Check if liquidation opportunity
        if position.is_liquidatable() {
            let _ = self.event_tx.send(BotEvent::LiquidationOpportunity(user));
        }

        // Remove from processing set
        self.processing_users.write().await.remove(&user);

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
                    if let Err(e) = self.liquidation_handler.handle_opportunity(user).await {
                        error!(
                            "Failed to handle liquidation opportunity for {:?}: {}",
                            user, e
                        );
                    }
                }
                BotEvent::PriceUpdate(asset) => {
                    debug!("Price update detected for asset: {:?}", asset);
                    // Could trigger a broader scan of users holding this asset
                }
                BotEvent::DatabaseSync(positions) => {
                    debug!("Database sync requested for {} positions", positions.len());
                    for position in positions {
                        if let Err(e) = save_user_position(&self.db_pool, &position).await {
                            error!("Failed to sync position for {:?}: {}", position.address, e);
                        }
                    }
                }
            }
        }

        Ok(())
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
                .filter(|entry| entry.value().is_liquidatable())
                .count();

            info!(
                "📊 Status Report: {} positions tracked, {} at risk, {} liquidatable",
                position_count, at_risk_count, liquidatable_count
            );

            if let Err(e) = log_monitoring_event(
                &self.db_pool,
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

    pub async fn run(&self) -> Result<()> {
        let using_websocket = self.config.ws_url.starts_with("wss://")
            && !self.config.ws_url.contains("sepolia.base.org");

        if using_websocket {
            info!("🚀 Starting Aave v3 Liquidation Bot with Real-Time WebSocket Monitoring");
        } else {
            info!("🚀 Starting Aave v3 Liquidation Bot with Polling-Based Monitoring");
        }

        // Initialize monitoring components
        let scanner = PeriodicScanner::new(
            Arc::new(self.config.clone()),
            self.db_pool.clone(),
            self.event_tx.clone(),
        );

        // Start all monitoring tasks concurrently
        let event_processor = self.run_event_processor();
        let periodic_scan = scanner.start_scanning();
        let status_reporter = self.start_status_reporter();

        // Handle WebSocket monitoring if available
        let event_monitoring = if using_websocket {
            match WebSocketMonitor::new(&self.config.ws_url, self.event_tx.clone()).await {
                Ok(monitor) => {
                    tokio::spawn(async move {
                        if let Err(e) = monitor.start_monitoring().await {
                            error!("WebSocket monitoring failed: {}", e);
                        }
                    });
                }
                Err(e) => {
                    warn!("Failed to initialize WebSocket monitor: {}", e);
                }
            }
        };

        // Test basic functionality first
        if let Some(target_user) = self.config.target_user {
            info!("🎯 Testing with target user: {:?}", target_user);
            if let Err(e) = self.update_user_position(target_user).await {
                error!("Failed to check target user: {}", e);
            }
        }

        // Log startup
        let monitoring_mode = if using_websocket {
            "Real-time WebSocket monitoring enabled"
        } else {
            "Polling mode monitoring enabled"
        };

        if let Err(e) = log_monitoring_event(&self.db_pool, "bot_started", None, Some(monitoring_mode))
            .await
        {
            error!("Failed to log startup: {}", e);
        }

        // Run all tasks
        tokio::try_join!(event_processor, periodic_scan, status_reporter)?;

        Ok(())
    }
}