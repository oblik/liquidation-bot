use crate::database::DatabasePool;
use alloy_contract::ContractInstance;
use alloy_primitives::{Address, U256};
use alloy_provider::Provider;
use dashmap::DashMap;
use eyre::Result;
use parking_lot::RwLock as SyncRwLock;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::mpsc;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

use crate::config::BotConfig;
use crate::database;
use crate::events::BotEvent;
use crate::models::{AssetConfig, UserPosition};

// Threshold constants for health factor calculations (in 18 decimals)
const LIQUIDATION_THRESHOLD: u64 = 1000000000000000000; // 1.0 * 1e18 - liquidation can occur
const CRITICAL_THRESHOLD: u64 = 1100000000000000000; // 1.1 * 1e18 - critically at risk
const CHANGE_THRESHOLD: u64 = 10000000000000000; // 0.01 * 1e18 - 1% change for logging

/// Guard to ensure user is removed from processing set when dropped
struct ProcessingGuard {
    user: Address,
    processing_users: Arc<SyncRwLock<HashSet<Address>>>,
}

impl ProcessingGuard {
    fn new(user: Address, processing_users: Arc<SyncRwLock<HashSet<Address>>>) -> Option<Self> {
        {
            let mut processing = processing_users.write();
            if processing.contains(&user) {
                debug!("User {:?} already being processed, skipping", user);
                return None;
            }
            processing.insert(user);
        } // Explicit scope ensures `processing` is dropped here

        Some(ProcessingGuard {
            user,
            processing_users,
        })
    }
}

impl Drop for ProcessingGuard {
    fn drop(&mut self) {
        // Now we can use synchronous operations, avoiding the memory leak
        let mut processing = self.processing_users.write();
        processing.remove(&self.user);
        debug!("Cleaned up processing state for user {:?}", self.user);
    }
}

/// Helper function to format health factor in human-readable format
fn format_health_factor(hf: U256) -> String {
    // Health factors are in 18 decimals (wei-like format)
    // Convert to human readable by dividing by 10^18
    let hf_str = hf.to_string();
    let hf_f64: f64 = match hf_str.parse::<f64>() {
        Ok(val) => val / 1e18,
        Err(_) => {
            // Fallback for very large numbers that can't be parsed as f64
            // Just show the raw value
            return format!("{} (parse_error)", hf);
        }
    };
    format!("{} ({:.3})", hf, hf_f64)
}

/// Helper function to safely parse U256 from contract call result
fn parse_u256_from_result(
    result: &[alloy_dyn_abi::DynSolValue],
    index: usize,
    field_name: &str,
) -> Result<U256> {
    let value = result.get(index).ok_or_else(|| {
        eyre::eyre!(
            "Missing result at index {} for field '{}'",
            index,
            field_name
        )
    })?;

    let uint_value = value.as_uint().ok_or_else(|| {
        eyre::eyre!(
            "Failed to parse field '{}' as uint at index {}",
            field_name,
            index
        )
    })?;

    Ok(uint_value.0)
}

pub async fn check_user_health<P>(
    provider: &Arc<P>,
    pool_address: Address,
    user_address: Address,
    max_retries: u32,
) -> Result<UserPosition>
where
    P: Provider,
{
    let mut attempt = 0;
    let mut base_delay = Duration::from_millis(100);

    loop {
        attempt += 1;

        match try_check_user_health(provider, pool_address, user_address).await {
            Ok(data) => return Ok(data),
            Err(e) => {
                let error_msg = e.to_string();

                // Check for rate limiting or network errors
                if error_msg.contains("429")
                    || error_msg.contains("rate limit")
                    || error_msg.contains("too many requests")
                    || error_msg.contains("connection")
                    || error_msg.contains("timeout")
                {
                    if attempt >= max_retries {
                        error!(
                            "Max retries ({}) exceeded for user health check: {}",
                            max_retries, e
                        );
                        return Err(e);
                    }

                    // Exponential backoff with jitter
                    let jitter_ms = (SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_millis()
                        % 100) as u64;
                    let jitter = Duration::from_millis(jitter_ms);
                    let delay = base_delay + jitter;

                    warn!(
                        "Rate limited or network error (attempt {}/{}), retrying in {:?}: {}",
                        attempt, max_retries, delay, error_msg
                    );

                    sleep(delay).await;
                    base_delay = std::cmp::min(base_delay * 2, Duration::from_secs(30)); // Cap at 30s
                    continue;
                }

                // For other errors, fail immediately
                error!("User health check failed for {}: {}", user_address, e);
                return Err(e);
            }
        }
    }
}

