use alloy_contract::ContractInstance;
use alloy_primitives::{Address, U256};
use alloy_provider::Provider;
use eyre::Result;
use sqlx::{Pool, Sqlite};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

use super::{assets, executor, profitability};
use crate::database;
use crate::models::UserPosition;

/// Fetch user's actual collateral and debt assets from the blockchain
async fn get_user_assets<P>(
    pool_contract: &ContractInstance<alloy_transport::BoxTransport, Arc<P>>,
    user: Address,
) -> Result<(Vec<Address>, Vec<Address>)>
where
    P: Provider,
{
    debug!("Fetching user configuration for: {:?}", user);

    // Get user configuration bitfield
    let user_config_args = [alloy_dyn_abi::DynSolValue::Address(user)];
    let user_config_call = pool_contract.function("getUserConfiguration", &user_config_args)?;
    let user_config_result = user_config_call.call().await?;

    // Validate that we have at least one element in the result
    if user_config_result.is_empty() {
        return Err(eyre::eyre!("Empty user configuration result"));
    }

    // Extract the configuration data from the tuple
    let config_data = if let alloy_dyn_abi::DynSolValue::Tuple(tuple) = &user_config_result[0] {
        if tuple.is_empty() {
            return Err(eyre::eyre!("Empty user configuration tuple"));
        }
        if let alloy_dyn_abi::DynSolValue::Uint(data, _) = &tuple[0] {
            *data
        } else {
            return Err(eyre::eyre!("Invalid user configuration data format"));
        }
    } else {
        return Err(eyre::eyre!("Invalid user configuration result format"));
    };

    // Get reserves list
    let reserves_call = pool_contract.function("getReservesList", &[])?;
    let reserves_result = reserves_call.call().await?;

    // Validate that we have at least one element in the result
    if reserves_result.is_empty() {
        return Err(eyre::eyre!("Empty reserves list result"));
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
            return Err(eyre::eyre!("Invalid reserves list format"));
        };

    debug!("Found {} reserves in the pool", reserves.len());

    // Decode user configuration bitfield
    let mut user_collateral_assets = Vec::new();
    let mut user_debt_assets = Vec::new();

    // Each asset has 2 bits in the configuration:
    // - Bit 2*i: whether the asset is used as collateral
    // - Bit 2*i+1: whether the asset is borrowed
    for (i, &reserve_address) in reserves.iter().enumerate() {
        let collateral_bit = (config_data >> (2 * i)) & U256::from(1u8);
        let borrowing_bit = (config_data >> (2 * i + 1)) & U256::from(1u8);

        if collateral_bit != U256::ZERO {
            user_collateral_assets.push(reserve_address);
            debug!("User has {} as collateral", reserve_address);
        }

        if borrowing_bit != U256::ZERO {
            user_debt_assets.push(reserve_address);
            debug!("User has {} as debt", reserve_address);
        }
    }

    info!(
        "User {} has {} collateral assets and {} debt assets",
        user,
        user_collateral_assets.len(),
        user_debt_assets.len()
    );

    Ok((user_collateral_assets, user_debt_assets))
}

