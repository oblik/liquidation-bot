# Circuit Breaker for Extreme Market Conditions

The liquidation bot now includes a sophisticated circuit breaker system designed to automatically halt operations during extreme market conditions, preventing erratic behavior and excessive gas spending during black-swan events.

## Overview

The circuit breaker monitors three key market conditions:
1. **Price Volatility** - Detects sudden price crashes or spikes
2. **Liquidation Frequency** - Identifies flooding of liquidations in short time periods  
3. **Gas Price Spikes** - Monitors for extremely high gas costs

When extreme conditions are detected, the circuit breaker automatically transitions through different states to protect the bot from dangerous market conditions.

## Circuit Breaker States

### 🟢 Closed (Normal Operation)
- All liquidations are allowed
- Continuous monitoring of market conditions
- Default state when conditions are normal

### 🔴 Open (Emergency Mode)
- **ALL liquidations are blocked**
- Triggered when extreme conditions are detected
- Automatic transition to half-open after cooldown period
- Prevents erratic behavior during market chaos

### 🟡 Half-Open (Testing Recovery)
- Limited operations allowed to test if conditions have improved
- Allows one test liquidation every 30 seconds
- Either transitions back to closed (if conditions improve) or open (if still extreme)

### ⚪ Disabled (Manual Override)
- Circuit breaker functionality is completely disabled
- Use only in emergency situations where manual control is required
- No liquidations are allowed when disabled

## Configuration

Add these environment variables to configure the circuit breaker:

```bash
# Enable/disable circuit breaker (default: false)
CIRCUIT_BREAKER_ENABLED=true

# Price volatility threshold - percentage change that triggers circuit breaker (default: 10.0)
MAX_PRICE_VOLATILITY_THRESHOLD=10.0

# Maximum liquidations per minute before triggering (default: 10)
MAX_LIQUIDATIONS_PER_MINUTE=10

# Time window for monitoring conditions in seconds (default: 300)
CIRCUIT_BREAKER_MONITORING_WINDOW_SECS=300

# Cooldown period before transitioning from open to half-open (default: 300)
CIRCUIT_BREAKER_COOLDOWN_SECS=300

# Gas price multiplier thresholds
MIN_GAS_PRICE_MULTIPLIER=1
MAX_GAS_PRICE_MULTIPLIER=5
```

## Recommended Settings

### Conservative (High Safety)
```bash
CIRCUIT_BREAKER_ENABLED=true
MAX_PRICE_VOLATILITY_THRESHOLD=5.0
MAX_LIQUIDATIONS_PER_MINUTE=5
CIRCUIT_BREAKER_MONITORING_WINDOW_SECS=300
CIRCUIT_BREAKER_COOLDOWN_SECS=600
MAX_GAS_PRICE_MULTIPLIER=3
```

### Moderate (Balanced)
```bash
CIRCUIT_BREAKER_ENABLED=true
MAX_PRICE_VOLATILITY_THRESHOLD=10.0
MAX_LIQUIDATIONS_PER_MINUTE=10
CIRCUIT_BREAKER_MONITORING_WINDOW_SECS=300
CIRCUIT_BREAKER_COOLDOWN_SECS=300
MAX_GAS_PRICE_MULTIPLIER=5
```

### Aggressive (Lower Safety)
```bash
CIRCUIT_BREAKER_ENABLED=true
MAX_PRICE_VOLATILITY_THRESHOLD=15.0
MAX_LIQUIDATIONS_PER_MINUTE=20
CIRCUIT_BREAKER_MONITORING_WINDOW_SECS=180
CIRCUIT_BREAKER_COOLDOWN_SECS=180
MAX_GAS_PRICE_MULTIPLIER=8
```

## How It Works

### Monitoring
The circuit breaker continuously monitors:
- Price changes across all monitored assets
- Frequency of liquidation events
- Current gas price multipliers

### Triggering Conditions

#### Price Volatility Trigger
```
Volatility = |Current Price - Initial Price| / Initial Price * 100
Trigger if: Volatility > MAX_PRICE_VOLATILITY_THRESHOLD
```

#### Liquidation Flood Trigger
```
Liquidations per minute = (Liquidation count in window) * 60 / window_seconds
Trigger if: Liquidations per minute > MAX_LIQUIDATIONS_PER_MINUTE
```

#### Gas Price Spike Trigger
```
Trigger if: Current gas multiplier > MAX_GAS_PRICE_MULTIPLIER
```

### State Transitions

```
Closed ──(extreme conditions)──> Open ──(cooldown timer)──> Half-Open
   ↑                                                           │
   └─────────────────(conditions normal)─────────────────────┘
```

## Monitoring and Alerts

### Log Messages
The circuit breaker produces clear log messages:

```
🚨 CIRCUIT BREAKER ACTIVATED: ExtremeVolatility { volatility_percent: 15.2 } - All liquidations suspended for 300 seconds
🟡 Circuit breaker HALF-OPEN - Testing market conditions
✅ Circuit breaker CLOSED - Normal operations resumed
```

