use alloy_primitives::{Address, U256};
use alloy_provider::Provider;
use alloy_rpc_types::{Filter, BlockNumberOrTag};
use alloy_sol_types::SolEvent;
use eyre::Result;
use sqlx::{Pool, Sqlite};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info, warn, debug};

use crate::events::BotEvent;
use crate::models::{Borrow, Supply, Repay, Withdraw, UserPosition};
use crate::monitoring::scanner;

const BLOCKS_TO_SCAN: u64 = 50000; // Scan last ~50k blocks (~7 days on Base)
const MAX_USERS_TO_PROCESS: usize = 1000; // Limit initial discovery to prevent overwhelming

/// Initial user discovery by scanning historical Aave events
pub async fn discover_initial_users<P>(
    provider: Arc<P>,
    pool_address: Address,
    db_pool: &Pool<Sqlite>,
    event_tx: mpsc::UnboundedSender<BotEvent>,
) -> Result<HashSet<Address>>
where
    P: Provider + 'static,
{
    info!("🔍 Starting initial user discovery by scanning historical events...");

    // Get current block number
    let current_block = provider.get_block_number().await?;
    let from_block = current_block.saturating_sub(BLOCKS_TO_SCAN);

    info!(
        "Scanning blocks {} to {} for Aave user activity",
        from_block, current_block
    );

    let mut discovered_users = HashSet::new();

    // Scan for different event types that indicate user activity
    let event_types = vec![
        ("Borrow", Borrow::SIGNATURE_HASH),
        ("Supply", Supply::SIGNATURE_HASH),
        ("Repay", Repay::SIGNATURE_HASH),
        ("Withdraw", Withdraw::SIGNATURE_HASH),
    ];

    // Scan in chunks to avoid RPC provider limits (Alchemy limits to 500 blocks)
    let chunk_size = 400u64;
    let total_blocks = current_block - from_block;
    let num_chunks = (total_blocks + chunk_size - 1) / chunk_size; // Round up

    for (event_name, signature_hash) in event_types {
        info!("🔎 Scanning for {} events in {} chunks...", event_name, num_chunks);

        for chunk_idx in 0..num_chunks {
            let chunk_from = from_block + (chunk_idx * chunk_size);
            let chunk_to = std::cmp::min(chunk_from + chunk_size - 1, current_block);

            let filter = Filter::new()
                .address(pool_address)
                .event_signature(signature_hash)
                .from_block(BlockNumberOrTag::Number(chunk_from))
                .to_block(BlockNumberOrTag::Number(chunk_to));

            match provider.get_logs(&filter).await {
                Ok(logs) => {
                    if !logs.is_empty() {
                        info!("Found {} {} events in chunk {}/{} (blocks {}-{})", 
                              logs.len(), event_name, chunk_idx + 1, num_chunks, chunk_from, chunk_to);
                    }
                    
                    for log in logs {
                        // Extract user address from log topics
                        if let Some(user_address) = extract_user_address_from_log(&log, event_name) {
                            discovered_users.insert(user_address);
                            
                            // Stop if we've discovered enough users
                            if discovered_users.len() >= MAX_USERS_TO_PROCESS {
                                warn!(
                                    "Reached maximum user discovery limit ({}), stopping scan",
                                    MAX_USERS_TO_PROCESS
                                );
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to get {} events for chunk {}/{}: {}", event_name, chunk_idx + 1, num_chunks, e);
                    // Continue with next chunk rather than failing completely
                    continue;
                }
            }

            // Brief delay between chunks to avoid rate limiting
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

            if discovered_users.len() >= MAX_USERS_TO_PROCESS {
                break;
            }
        }

        if discovered_users.len() >= MAX_USERS_TO_PROCESS {
            break;
        }
    }

    info!(
        "✅ Initial discovery completed. Found {} unique users",
        discovered_users.len()
    );

    // Now check health for each discovered user and populate the database
    info!("🏥 Checking health for discovered users...");
    
    let mut processed_count = 0;
    let mut at_risk_count = 0;

    for &user_address in &discovered_users {
        match scanner::check_user_health(&provider, pool_address, user_address, 3).await {
            Ok(position) => {
                processed_count += 1;

                // Save to database
                if let Err(e) = crate::database::save_user_position(db_pool, &position).await {
                    error!("Failed to save user position for {:?}: {}", user_address, e);
                    continue;
                }

                // Count at-risk users
                if position.is_at_risk {
                    at_risk_count += 1;
                    info!(
                        "⚠️  Discovered at-risk user: {:?} (HF: {})",
                        user_address,
                        format_health_factor(position.health_factor)
                    );

                    // Send event for immediate processing
                    let _ = event_tx.send(BotEvent::UserPositionChanged(user_address));
                }

                // Brief delay to avoid overwhelming the RPC
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

                if processed_count % 50 == 0 {
                    info!("Processed {}/{} users...", processed_count, discovered_users.len());
                }
            }
            Err(e) => {
                debug!("Failed to check health for user {:?}: {}", user_address, e);
                // Continue with next user rather than failing completely
            }
        }
    }

    info!(
        "✅ User discovery completed: {} users processed, {} at risk",
        processed_count, at_risk_count
    );

    // Log the discovery event
    if let Err(e) = crate::database::log_monitoring_event(
        db_pool,
        "initial_user_discovery",
        None,
        Some(&format!(
            "Discovered {} users, {} at risk from last {} blocks",
            processed_count, at_risk_count, BLOCKS_TO_SCAN
        )),
    )
    .await
    {
        error!("Failed to log discovery event: {}", e);
    }

    Ok(discovered_users)
}

/// Extract user address from event log based on event type
fn extract_user_address_from_log(log: &alloy_rpc_types::Log, event_name: &str) -> Option<Address> {
    let topics = log.topics();
    
    // Different events have user address in different topic positions
    match event_name {
        "Borrow" | "Supply" => {
            // Borrow/Supply events have user in topic[1] (after event signature)
            if topics.len() >= 2 {
                extract_address_from_topic(topics[1])
            } else {
                None
            }
        }
        "Repay" | "Withdraw" => {
            // Repay/Withdraw events have user in topic[2]
            if topics.len() >= 3 {
                extract_address_from_topic(topics[2])
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Extract address from a topic (last 20 bytes)
fn extract_address_from_topic(topic: alloy_primitives::FixedBytes<32>) -> Option<Address> {
    let topic_bytes = topic.as_slice();
    if topic_bytes.len() >= 20 {
        // Address is the last 20 bytes of the topic
        let addr_bytes = &topic_bytes[12..32];
        Address::try_from(addr_bytes).ok()
    } else {
        None
    }
}

/// Format health factor for display (same as in scanner.rs)
fn format_health_factor(hf: U256) -> String {
    if hf == U256::MAX {
        "∞".to_string()
    } else {
        let hf_f64 = hf.to_string().parse::<f64>().unwrap_or(0.0) / 1e18;
        format!("{:.3}", hf_f64)
    }
}

/// Alternative discovery method using subgraph (if available)
pub async fn discover_users_via_subgraph(
    subgraph_url: &str,
    _max_users: usize,
) -> Result<Vec<UserSubgraphData>> {
    info!("🌐 Attempting to discover users via subgraph...");
    
    // Note: This is a placeholder for subgraph integration
    // The Graph's decentralized network requires an API key for most queries
    // For now, we'll return empty and rely on event scanning
    
    warn!("Subgraph discovery not yet implemented - falling back to event scanning");
    Ok(Vec::new())
}

/// User data from subgraph query
#[derive(Debug, Clone)]
pub struct UserSubgraphData {
    pub address: Address,
    pub total_collateral_eth: String,
    pub total_debt_eth: String,
    pub health_factor: Option<String>,
}