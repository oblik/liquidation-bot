use alloy_primitives::{Address, U256};
use alloy_provider::Provider;
use eyre::Result;
use sqlx::{Pool, Sqlite};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{error, info, warn};

use crate::database;
use crate::models::{LiquidationAssetConfig, UserPosition};
use super::{assets, profitability, executor};

/// Handle a detected liquidation opportunity with real profitability calculation and execution
pub async fn handle_liquidation_opportunity<P>(
    provider: Arc<P>,
    db_pool: &Pool<Sqlite>,
    user: Address,
    min_profit_threshold: U256,
    liquidator_contract_address: Option<Address>,
    signer: Option<alloy_signer_local::PrivateKeySigner>,
) -> Result<()>
where
    P: Provider + 'static,
{
    info!("🎯 LIQUIDATION OPPORTUNITY DETECTED for user: {}", user);

    // Log the opportunity detection
    database::log_monitoring_event(
        db_pool,
        "liquidation_opportunity_detected",
        Some(user),
        Some("Liquidation opportunity detected - health factor below threshold"),
    )
    .await?;

    // Get user position from database
    let user_position = match get_user_position_from_db(db_pool, user).await {
        Ok(Some(position)) => position,
        Ok(None) => {
            warn!("User position not found in database: {}", user);
            return Ok(());
        }
        Err(e) => {
            error!("Failed to get user position: {}", e);
            return Err(e);
        }
    };

    // Initialize asset configurations
    let asset_configs = assets::init_base_sepolia_assets();

    // For this example, we'll simulate the user's collateral and debt assets
    // In a real implementation, you'd call getUserConfiguration to get actual assets
    let user_collateral_assets = vec![
        "0x4200000000000000000000000000000000000006".parse()?, // WETH
    ];
    let user_debt_assets = vec![
        "0x036CbD53842c5426634e7929541eC2318f3dCF7e".parse()?, // USDC
    ];

    // Find the best liquidation pair
    let (collateral_asset_addr, debt_asset_addr) = match assets::find_best_liquidation_pair(
        &asset_configs,
        &user_collateral_assets,
        &user_debt_assets,
    ) {
        Some(pair) => pair,
        None => {
            warn!("No suitable liquidation pair found for user: {}", user);
            return Ok(());
        }
    };

    // Get asset configurations
    let collateral_asset = assets::get_asset_config(&asset_configs, collateral_asset_addr)
        .ok_or_else(|| eyre::eyre!("Collateral asset config not found"))?;
    let debt_asset = assets::get_asset_config(&asset_configs, debt_asset_addr)
        .ok_or_else(|| eyre::eyre!("Debt asset config not found"))?;

    info!(
        "📊 Analyzing liquidation: {} collateral -> {} debt",
        collateral_asset.symbol, debt_asset.symbol
    );

    // Calculate profitability
    let opportunity = profitability::calculate_liquidation_profitability(
        provider.clone(),
        &user_position,
        collateral_asset,
        debt_asset,
        min_profit_threshold,
    )
    .await?;

    // Validate the opportunity
    if !profitability::validate_liquidation_opportunity(&opportunity, min_profit_threshold) {
        info!("❌ Liquidation opportunity rejected - not profitable enough");
        
        database::log_monitoring_event(
            db_pool,
            "liquidation_rejected",
            Some(user),
            Some(&format!(
                "Liquidation rejected: profit {} < threshold {} wei",
                opportunity.estimated_profit, min_profit_threshold
            )),
        )
        .await?;
        
        return Ok(());
    }

    info!("✅ Liquidation opportunity validated - proceeding with execution");

    // Execute liquidation if we have the necessary components
    match (liquidator_contract_address, signer) {
        (Some(contract_addr), Some(signer)) => {
            // Create liquidation executor
            let executor = executor::LiquidationExecutor::new(
                provider.clone(),
                signer,
                contract_addr,
            )?;

            // Verify contract setup
            if let Err(e) = executor.verify_contract_setup().await {
                error!("Contract setup verification failed: {}", e);
                return Err(e);
            }

            // Execute the liquidation
            match executor.execute_liquidation(&opportunity).await {
                Ok(tx_hash) => {
                    info!("🎉 Liquidation executed successfully! TX: {}", tx_hash);
                    
                    // Log successful execution
                    database::log_monitoring_event(
                        db_pool,
                        "liquidation_executed",
                        Some(user),
                        Some(&format!(
                            "Liquidation executed successfully. TX: {}, Profit: {} wei",
                            tx_hash, opportunity.estimated_profit
                        )),
                    )
                    .await?;

                    // Save liquidation record
                    save_liquidation_record(db_pool, &opportunity, &tx_hash).await?;
                }
                Err(e) => {
                    error!("Failed to execute liquidation: {}", e);
                    
                    database::log_monitoring_event(
                        db_pool,
                        "liquidation_failed",
                        Some(user),
                        Some(&format!("Liquidation execution failed: {}", e)),
                    )
                    .await?;
                    
                    return Err(e);
                }
            }
        }
        _ => {
            // Missing liquidator contract or signer - just simulate
            warn!("⏳ Liquidation execution not available - missing contract address or signer");
            warn!("This would be a profitable liquidation worth {} wei", opportunity.estimated_profit);
            
            database::log_monitoring_event(
                db_pool,
                "liquidation_simulated",
                Some(user),
                Some(&format!(
                    "Simulated profitable liquidation: {} wei profit",
                    opportunity.estimated_profit
                )),
            )
            .await?;
        }
    }

    Ok(())
}

