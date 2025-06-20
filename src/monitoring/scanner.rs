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
use sqlx::{Pool, Sqlite};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info};

pub async fn check_user_health<P>(
    _provider: Arc<P>,
    pool_contract: &ContractInstance<alloy_transport::BoxTransport, Arc<P>>,
    user: Address,
) -> Result<UserPosition>
where
    P: Provider,
{
    debug!("Checking health factor for user: {:?}", user);

    // Call getUserAccountData
    let args = [alloy_dyn_abi::DynSolValue::Address(user)];
    let call = pool_contract.function("getUserAccountData", &args)?;
    let result = call.call().await?;

    // Parse the result
    let total_collateral_base = result[0].as_uint().unwrap().0;
    let total_debt_base = result[1].as_uint().unwrap().0;
    let available_borrows_base = result[2].as_uint().unwrap().0;
    let current_liquidation_threshold = result[3].as_uint().unwrap().0;
    let ltv = result[4].as_uint().unwrap().0;
    let health_factor = result[5].as_uint().unwrap().0;

    let is_at_risk = health_factor < U256::from(1200000000000000000u64); // 1.2 in 18 decimals

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

    debug!("User position: {:?}", position);
    Ok(position)
}

pub async fn update_user_position<P>(
    provider: Arc<P>,
    pool_contract: &ContractInstance<alloy_transport::BoxTransport, Arc<P>>,
    db_pool: &Pool<Sqlite>,
    user_positions: Arc<DashMap<Address, UserPosition>>,
    processing_users: Arc<RwLock<HashSet<Address>>>,
    event_tx: mpsc::UnboundedSender<BotEvent>,
    health_factor_threshold: U256,
    user: Address,
) -> Result<()>
where
    P: Provider,
{
    // Check if already processing this user
    {
        let processing = processing_users.read().await;
        if processing.contains(&user) {
            debug!("User {:?} already being processed, skipping", user);
            return Ok(());
        }
    }

    // Add to processing set
    {
        let mut processing = processing_users.write().await;
        processing.insert(user);
    }

    // Ensure we remove from processing set when done
    let _guard = scopeguard::guard((), |_| {
        let processing_users = processing_users.clone();
        let user = user;
        tokio::spawn(async move {
            let mut processing = processing_users.write().await;
            processing.remove(&user);
        });
    });

    match check_user_health(provider, pool_contract, user).await {
        Ok(position) => {
            let old_position = user_positions.get(&user).map(|p| p.clone());

            // Update in memory
            user_positions.insert(user, position.clone());

            // Save to database
            if let Err(e) = database::save_user_position(db_pool, &position).await {
                error!("Failed to save user position: {}", e);
            }

            // Check for liquidation opportunity
            if position.health_factor < U256::from(10u128.pow(18))
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
                            user, position.health_factor
                        );
                        if let Err(e) = database::log_monitoring_event(
                            db_pool,
                            "user_at_risk",
                            Some(user),
                            Some(&format!(
                                "Health factor dropped to {}",
                                position.health_factor
                            )),
                        )
                        .await
                        {
                            error!("Failed to log at-risk event: {}", e);
                        }
                    }
                } else {
                    info!(
                        "⚠️  AT-RISK USER: {:?} (HF: {})",
                        user, position.health_factor
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
            .filter(|entry| entry.value().health_factor < U256::from(10u128.pow(18)))
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
