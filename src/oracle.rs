use alloy_primitives::{Address, U256};
use alloy_provider::Provider;
use chrono::Utc;
use dashmap::DashMap;
use eyre::Result;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use crate::models::{AssetConfig, BotEvent, PriceFeed};

pub async fn start_oracle_monitoring<P>(
    provider: Arc<P>,
    asset_configs: HashMap<Address, AssetConfig>,
    price_feeds: Arc<DashMap<Address, PriceFeed>>,
    event_tx: mpsc::UnboundedSender<BotEvent>,
) -> Result<()>
where
    P: Provider + 'static,
{
    info!("🔮 Starting oracle price monitoring for {} assets...", asset_configs.len());

    for (asset_address, config) in asset_configs.iter() {
        info!("Monitoring price feed for {} ({})", config.symbol, asset_address);

        // Initialize price feed
        match fetch_price_from_oracle(&provider, config.chainlink_feed, &config.symbol).await {
            Ok(initial_price) => {
                let price_feed = PriceFeed {
                    asset_address: *asset_address,
                    feed_address: config.chainlink_feed,
                    asset_symbol: config.symbol.clone(),
                    last_price: initial_price,
                    last_updated: Utc::now(),
                    price_change_threshold: config.price_change_threshold,
                };
                
                price_feeds.insert(*asset_address, price_feed);
                info!("✅ Initialized {} price feed: {}", config.symbol, initial_price);
            }
            Err(e) => {
                error!("❌ Failed to initialize price feed for {}: {}", config.symbol, e);
            }
        }
    }

    // Start periodic price polling
    start_periodic_price_polling(provider, price_feeds, event_tx).await?;

    Ok(())
}

pub async fn start_periodic_price_polling<P>(
    provider: Arc<P>,
    price_feeds: Arc<DashMap<Address, PriceFeed>>,
    event_tx: mpsc::UnboundedSender<BotEvent>,
) -> Result<()>
where
    P: Provider + 'static,
{
    info!("🔄 Starting periodic price polling every 30 seconds...");

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));

        loop {
            interval.tick().await;

            // Poll all price feeds
            for mut entry in price_feeds.iter_mut() {
                let asset_address = *entry.key();
                let price_feed = entry.value_mut();

                match fetch_price_from_oracle(
                    &provider,
                    price_feed.feed_address,
                    &price_feed.asset_symbol,
                ).await {
                    Ok(new_price) => {
                        let old_price = price_feed.last_price;
                        
                        // Calculate percentage change
                        let price_change_pct = if old_price > U256::ZERO {
                            let diff = if new_price > old_price {
                                new_price - old_price
                            } else {
                                old_price - new_price
                            };
                            
                            // Convert to percentage (multiply by 100, divide by old_price)
                            let change_pct = (diff * U256::from(10000)) / old_price; // 10000 = 100 * 100 for 2 decimal places
                            change_pct.to::<u64>() as f64 / 100.0
                        } else {
                            100.0 // If old price was 0, consider it 100% change
                        };

                        debug!(
                            "Price update for {}: {} -> {} ({}% change)",
                            price_feed.asset_symbol, old_price, new_price, price_change_pct
                        );

                        // Check if price change exceeds threshold
                        if price_change_pct >= price_feed.price_change_threshold {
                            info!(
                                "🔥 Significant price change for {}: {:.2}% (threshold: {:.2}%)",
                                price_feed.asset_symbol, price_change_pct, price_feed.price_change_threshold * 100.0
                            );

                            // Send oracle price change event
                            let _ = event_tx.send(BotEvent::OraclePriceChanged(asset_address, new_price));
                        }

                        // Update price feed
                        price_feed.last_price = new_price;
                        price_feed.last_updated = Utc::now();
                    }
                    Err(e) => {
                        error!("Failed to fetch price for {}: {}", price_feed.asset_symbol, e);
                    }
                }
            }
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
    P: Provider + 'static,
{
    debug!("Fetching price from oracle {} for {}", feed_address, symbol);

    // Call latestRoundData() on the Chainlink price feed
    let function_selector = alloy_primitives::hex!("feaf968c"); // latestRoundData()
    let call_data = alloy_primitives::Bytes::from(function_selector.to_vec());

    let tx = alloy_rpc_types::TransactionRequest::default()
        .to(feed_address)
        .input(call_data.into());

    let result = provider.call(&tx, None).await?;

    // Parse the result - latestRoundData returns:
    // (uint80 roundId, int256 answer, uint256 startedAt, uint256 updatedAt, uint80 answeredInRound)
    if result.len() >= 160 { // 5 * 32 bytes
        // Extract answer (int256) from bytes 32-64
        let answer_bytes = &result[32..64];
        let price = U256::from_be_bytes(answer_bytes.try_into().unwrap_or([0u8; 32]));
        
        debug!("Fetched price for {}: {}", symbol, price);
        Ok(price)
    } else {
        Err(eyre::eyre!("Invalid response from price oracle for {}", symbol))
    }
}

pub async fn handle_oracle_price_change<P>(
    asset_address: Address,
    new_price: U256,
    users_by_collateral: &Arc<DashMap<Address, HashSet<Address>>>,
    pool_contract: &alloy_contract::ContractInstance<Arc<P>>,
    db_pool: &sqlx::Pool<sqlx::Sqlite>,
) -> Result<()>
where
    P: Provider + 'static,
{
    info!("🔄 Handling oracle price change for asset {}: {}", asset_address, new_price);

    // Get users who have this asset as collateral
    if let Some(users) = users_by_collateral.get(&asset_address) {
        info!("Recalculating positions for {} users affected by price change", users.len());

        for user in users.iter() {
            // Update user position due to price change
            if let Err(e) = crate::position::update_user_position(pool_contract, *user, db_pool).await {
                error!("Failed to update position for user {} after price change: {}", user, e);
            }
        }
    } else {
        debug!("No users found with asset {} as collateral", asset_address);
    }

    Ok(())
}