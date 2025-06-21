use alloy_primitives::Address;
use std::collections::HashMap;
use crate::models::LiquidationAssetConfig;

/// Initialize asset configurations for Base Sepolia testnet
pub fn init_base_sepolia_assets() -> HashMap<Address, LiquidationAssetConfig> {
    let mut assets = HashMap::new();

    // WETH (Wrapped Ether) - Primary asset
    let weth = LiquidationAssetConfig {
        address: "0x4200000000000000000000000000000000000006".parse().unwrap(),
        symbol: "WETH".to_string(),
        decimals: 18,
        asset_id: 0, // Asset ID for L2Pool encoding
        liquidation_bonus: 500, // 5% liquidation bonus
        is_collateral: true,
        is_borrowable: true,
    };
    assets.insert(weth.address, weth);

    // USDC - Major stablecoin
    let usdc = LiquidationAssetConfig {
        address: "0x036CbD53842c5426634e7929541eC2318f3dCF7e".parse().unwrap(),
        symbol: "USDC".to_string(),
        decimals: 6,
        asset_id: 1,
        liquidation_bonus: 450, // 4.5% liquidation bonus
        is_collateral: true,
        is_borrowable: true,
    };
    assets.insert(usdc.address, usdc);

    // cbETH - Coinbase Wrapped Staked ETH
    let cbeth = LiquidationAssetConfig {
        address: "0x2Ae3F1Ec7F1F5012CFEab0185bfc7aa3cf0DEc22".parse().unwrap(),
        symbol: "cbETH".to_string(),
        decimals: 18,
        asset_id: 2,
        liquidation_bonus: 700, // 7% liquidation bonus (higher risk)
        is_collateral: true,
        is_borrowable: false,
    };
    assets.insert(cbeth.address, cbeth);

    // DAI - Decentralized stablecoin
    let dai = LiquidationAssetConfig {
        address: "0xcf4dA3b4F6e7c1a2bC6e45b0C8b3d9d8e7f2C5B1".parse().unwrap(), // Placeholder - verify actual address
        symbol: "DAI".to_string(),
        decimals: 18,
        asset_id: 3,
        liquidation_bonus: 500, // 5% liquidation bonus
        is_collateral: true,
        is_borrowable: true,
    };
    assets.insert(dai.address, dai);

    assets
}

/// Get asset configuration by address
pub fn get_asset_config(assets: &HashMap<Address, LiquidationAssetConfig>, address: Address) -> Option<&LiquidationAssetConfig> {
    assets.get(&address)
}

/// Get all collateral assets
pub fn get_collateral_assets(assets: &HashMap<Address, LiquidationAssetConfig>) -> Vec<&LiquidationAssetConfig> {
    assets.values().filter(|asset| asset.is_collateral).collect()
}

/// Get all borrowable assets
pub fn get_borrowable_assets(assets: &HashMap<Address, LiquidationAssetConfig>) -> Vec<&LiquidationAssetConfig> {
    assets.values().filter(|asset| asset.is_borrowable).collect()
}

/// Find best liquidation pair for a user's position
pub fn find_best_liquidation_pair(
    assets: &HashMap<Address, LiquidationAssetConfig>,
    user_collateral_assets: &[Address],
    user_debt_assets: &[Address],
) -> Option<(Address, Address)> {
    // Simple strategy: prefer WETH as collateral, USDC as debt
    let weth_addr: Address = "0x4200000000000000000000000000000000000006".parse().unwrap();
    let usdc_addr: Address = "0x036CbD53842c5426634e7929541eC2318f3dCF7e".parse().unwrap();

    // Check if user has WETH collateral and USDC debt
    if user_collateral_assets.contains(&weth_addr) && user_debt_assets.contains(&usdc_addr) {
        return Some((weth_addr, usdc_addr)); // (collateral, debt)
    }

    // Fallback: pick first available collateral and debt
    for &collateral in user_collateral_assets {
        for &debt in user_debt_assets {
            if assets.contains_key(&collateral) && assets.contains_key(&debt) {
                return Some((collateral, debt));
            }
        }
    }

    None
}