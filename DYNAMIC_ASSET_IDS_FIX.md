# Dynamic Asset IDs Fix for Aave Liquidation Bot

## Problem Statement

The original implementation had a critical flaw where asset IDs were manually assigned in `src/liquidation/assets.rs`, assuming a fixed reserve index ordering.

**The Issue**: Aave's reserve list can change over time when:
- New assets are added to the protocol
- Assets are removed or deprecated  
- Reserve ordering is modified by governance
- Protocol upgrades change the reserve structure

This would cause liquidations to fail with incorrect asset IDs.

## Solution: Dynamic Reserve Index Fetching

The fix implements dynamic asset ID retrieval using Aave's `UiPoolDataProvider.getReservesList()` function.

### Key Changes

1. **Dynamic Reserve Fetching** (`src/liquidation/assets.rs`)
   - Added `fetch_reserve_indices()` function to query Aave contracts
   - Uses UiPoolDataProvider to get current reserve list ordering
   - Maps asset addresses to their current indices

2. **Asset Configuration Updates**
   - Added `init_base_mainnet_assets_async()` for dynamic initialization  
   - Asset IDs now fetched from live Aave contracts
   - Fallback to hardcoded values if dynamic fetch fails

3. **Bot Integration** (`src/bot.rs`)
   - Bot now calls dynamic asset fetching on startup
   - Graceful fallback with warning if dynamic fetch fails
   - Passes asset configs to liquidation executor

4. **Executor Updates** (`src/liquidation/executor.rs`)
   - Removed hardcoded asset ID mappings
   - Now uses asset configuration lookup for IDs
   - Better error messages for unknown assets

## Benefits

✅ **Automatic Adaptation**: Asset IDs fetched directly from Aave contracts
✅ **Reliability**: Eliminates failed liquidations due to incorrect asset IDs  
✅ **Future-Proof**: Works with any new assets added to Aave
✅ **Robustness**: Includes fallback to hardcoded values if needed

## Usage

On bot startup:
1. Fetches current reserve indices from Aave
2. Creates asset configs with correct dynamic IDs
3. Falls back to hardcoded values if dynamic fetch fails
4. Liquidations use correct asset IDs from configurations

The fix ensures liquidations continue working correctly even when Aave's reserve ordering changes over time.
