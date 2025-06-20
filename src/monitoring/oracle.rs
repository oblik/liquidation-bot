use alloy_contract::{ContractInstance, Interface};
use alloy_json_abi::JsonAbi;
use alloy_primitives::{Address, U256};
use alloy_provider::Provider;
use alloy_rpc_types::Filter;
use alloy_sol_types::sol;
use chrono::Utc;
use eyre::Result;
use futures::StreamExt;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::events::BotEvent;

// Define Chainlink price feed events
sol! {
    #[derive(Debug)]
    event AnswerUpdated(
        int256 indexed current,
        uint256 indexed roundId,
        uint256 updatedAt
    );
}

// Asset price information
#[derive(Debug, Clone)]
pub struct AssetPrice {
    pub asset: Address,
    pub price: U256,
    pub last_updated: u64,
    pub decimals: u8,
}

// Oracle price monitoring configuration
#[derive(Debug, Clone)]
pub struct OracleConfig {
    pub price_feeds: HashMap<Address, Address>, // asset -> price feed address
    pub price_change_threshold: u64, // basis points (e.g., 500 = 5%)
    pub monitoring_enabled: bool,
}

impl Default for OracleConfig {
    fn default() -> Self {
        let mut price_feeds = HashMap::new();
        
        // Base mainnet common assets and their Chainlink price feeds
        // ETH/USD - 0x71041dddad3595F9CEd3DcCFBe3D1F4b0a16Bb70
        price_feeds.insert(
            "0x4200000000000000000000000000000000000006".parse().unwrap(), // ETH
            "0x71041dddad3595F9CEd3DcCFBe3D1F4b0a16Bb70".parse().unwrap()
        );
        
        // USDC/USD - 0x7e860098F58bBFC8648a4311b374B1D669a2bc6B
        price_feeds.insert(
            "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913".parse().unwrap(), // USDC
            "0x7e860098F58bBFC8648a4311b374B1D669a2bc6B".parse().unwrap()
        );
        
        // wstETH/ETH - 0xB88BAc61A4Ca37C43a3725912B1f472c9A5bc061
        price_feeds.insert(
            "0xc1CBa3fCea344f92D9239c08C0568f6F2F0ee452".parse().unwrap(), // wstETH
            "0xB88BAc61A4Ca37C43a3725912B1f472c9A5bc061".parse().unwrap()
        );

        Self {
            price_feeds,
            price_change_threshold: 500, // 5% default
            monitoring_enabled: true,
        }
    }
}

#[derive(Clone)]
pub struct OraclePriceMonitor<P> {
    provider: Arc<P>,
    config: OracleConfig,
    event_tx: mpsc::UnboundedSender<BotEvent>,
    asset_prices: Arc<tokio::sync::RwLock<HashMap<Address, AssetPrice>>>,
}