async fn try_check_user_health<P>(
    provider: &Arc<P>,
    pool_address: Address,
    user_address: Address,
) -> Result<UserPosition>
where
    P: Provider,
{
    debug!("Checking health factor for user: {:?}", user_address);

    // Call getUserAccountData with proper error handling
    let call_data = match alloy_primitives::hex::decode("bf92857c") {
        Ok(data) => data,
        Err(e) => {
            return Err(eyre::eyre!(
                "Failed to decode getUserAccountData selector: {}",
                e
            ));
        }
    };

    // Encode the user address parameter
    let mut full_call_data = call_data;
    let mut user_bytes = [0u8; 32];
    let user_address_bytes = user_address.as_slice();
    user_address_bytes.iter().enumerate().for_each(|(i, &b)| {
        user_bytes[i + 12] = b; // Address is 20 bytes, pad to 32
    });
    full_call_data.extend_from_slice(&user_bytes);

    let call_request = alloy_rpc_types::TransactionRequest {
        to: Some(pool_address.into()),
        input: alloy_rpc_types::TransactionInput::new(full_call_data.into()),
        ..Default::default()
    };

    let result = provider.call(&call_request).await?;

    // Parse the result - getUserAccountData returns 6 uint256 values
    if result.len() < 192 {
        // 6 * 32 bytes
        return Err(eyre::eyre!(
            "Invalid response length from getUserAccountData: {} bytes, expected at least 192",
            result.len()
        ));
    }

    // Parse the 6 uint256 values returned by getUserAccountData
    let total_collateral_base = U256::from_be_slice(&result[0..32]);
    let total_debt_base = U256::from_be_slice(&result[32..64]);
    let available_borrows_base = U256::from_be_slice(&result[64..96]);
    let current_liquidation_threshold = U256::from_be_slice(&result[96..128]);
    let ltv = U256::from_be_slice(&result[128..160]);
    let health_factor = U256::from_be_slice(&result[160..192]);

    debug!(
        "User {} health data: collateral={}, debt={}, health_factor={}",
        user_address, total_collateral_base, total_debt_base, health_factor
    );

    // Check if user is at risk (health factor < 1.1)
    let risk_threshold = U256::from(1100000000000000000u64); // 1.1 in 18 decimals
    let is_at_risk = health_factor < risk_threshold && health_factor > U256::ZERO;

    let position = UserPosition {
        address: user_address,
        total_collateral_base,
        total_debt_base,
        available_borrows_base,
        current_liquidation_threshold,
        ltv,
        health_factor,
        last_updated: chrono::Utc::now(),
        is_at_risk,
    };

    Ok(position)
}

/// Helper function to get user's collateral assets from the blockchain
async fn get_user_collateral_assets<P>(
    pool_contract: &ContractInstance<alloy_transport::BoxTransport, Arc<P>>,
    user: Address,
) -> Result<Vec<Address>>
where
    P: Provider,
{
    debug!("Fetching user collateral assets for: {:?}", user);

    // Get user configuration bitfield
    let user_config_args = [alloy_dyn_abi::DynSolValue::Address(user)];
    let user_config_call = pool_contract.function("getUserConfiguration", &user_config_args)?;
    let user_config_result = user_config_call.call().await?;

    // Validate that we have at least one element in the result
    if user_config_result.is_empty() {
        return Err(eyre::eyre!("Empty result from getUserConfiguration call"));
    }

    // Extract the configuration data from the tuple
    let config_data = if let alloy_dyn_abi::DynSolValue::Tuple(tuple) = &user_config_result[0] {
        if tuple.is_empty() {
            return Err(eyre::eyre!("Empty tuple in getUserConfiguration result"));
        }
        if let alloy_dyn_abi::DynSolValue::Uint(data, _) = &tuple[0] {
            *data
        } else {
            return Err(eyre::eyre!("Failed to parse configuration data as uint"));
        }
    } else {
        return Err(eyre::eyre!("getUserConfiguration result is not a tuple"));
    };

    // Get reserves list
    let reserves_call = pool_contract.function("getReservesList", &[])?;
    let reserves_result = reserves_call.call().await?;

    // Validate that we have at least one element in the result
    if reserves_result.is_empty() {
        return Err(eyre::eyre!("Empty result from getReservesList call"));
    }

    // Extract reserves array
    let reserves: Vec<Address> =
        if let alloy_dyn_abi::DynSolValue::Array(array) = &reserves_result[0] {
            array
                .iter()
                .filter_map(|value| {
                    if let alloy_dyn_abi::DynSolValue::Address(addr) = value {
                        Some(*addr)
                    } else {
                        None
                    }
                })
                .collect()
        } else {
            return Err(eyre::eyre!("getReservesList result is not an array"));
        };

    debug!("Found {} reserves in the pool", reserves.len());

    // Decode user configuration bitfield to find collateral assets
    let mut user_collateral_assets = Vec::new();

    // Each asset has 2 bits in the configuration:
    // - Bit 2*i: whether the asset is used as collateral
    // - Bit 2*i+1: whether the asset is borrowed
    for (i, &reserve_address) in reserves.iter().enumerate() {
        let collateral_bit = (config_data >> (2 * i)) & U256::from(1u8);

        if collateral_bit != U256::ZERO {
            user_collateral_assets.push(reserve_address);
            debug!("User has {} as collateral", reserve_address);
        }
    }

    debug!(
        "User {} has {} collateral assets",
        user,
        user_collateral_assets.len()
    );

    // Note: Returning an empty vector here is valid - it means the user genuinely
    // has no collateral assets, which is different from parsing errors above
    Ok(user_collateral_assets)
}

