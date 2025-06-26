use crate::models::LiquidationAssetConfig;
use alloy_primitives::Address;
use std::collections::HashMap;

/// Initialize asset configurations for Base mainnet
pub fn init_base_mainnet_assets() -> HashMap<Address, LiquidationAssetConfig> {
    let mut assets = HashMap::new();

    // WETH (Wrapped Ether) - Base mainnet
    let weth = LiquidationAssetConfig {
        address: "0x4200000000000000000000000000000000000006"
            .parse()
            .unwrap(),
        symbol: "WETH".to_string(),
        decimals: 18,
        asset_id: 0,            // Asset ID for L2Pool encoding
        liquidation_bonus: 500, // 5% liquidation bonus
        is_collateral: true,
        is_borrowable: true,
    };
    assets.insert(weth.address, weth);

    // USDC - Base mainnet
    let usdc = LiquidationAssetConfig {
        address: "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913"
            .parse()
            .unwrap(),
        symbol: "USDC".to_string(),
        decimals: 6,
        asset_id: 1,
        liquidation_bonus: 450, // 4.5% liquidation bonus
        is_collateral: true,
        is_borrowable: true,
    };
    assets.insert(usdc.address, usdc);

    // cbETH - Coinbase Wrapped Staked ETH - Base mainnet
    let cbeth = LiquidationAssetConfig {
        address: "0x2Ae3F1Ec7F1F5012CFEab0185bfc7aa3cf0DEc22"
            .parse()
            .unwrap(),
        symbol: "cbETH".to_string(),
        decimals: 18,
        asset_id: 2,
        liquidation_bonus: 700, // 7% liquidation bonus (higher risk)
        is_collateral: true,
        is_borrowable: false,
    };
    assets.insert(cbeth.address, cbeth);

    assets
}

/// Get asset configuration by address
pub fn get_asset_config(
    assets: &HashMap<Address, LiquidationAssetConfig>,
    address: Address,
) -> Option<&LiquidationAssetConfig> {
    assets.get(&address)
}

/// Get all collateral assets
pub fn get_collateral_assets(
    assets: &HashMap<Address, LiquidationAssetConfig>,
) -> Vec<&LiquidationAssetConfig> {
    assets
        .values()
        .filter(|asset| asset.is_collateral)
        .collect()
}

/// Get all borrowable assets
pub fn get_borrowable_assets(
    assets: &HashMap<Address, LiquidationAssetConfig>,
) -> Vec<&LiquidationAssetConfig> {
    assets
        .values()
        .filter(|asset| asset.is_borrowable)
        .collect()
}

/// Find best liquidation pair for a user's position based on profitability analysis
pub fn find_best_liquidation_pair(
    assets: &HashMap<Address, LiquidationAssetConfig>,
    user_collateral_assets: &[Address],
    user_debt_assets: &[Address],
) -> Option<(Address, Address)> {
    if user_collateral_assets.is_empty() || user_debt_assets.is_empty() {
        return None;
    }

    let mut best_pair: Option<(Address, Address)> = None;
    let mut best_score = 0u32;

    // Evaluate all possible collateral/debt combinations
    for &collateral_addr in user_collateral_assets {
        for &debt_addr in user_debt_assets {
            // Skip if assets not configured
            let collateral_config = match assets.get(&collateral_addr) {
                Some(config) => config,
                None => continue,
            };
            let debt_config = match assets.get(&debt_addr) {
                Some(config) => config,
                None => continue,
            };

            // Skip if collateral can't be used as collateral or debt can't be borrowed
            if !collateral_config.is_collateral || !debt_config.is_borrowable {
                continue;
            }

            // Calculate profitability score for this pair
            let score = calculate_liquidation_pair_score(collateral_config, debt_config);

            if score > best_score {
                best_score = score;
                best_pair = Some((collateral_addr, debt_addr));
            }
        }
    }

    best_pair
}

/// Calculate a score for a collateral/debt pair to determine profitability
/// Higher score indicates more profitable liquidation
fn calculate_liquidation_pair_score(
    collateral_config: &LiquidationAssetConfig,
    debt_config: &LiquidationAssetConfig,
) -> u32 {
    let mut score = 0u32;

    // Primary factor: liquidation bonus (higher is better)
    score += collateral_config.liquidation_bonus as u32;

    // Bonus for same-asset liquidations (no swap needed, lower gas, no slippage)
    if collateral_config.address == debt_config.address {
        score += 200; // Significant bonus for same-asset liquidations
    }

    // Bonus for liquid asset pairs (prioritize major assets)
    // Higher decimals generally indicate more liquid/established assets
    if collateral_config.decimals == 18 && debt_config.decimals >= 6 {
        score += 50; // Bonus for standard token pairs
    }

    // Bonus for stablecoin debt (easier to handle)
    if is_stablecoin(&debt_config.symbol) {
        score += 30;
    }

    // Bonus for major collateral assets (ETH, WETH, cbETH)
    if is_major_collateral(&collateral_config.symbol) {
        score += 20;
    }

    score
}

