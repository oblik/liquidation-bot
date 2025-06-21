use alloy_primitives::{Address, U256};
use eyre::Result;

// Configuration struct
#[derive(Debug, Clone)]
pub struct BotConfig {
    pub rpc_url: String,
    pub ws_url: String,
    pub private_key: String,
    pub liquidator_contract: Option<Address>,
    pub min_profit_threshold: U256,
    pub gas_price_multiplier: u64,
    pub target_user: Option<Address>,
    pub database_url: String,
    pub health_factor_threshold: U256, // Alert if HF below this (e.g., 1.1)
    pub monitoring_interval_secs: u64,
}

impl BotConfig {
    pub fn from_env() -> Result<Self> {
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