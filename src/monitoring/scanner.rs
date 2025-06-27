use crate::config::BotConfig;
use crate::database;
use crate::events::BotEvent;
use crate::models::UserPosition;
use alloy_contract::ContractInstance;
use alloy_primitives::{Address, U256};
use alloy_provider::Provider;
use chrono::Utc;
use dashmap::DashMap;
use eyre::Result;
use parking_lot::RwLock as SyncRwLock;
use sqlx::{Pool, Sqlite};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
use tracing::{debug, error, info, warn};

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
        return Ok(Vec::new());
    }

    // Extract the configuration data from the tuple
    let config_data = if let alloy_dyn_abi::DynSolValue::Tuple(tuple) = &user_config_result[0] {
        if tuple.is_empty() {
            return Ok(Vec::new());
        }
        if let alloy_dyn_abi::DynSolValue::Uint(data, _) = &tuple[0] {
            *data
        } else {
            return Ok(Vec::new());
        }
    } else {
        return Ok(Vec::new());
    };

    // Get reserves list
    let reserves_call = pool_contract.function("getReservesList", &[])?;
    let reserves_result = reserves_call.call().await?;

    // Validate that we have at least one element in the result
    if reserves_result.is_empty() {
        return Ok(Vec::new());
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
            return Ok(Vec::new());
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

    Ok(user_collateral_assets)
}

/// Update the users_by_collateral mapping for a specific user
async fn update_user_collateral_mapping<P>(
    pool_contract: &ContractInstance<alloy_transport::BoxTransport, Arc<P>>,
    user: Address,
    users_by_collateral: &Arc<DashMap<Address, HashSet<Address>>>,
) -> Result<()>
where
    P: Provider,
{
    // Get user's collateral assets
    let collateral_assets = match get_user_collateral_assets(pool_contract, user).await {
        Ok(assets) => assets,
        Err(e) => {
            debug!("Failed to get collateral assets for user {:?}: {}", user, e);
            return Ok(()); // Don't fail the entire operation for one user
        }
    };

    // Remove user from all existing collateral mappings first
    for mut entry in users_by_collateral.iter_mut() {
        entry.remove(&user);
    }

    // Add user to correct collateral asset mappings
    for asset_address in collateral_assets {
        users_by_collateral
            .entry(asset_address)
            .or_insert_with(HashSet::new)
            .insert(user);
        debug!("Mapped user {:?} to collateral asset {}", user, asset_address);
    }

    Ok(())
}

pub async fn update_user_position<P>(
    provider: Arc<P>,
    pool_contract: &ContractInstance<alloy_transport::BoxTransport, Arc<P>>,
    db_pool: &Pool<Sqlite>,
    user_positions: Arc<DashMap<Address, UserPosition>>,
    processing_users: Arc<SyncRwLock<HashSet<Address>>>,
    event_tx: mpsc::UnboundedSender<BotEvent>,
    health_factor_threshold: U256,
    user: Address,
    users_by_collateral: Option<Arc<DashMap<Address, HashSet<Address>>>>,
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
                    // Update the collateral mapping for this user
                    if let Err(e) = update_user_collateral_mapping(
                        pool_contract,
                        user,
                        users_by_collateral,
                    ).await {
                        error!("Failed to update collateral mapping for user {:?}: {}", user, e);
                        
                        // Fallback to WETH mapping for Base mainnet
                        match "0x4200000000000000000000000000000000000006".parse::<Address>() {
                            Ok(weth_address) => {
                                users_by_collateral
                                    .entry(weth_address)
                                    .or_insert_with(HashSet::new)
                                    .insert(user);
                                debug!("Added user {:?} to WETH collateral tracking (fallback)", user);
                            }
                            Err(e) => {
                                error!(
                                    "Failed to parse WETH address for collateral tracking: {}",
                                    e
                                );
                            }
                        }
                    } else {
                        debug!("✅ Successfully updated collateral mapping for user {:?}", user);
                    }
                }
            }

            // Only check for liquidation opportunity if database save was successful
            if database_save_successful
                && position.health_factor < U256::from(LIQUIDATION_THRESHOLD)
                && position.total_debt_base > U256::ZERO
            {
                debug!(
                    "🎯 Sending liquidation opportunity event for user: {:?}",
                    user
                );
                let _ = event_tx.send(BotEvent::LiquidationOpportunity(user));
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
    db_pool: Pool<Sqlite>,
    event_tx: mpsc::UnboundedSender<BotEvent>,
    config: BotConfig,
) -> Result<()>
where
    P: Provider,
{
    info!("Starting periodic position scan...");

    let mut interval = tokio::time::interval(
        tokio::time::Duration::from_secs(config.monitoring_interval_secs * 6), // Slower than event-driven updates
    );

    loop {
        interval.tick().await;

        // Get at-risk users from database
        let at_risk_users = match database::get_at_risk_users(&db_pool).await {
            Ok(users) => users,
            Err(e) => {
                error!("Failed to get at-risk users: {}", e);
                continue;
            }
        };

        info!("Scanning {} at-risk users", at_risk_users.len());

        // Check health for each user with rate limiting
        let mut checked_users = 0;
        let mut at_risk_users_count = 0;

        for user in &at_risk_users {
            // Add small delay between checks to avoid rate limiting
            if checked_users > 0 && checked_users % 10 == 0 {
                sleep(Duration::from_millis(200)).await; // Brief pause every 10 users
            }

            match check_user_health(&provider, pool_address, *user, 3).await {
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

                        // Send to liquidation opportunity channel
                        if let Err(e) = event_tx.send(BotEvent::LiquidationOpportunity(*user)) {
                            warn!("Failed to send liquidation opportunity: {}", e);
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

        // If we have a specific target user, always check them
        if let Some(target_user) = config.target_user {
            let _ = event_tx.send(BotEvent::UserPositionChanged(target_user));
        }

        info!(
            "📊 Status Report: {} positions tracked, {} at risk, {} liquidatable",
            at_risk_users.len(),
            at_risk_users_count,
            at_risk_users.len() - at_risk_users_count
        );

        if let Err(e) = database::log_monitoring_event(
            &db_pool,
            "status_report",
            None,
            Some(&format!(
                "positions:{}, at_risk:{}, liquidatable:{}",
                at_risk_users.len(),
                at_risk_users_count,
                at_risk_users.len() - at_risk_users_count
            )),
        )
        .await
        {
            error!("Failed to log status report: {}", e);
        }
    }
}

pub async fn start_status_reporter(
    db_pool: Pool<Sqlite>,
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
