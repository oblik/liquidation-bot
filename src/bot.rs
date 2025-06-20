use alloy_contract::{ContractInstance, Interface};
use alloy_primitives::{Address, U256};
use alloy_provider::Provider;
use dashmap::DashMap;
use eyre::Result;
use sqlx::{Pool, Sqlite};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info};

use crate::config::BotConfig;
use crate::database;
use crate::events::BotEvent;
use crate::liquidation;
use crate::models::{AssetConfig, HardhatArtifact, PriceFeed, UserPosition};
use crate::monitoring::{oracle, scanner, websocket};

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
        let liquidator_contract = None;

        // Create event channels for internal communication
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        // Initialize asset configurations for Base Sepolia
        let asset_configs = oracle::init_asset_configs();

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

    async fn run_event_processor(&self) -> Result<()> {
        info!("Starting event processor...");

        let mut event_rx = self.event_rx.lock().await;

        while let Some(event) = event_rx.recv().await {
            match event {
                BotEvent::UserPositionChanged(user) => {
                    if let Err(e) = scanner::update_user_position(
                        self.provider.clone(),
                        &self.pool_contract,
                        &self.db_pool,
                        self.user_positions.clone(),
                        self.processing_users.clone(),
                        self.event_tx.clone(),
                        self.config.health_factor_threshold,
                        user,
                    )
                    .await
                    {
                        error!("Failed to update user position for {:?}: {}", user, e);
                    }
                }
                BotEvent::LiquidationOpportunity(user) => {
                    if let Err(e) = liquidation::handle_liquidation_opportunity(
                        &self.db_pool,
                        user,
                        self.config.min_profit_threshold,
                    )
                    .await
                    {
                        error!("Failed to handle liquidation opportunity for {:?}: {}", user, e);
                    }
                }
                BotEvent::PriceUpdate(asset, old_price, new_price) => {
                    debug!("Price update detected for asset: {:?}", asset);
                    // Could trigger a broader scan of users holding this asset
                }
                BotEvent::DatabaseSync(positions) => {
                    debug!("Database sync requested for {} positions", positions.len());
                    for position in positions {
                        if let Err(e) = database::save_user_position(&self.db_pool, &position).await {
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

    async fn handle_oracle_price_change(&self, asset_address: Address, new_price: U256) -> Result<()> {
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
                self.db_pool.clone(),
                self.event_tx.clone(),
                self.config.clone(),
            ),
            scanner::start_status_reporter(
                self.db_pool.clone(),
                self.user_positions.clone(),
            ),
        )?;

        Ok(())
    }
}