impl<P> OraclePriceMonitor<P>
where
    P: Provider + 'static,
{
    pub fn new(
        provider: Arc<P>,
        config: OracleConfig,
        event_tx: mpsc::UnboundedSender<BotEvent>,
    ) -> Self {
        Self {
            provider,
            config,
            event_tx,
            asset_prices: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        }
    }

    pub async fn start_monitoring(&self) -> Result<()> {
        if !self.config.monitoring_enabled {
            info!("Oracle price monitoring disabled");
            return Ok(());
        }

        info!("🔍 Starting Oracle Price Monitoring for {} assets", self.config.price_feeds.len());

        // Initialize current prices
        self.initialize_prices().await?;

        // Start monitoring each price feed
        for (asset, feed_address) in &self.config.price_feeds {
            let asset = *asset;
            let feed_address = *feed_address;
            
            let provider = self.provider.clone();
            let event_tx = self.event_tx.clone();
            let asset_prices = self.asset_prices.clone();
            let threshold = self.config.price_change_threshold;

            tokio::spawn(async move {
                if let Err(e) = Self::monitor_price_feed(
                    provider,
                    asset,
                    feed_address,
                    event_tx,
                    asset_prices,
                    threshold,
                ).await {
                    error!("Failed to monitor price feed for asset {:?}: {}", asset, e);
                }
            });
        }

        Ok(())
    }

    async fn initialize_prices(&self) -> Result<()> {
        info!("Initializing current asset prices...");
        
        for (asset, feed_address) in &self.config.price_feeds {
            match self.get_current_price(*feed_address).await {
                Ok((price, decimals)) => {
                    let asset_price = AssetPrice {
                        asset: *asset,
                        price,
                        last_updated: chrono::Utc::now().timestamp() as u64,
                        decimals,
                    };
                    
                    self.asset_prices.write().await.insert(*asset, asset_price.clone());
                    info!("Initialized price for asset {:?}: {} (decimals: {})", asset, price, decimals);
                }
                Err(e) => {
                    warn!("Failed to initialize price for asset {:?}: {}", asset, e);
                }
            }
        }
        
        Ok(())
    }

    async fn get_current_price(&self, feed_address: Address) -> Result<(U256, u8)> {
        // Standard Chainlink aggregator ABI
        let abi_json = r#"[
            {
                "inputs": [],
                "name": "latestRoundData",
                "outputs": [
                    {"internalType": "uint80", "name": "roundId", "type": "uint80"},
                    {"internalType": "int256", "name": "answer", "type": "int256"},
                    {"internalType": "uint256", "name": "startedAt", "type": "uint256"},
                    {"internalType": "uint256", "name": "updatedAt", "type": "uint256"},
                    {"internalType": "uint80", "name": "answeredInRound", "type": "uint80"}
                ],
                "stateMutability": "view",
                "type": "function"
            },
            {
                "inputs": [],
                "name": "decimals",
                "outputs": [{"internalType": "uint8", "name": "", "type": "uint8"}],
                "stateMutability": "view",
                "type": "function"
            }
        ]"#;

        let abi: JsonAbi = serde_json::from_str(abi_json)?;
        let interface = Interface::new(abi);
        let contract = interface.connect(feed_address, self.provider.clone());

        // Get decimals
        let decimals_call = contract.function("decimals", &[])?;
        let decimals_result: Vec<alloy_dyn_abi::DynSolValue> = decimals_call.call().await?;
        let decimals = if let Some(alloy_dyn_abi::DynSolValue::Uint(val, _)) = decimals_result.get(0) {
            *val as u8
        } else {
            8 // Default to 8 decimals for most Chainlink feeds
        };

        // Get latest price
        let price_call = contract.function("latestRoundData", &[])?;
        let price_result: Vec<alloy_dyn_abi::DynSolValue> = price_call.call().await?;
        
        let price = if let Some(alloy_dyn_abi::DynSolValue::Int(val, _)) = price_result.get(1) {
            // Convert signed int to unsigned (prices should always be positive)
            U256::from_limbs(val.into_limbs())
        } else {
            return Err(eyre::eyre!("Failed to parse price from oracle response"));
        };

        Ok((price, decimals))
    }

    async fn monitor_price_feed(
        provider: Arc<P>,
        asset: Address,
        feed_address: Address,
        event_tx: mpsc::UnboundedSender<BotEvent>,
        asset_prices: Arc<tokio::sync::RwLock<HashMap<Address, AssetPrice>>>,
        threshold: u64,
    ) -> Result<()> {
        info!("Monitoring price feed for asset {:?} at {:?}", asset, feed_address);

        // Create filter for AnswerUpdated events
        let filter = Filter::new()
            .address(feed_address)
            .event_signature(AnswerUpdated::SIGNATURE_HASH);

        let sub = provider.subscribe_logs(&filter).await?;
        let mut stream = sub.into_stream();

        while let Some(log) = stream.next().await {
            if let Ok(answer_event) = AnswerUpdated::decode_log(&log) {
                debug!("Price update for asset {:?}: {}", asset, answer_event.current);
                
                let new_price = U256::from_limbs(answer_event.current.into_limbs());
                
                // Check if price change is significant
                let should_notify = {
                    let prices = asset_prices.read().await;
                    if let Some(old_price) = prices.get(&asset) {
                        Self::is_significant_price_change(old_price.price, new_price, threshold)
                    } else {
                        true // First price update
                    }
                };

                if should_notify {
                    info!("Significant price change detected for asset {:?}: {}", asset, new_price);
                    
                    // Update stored price
                    {
                        let mut prices = asset_prices.write().await;
                        if let Some(asset_price) = prices.get_mut(&asset) {
                            asset_price.price = new_price;
                            asset_price.last_updated = answer_event.updatedAt.to::<u64>();
                        }
                    }
                    
                    // Notify the bot about the price update
                    let _ = event_tx.send(BotEvent::PriceUpdate(asset));
                }
            }
        }

        Ok(())
    }

    fn is_significant_price_change(old_price: U256, new_price: U256, threshold_bp: u64) -> bool {
        if old_price.is_zero() {
            return true;
        }

        let diff = if new_price > old_price {
            new_price - old_price
        } else {
            old_price - new_price
        };

        // Calculate percentage change in basis points
        let change_bp = (diff * U256::from(10000u64)) / old_price;
        
        change_bp >= U256::from(threshold_bp)
    }

    pub async fn get_asset_price(&self, asset: Address) -> Option<AssetPrice> {
        self.asset_prices.read().await.get(&asset).cloned()
    }

    pub async fn get_all_prices(&self) -> HashMap<Address, AssetPrice> {
        self.asset_prices.read().await.clone()
    }
}