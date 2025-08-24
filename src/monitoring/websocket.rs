use crate::events::BotEvent;
use crate::monitoring::scanner;
use alloy_primitives::{Address, U256};
use alloy_provider::{Provider, ProviderBuilder, WsConnect};
use alloy_rpc_types::{BlockNumberOrTag, Filter, Log};
use alloy_sol_types::SolEvent;
use eyre::Result;
use futures::StreamExt;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, warn};

use crate::models::{Borrow, Repay, Supply, Withdraw};

// Static variable to track last processed block for polling mode
static LAST_PROCESSED_BLOCK: AtomicU64 = AtomicU64::new(0);

// Dedupe mechanism for fast path liquidations
type DedupeMap = Arc<tokio::sync::RwLock<HashMap<Address, u64>>>;
static FAST_PATH_DEDUPE: tokio::sync::OnceCell<DedupeMap> = tokio::sync::OnceCell::const_new();

async fn get_dedupe_map() -> &'static DedupeMap {
    FAST_PATH_DEDUPE.get_or_init(|| async {
        Arc::new(tokio::sync::RwLock::new(HashMap::new()))
    }).await
}

// Dedupe window in seconds
const DEDUPE_WINDOW_SECS: u64 = 2;

pub async fn try_connect_websocket(ws_url: &str) -> Result<Arc<dyn Provider>> {
    let ws_connect = WsConnect::new(ws_url.to_string());
    let ws_provider = ProviderBuilder::new().on_ws(ws_connect).await?;
    Ok(Arc::new(ws_provider.boxed()))
}

pub async fn start_event_monitoring<P>(
    provider: Arc<P>,
    ws_provider: Arc<dyn Provider>,
    ws_url: &str,
    event_tx: mpsc::UnboundedSender<BotEvent>,
    priority_liquidation_tx: Option<mpsc::UnboundedSender<Address>>,
) -> Result<()>
where
    P: Provider + 'static,
{
    // Check if we're using WebSocket or HTTP fallback
    let using_websocket = ws_url.starts_with("wss://") || ws_url.starts_with("ws://");

    if !using_websocket {
        info!("Event monitoring initialized (using HTTP polling mode)");
        warn!("WebSocket event subscriptions skipped - URL does not use WebSocket protocol");
        warn!("For real-time monitoring, configure WS_URL with a proper WebSocket RPC endpoint");

        // Instead of exiting early, start polling-based event monitoring
        info!("🔄 Starting getLogs-based polling for continuous event discovery...");
        return start_polling_event_monitoring(provider, event_tx, priority_liquidation_tx).await;
    }

    info!("🚀 Starting real-time WebSocket event monitoring...");

    let pool_address: Address = "0xA238Dd80C259a72e81d7e4664a9801593F98d1c5".parse()?;

    // Create a general filter for all events from the Aave pool
    let pool_filter = Filter::new().address(pool_address);

    tokio::spawn(async move {
        info!("Subscribing to Aave Pool events...");
        let sub = match ws_provider.subscribe_logs(&pool_filter).await {
            Ok(sub) => {
                info!("✅ Successfully subscribed to Aave Pool events!");
                sub
            }
            Err(e) => {
                error!("❌ Failed to subscribe to logs: {}", e);
                return;
            }
        };

        let mut stream = sub.into_stream();
        info!("🎧 Listening for real-time Aave events...");

        while let Some(log) = stream.next().await {
            if let Err(e) = handle_log_event(log, &event_tx, &priority_liquidation_tx, &provider, pool_address).await {
                error!("Error handling log event: {}", e);
            }
        }
    });

    info!("✅ WebSocket event subscriptions established");
    Ok(())
}

/// Polling-based event monitoring for HTTP fallback mode
async fn start_polling_event_monitoring<P>(
    provider: Arc<P>,
    event_tx: mpsc::UnboundedSender<BotEvent>,
    priority_liquidation_tx: Option<mpsc::UnboundedSender<Address>>,
) -> Result<()>
where
    P: Provider + 'static,
{
    let pool_address: Address = "0xA238Dd80C259a72e81d7e4664a9801593F98d1c5".parse()?;

    // Initialize last processed block to current block
    let current_block = provider.get_block_number().await?;
    LAST_PROCESSED_BLOCK.store(current_block, Ordering::Relaxed);

    info!("Starting polling from block: {}", current_block);

    // Event signatures for key Aave events
    let key_events = vec![
        ("Borrow", Borrow::SIGNATURE_HASH),
        ("Supply", Supply::SIGNATURE_HASH),
        ("Repay", Repay::SIGNATURE_HASH),
        ("Withdraw", Withdraw::SIGNATURE_HASH),
    ];

    // Create interval for polling (every 10 seconds to balance real-time vs rate limits)
    let mut poll_interval = interval(Duration::from_secs(10));

    tokio::spawn(async move {
        info!("🔄 Polling loop started for event discovery");

        loop {
            poll_interval.tick().await;

            if let Err(e) = poll_for_events(&provider, pool_address, &key_events, &event_tx, &priority_liquidation_tx).await {
                error!("Error during event polling: {}", e);
                // Continue polling even if one round fails
            }
        }
    });

    info!("✅ Polling-based event monitoring established");
    Ok(())
}

