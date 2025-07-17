use alloy_primitives::{Address, U256};
use chrono::Utc;
use liquidation_bot::models::{
    GasEstimate, LiquidationAssetConfig, LiquidationOpportunity, UserPosition,
};
use std::str::FromStr;

fn create_weth_config() -> LiquidationAssetConfig {
    LiquidationAssetConfig {
        address: Address::from_str("0x4200000000000000000000000000000000000006").unwrap(),
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
        address: Address::from_str("0x036CbD53842c5426634e7929541eC2318f3dCF7e").unwrap(),
        symbol: "USDC".to_string(),
        decimals: 6,
        asset_id: 1,
        liquidation_bonus: 450, // 4.5% liquidation bonus
        is_collateral: true,
        is_borrowable: true,
    }
}

fn create_gas_estimate(gas_price_gwei: u64) -> GasEstimate {
    let gas_price = U256::from(gas_price_gwei) * U256::from(1_000_000_000u64);
    let gas_limit = U256::from(800_000u64);
    let priority_fee = gas_price * U256::from(20) / U256::from(100);
    let total_cost = gas_limit * (gas_price + priority_fee);

    GasEstimate {
        base_fee: gas_price,
        priority_fee,
        gas_limit,
        total_cost,
    }
}

fn simulate_liquidation(
    user_position: &UserPosition,
    collateral_asset: &LiquidationAssetConfig,
    debt_asset: &LiquidationAssetConfig,
    gas_estimate: &GasEstimate,
    min_profit_threshold: U256,
) -> LiquidationOpportunity {
    // Calculate maximum liquidation amount (50% of debt)
    let max_debt_to_cover = user_position.total_debt_base * U256::from(5000) / U256::from(10000);

    // Calculate expected collateral received with liquidation bonus
    let bonus_multiplier = U256::from(10000 + collateral_asset.liquidation_bonus);
    let expected_collateral = max_debt_to_cover * bonus_multiplier / U256::from(10000);
    let liquidation_bonus = expected_collateral - max_debt_to_cover;

    // Calculate flash loan fee (0.05%)
    let flash_loan_fee = max_debt_to_cover * U256::from(5) / U256::from(10000);

    // Estimate swap slippage (1% if different assets)
    let swap_slippage = if collateral_asset.address != debt_asset.address {
        expected_collateral * U256::from(100) / U256::from(10000)
    } else {
        U256::ZERO
    };

    // Calculate net profit
    let total_costs = flash_loan_fee + gas_estimate.total_cost + swap_slippage;
    let estimated_profit = if liquidation_bonus > total_costs {
        liquidation_bonus - total_costs
    } else {
        U256::ZERO
    };

    let profit_threshold_met = estimated_profit >= min_profit_threshold;

    LiquidationOpportunity {
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
    }
}

