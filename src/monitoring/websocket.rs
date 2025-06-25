use crate::events::BotEvent;
use alloy_primitives::Address;
use alloy_provider::{Provider, ProviderBuilder, WsConnect};
use alloy_rpc_types::{Filter, Log};
use eyre::Result;
use futures::StreamExt;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

pub async fn try_connect_websocket(ws_url: &str) -> Result<Arc<dyn Provider>> {
    let ws_connect = WsConnect::new(ws_url.to_string());
    let ws_provider = ProviderBuilder::new().on_ws(ws_connect).await?;
    Ok(Arc::new(ws_provider.boxed()))
}

pub async fn start_event_monitoring<P>(
    _provider: Arc<P>,
    ws_provider: Arc<dyn Provider>,
    ws_url: &str,
    event_tx: mpsc::UnboundedSender<BotEvent>,
) -> Result<()>
where
    P: Provider + 'static,
{
    // Check if we're using WebSocket or HTTP fallback
    // More specific check: only disable websockets if it's clearly an HTTP URL
    let using_websocket = ws_url.starts_with("wss://") || ws_url.starts_with("ws://");

    if !using_websocket {
        info!("Event monitoring initialized (using HTTP polling mode)");
        warn!("WebSocket event subscriptions skipped - URL does not use WebSocket protocol");
        warn!("For real-time monitoring, configure WS_URL with a proper WebSocket RPC endpoint");
        return Ok(());
    }

    info!("🚀 Starting real-time WebSocket event monitoring...");

    let pool_address: Address = "0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2".parse()?;

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

pub async fn handle_log_event(log: Log, event_tx: &mpsc::UnboundedSender<BotEvent>) -> Result<()> {
    // For now, extract user addresses from log topics manually
    // In Aave events, user addresses are typically in topic[1] or topic[2]
    let topics = log.topics();

    debug!("Processing log event with {} topics", topics.len());

    // Most Aave events have user address in topic[1] (after the event signature)
    if topics.len() >= 2 {
        // Extract the user address from topic[1] (assuming it's an address)
        let user_bytes = topics[1].as_slice();
        if user_bytes.len() >= 20 {
            // Address is the last 20 bytes of the topic
            let addr_bytes = &user_bytes[12..32];
            if let Ok(user_addr) = Address::try_from(addr_bytes) {
                debug!("Detected event for user: {}", user_addr);
                let _ = event_tx.send(BotEvent::UserPositionChanged(user_addr));
            }
        }
    }

    // Also check topic[2] for events that might have user there
    if topics.len() >= 3 {
        let user_bytes = topics[2].as_slice();
        if user_bytes.len() >= 20 {
            let addr_bytes = &user_bytes[12..32];
            if let Ok(user_addr) = Address::try_from(addr_bytes) {
                debug!("Detected additional event for user: {}", user_addr);
                let _ = event_tx.send(BotEvent::UserPositionChanged(user_addr));
            }
        }
    }

    Ok(())
}