/// Update the users_by_collateral mapping for a specific user
pub async fn update_user_collateral_mapping<P>(
    pool_contract: &ContractInstance<alloy_transport::BoxTransport, Arc<P>>,
    user: Address,
    users_by_collateral: &Arc<DashMap<Address, HashSet<Address>>>,
    asset_configs: Option<&HashMap<Address, AssetConfig>>,
) -> Result<()>
where
    P: Provider,
{
    // Get user's collateral assets
    let collateral_assets = get_user_collateral_assets(pool_contract, user).await?;

    // Remove user from all existing collateral mappings first
    for mut entry in users_by_collateral.iter_mut() {
        entry.value_mut().remove(&user);
    }

    // If no collateral assets found but we have asset configs, that's normal
    // (user might not have any collateral), don't use fallbacks
    if collateral_assets.is_empty() {
        debug!("User {:?} has no collateral assets", user);
        return Ok(());
    }

    // Add user to correct collateral asset mappings
    for asset_address in &collateral_assets {
        users_by_collateral
            .entry(*asset_address)
            .or_insert_with(HashSet::new)
            .insert(user);

        // Log with asset symbol if available
        let asset_symbol = asset_configs
            .and_then(|configs| configs.get(asset_address))
            .map(|config| config.symbol.as_str())
            .unwrap_or("Unknown");

        debug!(
            "Mapped user {:?} to collateral asset {} ({})",
            user, asset_address, asset_symbol
        );
    }

    Ok(())
}

