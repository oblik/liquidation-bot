# Liquidation Monitor Module

## Overview

The Liquidation Monitor is a specialized module for tracking and analyzing liquidation events on the Aave protocol. It provides real-time monitoring, detailed logging, and comprehensive statistics about liquidation activities.

## Features

- **Real-time Event Monitoring**: Listens to `LiquidationCall` events via WebSocket or HTTP polling
- **Detailed Logging**: Logs comprehensive information about each liquidation event
- **Statistics Tracking**: Maintains statistics including:
  - Total liquidations count
  - Total debt covered and collateral liquidated
  - Unique liquidators and users liquidated
  - Asset pair frequencies
  - Hourly liquidation patterns
  - Average liquidation frequency
- **Flexible Configuration**: Supports environment variables, config files, and command-line arguments
- **Historical Analysis**: Can analyze past liquidation events within specified block ranges
- **File Logging**: Optional JSON Lines format logging for data analysis

## Installation

The liquidation monitor is included as part of the liquidation bot project. Build it using:

```bash
cargo build --release --bin liquidation-monitor
```

## Usage

### Basic Monitoring

Start monitoring liquidations in real-time:

```bash
./target/release/liquidation-monitor monitor
```

### With Custom Configuration

Use a configuration file:

```bash
./target/release/liquidation-monitor --config config.json monitor
```

### Command-Line Options

```bash
liquidation-monitor [OPTIONS] [COMMAND]

Commands:
  monitor       Start monitoring liquidation events in real-time
  historical    Analyze historical liquidation events
  generate-config  Generate a sample configuration file

Options:
  -c, --config <CONFIG>          Path to configuration file
      --rpc-url <RPC_URL>        Override RPC URL
      --ws-url <WS_URL>          Override WebSocket URL
      --pool-address <ADDRESS>    Override pool address
  -v, --verbose                  Enable verbose logging
      --log-file <FILE>          Log liquidations to file
      --stats-interval <MINS>     Statistics reporting interval (default: 30)
```

### Monitor Subcommand

```bash
liquidation-monitor monitor [OPTIONS]

Options:
  --max-events <NUM>    Maximum events to keep in memory (default: 1000)
```

### Historical Analysis

Analyze liquidations between specific blocks:

```bash
liquidation-monitor historical --from-block 1000000 --to-block 2000000
```

### Generate Configuration

Create a sample configuration file:

```bash
liquidation-monitor generate-config config.json
```

## Configuration

### Environment Variables

The monitor can be configured using environment variables:

- `RPC_URL` or `LIQUIDATION_MONITOR_RPC_URL`: Blockchain RPC endpoint
- `WS_URL` or `LIQUIDATION_MONITOR_WS_URL`: WebSocket endpoint (optional)
- `POOL_ADDRESS` or `LIQUIDATION_MONITOR_POOL_ADDRESS`: Aave L2Pool contract address
- `LIQUIDATION_MONITOR_MAX_EVENTS`: Maximum events to store in memory
- `LIQUIDATION_MONITOR_LOG_TO_FILE`: Enable file logging (true/false)
- `LIQUIDATION_MONITOR_LOG_FILE`: Path to log file
- `LIQUIDATION_MONITOR_STATS_INTERVAL`: Statistics reporting interval in minutes
- `LIQUIDATION_MONITOR_VERBOSE`: Enable verbose logging (true/false)

### Configuration File

Create a JSON configuration file:

```json
{
  "rpc_url": "https://base-mainnet.g.alchemy.com/v2/YOUR_KEY",
  "ws_url": "wss://base-mainnet.g.alchemy.com/v2/YOUR_KEY",
  "pool_address": "0xA238Dd80C259a72e81d7e4664a9801593F98d1c5",
  "max_events_stored": 1000,
  "log_to_file": true,
  "log_file_path": "liquidations.jsonl",
  "stats_interval_minutes": 30,
  "verbose": true,
  "asset_names": {
    "0x4200000000000000000000000000000000000006": "WETH",
    "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913": "USDC"
  }
}
```

## Output Format

### Console Output

Each liquidation event is logged with the following information:

