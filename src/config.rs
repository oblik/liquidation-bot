use alloy_primitives::{Address, U256};
use eyre::Result;
use tracing::warn;

// Asset loading method configuration
#[derive(Debug, Clone)]
pub enum AssetLoadingMethod {
    /// Load from Aave protocol with fallback to hardcoded values
    DynamicWithFallback,
    /// Load all assets dynamically from Aave protocol
    FullyDynamic,
    /// Load from external config file
    FromFile(String),
    /// Use hardcoded asset configurations only
    Hardcoded,
}

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
    pub health_factor_threshold: U256, // Alert/at-risk threshold (e.g., 1.1)
                                        // Should be > 1.0 (liquidation threshold) for early warning
    pub monitoring_interval_secs: u64,
    pub asset_loading_method: AssetLoadingMethod,
    pub at_risk_scan_limit: Option<usize>, // Max users to check per scan cycle (None = unlimited)
    pub full_rescan_interval_minutes: u64, // How often to do a full rescan in minutes
    // User archival configuration
    pub archive_zero_debt_users: bool, // Whether to archive users with zero debt
    pub zero_debt_cooldown_hours: u64, // Hours to wait before archiving users with zero debt
    pub safe_health_factor_threshold: U256, // Health factor threshold above which users are considered "safe" (e.g., 10.0)
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
                        "Invalid MIN_PROFIT_THRESHOLD '{}': {}. Using default 0.01 ETH.",
                        threshold_str, e
                    );
                    U256::from(10000000000000000u64) // 0.01 ETH wei default
                }
            },
            Err(_) => U256::from(10000000000000000u64), // 0.01 ETH wei default
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

        let asset_loading_method = match std::env::var("ASSET_LOADING_METHOD") {
            Ok(method_str) => match method_str.to_lowercase().as_str() {
                "dynamic" | "dynamic_with_fallback" => AssetLoadingMethod::DynamicWithFallback,
                "fully_dynamic" | "full_dynamic" => AssetLoadingMethod::FullyDynamic,
                "hardcoded" => AssetLoadingMethod::Hardcoded,
                path if path.starts_with("file:") => {
                    AssetLoadingMethod::FromFile(path.strip_prefix("file:").unwrap().to_string())
                }
                _ => {
                    warn!("Unknown ASSET_LOADING_METHOD '{}'. Using default 'dynamic_with_fallback'.", method_str);
                    AssetLoadingMethod::DynamicWithFallback
                }
            },
            Err(_) => AssetLoadingMethod::DynamicWithFallback,
        };

        let at_risk_scan_limit = match std::env::var("AT_RISK_SCAN_LIMIT") {
            Ok(limit_str) => match limit_str.parse::<usize>() {
                Ok(limit) => {
                    if limit == 0 {
                        warn!("AT_RISK_SCAN_LIMIT cannot be 0. Using unlimited scanning.");
                        None
                    } else {
                        Some(limit)
                    }
                }
                Err(e) => {
                    warn!(
                        "Invalid AT_RISK_SCAN_LIMIT '{}': {}. Using unlimited scanning.",
                        limit_str, e
                    );
                    None
                }
            },
            Err(_) => None, // Default to unlimited scanning
        };

        let full_rescan_interval_minutes = match std::env::var("FULL_RESCAN_INTERVAL_MINUTES") {
            Ok(interval_str) => match interval_str.parse::<u64>() {
                Ok(interval) => {
                    if interval == 0 {
                        warn!("FULL_RESCAN_INTERVAL_MINUTES cannot be 0. Using default 60 minutes.");
                        60
                    } else {
                        interval
                    }
                }
                Err(e) => {
                    warn!(
                        "Invalid FULL_RESCAN_INTERVAL_MINUTES '{}': {}. Using default 60 minutes.",
                        interval_str, e
                    );
                    60
                }
            },
            Err(_) => 60, // Default to 60 minutes
        };

        let archive_zero_debt_users = match std::env::var("ARCHIVE_ZERO_DEBT_USERS") {
            Ok(value) => value.parse::<bool>().unwrap_or(false),
            Err(_) => false,
        };

        let zero_debt_cooldown_hours = match std::env::var("ZERO_DEBT_COOLDOWN_HOURS") {
            Ok(hours_str) => hours_str.parse::<u64>().unwrap_or(24), // Default to 24 hours
            Err(_) => 24,
        };

        let safe_health_factor_threshold = match std::env::var("SAFE_HEALTH_FACTOR_THRESHOLD") {
            Ok(threshold_str) => match threshold_str.parse::<U256>() {
                Ok(threshold) => threshold,
                Err(e) => {
                    warn!(
                        "Invalid SAFE_HEALTH_FACTOR_THRESHOLD '{}': {}. Using default 10.0.",
                        threshold_str, e
                    );
                    U256::from(10000000000000000000u64) // 10.0 ETH wei default
                }
            },
            Err(_) => U256::from(10000000000000000000u64), // 10.0 ETH wei default
        };

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
            asset_loading_method,
            at_risk_scan_limit,
            full_rescan_interval_minutes,
            archive_zero_debt_users,
            zero_debt_cooldown_hours,
            safe_health_factor_threshold,
        })
    }
}