### Status Reporting
Every 5 minutes, the bot logs a comprehensive status report:

```
🔒 Circuit Breaker Status Report:
   State: Closed
   Health Score: 85/100
   Conditions Improving: true
   Total Activations: 3
   Liquidations Blocked: 15
   Current Volatility: 2.5% (max: 10.0%)
   Liquidations/min: 2 (max: 10)
   Gas Multiplier: 2x (max: 5x)
   Time Since Last Activation: 1800s
```

### Health Score
- **90-100**: Excellent conditions, very low risk
- **70-89**: Good conditions, normal operations safe
- **50-69**: Moderate conditions, increased monitoring
- **30-49**: Poor conditions, circuit breaker may trigger soon
- **0-29**: Dangerous conditions, circuit breaker likely active

## Manual Control

### Emergency Disable
```rust
// Completely disable circuit breaker (emergency use only)
bot.disable_circuit_breaker().await?;
```

### Re-enable
```rust
// Re-enable after manual disable
bot.enable_circuit_breaker().await?;
```

### Reset
```rust
// Reset all state and history (emergency use only)
bot.circuit_breaker.reset().await?;
```

### Status Check
```rust
let (state, stats) = bot.get_circuit_breaker_status();
println!("Circuit breaker state: {:?}", state);
println!("Total activations: {}", stats.total_activations);
```

## Integration with External Systems

### Webhook Alerts (Example)
```rust
// In send_external_alert() function, add:
if matches!(alert.state_change, CircuitBreakerState::Open) {
    // Send to Slack
    let webhook_url = "https://hooks.slack.com/services/YOUR/WEBHOOK/URL";
    let payload = json!({
        "text": format!("🚨 LIQUIDATION BOT CIRCUIT BREAKER ACTIVATED: {}", alert.message)
    });
    
    // HTTP POST to webhook
    reqwest::Client::new()
        .post(webhook_url)
        .json(&payload)
        .send()
        .await?;
}
```

### Monitoring Dashboard Integration
The `CircuitBreakerStatusReport` struct is serializable and can be easily integrated with monitoring dashboards:

```rust
// Get status as JSON for dashboard
let status = bot.circuit_breaker.get_status_report();
let json_status = serde_json::to_string(&status)?;
```

## Black Swan Event Scenarios

### Flash Crash Example
1. ETH price drops 20% in 2 minutes
2. Volatility trigger activates (> 10% threshold)
3. Circuit breaker goes OPEN - all liquidations blocked
4. Bot waits 5 minutes (cooldown period)
5. Transitions to HALF-OPEN for testing
6. If conditions stable, returns to CLOSED

### Network Congestion Example
1. Gas prices spike to 10x normal
2. Gas price trigger activates (> 5x threshold)  
3. Circuit breaker goes OPEN - prevents expensive transactions
4. Waits for gas prices to normalize
5. Gradually resumes operations

### Liquidation Cascade Example
1. Large position gets liquidated, triggering others
2. 15 liquidations occur in 1 minute (> 10/min threshold)
3. Circuit breaker goes OPEN - prevents joining the cascade
4. Allows market to stabilize before resuming

## Best Practices

### Initial Deployment
1. Start with conservative settings
2. Monitor for false positives
3. Gradually adjust thresholds based on market behavior
4. Test manual controls in staging environment

### Production Operation
1. Monitor health scores regularly
2. Investigate any circuit breaker activations
3. Adjust thresholds seasonally (higher volatility periods)
4. Have manual override procedures documented

### Emergency Response
1. **Never disable** circuit breaker during active market stress
2. Wait for cooldown periods to complete naturally
3. Investigate root cause before re-enabling
4. Consider increasing thresholds if false positives occur

### Maintenance
1. Review circuit breaker logs weekly
2. Analyze activation patterns for optimization
3. Update thresholds based on historical data
4. Test emergency procedures monthly

## Troubleshooting

### Circuit Breaker Won't Activate
- Check `CIRCUIT_BREAKER_ENABLED=true`
- Verify thresholds aren't too high
- Confirm price feeds are updating
- Check monitoring window settings

### Too Many False Positives  
- Increase volatility threshold
- Extend monitoring window
- Reduce liquidation frequency threshold
- Consider market-specific adjustments

### Circuit Breaker Stuck Open
- Check if conditions are still extreme
- Verify cooldown period has elapsed
- Look for ongoing market stress
- Consider manual reset if system error

### No Alerts Received
- Verify alert processor is running
- Check external webhook configurations
- Confirm log level settings
- Test manual alert sending

## Future Enhancements

Planned improvements include:
- Machine learning based market condition prediction
- Integration with external market data feeds
- Configurable escalation levels
- Historical pattern analysis
- Cross-chain condition monitoring
- API endpoints for external control systems

## Security Considerations

- Circuit breaker state is not externally controllable by default
- Manual overrides require direct bot access
- All state changes are logged with timestamps
- Failed operations are counted and reported
- Emergency reset capability for system recovery