```
🔴 ===== LIQUIDATION DETECTED ===== 🔴
⏰ Time: 2024-01-15 14:30:45
📦 Block: 12345678
🔗 Tx: 0xabc...def
👤 User Liquidated: 0x123...456
🏦 Liquidator: 0x789...abc
💰 Collateral Asset: 0x420...006
💸 Debt Asset: 0x833...913
📉 Debt Covered: 1000000000000000000 wei
📊 Collateral Liquidated: 1100000000000000000 wei
🎯 Receive aToken: false
💎 Liquidation Bonus: ~10%
=====================================
```

### File Output (JSON Lines)

When file logging is enabled, each event is saved as a JSON object:

```json
{
  "timestamp": "2024-01-15T14:30:45Z",
  "block_number": 12345678,
  "transaction_hash": "0xabc...def",
  "collateral_asset": "0x420...006",
  "debt_asset": "0x833...913",
  "user": "0x123...456",
  "liquidator": "0x789...abc",
  "debt_to_cover": "1000000000000000000",
  "liquidated_collateral_amount": "1100000000000000000",
  "receive_atoken": false
}
```

### Statistics Summary

Periodic statistics reports include:

```
📊 ===== LIQUIDATION STATISTICS ===== 📊
⏱️  Monitoring Duration: 2h 30m
📈 Total Liquidations: 42
💰 Total Debt Covered: 1500000000000000000000 wei
🏦 Total Collateral Liquidated: 1650000000000000000000 wei
👥 Unique Liquidators: 8
🎯 Unique Users Liquidated: 35

🏆 Top Liquidators:
   0xabc...def: 15 liquidations
   0x123...456: 10 liquidations

💱 Most Common Asset Pairs:
   Collateral: WETH -> Debt: USDC: 25 times
   Collateral: USDC -> Debt: WETH: 17 times

⚡ Average Frequency: 16.8 liquidations/hour
🕐 Last Liquidation: 2024-01-15 14:30:45 UTC
=====================================
```

## Integration with Main Bot

The liquidation monitor can be integrated with the main liquidation bot to provide enhanced monitoring capabilities:

```rust
use liquidation_bot::monitoring::{LiquidationMonitor, LiquidationMonitorConfig};

// Create monitor from config
let config = LiquidationMonitorConfig::from_env()?;
let monitor = LiquidationMonitor::new(
    &config.rpc_url,
    config.pool_address,
    config.max_events_stored,
    config.log_to_file,
    config.log_file_path,
).await?;

// Start monitoring in background
tokio::spawn(async move {
    monitor.start_monitoring().await
});
```

## Use Cases

1. **Performance Analysis**: Track liquidation frequency and patterns to optimize bot performance
2. **Market Analysis**: Analyze which asset pairs are most frequently liquidated
3. **Competition Analysis**: Identify top liquidators and their strategies
4. **Risk Assessment**: Monitor liquidation volumes and frequencies for risk management
5. **Historical Research**: Analyze past liquidation events for strategy development

## Troubleshooting

### No Events Detected

1. Verify the pool address is correct for your network
2. Check RPC/WebSocket connection is working
3. Ensure the network has liquidation activity

### WebSocket Connection Issues

The monitor automatically falls back to HTTP polling if WebSocket connection fails. For best performance, use a WebSocket-enabled RPC endpoint.

### High Memory Usage

Reduce `max_events_stored` to limit memory usage:

```bash
liquidation-monitor monitor --max-events 100
```

## Development

### Adding Custom Metrics

Extend the `LiquidationStats` struct in `src/monitoring/liquidation_monitor.rs`:

```rust
pub struct LiquidationStats {
    // ... existing fields ...
    pub custom_metric: HashMap<String, u64>,
}
```

### Custom Event Processing

Modify the `process_liquidation_log` method to add custom processing logic:

```rust
async fn process_liquidation_log(&self, log: Log) -> Result<()> {
    // ... existing processing ...
    
    // Add custom logic here
    self.custom_processing(&event).await;
    
    Ok(())
}
```

## Performance Considerations

- **WebSocket vs Polling**: WebSocket provides real-time updates with lower latency
- **Memory Management**: Events are capped at `max_events_stored` to prevent memory issues
- **File I/O**: File logging is asynchronous to minimize performance impact
- **Statistics Calculation**: Statistics are updated incrementally for efficiency