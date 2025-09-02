use alloy_primitives::{Address, U256};
use alloy_provider::{Provider, ProviderBuilder, WsConnect};
use alloy_rpc_types::{BlockNumberOrTag, Filter, Log};
use alloy_sol_types::SolEvent;
use chrono::{DateTime, Local, Utc};
use eyre::Result;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::interval;
use tracing::{error, info, warn};

use crate::models::LiquidationCall;

/// Statistics for liquidation monitoring
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LiquidationStats {
    pub total_liquidations: u64,
    pub total_debt_covered: U256,
    pub total_collateral_liquidated: U256,
    pub unique_liquidators: HashMap<Address, u64>,
    pub unique_users_liquidated: HashMap<Address, u64>,
    pub asset_pairs: HashMap<(Address, Address), u64>, // (collateral, debt) -> count
    pub hourly_liquidations: HashMap<String, u64>,     // hour string -> count
    pub last_liquidation_time: Option<DateTime<Utc>>,
    pub monitoring_started: DateTime<Utc>,
}

/// Liquidation event details for logging
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidationEvent {
    pub timestamp: DateTime<Utc>,
    pub block_number: u64,
    pub transaction_hash: String,
    pub collateral_asset: Address,
    pub debt_asset: Address,
    pub user: Address,
    pub liquidator: Address,
    pub debt_to_cover: U256,
    pub liquidated_collateral_amount: U256,
    pub receive_atoken: bool,
    pub gas_used: Option<U256>,
    pub effective_gas_price: Option<U256>,
}

/// Main liquidation monitor struct
pub struct LiquidationMonitor {
    provider: Arc<dyn Provider>,
    pool_address: Address,
    stats: Arc<RwLock<LiquidationStats>>,
    events: Arc<RwLock<Vec<LiquidationEvent>>>,
    max_events_stored: usize,
    log_to_file: bool,
    file_path: Option<String>,
}

impl LiquidationMonitor {
    /// Create a new liquidation monitor
    pub async fn new(
        rpc_url: &str,
        pool_address: Address,
        max_events_stored: usize,
        log_to_file: bool,
        file_path: Option<String>,
    ) -> Result<Self> {
        // Try to connect via WebSocket first, fall back to HTTP
        let provider: Arc<dyn Provider> =
            if rpc_url.starts_with("ws://") || rpc_url.starts_with("wss://") {
                let ws_connect = WsConnect::new(rpc_url.to_string());
                let ws_provider = ProviderBuilder::new().on_ws(ws_connect).await?;
                Arc::new(ws_provider.boxed())
            } else {
                let http_provider = ProviderBuilder::new().on_http(rpc_url.parse()?);
                Arc::new(http_provider.boxed())
            };

        let stats = Arc::new(RwLock::new(LiquidationStats {
            monitoring_started: Utc::now(),
            ..Default::default()
        }));

        Ok(Self {
            provider,
            pool_address,
            stats,
            events: Arc::new(RwLock::new(Vec::new())),
            max_events_stored,
            log_to_file,
            file_path,
        })
    }

    /// Start monitoring liquidation events
    pub async fn start_monitoring(&self) -> Result<()> {
        info!(
            "🔍 Starting liquidation monitoring for pool: {}",
            self.pool_address
        );
        info!(
            "📊 Monitoring started at: {}",
            Local::now().format("%Y-%m-%d %H:%M:%S")
        );

        // Create filter for LiquidationCall events
        let event_signature = LiquidationCall::SIGNATURE_HASH;
        let filter = Filter::new()
            .address(self.pool_address)
            .event_signature(event_signature);

        // Try WebSocket first, fall back to polling on error
        match self.monitor_with_websocket(filter.clone()).await {
            Ok(_) => Ok(()),
            Err(_) => {
                info!("WebSocket not available, falling back to HTTP polling");
                self.monitor_with_polling(filter).await
            }
        }
    }

    /// Monitor using WebSocket subscription
    async fn monitor_with_websocket(&self, filter: Filter) -> Result<()> {
        info!("📡 Using WebSocket subscription for real-time monitoring");

        let subscription = self.provider.subscribe_logs(&filter).await?;
        let mut stream = subscription.into_stream();

        info!("✅ Successfully subscribed to LiquidationCall events");

        while let Some(log) = stream.next().await {
            if let Err(e) = self.process_liquidation_log(log).await {
                error!("Error processing liquidation event: {}", e);
            }
        }

        Ok(())
    }

