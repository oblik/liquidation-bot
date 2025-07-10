# Oracle Module Generalization Fix

## Problem Description

The Oracle module had a limitation where only WETH was effectively tracked in the `users_by_collateral` mapping. Although the mechanism existed to monitor other collaterals (USDC, etc.), it was not generalized and relied on hardcoded WETH fallbacks.

## Root Cause

1. **Hardcoded WETH Fallback**: In `src/monitoring/scanner.rs` line 422-437, there was a fallback mechanism that only added users to WETH collateral tracking when the regular collateral mapping failed.

2. **Limited Price Change Handling**: In `src/bot.rs`, the `handle_oracle_price_change` method only triggered health checks for users mapped to the specific asset that changed price, missing users who had other collateral assets.

3. **Missing Asset Configuration**: The scanner functions didn't receive asset configuration information to properly map users to all their collateral assets.

## Changes Made

### 1. Updated Oracle Price Change Handling (`src/bot.rs`)

**Before**:
```rust
// Only checked users with the specific asset that changed
if let Some(users) = self.users_by_collateral.get(&asset_address) {
    // Only trigger checks for users with this specific asset
}
```

**After**:
```rust
// Check all users with ANY collateral since price changes can affect cross-collateral health
let mut users_to_check = HashSet::new();

// Collect users with the specific asset
if let Some(asset_users) = self.users_by_collateral.get(&asset_address) {
    for user in asset_users.iter() {
        users_to_check.insert(*user);
    }
}

// Additionally, collect users from all other tracked collateral assets
for entry in self.users_by_collateral.iter() {
    for user in entry.value().iter() {
        users_to_check.insert(*user);
    }
}
```

### 2. Generalized Collateral Mapping (`src/monitoring/scanner.rs`)

**Before**:
```rust
// Had hardcoded WETH fallback
match "0x4200000000000000000000000000000000000006".parse::<Address>() {
    Ok(weth_address) => {
        users_by_collateral
            .entry(weth_address)
            .or_insert_with(HashSet::new)
            .insert(user);
    }
}
```

**After**:
```rust
// Uses actual asset configuration to map all collateral types
pub async fn update_user_collateral_mapping<P>(
    pool_contract: &ContractInstance<alloy_transport::BoxTransport, Arc<P>>,
    user: Address,
    users_by_collateral: &Arc<DashMap<Address, HashSet<Address>>>,
    asset_configs: Option<&HashMap<Address, AssetConfig>>, // NEW PARAMETER
) -> Result<()>
```

### 3. Enhanced Asset Configuration Support

- **Added AssetConfig parameter** to `update_user_collateral_mapping` function
- **Updated function signatures** to pass asset configurations throughout the call chain
- **Improved logging** to show asset symbols instead of just addresses

### 4. Updated Function Signatures

**Scanner Functions Updated**:
- `update_user_collateral_mapping`: Added `asset_configs` parameter
- `update_user_position`: Added `asset_configs` parameter  
- `run_periodic_scan`: Added `asset_configs` parameter

**Bot Event Processing Updated**:
- Updated call to `scanner::update_user_position` to pass `Some(&self.asset_configs)`

## Benefits

1. **All Collateral Types Tracked**: The system now properly tracks users for ALL configured collateral assets (WETH, USDC, etc.), not just WETH.

2. **Comprehensive Price Change Response**: When any asset price changes, all users with any collateral are checked, ensuring no liquidation opportunities are missed due to cross-collateral effects.

3. **No Hardcoded Asset Preferences**: Removed the hardcoded WETH fallback, making the system truly asset-agnostic.

4. **Better Logging**: Asset symbols are now displayed in logs instead of just addresses, making monitoring more user-friendly.

5. **Scalable Design**: New collateral assets can be added to the asset configuration without code changes to the tracking mechanism.

## Asset Configuration

The system now uses the `AssetConfig` from `oracle::init_asset_configs()` which includes:

```rust
// Current Base mainnet configuration
WETH: 0x4200000000000000000000000000000000000006
USDC: 0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913
```

Adding new assets only requires updating the `init_asset_configs()` function with the new asset's address and Chainlink feed.

## Testing Recommendations

1. **Multi-Asset Price Changes**: Test scenarios where USDC price changes affect users with WETH collateral and vice versa.

2. **Cross-Collateral Liquidations**: Verify that users with multiple collateral types are properly tracked and their health factors are recalculated when any relevant price changes.

3. **New Asset Addition**: Test adding a new asset to the configuration and verifying it gets tracked properly.

## Files Modified

- `src/bot.rs`: Updated `handle_oracle_price_change` method and event processing
- `src/monitoring/scanner.rs`: Generalized collateral mapping functions and removed WETH fallback
- Added proper imports for `AssetConfig` and `HashMap`

The Oracle module is now truly generalized and will track dependencies for all configured collateral types, providing comprehensive event emission on any price change.