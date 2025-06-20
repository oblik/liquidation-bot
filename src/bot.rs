use alloy_contract::{ContractInstance, Interface};
use alloy_primitives::{Address, U256};
use alloy_provider::{Provider, ProviderBuilder, WsConnect};
use chrono::Utc;
use dashmap::DashMap;
use eyre::Result;
use sqlx::{Pool, Sqlite};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{error, info, warn};

use crate::config::{init_asset_configs, BotConfig};
use crate::database;
use crate::events;
use crate::models::{AssetConfig, BotEvent, HardhatArtifact, PriceFeed, UserPosition};
use crate::oracle;
use crate::position;

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
    // Oracle price monitoring
    price_feeds: Arc<DashMap<Address, PriceFeed>>,
    asset_configs: HashMap<Address, AssetConfig>,
    users_by_collateral: Arc<DashMap<Address, HashSet<Address>>>, // asset -> users holding it as collateral
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
        let db_pool = database::init_database(&config.database_url).await?;

        // For now, liquidator contract is optional
        let liquidator_contract = None;

        // Create event channels for internal communication
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        // Initialize asset configurations for Base Sepolia
        let asset_configs = init_asset_configs();

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

    async fn start_event_monitoring(&self) -> Result<()> {
        events::start_event_monitoring(&self.ws_provider, &self.config.ws_url, self.event_tx.clone()).await
    }

    async fn run_event_processor(&self) -> Result<()> {
        info!("Starting event processor...");

        let mut event_rx = self.event_rx.lock().await;

        while let Some(event) = event_rx.recv().await {
            match event {
                BotEvent::UserPositionChanged(user) => {
                    if let Err(e) = position::update_user_position(&self.pool_contract, user, &self.db_pool).await {
                        error!("Failed to update user position for {:?}: {}", user, e);
                    }
                }
                BotEvent::LiquidationOpportunity(user) => {
                    if let Err(e) = position::handle_liquidation_opportunity(
                        &self.pool_contract,
                        user,
                        self.config.min_profit_threshold,
                        &self.db_pool,
                    ).await {
                        error!("Failed to handle liquidation opportunity for {:?}: {}", user, e);
                    }
                }
                BotEvent::PriceUpdate(asset, old_price, new_price) => {
                    tracing::debug!("Price update detected for asset: {:?}", asset);
                    // Could trigger a broader scan of users holding this asset
                }
                BotEvent::DatabaseSync(positions) => {
                    tracing::debug!("Database sync requested for {} positions", positions.len());
                    for position in positions {
                        if let Err(e) = database::save_user_position(&self.db_pool, &position).await {
                            error!("Failed to sync position for {:?}: {}", position.address, e);
                        }
                    }
                }
                BotEvent::OraclePriceChanged(asset, new_price) => {
                    tracing::debug!("Oracle price changed for asset: {:?}", asset);
                    if let Err(e) = oracle::handle_oracle_price_change(
                        asset,
                        new_price,
                        &self.users_by_collateral,
                        &self.pool_contract,
                        &self.db_pool,
                    ).await {
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
            let at_risk_users = match database::get_at_risk_users(&self.db_pool).await {
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
                let _ = self.event_tx.send(BotEvent::UserPositionChanged(target_user));
            }
        }
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

            if let Err(e) = database::log_monitoring_event(
                &self.db_pool,
                "status_report",
                None,
                Some(&format!(
                    "positions:{}, at_risk:{}, liquidatable:{}",
                    position_count, at_risk_count, liquidatable_count
                )),
            ).await {
                error!("Failed to log status report: {}", e);
            }
        }
    }

    async fn start_oracle_monitoring(&self) -> Result<()> {
        oracle::start_oracle_monitoring(
            self.provider.clone(),
            self.asset_configs.clone(),
            self.price_feeds.clone(),
            self.event_tx.clone(),
        ).await
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