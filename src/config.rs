use alloy_primitives::{Address, U256};
use eyre::Result;
use tracing::warn;

// Configuration struct
#[derive(Debug, Clone)]
pub struct BotConfig {
    pub rpc_url: String,
    pub ws_url: String,
    pub private_key: String,
    pub pool_address: Address,
    pub liquidator_contract: Option<Address>,
    pub min_profit_threshold: U256,
    pub gas_price_multiplier: u64,
    pub target_user: Option<Address>,
    pub database_url: String,
    pub health_factor_threshold: U256, // Alert/at-risk threshold (e.g., 1.1)
                                        // Should be > 1.0 (liquidation threshold) for early warning
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

        // Configure pool address with intelligent defaults
        let pool_address = match std::env::var("POOL_ADDRESS") {
            Ok(addr_str) => match addr_str.parse::<Address>() {
                Ok(addr) => addr,
                Err(e) => {
                    return Err(eyre::eyre!(
                        "Invalid POOL_ADDRESS '{}': {}",
                        addr_str, e
                    ));
                }
            },
            Err(_) => {
                // Auto-detect pool address based on RPC URL
                if rpc_url.contains("sepolia") || rpc_url.contains("84532") {
                    // Base Sepolia testnet
                    "0xA37D7E3d3CaD89b44f9a08A96fE01a9F39Bd7794".parse()?
                } else if rpc_url.contains("base") || rpc_url.contains("8453") {
                    // Base mainnet
                    "0xA238Dd80C259a72e81d7e4664a9801593F98d1c5".parse()?
                } else {
                    // Default to Base mainnet
                    warn!("Could not auto-detect network from RPC URL. Defaulting to Base mainnet pool address.");
                    "0xA238Dd80C259a72e81d7e4664a9801593F98d1c5".parse()?
                }
            }
        };

        let liquidator_contract = match std::env::var("LIQUIDATOR_CONTRACT") {
            Ok(addr_str) => match addr_str.parse::<Address>() {
                Ok(addr) => Some(addr),
                Err(e) => {
                    warn!(
                        "Invalid LIQUIDATOR_CONTRACT address '{}': {}. Using None.",
                        addr_str, e
                    );
                    None
                }
            },
            Err(_) => None,
        };

        let min_profit_threshold = match std::env::var("MIN_PROFIT_THRESHOLD") {
            Ok(threshold_str) => match threshold_str.parse::<U256>() {
                Ok(threshold) => threshold,
                Err(e) => {
                    warn!(
                        "Invalid MIN_PROFIT_THRESHOLD '{}': {}. Using default 5 ETH.",
                        threshold_str, e
                    );
                    U256::from(5000000000000000000u64) // 5 ETH wei default
                }
            },
            Err(_) => U256::from(5000000000000000000u64), // 5 ETH wei default
        };

        let gas_price_multiplier = match std::env::var("GAS_PRICE_MULTIPLIER") {
            Ok(multiplier_str) => match multiplier_str.parse::<u64>() {
                Ok(multiplier) => multiplier,
                Err(e) => {
                    warn!(
                        "Invalid GAS_PRICE_MULTIPLIER '{}': {}. Using default 2.",
                        multiplier_str, e
                    );
                    2
                }
            },
            Err(_) => 2,
        };

        let target_user = match std::env::var("TARGET_USER") {
            Ok(addr_str) => match addr_str.parse::<Address>() {
                Ok(addr) => Some(addr),
                Err(e) => {
                    warn!(
                        "Invalid TARGET_USER address '{}': {}. Using None.",
                        addr_str, e
                    );
                    None
                }
            },
            Err(_) => None,
        };

        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "sqlite:liquidation_bot.db".to_string());

        let health_factor_threshold = match std::env::var("HEALTH_FACTOR_THRESHOLD") {
            Ok(threshold_str) => match threshold_str.parse::<U256>() {
                Ok(threshold) => threshold,
                Err(e) => {
                    warn!(
                        "Invalid HEALTH_FACTOR_THRESHOLD '{}': {}. Using default 1.1.",
                        threshold_str, e
                    );
                    U256::from(1100000000000000000u64) // 1.1 ETH wei default
                }
            },
            Err(_) => U256::from(1100000000000000000u64), // 1.1 ETH wei default
        };

        let monitoring_interval_secs = match std::env::var("MONITORING_INTERVAL_SECS") {
            Ok(interval_str) => match interval_str.parse::<u64>() {
                Ok(interval) => {
                    if interval == 0 {
                        warn!("MONITORING_INTERVAL_SECS cannot be 0. Using default 5 seconds.");
                        5
                    } else {
                        interval
                    }
                }
                Err(e) => {
                    warn!(
                        "Invalid MONITORING_INTERVAL_SECS '{}': {}. Using default 5 seconds.",
                        interval_str, e
                    );
                    5
                }
            },
            Err(_) => 5,
        };

        Ok(Self {
            rpc_url,
            ws_url,
            private_key,
            pool_address,
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
