use alloy_contract::ContractInstance;
use alloy_primitives::{Address, U256};
use alloy_provider::Provider;
use chrono::Utc;
use eyre::Result;
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use crate::models::UserPosition;

pub async fn check_user_health<P>(
    pool_contract: &ContractInstance<Arc<P>>,
    user: Address,
) -> Result<UserPosition>
where
    P: Provider + 'static,
{
    debug!("Checking health factor for user: {:?}", user);

    // Call getUserAccountData
    let args = [alloy_dyn_abi::DynSolValue::Address(user)];
    let call = pool_contract.function("getUserAccountData", &args)?;
    let result = call.call().await?;

    // Parse the result - getUserAccountData returns:
    // (totalCollateralBase, totalDebtBase, availableBorrowsBase, currentLiquidationThreshold, ltv, healthFactor)
    let values = result.as_tuple().unwrap();

    let total_collateral_base = values[0].as_uint().unwrap().0;
    let total_debt_base = values[1].as_uint().unwrap().0;
    let available_borrows_base = values[2].as_uint().unwrap().0;
    let current_liquidation_threshold = values[3].as_uint().unwrap().0;
    let ltv = values[4].as_uint().unwrap().0;
    let health_factor = values[5].as_uint().unwrap().0;

    let health_factor_threshold = U256::from(1000000000000000000u64); // 1.0 in 18 decimals
    let is_at_risk = health_factor > U256::ZERO && health_factor < health_factor_threshold;

    debug!(
        "User {:?} - Health Factor: {}, Collateral: {}, Debt: {}, At Risk: {}",
        user, health_factor, total_collateral_base, total_debt_base, is_at_risk
    );

    Ok(UserPosition {
        address: user,
        total_collateral_base,
        total_debt_base,
        available_borrows_base,
        current_liquidation_threshold,
        ltv,
        health_factor,
        last_updated: Utc::now(),
        is_at_risk,
    })
}

pub async fn update_user_position<P>(
    pool_contract: &ContractInstance<Arc<P>>,
    user: Address,
    db_pool: &sqlx::Pool<sqlx::Sqlite>,
) -> Result<()>
where
    P: Provider + 'static,
{
    match check_user_health(pool_contract, user).await {
        Ok(position) => {
            // Save to database
            if let Err(e) = crate::database::save_user_position(db_pool, &position).await {
                error!("Failed to save user position to database: {}", e);
            }

            // Log if user is at risk
            if position.is_at_risk {
                warn!(
                    "🚨 User {} is at liquidation risk! Health Factor: {}",
                    user, position.health_factor
                );
                
                // Log monitoring event
                if let Err(e) = crate::database::log_monitoring_event(
                    db_pool,
                    "LIQUIDATION_RISK",
                    Some(user),
                    Some(&format!("Health Factor: {}", position.health_factor)),
                ).await {
                    error!("Failed to log monitoring event: {}", e);
                }
            }
        }
        Err(e) => {
            error!("Failed to check user health for {}: {}", user, e);
        }
    }

    Ok(())
}

pub async fn handle_liquidation_opportunity<P>(
    pool_contract: &ContractInstance<Arc<P>>,
    user: Address,
    min_profit_threshold: U256,
    db_pool: &sqlx::Pool<sqlx::Sqlite>,
) -> Result<()>
where
    P: Provider + 'static,
{
    info!("🎯 Checking liquidation opportunity for user: {}", user);

    // Check user health
    let position = check_user_health(pool_contract, user).await?;
    
    if !position.is_at_risk {
        debug!("User {} is not at liquidation risk", user);
        return Ok(());
    }

    // In a real implementation, you would:
    // 1. Calculate optimal liquidation parameters
    // 2. Estimate gas costs and profit
    // 3. Check if profit > min_profit_threshold
    // 4. Execute liquidation if profitable

    // For now, just log the opportunity
    info!(
        "💰 Liquidation opportunity detected for user {}! Health Factor: {}",
        user, position.health_factor
    );

    // Log the liquidation opportunity
    if let Err(e) = crate::database::log_monitoring_event(
        db_pool,
        "LIQUIDATION_OPPORTUNITY",
        Some(user),
        Some(&format!("Health Factor: {}, Collateral: {}, Debt: {}", 
               position.health_factor, position.total_collateral_base, position.total_debt_base)),
    ).await {
        error!("Failed to log liquidation opportunity: {}", e);
    }

    Ok(())
}