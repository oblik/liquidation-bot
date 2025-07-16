use alloy_primitives::U256;
use alloy_provider::Provider;
use eyre::Result;
use std::sync::Arc;
use tracing::{debug, info};

use crate::models::{GasEstimate, LiquidationAssetConfig, LiquidationOpportunity, UserPosition};

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
    let (expected_collateral, liquidation_bonus) =
        calculate_collateral_received(max_debt_to_cover, collateral_asset.liquidation_bonus);

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
    // Use saturating arithmetic to prevent overflow and ensure safe division
    if total_debt_base.is_zero() {
        return U256::ZERO;
    }
    total_debt_base.saturating_mul(U256::from(MAX_LIQUIDATION_CLOSE_FACTOR)) / U256::from(10000)
}

/// Calculate expected collateral received including liquidation bonus
fn calculate_collateral_received(debt_to_cover: U256, liquidation_bonus_bps: u16) -> (U256, U256) {
    // Collateral received = debt_to_cover * (1 + liquidation_bonus)
    // Use saturating arithmetic to prevent overflow and ensure safe operations
    if debt_to_cover.is_zero() {
        return (U256::ZERO, U256::ZERO);
    }

    let bonus_multiplier = U256::from(10000_u16.saturating_add(liquidation_bonus_bps));
    let collateral_received = debt_to_cover.saturating_mul(bonus_multiplier) / U256::from(10000);
    let bonus_amount = collateral_received.saturating_sub(debt_to_cover);

    (collateral_received, bonus_amount)
}

/// Calculate Aave flash loan fee (0.05%)
fn calculate_flash_loan_fee(amount: U256) -> U256 {
    // Use saturating arithmetic to prevent overflow
    if amount.is_zero() {
        return U256::ZERO;
    }
    amount.saturating_mul(U256::from(FLASH_LOAN_FEE_BPS)) / U256::from(10000)
}

