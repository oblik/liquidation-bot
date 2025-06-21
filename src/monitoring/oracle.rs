use crate::events::BotEvent;
use crate::models::{AssetConfig, PriceFeed};
use alloy_primitives::{Address, U256};
use alloy_provider::Provider;
use alloy_rpc_types::Filter;
use chrono::Utc;
use dashmap::DashMap;
use eyre::Result;
use futures::StreamExt;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

pub fn init_asset_configs() -> HashMap<Address, AssetConfig> {
    let mut configs = HashMap::new();

    // Base Sepolia testnet asset configurations
    // Only including verified working oracle feeds

    // WETH - CONFIRMED WORKING ✅
    let weth_address = match "0x4200000000000000000000000000000000000006".parse() {
        Ok(addr) => addr,
        Err(e) => {
            error!("Failed to parse WETH address: {}", e);
            return configs;
        }
    };

    let weth_asset_address = match "0x4200000000000000000000000000000000000006".parse() {
        Ok(addr) => addr,
        Err(e) => {
            error!("Failed to parse WETH asset address: {}", e);
            return configs;
        }
    };

    let chainlink_feed_address = match "0x4aDC67696bA383F43DD60A9e78F2C97Fbbfc7cb1".parse() {
        Ok(addr) => addr,
        Err(e) => {
            error!("Failed to parse Chainlink feed address: {}", e);
            return configs;
        }
    };

    configs.insert(
        weth_address,
        AssetConfig {
            address: weth_asset_address,
            symbol: "WETH".to_string(),
            chainlink_feed: chainlink_feed_address, // ETH/USD on Base Sepolia ✅
            price_change_threshold: 0.005,          // 0.5% price change threshold (reduced from 2%)
        },
    );

    // Note: USDC/USDT oracle feeds are not available or working on Base Sepolia testnet
    // In production on Base mainnet, you would add:
    // - USDC: Different oracle address
    // - USDT: Different oracle address
    // - DAI: Different oracle address
    // For now, focusing on working WETH oracle for demonstration

    info!(
        "Initialized {} asset configuration(s) for oracle monitoring",
        configs.len()
    );
    info!("🎯 Active oracle feeds:");
    for config in configs.values() {
        info!(
            "   {} ({}): {}",
            config.symbol, config.address, config.chainlink_feed
        );
    }

    configs
}

pub async fn start_oracle_monitoring<P>(
    provider: Arc<P>,
    ws_provider: Arc<dyn Provider>,
    ws_url: &str,
    event_tx: mpsc::UnboundedSender<BotEvent>,
    asset_configs: HashMap<Address, AssetConfig>,
    price_feeds: Arc<DashMap<Address, PriceFeed>>,
) -> Result<()>
where
    P: Provider + 'static,
{
    info!("🔮 Starting Chainlink oracle price monitoring...");

    // Check if we're using WebSocket
    let using_websocket = ws_url.starts_with("wss://") && !ws_url.contains("sepolia.base.org");

    info!("🔧 Oracle monitoring mode decision:");
    info!("   WebSocket URL: {}", ws_url);
    info!("   Starts with wss://: {}", ws_url.starts_with("wss://"));
    info!(
        "   Contains sepolia.base.org: {}",
        ws_url.contains("sepolia.base.org")
    );
    info!("   Using WebSocket mode: {}", using_websocket);

    if !using_websocket {
        info!("🔄 Oracle monitoring will use periodic polling instead of real-time events");
        return start_periodic_price_polling(provider, event_tx, asset_configs, price_feeds).await;
    }

    info!("📡 Using real-time WebSocket oracle monitoring");
    info!("⚠️ Note: Oracle events may be infrequent on testnet");
    info!("💡 To see active monitoring, you can force polling mode by setting a non-wss:// WS_URL");

    // Also start periodic polling as backup to show activity
    info!("🔄 Starting backup polling to show oracle activity...");
    let _ = start_periodic_price_polling(
        provider.clone(),
        event_tx.clone(),
        asset_configs.clone(),
        price_feeds.clone(),
    )
    .await;

    // Subscribe to AnswerUpdated events from each price feed
    for (asset_address, asset_config) in &asset_configs {
        let price_feed = PriceFeed {
            asset_address: *asset_address,
            feed_address: asset_config.chainlink_feed,
            asset_symbol: asset_config.symbol.clone(),
            last_price: U256::ZERO,
            last_updated: Utc::now(),
            price_change_threshold: asset_config.price_change_threshold,
        };

        price_feeds.insert(*asset_address, price_feed);

        // Subscribe to AnswerUpdated events for this price feed
        let feed_filter = Filter::new().address(asset_config.chainlink_feed);

        let event_tx = event_tx.clone();
        let ws_provider = ws_provider.clone();
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
                if let Err(e) = handle_price_update_event(log, &event_tx, asset_addr, &symbol).await
                {
                    error!("Error handling price update for {}: {}", symbol, e);
                }
            }
        });
    }

    info!("✅ Oracle price monitoring subscriptions established");
    Ok(())
}

