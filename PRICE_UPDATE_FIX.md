# Price Update Fix: Proper User Reassessment on Oracle Changes

## Issue Fixed

Previously, Chainlink price updates emitted `PriceUpdate` events but no logic processed these to re-check user health factors. The `users_by_collateral` mapping was not properly populated, so when price changes occurred, affected users were not efficiently identified and reassessed.

## Solution Implemented

### 1. Proper Population of users_by_collateral Mapping

**Added new functions in `src/monitoring/scanner.rs`:**
- `get_user_collateral_assets()`: Calls `getUserConfiguration` and `getReservesList` to determine which assets a user has as collateral
- `update_user_collateral_mapping()`: Updates the `users_by_collateral` mapping for a specific user

### 2. Integration with User Position Updates

**Modified `update_user_position()` in `src/monitoring/scanner.rs`:**
- Replaced the TODO comment with actual implementation
- Now calls `update_user_collateral_mapping()` for every user position update
- Maintains fallback to WETH mapping if blockchain call fails

### 3. Enhanced Initial Discovery

**Updated `discover_initial_users()` in `src/monitoring/discovery.rs`:**
- Now sends `UserPositionChanged` events for ALL discovered users (not just at-risk ones)
- Ensures collateral mappings are populated during initial discovery

**Added `populate_initial_collateral_mapping()` in `src/bot.rs`:**
- Called during bot startup after initial discovery
- Queues all users from database for collateral mapping population
- Added `get_all_users()` function to `src/database.rs` to support this

### 4. Existing Oracle Price Change Logic Enhanced

The existing `handle_oracle_price_change()` function now works properly because:
- `users_by_collateral` mapping is properly populated with actual user collateral assets
- When price changes occur, all users holding that asset as collateral are efficiently identified
- Those users are enqueued for health factor reassessment via `UserPositionChanged` events

## How It Works

1. **At Startup:**
   - Initial discovery finds users and populates database
   - All discovered users get `UserPositionChanged` events sent
   - Each event triggers collateral mapping population

2. **During Operation:**
   - Any user position update populates/updates their collateral mapping
   - Oracle price changes lookup affected users from the mapping
   - Affected users are immediately enqueued for health factor reassessment

3. **On Price Updates:**
   - Chainlink price feed changes trigger `OraclePriceChanged` events
   - Bot looks up users in `users_by_collateral[asset_address]`
   - All affected users get `UserPositionChanged` events for reassessment
   - Health factors are recalculated with new prices

## Key Benefits

- **Efficient**: Only users actually affected by price changes are reassessed
- **Comprehensive**: All assets are properly tracked, not just WETH fallback
- **Real-time**: Price changes immediately trigger user reassessment
- **Robust**: Includes fallback mechanisms and error handling

## Files Modified

- `src/monitoring/scanner.rs`: Added collateral asset detection and mapping functions
- `src/monitoring/discovery.rs`: Enhanced to populate mappings for all users
- `src/bot.rs`: Added startup collateral mapping population
- `src/database.rs`: Added `get_all_users()` function