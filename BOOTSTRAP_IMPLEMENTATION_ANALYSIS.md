# Bootstrap Implementation Analysis

## Problem Summary

The Aave liquidation bot was experiencing a "cold start" problem where it would successfully initialize but fail to detect any liquidation opportunities. The logs consistently showed:
- "0 positions tracked, 0 at risk, 0 liquidatable"
- "No users mapped to USDC/WETH collateral yet"
- "Checking 0 at-risk users from database due to price change"

The root cause was that the bot was designed to be purely event-driven without an initial bootstrap mechanism to discover existing protocol users.

## Bootstrap Solution Architecture

### 1. Bootstrap Module (`src/bootstrap.rs`)

**Core Components:**
- `BootstrapConfig` struct with configurable parameters
- `discover_users_from_events()` function for historical event scanning
- `bootstrap_user_positions()` function for health status checking
- Event signature-based user extraction from Aave Pool logs

**Key Features:**
- Configurable scanning parameters (blocks, batch size, rate limits)
- Scans 100,000 blocks by default in batches of 2,000
- 1-second rate limiting delays to prevent RPC throttling
- Discovers up to 500 users initially
- Progress reporting during scan operations

**Event Signatures Monitored:**
```solidity
Supply(address indexed reserve, address user, address indexed onBehalfOf, uint256 amount, uint16 indexed referralCode)
Borrow(address indexed reserve, address user, address indexed onBehalfOf, uint256 amount, uint8 interestRateMode, uint256 borrowRate, uint16 indexed referralCode)
Repay(address indexed reserve, address indexed user, address indexed repayer, uint256 amount, bool useATokens)
Withdraw(address indexed reserve, address indexed user, address indexed to, uint256 amount)
LiquidationCall(address indexed collateralAsset, address indexed debtAsset, address indexed user, uint256 debtToCover, uint256 liquidatedCollateralAmount, address liquidator, bool receiveAToken)
```

### 2. Database Enhancements (`src/database.rs`)

**New Functions Added:**
- `get_all_tracked_users()` - Retrieves all users currently tracked in database
- `add_user_to_track()` - Adds discovered users to the tracking database

**Database Schema:**
The existing `user_positions` table supports the bootstrap system with fields for:
- User address
- Collateral and debt amounts
- Health factor tracking
- At-risk status flags
- Last updated timestamps

### 3. Configuration Intelligence (`src/config.rs`)

**Pool Address Auto-Detection:**
- Base Sepolia: `0xA37D7E3d3CaD89b44f9a08A96fE01a9F39Bd7794`
- Base Mainnet: `0xA238Dd80C259a72e81d7e4664a9801593F98d1c5`
- Intelligent detection based on RPC URL patterns
- Manual override via `POOL_ADDRESS` environment variable

**Configuration Structure:**
```rust
pub struct BotConfig {
    pub pool_address: Address,  // New field added
    // ... other existing fields
}
```

### 4. Bot Integration (`src/bot.rs`)

**Bootstrap Integration Points:**
- `bootstrap_users()` method added to main bot implementation
- Integrated into `run()` method with database emptiness check
- In-memory position tracking updates after bootstrap
- Collateral mapping for affected user tracking

**Bootstrap Flow:**
1. Check if database contains existing users
2. If empty, trigger bootstrap process
3. Discover users from historical events
4. Check current health status of discovered users
5. Store active positions in database
6. Update in-memory tracking structures

### 5. WebSocket Monitoring Updates (`src/monitoring/websocket.rs`)

**Configurability Improvements:**
- `start_event_monitoring()` now accepts `pool_address` parameter
- Removed hardcoded pool addresses
- Uses configured pool address from bot initialization

## Technical Implementation Details

### User Discovery Process

1. **Historical Event Scanning:**
   - Scans blocks in configurable batches
   - Applies rate limiting to prevent RPC throttling
   - Extracts user addresses from event log topics
   - Filters out zero addresses and invalid entries

2. **Health Status Verification:**
   - Calls `getUserAccountData()` for each discovered user
   - Parses returned data (6 uint256 values)
   - Calculates at-risk status (health factor < 1.1)
   - Stores only users with active positions (debt or collateral > 0)

3. **Database Population:**
   - Inserts discovered users into tracking database
   - Updates position data with current health metrics
   - Enables ongoing monitoring of discovered users

### Error Handling & Resilience

- Graceful handling of RPC failures during event scanning
- Continues processing if individual user checks fail
- Comprehensive logging for troubleshooting
- Rate limiting to prevent service disruption

## Environment & Compatibility

**Rust Toolchain:**
- Current version: Rust 1.87.0 (17067e9ac 2025-05-09)
- Resolved `base64ct v1.8.0` dependency conflicts requiring `edition2024` feature
- Updated from Rust 1.82.0 to resolve compatibility issues

**Dependencies:**
- Alloy v0.5.4 for Ethereum interactions
- SQLx v0.7 for database operations
- Tokio for async runtime
- Comprehensive error handling with `eyre`

## Expected Outcome

After implementing the bootstrap system, the bot should:

1. **Discovery Phase:**
   - Scan historical Aave events to find existing users
   - Discover hundreds of active protocol participants
   - Log progress during discovery process

2. **Health Assessment:**
   - Check current positions of discovered users
   - Identify at-risk users (health factor < 1.1)
   - Store active positions in database

3. **Ongoing Monitoring:**
   - Monitor discovered users for liquidation opportunities
   - Display actual user counts instead of zeros
   - Respond to price changes affecting tracked users

## Bootstrap Configuration

**Default Settings:**
```rust
BootstrapConfig {
    blocks_to_scan: 100000,      // ~14 days of Base blocks
    batch_size: 2000,            // 2000 blocks per batch
    rate_limit_delay_ms: 1000,   // 1 second between batches
    max_users_to_discover: 500,  // Initial user limit
}
```

**Environment Variables:**
- `POOL_ADDRESS` - Manual pool address override
- `DATABASE_URL` - SQLite database location
- `RPC_URL` - Used for network auto-detection

## Success Metrics

The bootstrap implementation addresses the core issue by transforming the bot from:
- **Before:** Purely reactive system with no initial data
- **After:** Proactive system that discovers and monitors existing users

**Expected Log Improvements:**
- "Found 250 unique users" instead of "0 positions tracked"
- "45 users at risk" instead of "0 at risk"
- "Checking 120 affected users" instead of "No users mapped to collateral"

This comprehensive bootstrap system ensures the liquidation bot can immediately begin monitoring real protocol participants rather than waiting indefinitely for new events to discover users.