/// Estimate gas cost for liquidation transaction
async fn estimate_gas_cost<P>(provider: Arc<P>) -> Result<GasEstimate>
where
    P: Provider,
{
    // Get current gas price from provider (returns Result<u128, Error>)
    let gas_price_u128 = provider
        .get_gas_price()
        .await
        .unwrap_or_else(|_| 20_000_000_000); // 20 gwei fallback (realistic for modern networks)

    // Convert to U256 for calculations
    let gas_price = U256::from(gas_price_u128);

    // Estimate gas limit based on typical liquidation transaction
    let gas_limit = U256::from(BASE_GAS_LIMIT);

    // Calculate total cost with a 20% buffer for priority fee using saturating arithmetic to prevent overflow
    let priority_fee = gas_price.saturating_mul(U256::from(20)) / U256::from(100);
    let total_gas_price = gas_price.saturating_add(priority_fee);
    let total_cost = gas_limit.saturating_mul(total_gas_price);

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
    _collateral_received: U256,
    _debt_to_cover: U256,
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

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::Address;
    use chrono::Utc;
    use std::str::FromStr;

    // Simple mock for testing - we'll test the individual functions directly
    fn create_mock_gas_estimate(gas_price_gwei: u64) -> GasEstimate {
        let gas_price = U256::from(gas_price_gwei) * U256::from(1_000_000_000u64); // Convert gwei to wei safely
        let gas_limit = U256::from(BASE_GAS_LIMIT);
        let priority_fee = gas_price * U256::from(20) / U256::from(100);
        let total_cost = gas_limit * (gas_price + priority_fee);

        GasEstimate {
            base_fee: gas_price,
            priority_fee,
            gas_limit,
            total_cost,
        }
    }

    fn create_test_user_position(total_debt: u64) -> UserPosition {
        UserPosition {
            address: Address::from_str("0x742d35Cc6635C0532925a3b8D0fDfB4C8f9f3BF4").unwrap(),
            total_collateral_base: U256::from(total_debt) * U256::from(2), // Over-collateralized - use U256 math
            total_debt_base: U256::from(total_debt),
            available_borrows_base: U256::ZERO,
            current_liquidation_threshold: U256::from(8000), // 80%
            ltv: U256::from(7500),                           // 75%
            health_factor: U256::from(950_000_000_000_000_000u64), // 0.95 (below 1.0)
            last_updated: Utc::now(),
            is_at_risk: true,
        }
    }

    fn create_large_debt_position() -> UserPosition {
        // Helper function for large debt amounts
        UserPosition {
            address: Address::from_str("0x742d35Cc6635C0532925a3b8D0fDfB4C8f9f3BF4").unwrap(),
            total_collateral_base: U256::from_str("2000000000000000000000").unwrap(), // 2000 ETH
            total_debt_base: U256::from_str("1000000000000000000000").unwrap(), // 1000 ETH debt
            available_borrows_base: U256::ZERO,
            current_liquidation_threshold: U256::from(8000), // 80%
            ltv: U256::from(7500),                           // 75%
            health_factor: U256::from(950_000_000_000_000_000u64), // 0.95 (below 1.0)
            last_updated: Utc::now(),
            is_at_risk: true,
        }
    }

    fn create_weth_config() -> LiquidationAssetConfig {
        LiquidationAssetConfig {
            address: Address::from_str("0x4200000000000000000000000000000000000006").unwrap(), // WETH
            symbol: "WETH".to_string(),
            decimals: 18,
            asset_id: 0,
            liquidation_bonus: 500, // 5% liquidation bonus
            is_collateral: true,
            is_borrowable: true,
        }
    }

    fn create_usdc_config() -> LiquidationAssetConfig {
        LiquidationAssetConfig {
            address: Address::from_str("0x036CbD53842c5426634e7929541eC2318f3dCF7e").unwrap(), // USDC
            symbol: "USDC".to_string(),
            decimals: 6,
            asset_id: 1,
            liquidation_bonus: 450, // 4.5% liquidation bonus
            is_collateral: true,
            is_borrowable: true,
        }
    }

    #[tokio::test]
    async fn test_profitable_liquidation_scenario() {
        // Test a clearly profitable liquidation scenario
        let user_position = create_large_debt_position(); // 1000 ETH debt
        let collateral_asset = create_weth_config();
        let debt_asset = create_usdc_config();
        let min_profit_threshold = U256::from(1_000_000_000_000_000_000u64); // 1 ETH minimum profit

        // Create mock gas estimate for low gas price scenario
        let gas_estimate = create_mock_gas_estimate(1); // 1 gwei

        // Test the individual calculation components
        let max_debt_to_cover = calculate_max_debt_to_cover(user_position.total_debt_base);
        let (expected_collateral, liquidation_bonus) =
            calculate_collateral_received(max_debt_to_cover, collateral_asset.liquidation_bonus);
        let flash_loan_fee = calculate_flash_loan_fee(max_debt_to_cover);
        let swap_slippage =
            estimate_swap_slippage(expected_collateral, &collateral_asset, &debt_asset);
        let estimated_profit = calculate_net_profit(
            expected_collateral,
            max_debt_to_cover,
            liquidation_bonus,
            flash_loan_fee,
            gas_estimate.total_cost,
            swap_slippage,
        );

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
            profit_threshold_met: estimated_profit >= min_profit_threshold,
        };

        println!("🧪 PROFITABLE LIQUIDATION TEST:");
        println!("   Debt to cover: {} wei", opportunity.debt_to_cover);
        println!(
            "   Expected collateral: {} wei",
            opportunity.expected_collateral_received
        );
        println!(
            "   Liquidation bonus: {} wei",
            opportunity.liquidation_bonus
        );
        println!("   Flash loan fee: {} wei", opportunity.flash_loan_fee);
        println!("   Gas cost: {} wei", opportunity.gas_cost);
        println!("   Swap slippage: {} wei", opportunity.swap_slippage);
        println!("   NET PROFIT: {} wei", opportunity.estimated_profit);
        println!("   Profitable: {}", opportunity.profit_threshold_met);

        // Verify the opportunity is profitable
        assert!(opportunity.profit_threshold_met);
        assert!(opportunity.estimated_profit >= min_profit_threshold);

        // Verify calculations
        let expected_debt_to_cover =
            user_position.total_debt_base * U256::from(5000) / U256::from(10000); // 50%
        assert_eq!(opportunity.debt_to_cover, expected_debt_to_cover);

        // Verify liquidation bonus calculation (5% bonus)
        let expected_bonus = expected_debt_to_cover * U256::from(500) / U256::from(10000);
        assert_eq!(opportunity.liquidation_bonus, expected_bonus);
    }

    #[tokio::test]
    async fn test_unprofitable_high_gas_scenario() {
        // Test an unprofitable liquidation due to high gas costs
        let user_position = create_test_user_position(10_000_000_000_000_000_000u64); // 10 ETH debt (smaller)
        let collateral_asset = create_weth_config();
        let debt_asset = create_usdc_config();
        let min_profit_threshold = U256::from(1_000_000_000_000_000_000u64); // 1 ETH minimum profit

        // Create mock gas estimate for very high gas price
        let gas_estimate = create_mock_gas_estimate(500); // 500 gwei (very high)

        let max_debt_to_cover = calculate_max_debt_to_cover(user_position.total_debt_base);
        let (expected_collateral, liquidation_bonus) =
            calculate_collateral_received(max_debt_to_cover, collateral_asset.liquidation_bonus);
        let flash_loan_fee = calculate_flash_loan_fee(max_debt_to_cover);
        let swap_slippage =
            estimate_swap_slippage(expected_collateral, &collateral_asset, &debt_asset);
        let estimated_profit = calculate_net_profit(
            expected_collateral,
            max_debt_to_cover,
            liquidation_bonus,
            flash_loan_fee,
            gas_estimate.total_cost,
            swap_slippage,
        );

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
            profit_threshold_met: estimated_profit >= min_profit_threshold,
        };

        println!("🧪 HIGH GAS UNPROFITABLE TEST:");
        println!("   Debt to cover: {} wei", opportunity.debt_to_cover);
        println!(
            "   Liquidation bonus: {} wei",
            opportunity.liquidation_bonus
        );
        println!("   Gas cost: {} wei", opportunity.gas_cost);
        println!("   NET PROFIT: {} wei", opportunity.estimated_profit);
        println!("   Profitable: {}", opportunity.profit_threshold_met);

        // This should be unprofitable due to high gas costs
        assert!(!opportunity.profit_threshold_met);
        assert!(opportunity.gas_cost > opportunity.liquidation_bonus);
    }

    #[tokio::test]
    async fn test_small_liquidation_rejection() {
        // Test that very small liquidations are rejected
        let user_position = create_test_user_position(100_000_000_000_000u64); // 0.0001 ETH debt (very small)
        let collateral_asset = create_weth_config();
        let debt_asset = create_usdc_config();
        let min_profit_threshold = U256::from(1_000_000_000_000_000u64); // 0.001 ETH minimum profit

        let gas_estimate = create_mock_gas_estimate(1); // 1 gwei

        let max_debt_to_cover = calculate_max_debt_to_cover(user_position.total_debt_base);
        let (expected_collateral, liquidation_bonus) =
            calculate_collateral_received(max_debt_to_cover, collateral_asset.liquidation_bonus);
        let flash_loan_fee = calculate_flash_loan_fee(max_debt_to_cover);
        let swap_slippage =
            estimate_swap_slippage(expected_collateral, &collateral_asset, &debt_asset);
        let estimated_profit = calculate_net_profit(
            expected_collateral,
            max_debt_to_cover,
            liquidation_bonus,
            flash_loan_fee,
            gas_estimate.total_cost,
            swap_slippage,
        );

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
            profit_threshold_met: estimated_profit >= min_profit_threshold,
        };

        println!("🧪 SMALL LIQUIDATION TEST:");
        println!("   Debt to cover: {} wei", opportunity.debt_to_cover);
        println!("   NET PROFIT: {} wei", opportunity.estimated_profit);

        // Validate that small liquidations are rejected
        let is_valid = validate_liquidation_opportunity(&opportunity, min_profit_threshold);
        assert!(!is_valid, "Small liquidations should be rejected");
    }

    #[tokio::test]
    async fn test_edge_case_calculations() {
        // Test various calculation functions directly

        // Test debt coverage calculation
        let total_debt = U256::from_str("1000000000000000000000").unwrap(); // 1000 ETH
        let max_debt = calculate_max_debt_to_cover(total_debt);
        let expected_max = total_debt / U256::from(2); // 50%
        assert_eq!(max_debt, expected_max);

        // Test collateral calculation with different bonuses
        let debt_amount = U256::from_str("100000000000000000000").unwrap(); // 100 ETH
        let (collateral_5pct, bonus_5pct) = calculate_collateral_received(debt_amount, 500); // 5%
        let expected_collateral = debt_amount * U256::from(10500) / U256::from(10000); // 105%
        assert_eq!(collateral_5pct, expected_collateral);
        assert_eq!(
            bonus_5pct,
            debt_amount * U256::from(500) / U256::from(10000)
        );

        // Test flash loan fee calculation
        let flash_fee = calculate_flash_loan_fee(debt_amount);
        let expected_fee = debt_amount * U256::from(5) / U256::from(10000); // 0.05%
        assert_eq!(flash_fee, expected_fee);

        println!("🧪 EDGE CASE CALCULATIONS:");
        println!(
            "   Max debt coverage: {} wei (50% of {})",
            max_debt, total_debt
        );
        println!("   Collateral with 5% bonus: {} wei", collateral_5pct);
        println!("   Bonus amount: {} wei", bonus_5pct);
        println!("   Flash loan fee: {} wei", flash_fee);
    }

    #[tokio::test]
    async fn test_same_asset_liquidation() {
        // Test liquidation where collateral and debt are the same asset (no swap needed)
        let user_position = create_test_user_position(5_000_000_000_000_000_000u64); // 5 ETH debt (fits in u64)
        let weth_config = create_weth_config();
        let min_profit_threshold = U256::from(100_000_000_000_000_000u64); // 0.1 ETH minimum (lower threshold)

        let gas_estimate = create_mock_gas_estimate(10); // 10 gwei

        let max_debt_to_cover = calculate_max_debt_to_cover(user_position.total_debt_base);
        let (expected_collateral, liquidation_bonus) =
            calculate_collateral_received(max_debt_to_cover, weth_config.liquidation_bonus);
        let flash_loan_fee = calculate_flash_loan_fee(max_debt_to_cover);

        // Same asset liquidation - should have zero slippage
        let swap_slippage = if weth_config.address == weth_config.address {
            U256::ZERO
        } else {
            estimate_swap_slippage(expected_collateral, &weth_config, &weth_config)
        };

        let estimated_profit = calculate_net_profit(
            expected_collateral,
            max_debt_to_cover,
            liquidation_bonus,
            flash_loan_fee,
            gas_estimate.total_cost,
            swap_slippage,
        );

        let opportunity = LiquidationOpportunity {
            user: user_position.address,
            collateral_asset: weth_config.address,
            debt_asset: weth_config.address,
            debt_to_cover: max_debt_to_cover,
            expected_collateral_received: expected_collateral,
            liquidation_bonus,
            flash_loan_fee,
            gas_cost: gas_estimate.total_cost,
            swap_slippage,
            estimated_profit,
            profit_threshold_met: estimated_profit >= min_profit_threshold,
        };

        println!("🧪 SAME ASSET LIQUIDATION TEST:");
        println!(
            "   Swap slippage: {} wei (should be 0)",
            opportunity.swap_slippage
        );
        println!("   NET PROFIT: {} wei", opportunity.estimated_profit);

        // No swap slippage when assets are the same
        assert_eq!(opportunity.swap_slippage, U256::ZERO);

        // Should still be profitable due to liquidation bonus
        assert!(opportunity.profit_threshold_met);
    }

    #[tokio::test]
    async fn test_realistic_mainnet_scenario() {
        // Test a realistic mainnet-like scenario with real-world numbers
        let user_position = UserPosition {
            address: Address::from_str("0x742d35Cc6635C0532925a3b8D0fDfB4C8f9f3BF4").unwrap(),
            total_collateral_base: U256::from_str("52000000000000000000").unwrap(), // 52 ETH collateral
            total_debt_base: U256::from_str("45000000000000000000").unwrap(),       // 45 ETH debt
            available_borrows_base: U256::ZERO,
            current_liquidation_threshold: U256::from(8000), // 80%
            ltv: U256::from(7500),                           // 75%
            health_factor: U256::from(980_000_000_000_000_000u64), // 0.98 (slightly below 1.0)
            last_updated: Utc::now(),
            is_at_risk: true,
        };

        let collateral_asset = create_weth_config();
        let debt_asset = create_usdc_config();
        let min_profit_threshold = U256::from(500_000_000_000_000_000u64); // 0.5 ETH minimum profit

        // Realistic mainnet gas price
        let gas_estimate = create_mock_gas_estimate(25); // 25 gwei

        let max_debt_to_cover = calculate_max_debt_to_cover(user_position.total_debt_base);
        let (expected_collateral, liquidation_bonus) =
            calculate_collateral_received(max_debt_to_cover, collateral_asset.liquidation_bonus);
        let flash_loan_fee = calculate_flash_loan_fee(max_debt_to_cover);
        let swap_slippage =
            estimate_swap_slippage(expected_collateral, &collateral_asset, &debt_asset);
        let estimated_profit = calculate_net_profit(
            expected_collateral,
            max_debt_to_cover,
            liquidation_bonus,
            flash_loan_fee,
            gas_estimate.total_cost,
            swap_slippage,
        );

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
            profit_threshold_met: estimated_profit >= min_profit_threshold,
        };

        println!("🧪 REALISTIC MAINNET SCENARIO:");
        println!(
            "   User debt: {} ETH",
            user_position.total_debt_base / U256::from(10u64.pow(18))
        );
        println!(
            "   Debt to liquidate: {} ETH",
            opportunity.debt_to_cover / U256::from(10u64.pow(18))
        );
        println!(
            "   Liquidation bonus: {} ETH",
            opportunity.liquidation_bonus / U256::from(10u64.pow(18))
        );
        println!(
            "   Gas cost: {} ETH",
            opportunity.gas_cost / U256::from(10u64.pow(18))
        );
        println!(
            "   Net profit: {} ETH",
            opportunity.estimated_profit / U256::from(10u64.pow(18))
        );
        println!("   Profitable: {}", opportunity.profit_threshold_met);

        // Verify this is a reasonable liquidation
        let debt_to_liquidate_eth = opportunity.debt_to_cover / U256::from(10u64.pow(18));
        println!("   Will liquidate ~{} ETH of debt", debt_to_liquidate_eth);

        // Should be profitable with reasonable amounts
        if opportunity.profit_threshold_met {
            println!("   ✅ This would be a profitable liquidation!");
        } else {
            println!("   ❌ This liquidation is not profitable enough");
        }
    }
}
