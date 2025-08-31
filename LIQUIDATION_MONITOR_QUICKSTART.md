# Liquidation Monitor - Quick Start Guide

## 🚀 Overview

The Liquidation Monitor is a specialized module that listens to `LiquidationCall` events from the Aave protocol on Base network. It provides real-time monitoring, detailed logging, and comprehensive statistics about liquidation activities.

## 📋 Prerequisites

- Rust 1.75+ (for building from source)
- OR Docker (for containerized deployment)
- RPC endpoint for Base network (e.g., Alchemy, Infura, or local node)

## 🛠️ Installation

### Option 1: Build from Source

```bash
# Clone the repository if you haven't already
git clone <repository-url>
cd liquidation-bot

# Build the liquidation monitor
cargo build --release --bin liquidation-monitor

# The binary will be at: ./target/release/liquidation-monitor
```

### Option 2: Using Docker

```bash
# Build the Docker image
docker build -f Dockerfile.liquidation-monitor -t liquidation-monitor .

# Or use docker-compose
docker-compose -f docker-compose.liquidation-monitor.yml build
```

## ⚙️ Configuration

### 1. Environment Variables

Copy the example environment file:

```bash
cp .env.liquidation-monitor.example .env
```

Edit `.env` and add your RPC URL:

```env
RPC_URL=https://base-mainnet.g.alchemy.com/v2/YOUR_ALCHEMY_KEY
WS_URL=wss://base-mainnet.g.alchemy.com/v2/YOUR_ALCHEMY_KEY
```

### 2. Configuration File (Optional)

Generate a configuration file:

```bash
./target/release/liquidation-monitor generate-config config.json
```

## 🎯 Quick Start

### Real-time Monitoring

#### Using Binary:
```bash
# Basic monitoring
./target/release/liquidation-monitor monitor

# With file logging
./target/release/liquidation-monitor --log-file liquidations.jsonl monitor

# With custom RPC
./target/release/liquidation-monitor --rpc-url YOUR_RPC_URL monitor

# Verbose mode
./target/release/liquidation-monitor --verbose monitor
```

#### Using Docker:
```bash
# Start monitoring
docker-compose -f docker-compose.liquidation-monitor.yml up

# Run in background
docker-compose -f docker-compose.liquidation-monitor.yml up -d

# View logs
docker-compose -f docker-compose.liquidation-monitor.yml logs -f
```

#### Using the Helper Script:
```bash
# Run the interactive menu
./examples/run_liquidation_monitor.sh
```

### Historical Analysis

Analyze past liquidations:

```bash
# Analyze specific block range
./target/release/liquidation-monitor historical --from-block 1000000 --to-block 2000000

# Analyze from block to latest
./target/release/liquidation-monitor historical --from-block 1000000
```

## 📊 Understanding the Output

### Event Logs

Each liquidation is logged with emojis for easy reading:

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
💎 Liquidation Bonus: ~10%
```

### Statistics Summary

Every 30 minutes (configurable), you'll see:

```
📊 ===== LIQUIDATION STATISTICS ===== 📊
⏱️  Monitoring Duration: 2h 30m
📈 Total Liquidations: 42
💰 Total Debt Covered: 1500000 ETH
👥 Unique Liquidators: 8
🎯 Unique Users Liquidated: 35
⚡ Average Frequency: 16.8 liquidations/hour
```

### JSON Output (File Logging)

When file logging is enabled, events are saved as JSON:

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
  "liquidated_collateral_amount": "1100000000000000000"
}
```

## 🔍 Common Use Cases

### 1. Monitor Liquidation Frequency
Track how often liquidations occur to understand market volatility:

```bash
./target/release/liquidation-monitor monitor --stats-interval 10
```

### 2. Identify Top Liquidators
See which addresses are performing the most liquidations:

```bash
# Monitor and check the statistics summary for "Top Liquidators"
./target/release/liquidation-monitor monitor
```

### 3. Analyze Specific Time Period
Study liquidations during a market event:

```bash
./target/release/liquidation-monitor historical \
  --from-block 12000000 \
  --to-block 12100000
```

### 4. Export Data for Analysis
Log all events to a file for later analysis:

```bash
./target/release/liquidation-monitor \
  --log-file liquidations_$(date +%Y%m%d).jsonl \
  monitor
```