/// Helper function to identify stablecoin assets
fn is_stablecoin(symbol: &str) -> bool {
    matches!(symbol, "USDC" | "USDT" | "DAI" | "BUSD" | "FRAX")
}

/// Helper function to identify major collateral assets
fn is_major_collateral(symbol: &str) -> bool {
    matches!(symbol, "ETH" | "WETH" | "cbETH" | "stETH" | "rETH")
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::Address;
    use std::str::FromStr;

    fn create_test_assets() -> HashMap<Address, LiquidationAssetConfig> {
        let mut assets = HashMap::new();

        // WETH - moderate bonus
        let weth = LiquidationAssetConfig {
            address: Address::from_str("0x4200000000000000000000000000000000000006").unwrap(),
            symbol: "WETH".to_string(),
            decimals: 18,
            asset_id: 0,
            liquidation_bonus: 500, // 5%
            is_collateral: true,
            is_borrowable: true,
        };
        assets.insert(weth.address, weth);

        // USDC - lower bonus
        let usdc = LiquidationAssetConfig {
            address: Address::from_str("0x036CbD53842c5426634e7929541eC2318f3dCF7e").unwrap(),
            symbol: "USDC".to_string(),
            decimals: 6,
            asset_id: 1,
            liquidation_bonus: 450, // 4.5%
            is_collateral: true,
            is_borrowable: true,
        };
        assets.insert(usdc.address, usdc);

        // cbETH - highest bonus
        let cbeth = LiquidationAssetConfig {
            address: Address::from_str("0x2Ae3F1Ec7F1F5012CFEab0185bfc7aa3cf0DEc22").unwrap(),
            symbol: "cbETH".to_string(),
            decimals: 18,
            asset_id: 2,
            liquidation_bonus: 700, // 7% - highest bonus
            is_collateral: true,
            is_borrowable: false,
        };
        assets.insert(cbeth.address, cbeth);

        // DAI - same bonus as WETH but DISABLED (not available on Base Sepolia)
        let dai = LiquidationAssetConfig {
            address: Address::from_str("0xcf4dA3b4F6e7c1a2bC6e45b0C8b3d9d8e7f2C5B1").unwrap(),
            symbol: "DAI".to_string(),
            decimals: 18,
            asset_id: 3,
            liquidation_bonus: 500, // 5%
            is_collateral: false,   // DISABLED: DAI not supported on Base Sepolia testnet
            is_borrowable: false,   // DISABLED: DAI not supported on Base Sepolia testnet
        };
        assets.insert(dai.address, dai);

        assets
    }

    #[test]
    fn test_dynamic_best_pair_selection() {
        let assets = create_test_assets();

        // Test case 1: Should select cbETH as collateral due to highest bonus
        let weth_addr = Address::from_str("0x4200000000000000000000000000000000000006").unwrap();
        let usdc_addr = Address::from_str("0x036CbD53842c5426634e7929541eC2318f3dCF7e").unwrap();
        let cbeth_addr = Address::from_str("0x2Ae3F1Ec7F1F5012CFEab0185bfc7aa3cf0DEc22").unwrap();
        let dai_addr = Address::from_str("0xcf4dA3b4F6e7c1a2bC6e45b0C8b3d9d8e7f2C5B1").unwrap();

        let user_collateral = vec![weth_addr, cbeth_addr];
        let user_debt = vec![usdc_addr, dai_addr];

        let result = find_best_liquidation_pair(&assets, &user_collateral, &user_debt);

        // Should pick cbETH as collateral (highest bonus: 700)
        // Debt asset could be either USDC or DAI (both stablecoins with similar scores)
        assert!(result.is_some());
        let (collateral, _debt) = result.unwrap();
        assert_eq!(collateral, cbeth_addr);

        // Test case 2: Verify it doesn't hardcode WETH/USDC preference
        // If only WETH collateral available, should still work
        let user_collateral_weth_only = vec![weth_addr];
        let user_debt_usdc_only = vec![usdc_addr];

        let result_weth =
            find_best_liquidation_pair(&assets, &user_collateral_weth_only, &user_debt_usdc_only);
        assert_eq!(result_weth, Some((weth_addr, usdc_addr)));
    }

    #[test]
    fn test_same_asset_liquidation_preference() {
        let assets = create_test_assets();

        let weth_addr = Address::from_str("0x4200000000000000000000000000000000000006").unwrap();
        let usdc_addr = Address::from_str("0x036CbD53842c5426634e7929541eC2318f3dCF7e").unwrap();

        // User has WETH as both collateral and debt (same asset liquidation)
        let user_collateral = vec![weth_addr, usdc_addr];
        let user_debt = vec![weth_addr, usdc_addr];

        let result = find_best_liquidation_pair(&assets, &user_collateral, &user_debt);

        // Should prefer WETH/WETH due to same-asset bonus (200 points)
        // WETH same-asset score: 500 (bonus) + 200 (same-asset) + 50 (decimals) + 20 (major collateral) = 770
        // USDC same-asset score: 450 (bonus) + 200 (same-asset) + 30 (stablecoin) = 680
        assert_eq!(result, Some((weth_addr, weth_addr)));
    }

    #[test]
    fn test_no_valid_pairs() {
        let assets = create_test_assets();

        // Test with assets not in configuration
        let unknown_addr = Address::from_str("0x1111111111111111111111111111111111111111").unwrap();
        let user_collateral = vec![unknown_addr];
        let user_debt = vec![unknown_addr];

        let result = find_best_liquidation_pair(&assets, &user_collateral, &user_debt);
        assert_eq!(result, None);
    }

    #[test]
    fn test_empty_asset_lists() {
        let assets = create_test_assets();

        // Test with empty lists
        let result = find_best_liquidation_pair(&assets, &[], &[]);
        assert_eq!(result, None);

        let weth_addr = Address::from_str("0x4200000000000000000000000000000000000006").unwrap();

        // Test with empty collateral
        let result = find_best_liquidation_pair(&assets, &[], &[weth_addr]);
        assert_eq!(result, None);

        // Test with empty debt
        let result = find_best_liquidation_pair(&assets, &[weth_addr], &[]);
        assert_eq!(result, None);
    }

    #[test]
    fn test_asset_constraints() {
        let mut assets = create_test_assets();

        // Make cbETH not usable as collateral
        let cbeth_addr = Address::from_str("0x2Ae3F1Ec7F1F5012CFEab0185bfc7aa3cf0DEc22").unwrap();
        if let Some(cbeth_config) = assets.get_mut(&cbeth_addr) {
            cbeth_config.is_collateral = false;
        }

        let weth_addr = Address::from_str("0x4200000000000000000000000000000000000006").unwrap();
        let usdc_addr = Address::from_str("0x036CbD53842c5426634e7929541eC2318f3dCF7e").unwrap();

        let user_collateral = vec![weth_addr, cbeth_addr];
        let user_debt = vec![usdc_addr];

        let result = find_best_liquidation_pair(&assets, &user_collateral, &user_debt);

        // Should select WETH/USDC since cbETH can't be used as collateral
        assert_eq!(result, Some((weth_addr, usdc_addr)));
    }

    #[test]
    fn test_no_hardcoded_weth_usdc_preference() {
        let assets = create_test_assets();
        
        // This test specifically verifies the bug fix: the algorithm should NOT
        // hardcode WETH/USDC preference when better options are available
        
        let weth_addr = Address::from_str("0x4200000000000000000000000000000000000006").unwrap();
        let usdc_addr = Address::from_str("0x036CbD53842c5426634e7929541eC2318f3dCF7e").unwrap();
        let cbeth_addr = Address::from_str("0x2Ae3F1Ec7F1F5012CFEab0185bfc7aa3cf0DEc22").unwrap();
        let dai_addr = Address::from_str("0xcf4dA3b4F6e7c1a2bC6e45b0C8b3d9d8e7f2C5B1").unwrap();

        // Scenario: User has both WETH/USDC AND cbETH/DAI available
        // Old buggy code would always prefer WETH/USDC
        // New code should prefer cbETH due to higher liquidation bonus (7% vs 5%)
        let user_collateral = vec![weth_addr, cbeth_addr];
        let user_debt = vec![usdc_addr, dai_addr];

        let result = find_best_liquidation_pair(&assets, &user_collateral, &user_debt);
        
        // Should NOT be WETH/USDC due to the hardcoded preference
        // Should be cbETH/X due to higher bonus
        assert!(result.is_some());
        let (collateral, _debt) = result.unwrap();
        
        // The key assertion: should choose cbETH (better bonus) over WETH
        assert_eq!(collateral, cbeth_addr, 
            "Algorithm should choose cbETH (7% bonus) over WETH (5% bonus), not hardcode WETH preference");
        
        // Additional verification: if we remove cbETH, it should fall back to WETH
        let user_collateral_no_cbeth = vec![weth_addr];
        let result_fallback = find_best_liquidation_pair(&assets, &user_collateral_no_cbeth, &user_debt);
        assert!(result_fallback.is_some());
        let (fallback_collateral, _) = result_fallback.unwrap();
        assert_eq!(fallback_collateral, weth_addr);
    }

    #[test]
    fn test_scoring_algorithm() {
        // Test individual scoring components
        let high_bonus_asset = LiquidationAssetConfig {
            address: Address::from_str("0x1111111111111111111111111111111111111111").unwrap(),
            symbol: "HIGH".to_string(),
            decimals: 18,
            asset_id: 0,
            liquidation_bonus: 800,
            is_collateral: true,
            is_borrowable: true,
        };

        let low_bonus_asset = LiquidationAssetConfig {
            address: Address::from_str("0x2222222222222222222222222222222222222222").unwrap(),
            symbol: "LOW".to_string(),
            decimals: 6,
            asset_id: 1,
            liquidation_bonus: 300,
            is_collateral: true,
            is_borrowable: true,
        };

        let high_score = calculate_liquidation_pair_score(&high_bonus_asset, &low_bonus_asset);
        let low_score = calculate_liquidation_pair_score(&low_bonus_asset, &high_bonus_asset);

        // High bonus should result in higher score
        assert!(high_score > low_score);
    }
}