/// Get user position from database
async fn get_user_position_from_db(
    db_pool: &Pool<Sqlite>,
    user: Address,
) -> Result<Option<UserPosition>> {
    let user_str = format!("{:?}", user);
    
    let row = sqlx::query!(
        "SELECT * FROM user_positions WHERE address = ?",
        user_str
    )
    .fetch_optional(db_pool)
    .await?;

    if let Some(row) = row {
        let position = UserPosition {
            address: user,
            total_collateral_base: row.total_collateral_base.parse()?,
            total_debt_base: row.total_debt_base.parse()?,
            available_borrows_base: row.available_borrows_base.parse()?,
            current_liquidation_threshold: row.current_liquidation_threshold.parse()?,
            ltv: row.ltv.parse()?,
            health_factor: row.health_factor.parse()?,
            last_updated: row.last_updated,
            is_at_risk: row.is_at_risk,
        };
        Ok(Some(position))
    } else {
        Ok(None)
    }
}

/// Save liquidation record to database
async fn save_liquidation_record(
    db_pool: &Pool<Sqlite>,
    opportunity: &crate::models::LiquidationOpportunity,
    tx_hash: &str,
) -> Result<()> {
    let user_str = format!("{:?}", opportunity.user);
    let collateral_str = format!("{:?}", opportunity.collateral_asset);
    let debt_str = format!("{:?}", opportunity.debt_asset);
    let debt_covered_str = opportunity.debt_to_cover.to_string();
    let collateral_received_str = opportunity.expected_collateral_received.to_string();
    let profit_str = opportunity.estimated_profit.to_string();

    sqlx::query!(
        r#"
        INSERT INTO liquidation_events (
            user_address, collateral_asset, debt_asset, debt_covered,
            collateral_received, profit, tx_hash, timestamp
        ) VALUES (?, ?, ?, ?, ?, ?, ?, datetime('now'))
        "#,
        user_str,
        collateral_str,
        debt_str,
        debt_covered_str,
        collateral_received_str,
        profit_str,
        tx_hash
    )
    .execute(db_pool)
    .await?;

    Ok(())
}

/// Legacy function for backward compatibility - now with enhanced functionality
pub async fn handle_liquidation_opportunity_legacy(
    db_pool: &Pool<Sqlite>,
    user: Address,
    min_profit_threshold: U256,
) -> Result<()> {
    warn!("Using legacy liquidation handler - functionality limited");
    
    // Log the opportunity
    database::log_monitoring_event(
        db_pool,
        "liquidation_opportunity_legacy",
        Some(user),
        Some("Legacy liquidation opportunity handler called"),
    )
    .await?;

    info!("🎯 LIQUIDATION OPPORTUNITY DETECTED for user: {}", user);
    info!("⚠️  Enhanced liquidation execution requires provider and signer");
    info!("💰 Minimum profit threshold: {} wei", min_profit_threshold);

    Ok(())
}