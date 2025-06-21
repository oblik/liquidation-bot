use alloy_primitives::{Address, U256};
use alloy_provider::Provider;
use eyre::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info};

use crate::models::{LiquidationAssetConfig, LiquidationOpportunity, GasEstimate, UserPosition};

// Constants for calculations
const FLASH_LOAN_FEE_BPS: u16 = 5; // 0.05% Aave flash loan fee
const MAX_LIQUIDATION_CLOSE_FACTOR: u16 = 5000; // 50% max liquidation
const SLIPPAGE_TOLERANCE_BPS: u16 = 100; // 1% slippage tolerance
const BASE_GAS_LIMIT: u64 = 800_000; // Base gas limit for liquidation

/// Calculate the profitability of a liquidation opportunity
pub async fn calculate_liquidation_profitability<P>(
    provider: Arc<P>,
    user_position: &UserPosition,
    collateral_asset: &LiquidationAssetConfig,
    debt_asset: &LiquidationAssetConfig,
    min_profit_threshold: U256,
) -> Result<LiquidationOpportunity>
where
    P: Provider,
{
    info!(
        "🔍 Calculating profitability for liquidation: {} collateral -> {} debt",
        collateral_asset.symbol, debt_asset.symbol
    );

    // Step 1: Calculate maximum liquidation amount (50% of debt)
    let max_debt_to_cover = calculate_max_debt_to_cover(user_position.total_debt_base);
    
    // Step 2: Calculate expected collateral received with liquidation bonus
    let (expected_collateral, liquidation_bonus) = calculate_collateral_received(
        max_debt_to_cover,
        collateral_asset.liquidation_bonus,
    );

    // Step 3: Calculate flash loan fee
    let flash_loan_fee = calculate_flash_loan_fee(max_debt_to_cover);

    // Step 4: Estimate gas costs
    let gas_estimate = estimate_gas_cost(provider.clone()).await?;

    // Step 5: Estimate swap slippage (if assets are different)
    let swap_slippage = if collateral_asset.address != debt_asset.address {
        estimate_swap_slippage(expected_collateral, collateral_asset, debt_asset)
    } else {
        U256::ZERO
    };

    // Step 6: Calculate net profit
    let estimated_profit = calculate_net_profit(
        expected_collateral,
        max_debt_to_cover,
        liquidation_bonus,
        flash_loan_fee,
        gas_estimate.total_cost,
        swap_slippage,
    );

    let profit_threshold_met = estimated_profit >= min_profit_threshold;

    let opportunity = LiquidationOpportunity {
        user: user_position.address,
        collateral_asset: collateral_asset.address,
        debt_asset: debt_asset.address,
        debt_to_cover: max_debt_to_cover,
        expected_collateral_received: expected_collateral,
        liquidation_bonus,
        flash_loan_fee,
        gas_cost: gas_estimate.total_cost,
        swap_slippage,
        estimated_profit,
        profit_threshold_met,
    };

    info!(
        "💰 Liquidation Analysis Complete:
        Debt to cover: {} wei
        Collateral received: {} wei
        Liquidation bonus: {} wei
        Flash loan fee: {} wei
        Gas cost: {} wei
        Swap slippage: {} wei
        NET PROFIT: {} wei
        Profitable: {}",
        max_debt_to_cover,
        expected_collateral,
        liquidation_bonus,
        flash_loan_fee,
        gas_estimate.total_cost,
        swap_slippage,
        estimated_profit,
        profit_threshold_met
    );

    Ok(opportunity)
}

/// Calculate maximum debt that can be covered (50% of total debt)
fn calculate_max_debt_to_cover(total_debt_base: U256) -> U256 {
    // Aave allows up to 50% of debt to be liquidated in a single transaction
    total_debt_base * U256::from(MAX_LIQUIDATION_CLOSE_FACTOR) / U256::from(10000)
}

/// Calculate expected collateral received including liquidation bonus
fn calculate_collateral_received(debt_to_cover: U256, liquidation_bonus_bps: u16) -> (U256, U256) {
    // Collateral received = debt_to_cover * (1 + liquidation_bonus)
    let bonus_multiplier = U256::from(10000 + liquidation_bonus_bps);
    let collateral_received = debt_to_cover * bonus_multiplier / U256::from(10000);
    let bonus_amount = collateral_received - debt_to_cover;
    
    (collateral_received, bonus_amount)
}

/// Calculate Aave flash loan fee (0.05%)
fn calculate_flash_loan_fee(amount: U256) -> U256 {
    amount * U256::from(FLASH_LOAN_FEE_BPS) / U256::from(10000)
}

/// Estimate gas cost for liquidation transaction
async fn estimate_gas_cost<P>(provider: Arc<P>) -> Result<GasEstimate>
where
    P: Provider,
{
    // Get current gas price from provider
    let gas_price = provider.get_gas_price().await.unwrap_or(U256::from(1_000_000_000)); // 1 gwei fallback
    
    // Estimate gas limit based on typical liquidation transaction
    let gas_limit = U256::from(BASE_GAS_LIMIT);
    
    // Calculate total cost with a 20% buffer for priority fee
    let priority_fee = gas_price * U256::from(20) / U256::from(100);
    let total_gas_price = gas_price + priority_fee;
    let total_cost = gas_limit * total_gas_price;

    debug!(
        "Gas estimate: base_fee={} wei, priority_fee={} wei, limit={}, total_cost={} wei",
        gas_price, priority_fee, gas_limit, total_cost
    );

    Ok(GasEstimate {
        base_fee: gas_price,
        priority_fee,
        gas_limit,
        total_cost,
    })
}

/// Estimate slippage for DEX swap
fn estimate_swap_slippage(
    amount_in: U256,
    _collateral_asset: &LiquidationAssetConfig,
    _debt_asset: &LiquidationAssetConfig,
) -> U256 {
    // Simple slippage estimation: 1% of swap amount
    // In production, you'd query DEX pools for more accurate estimates
    amount_in * U256::from(SLIPPAGE_TOLERANCE_BPS) / U256::from(10000)
}

/// Calculate net profit after all costs
fn calculate_net_profit(
    collateral_received: U256,
    debt_to_cover: U256,
    liquidation_bonus: U256,
    flash_loan_fee: U256,
    gas_cost: U256,
    swap_slippage: U256,
) -> U256 {
    // Net profit = liquidation_bonus - flash_loan_fee - gas_cost - swap_slippage
    // Note: collateral_received includes the liquidation bonus, so we use the bonus directly
    
    let total_costs = flash_loan_fee + gas_cost + swap_slippage;
    
    if liquidation_bonus > total_costs {
        liquidation_bonus - total_costs
    } else {
        U256::ZERO // No profit or loss
    }
}

/// Validate if liquidation opportunity meets minimum requirements
pub fn validate_liquidation_opportunity(
    opportunity: &LiquidationOpportunity,
    min_profit_threshold: U256,
) -> bool {
    // Check profitability
    if opportunity.estimated_profit < min_profit_threshold {
        debug!(
            "Liquidation rejected: profit {} < threshold {}",
            opportunity.estimated_profit, min_profit_threshold
        );
        return false;
    }

    // Check debt amount is meaningful (at least 0.001 ETH equivalent)
    let min_debt_threshold = U256::from(1_000_000_000_000_000u64); // 0.001 ETH in wei
    if opportunity.debt_to_cover < min_debt_threshold {
        debug!(
            "Liquidation rejected: debt amount {} too small",
            opportunity.debt_to_cover
        );
        return false;
    }

    true
}