pub async fn start_periodic_price_polling<P>(
    provider: Arc<P>,
    event_tx: mpsc::UnboundedSender<BotEvent>,
    asset_configs: HashMap<Address, AssetConfig>,
    price_feeds: Arc<DashMap<Address, PriceFeed>>,
) -> Result<()>
where
    P: Provider + 'static,
{
    info!("🔄 Starting periodic price polling (every 30 seconds)...");
    info!(
        "🎯 Monitoring {} assets for price changes",
        asset_configs.len()
    );

    for (_, config) in &asset_configs {
        info!(
            "📡 Will monitor {}: {} (threshold: {}%)",
            config.symbol,
            config.chainlink_feed,
            config.price_change_threshold * 100.0
        );
    }

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

                match fetch_price_from_oracle(
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
                            let price_change = if old_price > U256::ZERO && new_price > U256::ZERO {
                                let diff = if new_price > old_price {
                                    new_price - old_price
                                } else {
                                    old_price - new_price
                                };

                                // Prevent overflow by checking if multiplication would overflow
                                // Use checked arithmetic for safe calculation
                                match diff.checked_mul(U256::from(10000)) {
                                    Some(multiplied) => {
                                        if old_price > U256::ZERO {
                                            multiplied / old_price // Basis points
                                        } else {
                                            U256::ZERO
                                        }
                                    }
                                    None => {
                                        // Overflow detected, calculate manually with reduced precision
                                        warn!("Price change calculation overflow for {}, using fallback calculation", asset_config.symbol);
                                        // Calculate percentage without the 10000 multiplier first
                                        let percentage = diff / old_price;
                                        // Then multiply by 10000, but cap at reasonable threshold
                                        percentage.min(U256::from(10000)) * U256::from(10000)
                                    }
                                }
                            } else if old_price == U256::ZERO && new_price > U256::ZERO {
                                // First price update - use a reasonable threshold instead of MAX
                                // Set to a high but not MAX value that will trigger price change events
                                U256::from(10000) // 100% change in basis points
                            } else {
                                U256::ZERO // No change if both prices are zero or new price is zero
                            };

                            let threshold_bp =
                                U256::from((asset_config.price_change_threshold * 10000.0) as u64);

                            if price_change > threshold_bp
                                || (old_price == U256::ZERO && new_price > U256::ZERO)
                            {
                                feed.last_price = new_price;
                                feed.last_updated = Utc::now();

                                let change_pct = if old_price > U256::ZERO {
                                    // Safe conversion to f64 for display
                                    let change_u64 = if price_change <= U256::from(u64::MAX) {
                                        price_change.try_into().unwrap_or(0u64)
                                    } else {
                                        1_000_000u64 // Cap at 1M basis points for display
                                    };
                                    change_u64.min(1_000_000) as f64 / 100.0
                                } else {
                                    0.0
                                };

                                info!(
                                    "🚨 SIGNIFICANT PRICE CHANGE for {}: {} → {} ({}%)",
                                    asset_config.symbol, old_price, new_price, change_pct
                                );

                                let _ = event_tx
                                    .send(BotEvent::OraclePriceChanged(*asset_address, new_price));
                            } else {
                                // Even if the price change isn't "significant", update the stored price
                                // and trigger a lighter check for any existing at-risk users
                                feed.last_price = new_price;
                                feed.last_updated = Utc::now();

                                let change_bp = if price_change <= U256::from(u64::MAX) {
                                    price_change.try_into().unwrap_or(0u64)
                                } else {
                                    u64::MAX
                                };
                                let threshold_bp_val = if threshold_bp <= U256::from(u64::MAX) {
                                    threshold_bp.try_into().unwrap_or(0u64)
                                } else {
                                    u64::MAX
                                };

                                info!(
                                    "📊 {} price stable: {} (change: {}bp, need: {}bp)",
                                    asset_config.symbol, new_price, change_bp, threshold_bp_val
                                );

                                // Trigger a lighter oracle price change event even for smaller movements
                                // This ensures that at-risk users are still recalculated periodically
                                if old_price > U256::ZERO && price_change > U256::from(10) {
                                    // At least 10bp = 0.1% change
                                    info!("🔄 Triggering light health check due to minor price movement");
                                    let _ = event_tx.send(BotEvent::OraclePriceChanged(
                                        *asset_address,
                                        new_price,
                                    ));
                                }
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

pub async fn fetch_price_from_oracle<P>(
    provider: &Arc<P>,
    feed_address: Address,
    symbol: &str,
) -> Result<U256>
where
    P: Provider,
{
    // Create a simple call to the price feed's latestAnswer() function
    let call_data = match alloy_primitives::hex::decode("50d25bcd") {
        Ok(data) => data,
        Err(e) => {
            return Err(eyre::eyre!("Failed to decode latestAnswer selector: {}", e));
        }
    };

    let call_request = alloy_rpc_types::TransactionRequest {
        to: Some(feed_address.into()),
        input: alloy_rpc_types::TransactionInput::new(call_data.into()),
        ..Default::default()
    };

    match provider.call(&call_request).await {
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

pub async fn handle_price_update_event(
    _log: alloy_rpc_types::Log,
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
