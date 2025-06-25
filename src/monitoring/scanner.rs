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
use tokio::sync::mpsc;
use tracing::{debug, error, info};

// Threshold constants for health factor calculations (in 18 decimals)
const LIQUIDATION_THRESHOLD: u64 = 1000000000000000000; // 1.0 * 1e18 - liquidation can occur
const CRITICAL_THRESHOLD: u64 = 1100000000000000000;    // 1.1 * 1e18 - critically at risk 
const CHANGE_THRESHOLD: u64 = 10000000000000000;        // 0.01 * 1e18 - 1% change for logging

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
    _provider: Arc<P>,
    pool_contract: &ContractInstance<alloy_transport::BoxTransport, Arc<P>>,
    user: Address,
    health_factor_threshold: U256,
) -> Result<UserPosition>
where
    P: Provider,
{
    debug!("Checking health factor for user: {:?}", user);

    // Call getUserAccountData
    let args = [alloy_dyn_abi::DynSolValue::Address(user)];
    let call = pool_contract.function("getUserAccountData", &args)?;
    let result = call.call().await?;

    // Parse the result with proper error handling
    let total_collateral_base = parse_u256_from_result(&result, 0, "total_collateral_base")?;
    let total_debt_base = parse_u256_from_result(&result, 1, "total_debt_base")?;
    let available_borrows_base = parse_u256_from_result(&result, 2, "available_borrows_base")?;
    let current_liquidation_threshold =
        parse_u256_from_result(&result, 3, "current_liquidation_threshold")?;
    let ltv = parse_u256_from_result(&result, 4, "ltv")?;
    let health_factor = parse_u256_from_result(&result, 5, "health_factor")?;

    // Use the configurable threshold for consistent at-risk detection
    // This threshold should be set above the liquidation threshold (1.0) to provide early warning
    let is_at_risk = health_factor < health_factor_threshold;

    let position = UserPosition {
        address: user,
        total_collateral_base,
        total_debt_base,
        available_borrows_base,
        current_liquidation_threshold,
        ltv,
        health_factor,
        last_updated: Utc::now(),
        is_at_risk,
    };

    info!(
        "📊 User {:?} health check: HF={}, Collateral={}, Debt={}, At-risk={}",
        user,
        format_health_factor(health_factor),
        total_collateral_base,
        total_debt_base,
        is_at_risk
    );
    Ok(position)
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

    let result = check_user_health(provider, pool_contract, user, health_factor_threshold).await;

    match result {
        Ok(position) => {
            // Atomic read-modify-write operation to prevent race conditions
            let old_position = {
                // Clone the old position while holding the reference
                user_positions.get(&user).map(|p| p.clone())
            };

            // Update in memory first (this is atomic due to DashMap's internal locking)
            user_positions.insert(user, position.clone());

            // Save to database
            if let Err(e) = database::save_user_position(db_pool, &position).await {
                error!("Failed to save user position: {}", e);
            }

            // TODO: Populate users_by_collateral mapping by calling getUserConfiguration
            // and getReservesList to determine which assets this user has as collateral
            // For now, we'll add this user to WETH collateral as a fallback since that's what we monitor
            if let Some(users_by_collateral) = &users_by_collateral {
                if position.total_collateral_base > U256::ZERO {
                    // Base mainnet WETH address
                    match "0x4200000000000000000000000000000000000006".parse::<Address>() {
                        Ok(weth_address) => {
                            users_by_collateral
                                .entry(weth_address)
                                .or_insert_with(HashSet::new)
                                .insert(user);
                            debug!("Added user {:?} to WETH collateral tracking", user);
                        }
                        Err(e) => {
                            error!("Failed to parse WETH address for collateral tracking: {}", e);
                        }
                    }
                }
            }

            // Check for liquidation opportunity
            if position.health_factor < U256::from(LIQUIDATION_THRESHOLD)
                && position.total_debt_base > U256::ZERO
            {
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

pub async fn run_periodic_scan(
    db_pool: Pool<Sqlite>,
    event_tx: mpsc::UnboundedSender<BotEvent>,
    config: BotConfig,
) -> Result<()> {
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

        for user_addr in at_risk_users {
            let _ = event_tx.send(BotEvent::UserPositionChanged(user_addr));
        }

        // If we have a specific target user, always check them
        if let Some(target_user) = config.target_user {
            let _ = event_tx.send(BotEvent::UserPositionChanged(target_user));
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