Then analyze with tools like `jq`:

```bash
# Count liquidations per liquidator
cat liquidations_*.jsonl | jq -r '.liquidator' | sort | uniq -c | sort -rn

# Sum total debt covered
cat liquidations_*.jsonl | jq -r '.debt_to_cover' | paste -sd+ | bc
```

## 🐛 Troubleshooting

### No Events Detected

1. **Check RPC Connection:**
   ```bash
   curl -X POST YOUR_RPC_URL \
     -H "Content-Type: application/json" \
     -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}'
   ```

2. **Verify Pool Address:**
   - Base Mainnet: `0xA238Dd80C259a72e81d7e4664a9801593F98d1c5`

3. **Check Network Activity:**
   - Liquidations may not occur frequently during stable market conditions

### WebSocket Connection Failed

- The monitor automatically falls back to HTTP polling
- For real-time updates, ensure your RPC provider supports WebSocket

### High Memory Usage

Reduce the number of stored events:

```bash
./target/release/liquidation-monitor monitor --max-events 100
```

## 📈 Performance Tips

1. **Use WebSocket for Real-time Monitoring:**
   - Lower latency than HTTP polling
   - More efficient for long-running monitors

2. **Adjust Statistics Interval:**
   - Shorter intervals for active monitoring
   - Longer intervals for background monitoring

3. **File Logging Performance:**
   - Use SSD for better write performance
   - Rotate log files periodically

## 🔗 Integration with Main Bot

The monitor can be integrated with the main liquidation bot:

```rust
// In your bot code
use liquidation_bot::monitoring::LiquidationMonitor;

let monitor = LiquidationMonitor::new(
    rpc_url,
    pool_address,
    1000,  // max events
    true,  // log to file
    Some("liquidations.jsonl".to_string()),
).await?;

// Start monitoring in background
tokio::spawn(async move {
    monitor.start_monitoring().await
});
```

## 📝 Example Scenarios

### Scenario 1: 24/7 Monitoring Setup

```bash
# Using Docker for persistent monitoring
docker-compose -f docker-compose.liquidation-monitor.yml up -d

# Check logs
docker logs aave-liquidation-monitor -f

# Stop monitoring
docker-compose -f docker-compose.liquidation-monitor.yml down
```

### Scenario 2: Daily Analysis Script

```bash
#!/bin/bash
# daily_analysis.sh

DATE=$(date +%Y%m%d)
LOG_FILE="liquidations_$DATE.jsonl"

# Run monitor for 24 hours
timeout 24h ./target/release/liquidation-monitor \
  --log-file "$LOG_FILE" \
  monitor

# Generate report
echo "Daily Liquidation Report - $DATE" > "report_$DATE.txt"
echo "Total Events: $(wc -l < $LOG_FILE)" >> "report_$DATE.txt"
# Add more analysis...
```

### Scenario 3: Alert on High Activity

```bash
# Monitor and alert if liquidations exceed threshold
./target/release/liquidation-monitor monitor | while read line; do
  if echo "$line" | grep -q "LIQUIDATION DETECTED"; then
    COUNT=$((COUNT + 1))
    if [ $COUNT -gt 10 ]; then
      echo "ALERT: High liquidation activity detected!"
      # Send notification (email, Discord, etc.)
    fi
  fi
done
```

## 🚨 Important Notes

1. **Network Costs:** Monitoring consumes RPC credits/requests
2. **Data Storage:** JSON logs can grow large - implement rotation
3. **Privacy:** Be mindful of storing user addresses
4. **Rate Limits:** Respect RPC provider rate limits

## 📚 Further Reading

- [Full Documentation](docs/LIQUIDATION_MONITOR.md)
- [Aave Protocol Documentation](https://docs.aave.com)
- [Base Network Documentation](https://docs.base.org)

## 💡 Tips for Success

1. Start with short monitoring sessions to understand patterns
2. Use file logging for data analysis and record keeping
3. Monitor during different market conditions
4. Compare liquidation patterns across different time periods
5. Cross-reference with price movements for deeper insights

---

**Need Help?** Check the [troubleshooting guide](docs/LIQUIDATION_MONITOR.md#troubleshooting) or review the example scripts in `/examples/`.