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
    priority_liquidation_tx: Option<mpsc::UnboundedSender<Address>>,
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
            // and getReservesList to determine which assets this user has as collateral
            if let Some(users_by_collateral) = &users_by_collateral {
                if position.total_collateral_base > U256::ZERO {
                    // Update the collateral mapping for this user using all configured assets
                    if let Err(e) = update_user_collateral_mapping(
                        pool_contract,
                        user,
                        users_by_collateral,
                        asset_configs,
                    )
                    .await
                    {
                        error!(
                            "Failed to update collateral mapping for user {:?}: {}",
                            user, e
                        );
                    } else {
                        debug!(
                            "✅ Successfully updated collateral mapping for user {:?}",
                            user
                        );
                    }
                }
            }

            // Only check for liquidation opportunity if database save was successful
            if database_save_successful
                && position.health_factor < U256::from(LIQUIDATION_THRESHOLD)
                && position.total_debt_base > U256::ZERO
            {
                // Send to priority channel if available (for immediate processing)
                if let Some(priority_tx) = &priority_liquidation_tx {
                    debug!(
                        "⚡ Sending priority liquidation for user: {:?}",
                        user
                    );
                    if let Err(e) = priority_tx.send(user) {
                        warn!("Failed to send priority liquidation for user {:?}: {}", user, e);
                        // Fallback to regular event queue
                        let _ = event_tx.send(BotEvent::LiquidationOpportunity(user));
                    }
                } else {
                    // Use regular event queue if priority channel not available
                    debug!(
                        "🎯 Sending liquidation opportunity event for user: {:?}",
                        user
                    );
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
    _asset_configs: HashMap<Address, AssetConfig>,
    user_positions: Arc<DashMap<Address, UserPosition>>,
    priority_liquidation_tx: Option<mpsc::UnboundedSender<Address>>,
) -> Result<()>
where
    P: Provider,
{
    info!("Starting periodic position scan...");

    // Log configuration
    match config.at_risk_scan_limit {
        Some(limit) => info!(
            "🔧 At-risk scan limit configured: {} users per cycle",
            limit
        ),
        None => info!("🔧 At-risk scan limit: unlimited"),
    }
    info!(
        "🔧 Full rescan interval: {} minutes",
        config.full_rescan_interval_minutes
    );

    let mut interval = tokio::time::interval(
        tokio::time::Duration::from_secs(config.monitoring_interval_secs * 6), // Slower than event-driven updates
    );

    let mut full_rescan_interval = tokio::time::interval(
        tokio::time::Duration::from_secs(config.full_rescan_interval_minutes * 60), // Full rescan interval
    );

    // Separate archival interval to avoid timestamp conflicts
    // Calculate archival interval with overflow protection and bounds checking
    let archival_interval_secs = {
        let cooldown_hours = config.zero_debt_cooldown_hours;

        // Prevent zero duration which would cause continuous tight loop
        if cooldown_hours == 0 {
            warn!("zero_debt_cooldown_hours is 0, using minimum archival interval of 1 hour");
            3600 // 1 hour minimum
        } else {
            // Check for potential overflow: cooldown_hours * 3600 should not overflow u64
            let max_safe_hours = u64::MAX / 3600; // ~5.1 trillion hours
            if cooldown_hours > max_safe_hours {
                error!(
                    "zero_debt_cooldown_hours ({}) is too large and would cause overflow, using maximum safe interval of 7 days",
                    cooldown_hours
                );
                7 * 24 * 3600 // 7 days maximum
            } else {
                // Use checked multiplication to detect any remaining overflow edge cases
                match cooldown_hours.checked_mul(3600) {
                    Some(total_secs) => {
                        let interval_secs = total_secs / 4; // Check 4x per cooldown period
                                                            // Ensure minimum interval of 1 hour and maximum of 7 days
                        std::cmp::max(3600, std::cmp::min(interval_secs, 7 * 24 * 3600))
                    }
                    None => {
                        error!(
                            "Integer overflow when calculating archival interval for {} hours, using 7 day default",
                            cooldown_hours
                        );
                        7 * 24 * 3600 // 7 days fallback
                    }
                }
            }
        }
    };

    let mut archival_interval =
        tokio::time::interval(tokio::time::Duration::from_secs(archival_interval_secs));

    loop {
        tokio::select! {
            _ = interval.tick() => {
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
                    // Send to priority channel if available (for immediate processing)
                    if let Some(priority_tx) = &priority_liquidation_tx {
                        info!("⚡ User {:?} is LIQUIDATABLE (HF < 1.0) - sending priority liquidation", user.address);
                        if let Err(e) = priority_tx.send(user.address) {
                            warn!("Failed to send priority liquidation for user {:?}: {}", user.address, e);
                            // Fallback to regular event queue
                            if let Err(e) = event_tx.send(BotEvent::LiquidationOpportunity(user.address)) {
                                warn!("Failed to send liquidation opportunity fallback: {}", e);
                            }
                        }
                    } else {
                        info!("🎯 User {:?} is LIQUIDATABLE (HF < 1.0) - sending liquidation opportunity", user.address);
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
            }
            _ = full_rescan_interval.tick() => {
                // Full rescan: check all users to ensure complete coverage
                info!("🔍 Starting full rescan: checking all users to ensure complete coverage");

                let all_users = match crate::database::get_all_users(&db_pool).await {
                    Ok(users) => users,
                    Err(e) => {
                        error!("Failed to get all users for full rescan: {}", e);
                        continue;
                    }
                };

                info!("🔍 Full rescan: {} total users to check", all_users.len());

                let mut checked_users = 0;
                let mut at_risk_users_count = 0;
                let mut new_at_risk_found = 0;

                for user in &all_users {
                    // Add small delay between checks to avoid rate limiting
                    if checked_users > 0 && checked_users % 50 == 0 {
                        sleep(Duration::from_millis(500)).await; // Longer pause for full scans
                        info!("🔍 Full rescan progress: {}/{} users checked", checked_users, all_users.len());
                    }

                    match check_user_health(&provider, pool_address, user.address, 3).await {
                        Ok(position) => {
                            checked_users += 1;

                            // Update the position in database
                            if let Err(e) = crate::database::save_user_position(&db_pool, &position).await {
                                error!("Failed to store user position during full rescan: {}", e);
                            }

                            if position.is_at_risk {
                                at_risk_users_count += 1;

                                // Check if this user was not previously at-risk
                                if !user.is_at_risk {
                                    new_at_risk_found += 1;
                                    warn!(
                                        "🚨 NEW AT-RISK USER discovered in full rescan: {:?} (HF: {})",
                                        user.address,
                                        format_health_factor(position.health_factor)
                                    );
                                }

                                // Send liquidation opportunity for ANY user that is actually liquidatable (HF < 1.0)
                                // regardless of whether they're newly at-risk or not
                                if position.health_factor < U256::from(LIQUIDATION_THRESHOLD) && position.total_debt_base > U256::ZERO {
                                    // Send to priority channel if available (for immediate processing)
                                    if let Some(priority_tx) = &priority_liquidation_tx {
                                        info!("⚡ User {:?} is LIQUIDATABLE (HF < 1.0) - sending priority liquidation (full rescan)", user.address);
                                        if let Err(e) = priority_tx.send(user.address) {
                                            warn!("Failed to send priority liquidation for user {:?}: {}", user.address, e);
                                            // Fallback to regular event queue
                                            if let Err(e) = event_tx.send(BotEvent::LiquidationOpportunity(user.address)) {
                                                warn!("Failed to send liquidation opportunity fallback: {}", e);
                                            }
                                        }
                                    } else {
                                        info!("🎯 User {:?} is LIQUIDATABLE (HF < 1.0) - sending liquidation opportunity", user.address);
                                        if let Err(e) = event_tx.send(BotEvent::LiquidationOpportunity(user.address)) {
                                            warn!("Failed to send liquidation opportunity: {}", e);
                                        }
                                    }
                                } else if position.health_factor < U256::from(CRITICAL_THRESHOLD) {
                                    debug!("User {:?} is at-risk but NOT liquidatable yet (HF: {} >= 1.0)", user.address, format_health_factor(position.health_factor));
                                }
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

                info!(
                    "✅ Full rescan complete: {}/{} users checked, {} at-risk found, {} new at-risk discovered",
                    checked_users, all_users.len(), at_risk_users_count, new_at_risk_found
                );

                if let Err(e) = crate::database::log_monitoring_event(
                    &db_pool,
                    "full_rescan_complete",
                    None,
                    Some(&format!(
                        "checked:{}, total:{}, at_risk:{}, new_at_risk:{}",
                        checked_users, all_users.len(), at_risk_users_count, new_at_risk_found
                    )),
                )
                .await
                {
                    error!("Failed to log full rescan completion: {}", e);
                }
            }
            _ = archival_interval.tick() => {
                // Separate archival process - runs independently from full rescan
                if config.archive_zero_debt_users {
                    info!("🗄️ Starting user archival process...");

                    match crate::database::get_users_eligible_for_archival(
                        &db_pool,
                        config.zero_debt_cooldown_hours,
                        config.safe_health_factor_threshold,
                    ).await {
                        Ok(eligible_users) => {
                            if !eligible_users.is_empty() {
                                info!(
                                    "🗄️ Found {} users eligible for archival (zero debt for {}+ hours)",
                                    eligible_users.len(),
                                    config.zero_debt_cooldown_hours
                                );

                                let user_addresses: Vec<Address> = eligible_users.iter().map(|u| u.address).collect();

                                match crate::database::archive_zero_debt_users(
                                    &db_pool,
                                    &user_addresses,
                                    config.zero_debt_cooldown_hours,
                                    config.safe_health_factor_threshold,
                                ).await {
                                    Ok(archival_result) => {
                                        info!(
                                            "✅ Successfully archived {} zero-debt users from database",
                                            archival_result.archived_count
                                        );

                                        // Remove ONLY the actually archived users from in-memory DashMap to prevent memory bloat
                                        // This fixes the race condition where users might be removed from memory but not from database
                                        let mut removed_from_memory = 0;
                                        for &user_address in &archival_result.archived_addresses {
                                            if user_positions.remove(&user_address).is_some() {
                                                removed_from_memory += 1;
                                                debug!("🗑️ Removed archived user {:?} from memory", user_address);
                                            }
                                        }

                                        info!(
                                            "🧠 Cleaned up {} archived users from memory (expected: {})",
                                            removed_from_memory, archival_result.archived_count
                                        );

                                        // Log archival event with detailed information
                                        if let Err(e) = crate::database::log_monitoring_event(
                                            &db_pool,
                                            "users_archived",
                                            None,
                                            Some(&format!(
                                                "archived {} zero-debt users (db), cleaned {} from memory (actual: {})",
                                                archival_result.archived_count,
                                                removed_from_memory,
                                                archival_result.archived_addresses.len()
                                            )),
                                        ).await {
                                            error!("Failed to log archival event: {}", e);
                                        }

                                        // Warn if there's a mismatch between database and memory cleanup
                                        if removed_from_memory != archival_result.archived_count {
                                            warn!(
                                                "🚨 Memory cleanup mismatch: removed {} from memory but archived {} from database",
                                                removed_from_memory, archival_result.archived_count
                                            );
                                        }
                                    }
                                    Err(e) => {
                                        error!("Failed to archive zero-debt users: {}", e);
                                    }
                                }
                            } else {
                                debug!("🗄️ No users eligible for archival at this time");
                            }
                        }
                        Err(e) => {
                            error!("Failed to get users eligible for archival: {}", e);
                        }
                    }
                } else {
                    // Archival is disabled, just tick the interval
                    debug!("🗄️ User archival is disabled in configuration");
                }
            }
        }

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