/// Poll for new events since last processed block
async fn poll_for_events<P>(
    provider: &Arc<P>,
    pool_address: Address,
    key_events: &[(&str, alloy_primitives::FixedBytes<32>)],
    event_tx: &mpsc::UnboundedSender<BotEvent>,
    priority_liquidation_tx: &Option<mpsc::UnboundedSender<Address>>,
) -> Result<()>
where
    P: Provider,
{
    let current_block = provider.get_block_number().await?;
    let last_processed = LAST_PROCESSED_BLOCK.load(Ordering::Relaxed);

    // Skip if no new blocks
    if current_block <= last_processed {
        return Ok(());
    }

    let from_block = last_processed + 1;
    let blocks_to_process = current_block - last_processed;

    debug!(
        "Polling blocks {} to {} ({} new blocks)",
        from_block, current_block, blocks_to_process
    );

    let mut total_events_found = 0;

    // Poll for each event type
    for (event_name, signature_hash) in key_events {
        let filter = Filter::new()
            .address(pool_address)
            .event_signature(*signature_hash)
            .from_block(BlockNumberOrTag::Number(from_block))
            .to_block(BlockNumberOrTag::Number(current_block));

        match provider.get_logs(&filter).await {
            Ok(logs) => {
                if !logs.is_empty() {
                    info!(
                        "📊 Found {} {} events in blocks {}-{}",
                        logs.len(),
                        event_name,
                        from_block,
                        current_block
                    );
                    total_events_found += logs.len();
                }

                for log in logs {
                    if let Err(e) = handle_log_event(log, event_tx, priority_liquidation_tx, provider, pool_address).await {
                        error!("Error handling {} event: {}", event_name, e);
                    }
                }
            }
            Err(e) => {
                error!(
                    "Failed to get {} events for blocks {}-{}: {}",
                    event_name, from_block, current_block, e
                );
                // Continue with other event types
            }
        }

        // Brief delay between event type queries to avoid rate limiting
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    if total_events_found > 0 {
        info!(
            "✅ Processed {} total events from {} new blocks",
            total_events_found, blocks_to_process
        );
    }

    // Update last processed block
    LAST_PROCESSED_BLOCK.store(current_block, Ordering::Relaxed);

    Ok(())
}

pub async fn handle_log_event<P>(
    log: Log, 
    event_tx: &mpsc::UnboundedSender<BotEvent>,
    priority_liquidation_tx: &Option<mpsc::UnboundedSender<Address>>,
    provider: &Arc<P>,
    pool_address: Address,
) -> Result<()> 
where
    P: Provider,
{
    // For now, extract user addresses from log topics manually
    // In Aave events, user addresses are typically in topic[1] or topic[2]
    let topics = log.topics();

    debug!("Processing log event with {} topics", topics.len());

    let mut user_addresses = std::collections::HashSet::new();

    // Most Aave events have user address in topic[1] (after the event signature)
    if topics.len() >= 2 {
        // Extract the user address from topic[1] (assuming it's an address)
        let user_bytes = topics[1].as_slice();
        if user_bytes.len() >= 20 {
            // Address is the last 20 bytes of the topic
            let addr_bytes = &user_bytes[12..32];
            if let Ok(user_addr) = Address::try_from(addr_bytes) {
                user_addresses.insert(user_addr);
            }
        }
    }

    // Also check topic[2] for events that might have user there
    if topics.len() >= 3 {
        let user_bytes = topics[2].as_slice();
        if user_bytes.len() >= 20 {
            let addr_bytes = &user_bytes[12..32];
            if let Ok(user_addr) = Address::try_from(addr_bytes) {
                user_addresses.insert(user_addr);
            }
        }
    }

    // Process each unique user address
    for user_addr in user_addresses {
        debug!("Detected event for user: {}", user_addr);
        
        // WebSocket Fast Path: If priority liquidation channel is available, 
        // immediately check user health and route liquidatable users to priority channel
        if let Some(priority_tx) = priority_liquidation_tx {
            // Check dedupe to avoid spamming priority channel
            let current_time = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            
            let dedupe_map = get_dedupe_map().await;
            let should_check = {
                let mut map = dedupe_map.write().await;
                
                // Clean up old entries (older than dedupe window)
                map.retain(|_, &mut last_time| current_time - last_time < DEDUPE_WINDOW_SECS);
                
                // Check if we should process this user
                if let Some(&last_time) = map.get(&user_addr) {
                    if current_time - last_time < DEDUPE_WINDOW_SECS {
                        false // Skip, too recent
                    } else {
                        map.insert(user_addr, current_time);
                        true
                    }
                } else {
                    map.insert(user_addr, current_time);
                    true
                }
            };
            
            if should_check {
                debug!("⚡ Fast path: checking health for user: {}", user_addr);
                
                // Immediately check user health with retries
                match scanner::check_user_health(provider, pool_address, user_addr, 3).await {
                    Ok(position) => {
                        // If user is liquidatable (HF < 1.0 and has debt), send to priority channel
                        if position.health_factor < U256::from(1000000000000000000u64) // 1.0 * 1e18
                            && position.total_debt_base > U256::ZERO 
                        {
                            info!("⚡ Fast path liquidation detected for user: {:?} (HF: {})", 
                                  user_addr, position.health_factor);
                            
                            if let Err(e) = priority_tx.send(user_addr) {
                                warn!("Failed to send fast path liquidation for user {:?}: {}", user_addr, e);
                            }
                        } else {
                            debug!("Fast path: user {:?} not liquidatable (HF: {}, debt: {})", 
                                   user_addr, position.health_factor, position.total_debt_base);
                        }
                    }
                    Err(e) => {
                        debug!("Fast path health check failed for user {:?}: {}", user_addr, e);
                        // Continue to regular processing
                    }
                }
            } else {
                debug!("Fast path: skipping user {:?} due to dedupe window", user_addr);
            }
        }
        
        // Always enqueue UserPositionChanged for bookkeeping (normal processing)
        let _ = event_tx.send(BotEvent::UserPositionChanged(user_addr));
    }

    Ok(())
}
