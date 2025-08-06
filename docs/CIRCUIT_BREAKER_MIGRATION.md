# Circuit Breaker Migration Guide

## Critical Bug Fix: Failed Liquidation Tracking

### Overview

A critical bug has been identified and fixed in the Circuit Breaker's liquidation tracking system. The deprecated `record_market_data()` method cannot distinguish between failed liquidation attempts and regular market data updates, causing the circuit breaker to miss liquidation floods when liquidations fail.

### The Problem

The `record_market_data()` method has a fundamental design flaw:

```rust
// ⚠️ CRITICAL BUG: When liquidation_occurred is false, we cannot distinguish between:
// 1. A failed liquidation attempt (should count as an attempt)
// 2. A regular market data update (should not count)
liquidation_attempted: liquidation_occurred  // This is WRONG!
```

This causes the circuit breaker to **ignore failed liquidation attempts**, which can lead to:
- **Monitoring Blind Spot**: Failed liquidation floods are not detected
- **Financial Risk**: Bot continues attempting liquidations during network failures, wasting gas
- **Stability Risk**: Circuit breaker fails to activate during high-risk periods

### Migration Path

#### Step 1: Identify Current Usage

Search your codebase for uses of `record_market_data()`:

```bash
grep -r "record_market_data" --include="*.rs"
```

#### Step 2: Replace with Appropriate Methods

Replace each call based on its purpose:

##### For Liquidation Attempts (Successful or Failed)

**Before (INCORRECT):**
```rust
// This doesn't track failed attempts correctly!
circuit_breaker.record_market_data(
    None,
    liquidation_succeeded,  // false for failed attempts is NOT tracked
    Some(gas_price)
).await?;
```

**After (CORRECT):**
```rust
// This correctly tracks ALL liquidation attempts
circuit_breaker.record_liquidation_attempt(
    liquidation_succeeded,  // false for failed attempts IS tracked
    Some(gas_price)
).await?;
```

##### For Price Updates / Market Data

**Before:**
```rust
circuit_breaker.record_market_data(
    Some(new_price),
    false,  // Not a liquidation
    Some(gas_price)
).await?;
```

**After:**
```rust
circuit_breaker.record_price_update(
    Some(new_price),
    Some(gas_price)
).await?;
```

### Complete Migration Examples

#### Example 1: Bot Liquidation Handler

**Before:**
```rust
// Execute liquidation
let result = execute_liquidation(&user).await;
let success = result.is_ok();

// Record the result (BUG: failed attempts not tracked!)
circuit_breaker.record_market_data(None, success, Some(gas_price)).await?;
```

**After:**
```rust
// Execute liquidation
let result = execute_liquidation(&user).await;
let success = result.is_ok();

// Record the attempt (correctly tracks both success and failure)
circuit_breaker.record_liquidation_attempt(success, Some(gas_price)).await?;
```

#### Example 2: Oracle Price Updates

**Before:**
```rust
// On price change from oracle
circuit_breaker.record_market_data(
    Some(new_price),
    false,  // Not a liquidation
    Some(current_gas_price)
).await?;
```

**After:**
```rust
// On price change from oracle
circuit_breaker.record_price_update(
    Some(new_price),
    Some(current_gas_price)
).await?;
```

#### Example 3: Blocked Liquidation Attempts

**Before:**
```rust
if !circuit_breaker.is_liquidation_allowed() {
    // Liquidation blocked
    circuit_breaker.record_blocked_liquidation();
    // No record_market_data call - missing tracking!
    return;
}
```

**After:**
```rust
if !circuit_breaker.is_liquidation_allowed() {
    // Liquidation blocked
    circuit_breaker.record_blocked_liquidation();
    // Also record as a failed attempt for frequency monitoring
    circuit_breaker.record_liquidation_attempt(false, None).await?;
    return;
}
```

### Testing Your Migration

After migrating, verify the fix with these test scenarios:

#### Test 1: Failed Liquidation Flood Detection

```rust
#[tokio::test]
async fn test_failed_liquidation_flood_detection() {
    // Simulate multiple failed liquidation attempts
    for _ in 0..10 {
        circuit_breaker.record_liquidation_attempt(false, None).await?;
    }
    
    // Circuit breaker SHOULD activate
    assert_eq!(circuit_breaker.get_state(), CircuitBreakerState::Open);
}
```

#### Test 2: Price Updates Don't Trigger False Positives

```rust
#[tokio::test]
async fn test_price_updates_dont_trigger() {
    // Simulate multiple price updates
    for i in 0..20 {
        let price = base_price + (i * 100);
        circuit_breaker.record_price_update(Some(price), None).await?;
    }
    
    // Circuit breaker should NOT activate (no liquidations)
    assert_eq!(circuit_breaker.get_state(), CircuitBreakerState::Closed);
}
```

### API Reference

#### New Methods

##### `record_liquidation_attempt()`
```rust
pub async fn record_liquidation_attempt(
    &self,
    liquidation_succeeded: bool,  // true = successful, false = failed
    gas_price_wei: Option<U256>,
) -> Result<()>
```
Use for ANY liquidation attempt, whether successful or failed.

##### `record_price_update()`
```rust
pub async fn record_price_update(
    &self,
    price: Option<U256>,
    gas_price_wei: Option<U256>,
) -> Result<()>
```
Use for price updates and non-liquidation market data.

#### Deprecated Method

##### `record_market_data()` ⚠️ DEPRECATED
```rust
#[deprecated(since = "1.1.0")]
pub async fn record_market_data(
    &self,
    price: Option<U256>,
    liquidation_occurred: bool,  // CANNOT track failed attempts!
    gas_price_wei: Option<U256>,
) -> Result<()>
```
**DO NOT USE** - Cannot properly track failed liquidation attempts.

### Monitoring Impact

After migration, you should observe:

1. **More Accurate Liquidation Counts**: The circuit breaker will now count ALL liquidation attempts, not just successful ones.

2. **Better Network Failure Detection**: Failed liquidation floods during network congestion will now trigger the circuit breaker.

3. **Cleaner Separation of Concerns**: Price updates and liquidation attempts are now clearly separated.

### Timeline

- **v1.1.0**: New methods introduced, `record_market_data()` deprecated
- **v1.x**: Deprecation warnings active
- **v2.0.0**: `record_market_data()` will be removed

### Support

If you encounter issues during migration:

1. Check the test suite for examples
2. Review the inline documentation
3. Run the provided test scenarios to verify correct behavior

### Summary Checklist

- [ ] Identified all uses of `record_market_data()`
- [ ] Replaced liquidation tracking with `record_liquidation_attempt()`
- [ ] Replaced price updates with `record_price_update()`
- [ ] Added tracking for blocked liquidation attempts
- [ ] Tested failed liquidation flood detection
- [ ] Verified price updates don't cause false positives
- [ ] Updated any documentation or comments
- [ ] Deployed and monitored the fix

### Critical Reminder

**This is a CRITICAL safety fix**. The old API silently fails to track failed liquidation attempts, which can lead to financial losses during network congestion or gas spikes. Migrate as soon as possible to ensure your bot's circuit breaker functions correctly under all conditions.