/// Handle a detected liquidation opportunity with real profitability calculation and execution
pub async fn handle_liquidation_opportunity<P>(
    provider: Arc<P>,
    db_pool: &Pool<Sqlite>,
    user: Address,
    min_profit_threshold: U256,
    liquidator_contract_address: Option<Address>,
    signer: Option<alloy_signer_local::PrivateKeySigner>,
    pool_contract: &ContractInstance<alloy_transport::BoxTransport, Arc<P>>,
) -> Result<()>
where
    P: Provider + 'static,
{
    info!("🎯 LIQUIDATION OPPORTUNITY DETECTED for user: {:?}", user);

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
        Ok(Some(position)) => {
            debug!("✅ Found user position in database for liquidation opportunity");
            position
        }
        Ok(None) => {
            warn!("User position not found in database: {:?}", user);

            // Debug: Check how many total users are in the database
            match sqlx::query!("SELECT COUNT(*) as count FROM user_positions")
                .fetch_one(db_pool)
                .await
            {
                Ok(row) => {
                    warn!("📊 Total users in database: {}", row.count);
                }
                Err(e) => {
                    error!("Failed to count database users: {}", e);
                }
            }

            return Ok(());
        }
        Err(e) => {
            error!("Failed to get user position: {}", e);
            return Err(e);
        }
    };

    // Initialize asset configurations
    let asset_configs = assets::init_base_mainnet_assets();

    // Fetch user's actual collateral and debt assets from the blockchain
    let (user_collateral_assets, user_debt_assets) =
        match get_user_assets(pool_contract, user).await {
            Ok(assets) => assets,
            Err(e) => {
                error!("Failed to fetch user assets from blockchain: {}", e);
                // Fallback to empty vectors - this will cause the liquidation to be skipped
                (Vec::new(), Vec::new())
            }
        };

    // Validate that user has both collateral and debt
    if user_collateral_assets.is_empty() {
        warn!(
            "User {:?} has no collateral assets - cannot liquidate",
            user
        );
        return Ok(());
    }

    if user_debt_assets.is_empty() {
        warn!("User {:?} has no debt assets - nothing to liquidate", user);
        return Ok(());
    }

    // Find the best liquidation pair
    let (collateral_asset_addr, debt_asset_addr) = match assets::find_best_liquidation_pair(
        &asset_configs,
        &user_collateral_assets,
        &user_debt_assets,
    ) {
        Some(pair) => pair,
        None => {
            warn!("No suitable liquidation pair found for user: {:?}", user);
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
            // Create liquidation executor with asset configurations
            let executor = executor::LiquidationExecutor::new(
                provider.clone(),
                signer,
                contract_addr,
                asset_configs.clone(),
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
            warn!(
                "This would be a profitable liquidation worth {} wei",
                opportunity.estimated_profit
            );

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
    let user_str = user.to_string();

    debug!(
        "🔍 Looking up user in database: {} (formatted as: {})",
        user, user_str
    );

    // Try case-insensitive lookup to handle any format mismatches
    let row = sqlx::query!(
        "SELECT * FROM user_positions WHERE LOWER(address) = LOWER(?)",
        user_str
    )
    .fetch_optional(db_pool)
    .await?;

    if let Some(row) = row {
        debug!("✅ Found user position in database: {}", user_str);
        let position = UserPosition {
            address: user,
            total_collateral_base: row.total_collateral_base.parse()?,
            total_debt_base: row.total_debt_base.parse()?,
            available_borrows_base: row.available_borrows_base.parse()?,
            current_liquidation_threshold: row.current_liquidation_threshold.parse()?,
            ltv: row.ltv.parse()?,
            health_factor: row.health_factor.parse()?,
            last_updated: row.last_updated.and_utc(),
            is_at_risk: row.is_at_risk,
        };
        Ok(Some(position))
    } else {
        debug!("❌ User position not found in database: {}", user_str);
        Ok(None)
    }
}

/// Save liquidation record to database
async fn save_liquidation_record(
    db_pool: &Pool<Sqlite>,
    opportunity: &crate::models::LiquidationOpportunity,
    tx_hash: &str,
) -> Result<()> {
    // Use checksummed hex representation for consistent address storage (matches database storage format)
    let user_str = opportunity.user.to_string();
    let collateral_str = opportunity.collateral_asset.to_string();
    let debt_str = opportunity.debt_asset.to_string();
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

    info!("🎯 LIQUIDATION OPPORTUNITY DETECTED for user: {:?}", user);
    info!("⚠️  Enhanced liquidation execution requires provider and signer");
    info!("💰 Minimum profit threshold: {} wei", min_profit_threshold);

    Ok(())
}
