use alloy_primitives::Address;
use eyre::Result;
use serde::{Deserialize, Serialize};
use std::env;

/// Configuration for the liquidation monitor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidationMonitorConfig {
    /// RPC URL for connecting to the blockchain
    pub rpc_url: String,
    
    /// WebSocket URL for real-time monitoring (optional, falls back to polling)
    pub ws_url: Option<String>,
    
    /// Aave L2Pool contract address
    pub pool_address: Address,
    
    /// Maximum number of events to store in memory
    pub max_events_stored: usize,
    
    /// Whether to log events to a file
    pub log_to_file: bool,
    
    /// Path to the log file (if log_to_file is true)
    pub log_file_path: Option<String>,
    
    /// Interval for printing statistics (in minutes)
    pub stats_interval_minutes: u64,
    
    /// Whether to enable detailed logging
    pub verbose: bool,
    
    /// Block range for historical analysis (optional)
    pub historical_from_block: Option<u64>,
    pub historical_to_block: Option<u64>,
    
    /// Asset name mappings for better readability (optional)
    pub asset_names: Option<std::collections::HashMap<Address, String>>,
}

impl Default for LiquidationMonitorConfig {
    fn default() -> Self {
        Self {
            rpc_url: String::from("http://localhost:8545"),
            ws_url: None,
            pool_address: "0xA238Dd80C259a72e81d7e4664a9801593F98d1c5"
                .parse()
                .expect("Valid address"),
            max_events_stored: 1000,
            log_to_file: false,
            log_file_path: Some(String::from("liquidations.jsonl")),
            stats_interval_minutes: 30,
            verbose: true,
            historical_from_block: None,
            historical_to_block: None,
            asset_names: None,
        }
    }
}

impl LiquidationMonitorConfig {
    /// Create configuration from environment variables
    pub fn from_env() -> Result<Self> {
        dotenvy::dotenv().ok();
        
        let rpc_url = env::var("RPC_URL")
            .or_else(|_| env::var("LIQUIDATION_MONITOR_RPC_URL"))
            .unwrap_or_else(|_| String::from("http://localhost:8545"));
        
        let ws_url = env::var("WS_URL")
            .or_else(|_| env::var("LIQUIDATION_MONITOR_WS_URL"))
            .ok()
            .or_else(|| {
                // Try to derive WebSocket URL from RPC URL
                if rpc_url.starts_with("http://") {
                    Some(rpc_url.replace("http://", "ws://"))
                } else if rpc_url.starts_with("https://") {
                    Some(rpc_url.replace("https://", "wss://"))
                } else {
                    None
                }
            });
        
        let pool_address = env::var("POOL_ADDRESS")
            .or_else(|_| env::var("LIQUIDATION_MONITOR_POOL_ADDRESS"))
            .unwrap_or_else(|_| String::from("0xA238Dd80C259a72e81d7e4664a9801593F98d1c5"))
            .parse::<Address>()
            .map_err(|e| eyre::eyre!("Invalid pool address: {}", e))?;
        
        let max_events_stored = env::var("LIQUIDATION_MONITOR_MAX_EVENTS")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(1000);
        
        let log_to_file = env::var("LIQUIDATION_MONITOR_LOG_TO_FILE")
            .ok()
            .and_then(|s| s.parse::<bool>().ok())
            .unwrap_or(true);
        
        let log_file_path = env::var("LIQUIDATION_MONITOR_LOG_FILE")
            .ok()
            .or_else(|| Some(String::from("liquidations.jsonl")));
        
        let stats_interval_minutes = env::var("LIQUIDATION_MONITOR_STATS_INTERVAL")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(30);
        
        let verbose = env::var("LIQUIDATION_MONITOR_VERBOSE")
            .ok()
            .and_then(|s| s.parse::<bool>().ok())
            .unwrap_or(true);
        
        Ok(Self {
            rpc_url,
            ws_url,
            pool_address,
            max_events_stored,
            log_to_file,
            log_file_path,
            stats_interval_minutes,
            verbose,
            historical_from_block: None,
            historical_to_block: None,
            asset_names: None,
        })
    }
    
    /// Create configuration from a JSON file
    pub fn from_file(path: &str) -> Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let config: Self = serde_json::from_str(&contents)?;
        Ok(config)
    }
    
    /// Save configuration to a JSON file
    pub fn save_to_file(&self, path: &str) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }
    
    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        // Check if RPC URL is valid
        if self.rpc_url.is_empty() {
            return Err(eyre::eyre!("RPC URL cannot be empty"));
        }
        
        // Check if stats interval is reasonable
        if self.stats_interval_minutes == 0 {
            return Err(eyre::eyre!("Stats interval must be greater than 0"));
        }
        
        // Check if max events stored is reasonable
        if self.max_events_stored == 0 {
            return Err(eyre::eyre!("Max events stored must be greater than 0"));
        }
        
        Ok(())
    }
    
    /// Get a summary of the configuration
    pub fn summary(&self) -> String {
        format!(
            "Liquidation Monitor Configuration:\n\
             - RPC URL: {}\n\
             - WebSocket URL: {}\n\
             - Pool Address: {:?}\n\
             - Max Events Stored: {}\n\
             - Log to File: {} ({})\n\
             - Stats Interval: {} minutes\n\
             - Verbose: {}",
            self.rpc_url,
            self.ws_url.as_ref().unwrap_or(&String::from("None (using polling)")),
            self.pool_address,
            self.max_events_stored,
            self.log_to_file,
            self.log_file_path.as_ref().unwrap_or(&String::from("N/A")),
            self.stats_interval_minutes,
            self.verbose
        )
    }
}