    /// Monitor using HTTP polling
    async fn monitor_with_polling(&self, filter: Filter) -> Result<()> {
        info!("🔄 Using HTTP polling for event monitoring");

        let mut last_block = self.provider.get_block_number().await?;
        let mut poll_interval = interval(Duration::from_secs(12)); // Poll every block (~12s on Base)

        loop {
            poll_interval.tick().await;

            let current_block = match self.provider.get_block_number().await {
                Ok(block) => block,
                Err(e) => {
                    warn!("Failed to get current block number: {}", e);
                    continue;
                }
            };

            if current_block > last_block {
                // Fetch logs for the new blocks
                let from_block = last_block + 1;
                let filter = filter
                    .clone()
                    .from_block(BlockNumberOrTag::Number(from_block))
                    .to_block(BlockNumberOrTag::Number(current_block));

                match self.provider.get_logs(&filter).await {
                    Ok(logs) => {
                        for log in logs {
                            if let Err(e) = self.process_liquidation_log(log).await {
                                error!("Error processing liquidation event: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        error!(
                            "Failed to fetch logs for blocks {}-{}: {}",
                            from_block, current_block, e
                        );
                    }
                }

                last_block = current_block;
            }
        }
    }

    /// Process a liquidation log event
    async fn process_liquidation_log(&self, log: Log) -> Result<()> {
        // Convert alloy_rpc_types::Log to alloy_primitives::Log
        let primitive_log = alloy_primitives::Log {
            address: log.address(),
            data: alloy_primitives::LogData::new_unchecked(
                log.topics().to_vec(),
                log.data().data.clone(),
            ),
        };

        // Decode the LiquidationCall event
        let event = LiquidationCall::decode_log(&primitive_log, true)?;

        let block_number = log.block_number.unwrap_or(0);
        let tx_hash = log.transaction_hash.unwrap_or_default();

        // Create liquidation event record
        let liquidation_event = LiquidationEvent {
            timestamp: Utc::now(),
            block_number,
            transaction_hash: format!("{:?}", tx_hash),
            collateral_asset: event.collateralAsset,
            debt_asset: event.debtAsset,
            user: event.user,
            liquidator: event.liquidator,
            debt_to_cover: event.debtToCover,
            liquidated_collateral_amount: event.liquidatedCollateralAmount,
            receive_atoken: event.receiveAToken,
            gas_used: None, // Can be fetched from transaction receipt if needed
            effective_gas_price: None,
        };

        // Log the liquidation event
        self.log_liquidation(&liquidation_event).await;

        // Update statistics
        self.update_stats(&liquidation_event).await;

        // Store event
        self.store_event(liquidation_event).await;

        Ok(())
    }

    /// Log liquidation details
    async fn log_liquidation(&self, event: &LiquidationEvent) {
        let local_time = Local::now().format("%Y-%m-%d %H:%M:%S");

        info!("🔴 ===== LIQUIDATION DETECTED ===== 🔴");
        info!("⏰ Time: {}", local_time);
        info!("📦 Block: {}", event.block_number);
        info!("🔗 Tx: {}", event.transaction_hash);
        info!("👤 User Liquidated: {:?}", event.user);
        info!("🏦 Liquidator: {:?}", event.liquidator);
        info!("💰 Collateral Asset: {:?}", event.collateral_asset);
        info!("💸 Debt Asset: {:?}", event.debt_asset);
        info!("📉 Debt Covered: {} wei", event.debt_to_cover);
        info!(
            "📊 Collateral Liquidated: {} wei",
            event.liquidated_collateral_amount
        );
        info!("🎯 Receive aToken: {}", event.receive_atoken);

        // Calculate approximate liquidation bonus (if collateral > debt, bonus = difference)
        if event.liquidated_collateral_amount > event.debt_to_cover {
            let bonus = event.liquidated_collateral_amount - event.debt_to_cover;
            let bonus_percent = (bonus * U256::from(10000)) / event.debt_to_cover;
            info!(
                "💎 Liquidation Bonus: ~{}% (approximate)",
                bonus_percent / U256::from(100)
            );
        }

        info!("=====================================\n");

        // Optionally log to file
        if self.log_to_file {
            if let Some(ref path) = self.file_path {
                self.write_to_file(event, path).await;
            }
        }
    }

    /// Write liquidation event to file
    async fn write_to_file(&self, event: &LiquidationEvent, path: &str) {
        use tokio::fs::OpenOptions;
        use tokio::io::AsyncWriteExt;

        let json_line = match serde_json::to_string(event) {
            Ok(json) => json + "\n",
            Err(e) => {
                error!("Failed to serialize event: {}", e);
                return;
            }
        };

        let mut file = match OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await
        {
            Ok(file) => file,
            Err(e) => {
                error!("Failed to open log file: {}", e);
                return;
            }
        };

        if let Err(e) = file.write_all(json_line.as_bytes()).await {
            error!("Failed to write to log file: {}", e);
        }
    }

    /// Update statistics
    async fn update_stats(&self, event: &LiquidationEvent) {
        let mut stats = self.stats.write().await;

        // Update basic counters
        stats.total_liquidations += 1;
        stats.total_debt_covered = stats.total_debt_covered + event.debt_to_cover;
        stats.total_collateral_liquidated =
            stats.total_collateral_liquidated + event.liquidated_collateral_amount;
        stats.last_liquidation_time = Some(event.timestamp);

        // Update unique liquidators
        *stats
            .unique_liquidators
            .entry(event.liquidator)
            .or_insert(0) += 1;

        // Update unique users liquidated
        *stats.unique_users_liquidated.entry(event.user).or_insert(0) += 1;

        // Update asset pairs
        let pair = (event.collateral_asset, event.debt_asset);
        *stats.asset_pairs.entry(pair).or_insert(0) += 1;

        // Update hourly statistics
        let hour_key = event.timestamp.format("%Y-%m-%d %H:00").to_string();
        *stats.hourly_liquidations.entry(hour_key).or_insert(0) += 1;
    }

    /// Store event in memory
    async fn store_event(&self, event: LiquidationEvent) {
        let mut events = self.events.write().await;
        events.push(event);

        // Keep only the last N events
        if events.len() > self.max_events_stored {
            let drain_count = events.len() - self.max_events_stored;
            events.drain(0..drain_count);
        }
    }

    /// Get current statistics
    pub async fn get_stats(&self) -> LiquidationStats {
        self.stats.read().await.clone()
    }

    /// Get recent events
    pub async fn get_recent_events(&self, limit: usize) -> Vec<LiquidationEvent> {
        let events = self.events.read().await;
        let start = if events.len() > limit {
            events.len() - limit
        } else {
            0
        };
        events[start..].to_vec()
    }

    /// Print statistics summary
    pub async fn print_stats_summary(&self) {
        let stats = self.stats.read().await;
        let runtime = Utc::now().signed_duration_since(stats.monitoring_started);
        let hours = runtime.num_hours();
        let minutes = runtime.num_minutes() % 60;

        info!("\n📊 ===== LIQUIDATION STATISTICS ===== 📊");
        info!("⏱️  Monitoring Duration: {}h {}m", hours, minutes);
        info!("📈 Total Liquidations: {}", stats.total_liquidations);

        if stats.total_liquidations > 0 {
            info!("💰 Total Debt Covered: {} wei", stats.total_debt_covered);
            info!(
                "🏦 Total Collateral Liquidated: {} wei",
                stats.total_collateral_liquidated
            );
            info!("👥 Unique Liquidators: {}", stats.unique_liquidators.len());
            info!(
                "🎯 Unique Users Liquidated: {}",
                stats.unique_users_liquidated.len()
            );

            // Show top liquidators
            if !stats.unique_liquidators.is_empty() {
                info!("\n🏆 Top Liquidators:");
                let mut liquidators: Vec<_> = stats.unique_liquidators.iter().collect();
                liquidators.sort_by(|a, b| b.1.cmp(a.1));
                for (addr, count) in liquidators.iter().take(5) {
                    info!("   {:?}: {} liquidations", addr, count);
                }
            }

            // Show most common asset pairs
            if !stats.asset_pairs.is_empty() {
                info!("\n💱 Most Common Asset Pairs:");
                let mut pairs: Vec<_> = stats.asset_pairs.iter().collect();
                pairs.sort_by(|a, b| b.1.cmp(a.1));
                for ((collateral, debt), count) in pairs.iter().take(5) {
                    info!(
                        "   Collateral: {:?} -> Debt: {:?}: {} times",
                        collateral, debt, count
                    );
                }
            }

            // Calculate average liquidation frequency
            if hours > 0 {
                let avg_per_hour = stats.total_liquidations as f64 / hours as f64;
                info!(
                    "\n⚡ Average Frequency: {:.2} liquidations/hour",
                    avg_per_hour
                );
            }

            if let Some(last_time) = stats.last_liquidation_time {
                info!(
                    "🕐 Last Liquidation: {}",
                    last_time.format("%Y-%m-%d %H:%M:%S UTC")
                );
            }
        } else {
            info!("ℹ️  No liquidations detected yet");
        }

        info!("=====================================\n");
    }

    /// Start periodic stats reporting
    pub async fn start_stats_reporting(self: Arc<Self>, interval_minutes: u64) {
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(interval_minutes * 60));
            loop {
                interval.tick().await;
                self.print_stats_summary().await;
            }
        });
    }
}