pub async fn update_user_position<P>(
    provider: Arc<P>,
    pool_contract: &ContractInstance<alloy_transport::BoxTransport, Arc<P>>,
    db_pool: &DatabasePool,
    user_positions: Arc<DashMap<Address, UserPosition>>,
    processing_users: Arc<SyncRwLock<HashSet<Address>>>,
    event_tx: mpsc::UnboundedSender<BotEvent>,
    health_factor_threshold: U256,
    user: Address,
    users_by_collateral: Option<Arc<DashMap<Address, HashSet<Address>>>>,
    asset_configs: Option<&HashMap<Address, AssetConfig>>,
    priority_liquidation_tx: Option<mpsc::Sender<Address>>,  // New parameter for priority channel
) -> Result<()>
where
    P: Provider,
{
    // Use guard pattern to ensure reliable cleanup
    let _guard = match ProcessingGuard::new(user, processing_users.clone()) {
        Some(guard) => guard,
        None => return Ok(()), // User already being processed
    };

    let result = check_user_health(&provider, *pool_contract.address(), user, 3).await;

    match result {
        Ok(position) => {
            // Atomic read-modify-write operation to prevent race conditions
            let old_position = {
                // Clone the old position while holding the reference
                user_positions.get(&user).map(|p| p.clone())
            };

            // Update in memory first (this is atomic due to DashMap's internal locking)
            user_positions.insert(user, position.clone());

            // Save to database first before any events
            let database_save_successful =
                match crate::database::save_user_position(db_pool, &position).await {
                    Ok(()) => {
                        debug!(
                            "✅ Successfully saved user position to database: {:?}",
                            user
                        );
                        true
                    }
                    Err(e) => {
                        error!("Failed to save user position for {:?}: {}", user, e);
                        false
                    }
                };

            // Populate users_by_collateral mapping by calling getUserConfiguration
            if let Some(users_by_collateral) = users_by_collateral {
                if let Err(e) = update_user_collateral_mapping(
                    pool_contract,
                    user,
                    &users_by_collateral,
                    asset_configs,
                )
                .await
                {
                    warn!("Failed to update collateral mapping for {:?}: {}", user, e);
                }
            }

            // Check for liquidation opportunity and route to appropriate channel
            if database_save_successful
                && position.health_factor < U256::from(LIQUIDATION_THRESHOLD)
                && position.total_debt_base > U256::ZERO
            {
                debug!(
                    "🎯 Detected liquidation opportunity for user: {:?}",
                    user
                );
                
                // Route to priority channel if available, otherwise use regular event queue
                if let Some(priority_tx) = priority_liquidation_tx {
                    debug!("⚡ Sending to HIGH-PRIORITY liquidation channel");
                    if let Err(e) = priority_tx.send(user).await {
                        error!("Failed to send to priority liquidation channel: {}", e);
                        // Fallback to regular event queue
                        let _ = event_tx.send(BotEvent::LiquidationOpportunity(user));
                    }
                } else {
                    debug!("📮 Sending to regular event queue");
                    let _ = event_tx.send(BotEvent::LiquidationOpportunity(user));
                }
            }

            // Check if position became at-risk
            if position.is_at_risk && position.health_factor < health_factor_threshold {
                if let Some(old_pos) = old_position {
                    if !old_pos.is_at_risk {
                        info!(
                            "⚠️  NEW AT-RISK USER: {:?} (HF: {})",
                            user,
                            format_health_factor(position.health_factor)
                        );
                        if let Err(e) = database::log_monitoring_event(
                            db_pool,
                            "user_at_risk",
                            Some(user),
                            Some(&format!(
                                "Health factor dropped to {}",
                                format_health_factor(position.health_factor)
                            )),
                        )
                        .await
                        {
                            error!("Failed to log at-risk event: {}", e);
                        }
                    } else {
                        // User was already at-risk, check if health factor changed significantly
                        let hf_diff = if position.health_factor > old_pos.health_factor {
                            position.health_factor - old_pos.health_factor
                        } else {
                            old_pos.health_factor - position.health_factor
                        };

                        // Log if health factor changed by more than 1%
                        let change_threshold = U256::from(CHANGE_THRESHOLD);

                        if hf_diff > change_threshold {
                            let direction = if position.health_factor > old_pos.health_factor {
                                "↗️ IMPROVED"
                            } else {
                                "↘️ WORSENED"
                            };

                            info!(
                                "⚠️  AT-RISK USER {} HEALTH FACTOR: {:?} (HF: {} → {})",
                                direction,
                                user,
                                format_health_factor(old_pos.health_factor),
                                format_health_factor(position.health_factor)
                            );
                        } else {
                            // Log ongoing at-risk status if health factor is dangerously low
                            let danger_threshold = U256::from(CRITICAL_THRESHOLD);
                            if position.health_factor < danger_threshold {
                                info!(
                                    "🚨 CRITICALLY AT-RISK USER (ongoing): {:?} (HF: {} - NEAR LIQUIDATION!)",
                                    user, format_health_factor(position.health_factor)
                                );
                            } else {
                                // Just debug log for normal at-risk users with stable HF
                                debug!(
                                    "⚠️  AT-RISK USER (stable): {:?} (HF: {})",
                                    user,
                                    format_health_factor(position.health_factor)
                                );
                            }
                        }
                    }
                } else {
                    info!(
                        "⚠️  AT-RISK USER: {:?} (HF: {})",
                        user,
                        format_health_factor(position.health_factor)
                    );
                }
            } else if let Some(old_pos) = &old_position {
                // Check if user recovered from at-risk status
                if old_pos.is_at_risk && !position.is_at_risk {
                    info!(
                        "✅ USER RECOVERED: {:?} (HF: {} → {}) - No longer at risk!",
                        user,
                        format_health_factor(old_pos.health_factor),
                        format_health_factor(position.health_factor)
                    );
                }
            }
        }
        Err(e) => {
            error!("Failed to check user health for {:?}: {}", user, e);
        }
    }

    Ok(())
}

