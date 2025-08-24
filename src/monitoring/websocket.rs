use crate::events::BotEvent;
use alloy_primitives::Address;
use alloy_provider::{Provider, ProviderBuilder, WsConnect};
use alloy_rpc_types::{BlockNumberOrTag, Filter, Log};
use alloy_sol_types::SolEvent;
use eyre::Result;
use futures::StreamExt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, warn};

use crate::models::{Borrow, Repay, Supply, Withdraw};

// Static variable to track last processed block for polling mode
static LAST_PROCESSED_BLOCK: AtomicU64 = AtomicU64::new(0);

pub async fn try_connect_websocket(ws_url: &str) -> Result<Arc<dyn Provider>> {
    let mut ws_connect = WsConnect::new(ws_url.to_string());
    
    // Add authentication headers for DRPC
    if ws_url.contains("drpc.org") {
        // Extract the API key from the URL path
        if let Some(api_key_start) = ws_url.rfind('/') {
            let api_key = &ws_url[api_key_start + 1..];
            if !api_key.is_empty() {
                // Add Authorization header with the API key
                ws_connect = ws_connect.with_header("Authorization", format!("Bearer {}", api_key));
                info!("Added DRPC authentication header for WebSocket connection");
            }
        }
    }
    
    let ws_provider = ProviderBuilder::new().on_ws(ws_connect).await?;
    Ok(Arc::new(ws_provider.boxed()))
}

pub async fn start_event_monitoring<P>(
    provider: Arc<P>,
    ws_provider: Arc<dyn Provider>,
    ws_url: &str,
    event_tx: mpsc::UnboundedSender<BotEvent>,
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
        return start_polling_event_monitoring(provider, event_tx).await;
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
            if let Err(e) = handle_log_event(log, &event_tx).await {
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

            if let Err(e) = poll_for_events(&provider, pool_address, &key_events, &event_tx).await {
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
                    if let Err(e) = handle_log_event(log, event_tx).await {
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

pub async fn handle_log_event(log: Log, event_tx: &mpsc::UnboundedSender<BotEvent>) -> Result<()> {
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

    // Send events only for unique addresses to avoid duplicate update_user_position calls
    for user_addr in user_addresses {
        debug!("Detected event for user: {}", user_addr);
        let _ = event_tx.send(BotEvent::UserPositionChanged(user_addr));
    }

    Ok(())
}