fn main() -> eyre::Result<()> {
    println!("🧪 Liquidation Bot - Profitability Testing");
    println!("==========================================\n");

    // Scenario 1: Profitable liquidation
    println!("📊 SCENARIO 1: Profitable Liquidation (Low Gas)");
    println!("------------------------------------------------");

    let user_position = UserPosition {
        address: Address::from_str("0x742d35Cc6635C0532925a3b8D0fDfB4C8f9f3BF4")?,
        total_collateral_base: U256::from_str("120000000000000000000")?, // 120 ETH collateral
        total_debt_base: U256::from_str("100000000000000000000")?,       // 100 ETH debt
        available_borrows_base: U256::ZERO,
        current_liquidation_threshold: U256::from(8000), // 80%
        ltv: U256::from(7500),                           // 75%
        health_factor: U256::from(960_000_000_000_000_000u64), // 0.96 (below 1.0)
        last_updated: Utc::now(),
        is_at_risk: true,
    };

    let weth = create_weth_config();
    let usdc = create_usdc_config();
    let gas_estimate = create_gas_estimate(10); // 10 gwei
    let min_profit = U256::from_str("10000000000000000")?; // 0.01 ETH minimum

    let opportunity = simulate_liquidation(&user_position, &weth, &usdc, &gas_estimate, min_profit);

    println!("User Position:");
    println!(
        "  💰 Total Collateral: {} ETH",
        user_position.total_collateral_base / U256::from(10u64.pow(18))
    );
    println!(
        "  💸 Total Debt: {} ETH",
        user_position.total_debt_base / U256::from(10u64.pow(18))
    );
    println!(
        "  ⚡ Health Factor: {:.3}",
        (user_position.health_factor / U256::from(10u64.pow(15))).saturating_to::<u64>() as f64
            / 1000.0
    );
    println!();

    println!("Liquidation Analysis:");
    println!(
        "  🎯 Debt to Cover: {} ETH",
        opportunity.debt_to_cover / U256::from(10u64.pow(18))
    );
    println!(
        "  💎 Collateral Received: {} ETH",
        opportunity.expected_collateral_received / U256::from(10u64.pow(18))
    );
    println!(
        "  🎁 Liquidation Bonus: {} ETH",
        opportunity.liquidation_bonus / U256::from(10u64.pow(18))
    );
    println!(
        "  🏦 Flash Loan Fee: {} ETH",
        opportunity.flash_loan_fee / U256::from(10u64.pow(18))
    );
    println!(
        "  ⛽ Gas Cost: {} ETH",
        opportunity.gas_cost / U256::from(10u64.pow(18))
    );
    println!(
        "  💹 Swap Slippage: {} ETH",
        opportunity.swap_slippage / U256::from(10u64.pow(18))
    );
    println!(
        "  💰 NET PROFIT: {} ETH",
        opportunity.estimated_profit / U256::from(10u64.pow(18))
    );
    println!("  ✅ Profitable: {}", opportunity.profit_threshold_met);
    println!();

    // Scenario 2: Unprofitable due to high gas
    println!("📊 SCENARIO 2: Unprofitable Liquidation (High Gas)");
    println!("---------------------------------------------------");

    // Use MUCH higher gas to actually make it unprofitable
    let extreme_gas_estimate = create_gas_estimate(1000); // 1000 gwei (extreme)
    let opportunity2 = simulate_liquidation(
        &user_position,
        &weth,
        &usdc,
        &extreme_gas_estimate,
        min_profit,
    );

    println!("Same user position, but with 1000 gwei gas price:");

    // Fix gas cost display - use more precise formatting
    let gas_cost_eth = opportunity2.gas_cost.saturating_to::<u128>() as f64 / 1e18;
    let net_profit_eth = opportunity2.estimated_profit.saturating_to::<u128>() as f64 / 1e18;
    let liquidation_bonus_eth =
        opportunity2.liquidation_bonus.saturating_to::<u128>() as f64 / 1e18;

    println!("  🎁 Liquidation Bonus: {:.4} ETH", liquidation_bonus_eth);
    println!("  ⛽ Gas Cost: {:.4} ETH", gas_cost_eth);
    println!("  💰 NET PROFIT: {:.4} ETH", net_profit_eth);

    // Fix the contradictory emoji/message
    println!(
        "  {} Profitable: {}",
        if opportunity2.profit_threshold_met {
            "✅"
        } else {
            "❌"
        },
        opportunity2.profit_threshold_met
    );

    if !opportunity2.profit_threshold_met {
        println!("  🚫 Bot would SKIP this liquidation (gas too expensive)");
    }
    println!();

    // Scenario 3: Real world example
    println!("📊 SCENARIO 3: Realistic Mainnet Example");
    println!("----------------------------------------");

    let realistic_position = UserPosition {
        address: Address::from_str("0x742d35Cc6635C0532925a3b8D0fDfB4C8f9f3BF4")?,
        total_collateral_base: U256::from_str("52000000000000000000")?, // 52 ETH
        total_debt_base: U256::from_str("45000000000000000000")?,       // 45 ETH
        available_borrows_base: U256::ZERO,
        current_liquidation_threshold: U256::from(8000),
        ltv: U256::from(7500),
        health_factor: U256::from(980_000_000_000_000_000u64), // 0.98
        last_updated: Utc::now(),
        is_at_risk: true,
    };

    let realistic_gas = create_gas_estimate(25); // 25 gwei
    let lower_threshold = U256::from_str("5000000000000000")?; // 0.005 ETH minimum

    let opportunity3 = simulate_liquidation(
        &realistic_position,
        &weth,
        &usdc,
        &realistic_gas,
        lower_threshold,
    );

    println!("Realistic mainnet scenario:");
    println!(
        "  💰 User Debt: {} ETH",
        realistic_position.total_debt_base / U256::from(10u64.pow(18))
    );
    println!(
        "  🎯 Will Liquidate: {} ETH",
        opportunity3.debt_to_cover / U256::from(10u64.pow(18))
    );
    println!(
        "  💰 Expected Profit: {} ETH",
        opportunity3.estimated_profit / U256::from(10u64.pow(18))
    );
    println!(
        "  {} Outcome: {}",
        if opportunity3.profit_threshold_met {
            "✅"
        } else {
            "❌"
        },
        if opportunity3.profit_threshold_met {
            "Would execute this liquidation!"
        } else {
            "Would skip this liquidation"
        }
    );

    println!("\n🎯 Summary: The profitability calculation considers:");
    println!("   • Liquidation bonus (profit from protocol)");
    println!("   • Flash loan fees (Aave charges 0.05%)");
    println!("   • Gas costs (varies with network congestion)");
    println!("   • DEX slippage (when swapping assets)");
    println!("   • Minimum profit thresholds");

    println!("\n📝 To run this test: cargo run --bin test_liquidation");

    Ok(())
}