pub async fn run_periodic_scan<P>(
    provider: Arc<P>,
    pool_address: Address,
    db_pool: DatabasePool,
    event_tx: mpsc::UnboundedSender<BotEvent>,
    config: BotConfig,
    asset_configs: HashMap<Address, AssetConfig>,
    user_positions: Arc<DashMap<Address, UserPosition>>,
    priority_liquidation_tx: Option<mpsc::Sender<Address>>,  // New parameter
) -> Result<()>
where
    P: Provider,
{
    info!("🔍 Starting periodic health factor scanning...");

    let mut interval = tokio::time::interval(Duration::from_secs(config.monitoring_interval_secs));
    let mut last_full_rescan = std::time::SystemTime::now();
    let full_rescan_duration = Duration::from_secs(config.full_rescan_interval_minutes * 60);

    loop {
        interval.tick().await;
        let now = std::time::SystemTime::now();

        // Check if it's time for a full rescan
        let should_full_rescan = now
            .duration_since(last_full_rescan)
            .unwrap_or(Duration::from_secs(0))
            >= full_rescan_duration;

        if should_full_rescan {
            info!("🔄 Starting FULL rescan of all users...");

            // Get all users from database
            match database::get_all_users(&db_pool).await {
                Ok(users) => {
                    info!("📊 Found {} total users in database for full rescan", users.len());

                    for user in users {
                        // Skip zero addresses
                        if user.address == Address::ZERO {
                            continue;
                        }

                        // Check user health directly (no filtering)
                        match check_user_health(&provider, pool_address, user.address, 3).await {
                            Ok(position) => {
                                // Update position in memory
                                let old_position = user_positions.get(&user.address).map(|p| p.clone());
                                user_positions.insert(user.address, position.clone());

                                // Store in database
                                if let Err(e) =
                                    crate::database::save_user_position(&db_pool, &position).await
                                {
                                    error!("Failed to store user position: {}", e);
                                }

                                // Check if user became at-risk
                                if position.is_at_risk && old_position.as_ref().map_or(true, |old| !old.is_at_risk) {
                                    info!(
                                        "🚨 NEW AT-RISK USER discovered in full rescan: {:?} (HF: {})",
                                        user.address,
                                        format_health_factor(position.health_factor)
                                    );
                                }

                                // Send liquidation opportunity for ANY user that is actually liquidatable (HF < 1.0)
                                // regardless of whether they're newly at-risk or not
                                if position.health_factor < U256::from(LIQUIDATION_THRESHOLD) && position.total_debt_base > U256::ZERO {
                                    info!("🎯 User {:?} is LIQUIDATABLE (HF < 1.0) - sending liquidation opportunity", user.address);
                                    
                                    // Route to priority channel if available
                                    if let Some(ref priority_tx) = priority_liquidation_tx {
                                        if let Err(e) = priority_tx.send(user.address).await {
                                            warn!("Failed to send to priority channel: {}", e);
                                            // Fallback to regular event queue
                                            if let Err(e) = event_tx.send(BotEvent::LiquidationOpportunity(user.address)) {
                                                warn!("Failed to send liquidation opportunity: {}", e);
                                            }
                                        }
                                    } else {
                                        if let Err(e) = event_tx.send(BotEvent::LiquidationOpportunity(user.address)) {
                                            warn!("Failed to send liquidation opportunity: {}", e);
                                        }
                                    }
                                } else if position.health_factor < U256::from(CRITICAL_THRESHOLD) {
                                    debug!("User {:?} is at-risk but NOT liquidatable yet (HF: {} >= 1.0)", user.address, format_health_factor(position.health_factor));
                                }
                                
                                // Brief delay between individual checks
                                sleep(Duration::from_millis(100)).await; // Slightly longer for full scans
                            }
                            Err(e) => {
                                error!("Failed to check user health during full rescan for {:?}: {}", user.address, e);
                                // Continue with next user rather than failing completely
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to get all users for full rescan: {}", e);
                }
            }

            last_full_rescan = now; // Update last full rescan time
        }

        // Regular at-risk scan with configurable limit
        let at_risk_users = match crate::database::get_at_risk_users_with_limit(&db_pool, config.at_risk_scan_limit).await {
            Ok(users) => users,
            Err(e) => {
                error!("Failed to get at-risk users: {}", e);
                continue;
            }
        };

        let scan_type = match config.at_risk_scan_limit {
            Some(limit) => format!("regular (limited to {} users)", limit),
            None => "regular (unlimited)".to_string(),
        };
        info!("🔍 Starting {} scan: {} at-risk users", scan_type, at_risk_users.len());

        // Check health for each user with rate limiting
        let mut checked_users = 0;
        let mut at_risk_users_count = 0;

        for user in &at_risk_users {
            // Add small delay between checks to avoid rate limiting
            if checked_users > 0 && checked_users % 10 == 0 {
                sleep(Duration::from_millis(200)).await; // Brief pause every 10 users
            }

            match check_user_health(&provider, pool_address, user.address, 3).await {
                Ok(position) => {
                    checked_users += 1;

                    if position.is_at_risk {
                        at_risk_users_count += 1;
                        info!(
                            "⚠️  At-risk user found: {:?} (HF: {})",
                            user,
                            format_health_factor(position.health_factor)
                        );

                        // Store in database
                        if let Err(e) =
                            crate::database::save_user_position(&db_pool, &position).await
                        {
                            error!("Failed to store user position: {}", e);
                        }

                        // Only send liquidation opportunity if user is actually liquidatable (HF < 1.0)
                        if position.health_factor < U256::from(LIQUIDATION_THRESHOLD) && position.total_debt_base > U256::ZERO {
                            info!("🎯 User {:?} is LIQUIDATABLE (HF < 1.0) - sending liquidation opportunity", user.address);
                            
                            // Route to priority channel if available
                            if let Some(ref priority_tx) = priority_liquidation_tx {
                                if let Err(e) = priority_tx.send(user.address).await {
                                    warn!("Failed to send to priority channel: {}", e);
                                    // Fallback to regular event queue
                                    if let Err(e) = event_tx.send(BotEvent::LiquidationOpportunity(user.address)) {
                                        warn!("Failed to send liquidation opportunity: {}", e);
                                    }
                                }
                            } else {
                                if let Err(e) = event_tx.send(BotEvent::LiquidationOpportunity(user.address)) {
                                    warn!("Failed to send liquidation opportunity: {}", e);
                                }
                            }
                        } else if position.health_factor < U256::from(CRITICAL_THRESHOLD) {
                            debug!("User {:?} is at-risk but NOT liquidatable yet (HF: {} >= 1.0)", user.address, format_health_factor(position.health_factor));
                        }
                    }

                    // Brief delay between individual checks
                    sleep(Duration::from_millis(50)).await;
                }
                Err(e) => {
                    error!("Failed to check user health for {:?}: {}", user, e);
                    // Continue with next user rather than failing completely
                }
            }
        }

        info!(
            "✅ Regular scan complete: {} checked, {} at-risk found",
            checked_users, at_risk_users_count
        );

        // If we have a specific target user, always check them
        if let Some(target_user) = config.target_user {
            let _ = event_tx.send(BotEvent::UserPositionChanged(target_user));
        }
    }
}

pub async fn start_status_reporter(
    db_pool: DatabasePool,
    user_positions: Arc<DashMap<Address, UserPosition>>,
) -> Result<()> {
    info!("Starting status reporter...");

    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(300)); // Every 5 minutes

    loop {
        interval.tick().await;

        let position_count = user_positions.len();
        let at_risk_count = user_positions
            .iter()
            .filter(|entry| entry.value().is_at_risk)
            .count();
        let liquidatable_count = user_positions
            .iter()
            .filter(|entry| entry.value().health_factor < U256::from(LIQUIDATION_THRESHOLD))
            .count();

        // Get zero debt user count from database
        let zero_debt_count = match crate::database::get_zero_debt_user_count(&db_pool).await {
            Ok(count) => count,
            Err(e) => {
                error!("Failed to get zero debt user count: {}", e);
                -1 // Indicate error in count
            }
        };

        if zero_debt_count >= 0 {
            info!(
                "📊 Status Report: {} positions tracked, {} at risk, {} liquidatable, {} zero debt",
                position_count, at_risk_count, liquidatable_count, zero_debt_count
            );

            if let Err(e) = database::log_monitoring_event(
                &db_pool,
                "status_report",
                None,
                Some(&format!(
                    "positions:{}, at_risk:{}, liquidatable:{}, zero_debt:{}",
                    position_count, at_risk_count, liquidatable_count, zero_debt_count
                )),
            )
            .await
            {
                error!("Failed to log status report: {}", e);
            }
        } else {
            info!(
                "📊 Status Report: {} positions tracked, {} at risk, {} liquidatable",
                position_count, at_risk_count, liquidatable_count
            );

            if let Err(e) = database::log_monitoring_event(
                &db_pool,
                "status_report",
                None,
                Some(&format!(
                    "positions:{}, at_risk:{}, liquidatable:{}",
                    position_count, at_risk_count, liquidatable_count
                )),
            )
            .await
            {
                error!("Failed to log status report: {}", e);
            }
        }
    }
}
