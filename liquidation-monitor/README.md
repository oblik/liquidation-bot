# Liquidation Monitor

A real-time monitoring tool for Aave V3 liquidation events across multiple networks. This module listens to `LiquidationCall` events emitted by Aave V3 pools and provides detailed logging and statistics.

## Features

- 🔍 **Real-time Event Monitoring**: Listen to liquidation events as they happen
- 📊 **Detailed Statistics**: Track total liquidations, debt covered, collateral liquidated, and more
- 🌐 **Multi-network Support**: Works with Ethereum, Arbitrum, Optimism, Polygon, Base, Avalanche, and testnets
- 📜 **Historical Event Query**: Optionally query past liquidation events
- 📝 **Comprehensive Logging**: Console output with colors and rotating file logs
- 🎯 **Filtering Options**: Filter by specific users, assets, or minimum debt thresholds
- 💾 **Persistent Logs**: Daily rotating log files with automatic compression

## Installation

1. Clone or copy the liquidation-monitor directory to your project:

```bash
cd liquidation-monitor
```

2. Install dependencies:

```bash
npm install
```

3. Configure environment variables:

```bash
cp .env.example .env
```

Edit `.env` and add your RPC URL and configure settings as needed.

## Configuration

### Required Settings

- **RPC_URL** or **[NETWORK]_RPC_URL**: Your RPC endpoint URL (e.g., from Alchemy, Infura, or QuickNode)
- **NETWORK**: The network to monitor (default: mainnet)

### Optional Settings

- **QUERY_HISTORICAL**: Query historical events on startup (default: true)
- **HISTORICAL_BLOCKS**: Number of blocks to look back for historical events (default: 1000)
- **STATS_INTERVAL**: How often to display statistics in milliseconds (default: 60000)
- **LOG_LEVEL**: Logging verbosity - debug, info, warn, error (default: info)
- **LOG_TO_FILE**: Enable file logging (default: true)
- **WATCHED_USERS**: Comma-separated list of user addresses to specifically track
- **WATCHED_ASSETS**: Comma-separated list of asset addresses to specifically track
- **MIN_DEBT_THRESHOLD**: Minimum debt amount to log (in ETH)

## Usage

### Basic Usage

Start the monitor with default settings:

```bash
npm start
```

or

```bash
node index.js
```

### Network Selection

Monitor a specific network by setting the NETWORK environment variable:

```bash
NETWORK=arbitrum npm start
```

### Available Networks

- `mainnet` - Ethereum Mainnet
- `arbitrum` - Arbitrum One
- `optimism` - Optimism
- `polygon` - Polygon PoS
- `base` - Base
- `avalanche` - Avalanche C-Chain
- `sepolia` - Sepolia Testnet
- `goerli` - Goerli Testnet

## Output

### Console Output

The monitor displays:
- Connection status and network information
- Real-time liquidation events with detailed information:
  - User address (liquidated account)
  - Liquidator address
  - Collateral and debt assets
  - Amounts liquidated
  - Gas costs
  - Transaction details
- Periodic statistics summary

### Log Files

When file logging is enabled, the monitor creates:
- `logs/liquidations-YYYY-MM-DD.log` - All monitor logs
- `logs/events-YYYY-MM-DD.log` - Liquidation events only
- `logs/errors.log` - Error logs

## Event Details

Each liquidation event includes:
- **Timestamp**: When the liquidation occurred
- **Block Number**: The block containing the liquidation
- **Transaction Hash**: Link to the transaction
- **User**: The account being liquidated
- **Liquidator**: The account performing the liquidation
- **Collateral Asset**: The asset being liquidated
- **Debt Asset**: The debt being repaid
- **Debt to Cover**: Amount of debt repaid
- **Liquidated Collateral Amount**: Amount of collateral seized
- **Liquidation Bonus**: Approximate bonus percentage
- **Gas Details**: Gas used, price, and total cost
- **Receive aToken**: Whether the liquidator received aTokens

## Statistics Tracked

- Total number of liquidations
- Total debt covered (in ETH equivalent)
- Total collateral liquidated
- Unique users liquidated
- Unique asset pairs involved
- Average debt per liquidation
- Average collateral per liquidation
- Top liquidated asset pairs
- Runtime duration

## Example Output

```
🚀 Starting Aave V3 Liquidation Monitor
Network: Ethereum Mainnet
Pool Address: 0x87870Bca3F3fD6335C3F4ce8392D69350B4fA4E2
✅ Connected to network: homestead (chainId: 1)
Current block: 18500000
👂 Listening for liquidation events...

⚡ LIQUIDATION DETECTED
────────────────────────────────────────────────────────────
📅 Time: 2024-01-15T10:30:45.000Z
📦 Block: 18500123
🔗 Tx: 0xabcd...1234

👤 User (Liquidated): 0x1234...5678
💰 Liquidator: 0x9876...5432

📊 Assets:
  Collateral: WETH (0xC02a...6Cc2)
  Debt: USDC (0xA0b8...eB48)

💵 Amounts:
  Debt Covered: 5000.00 USDC
  Collateral Liquidated: 2.15 ETH
  Liquidation Bonus: ~5.00%

⛽ Gas:
  Gas Used: 250000
  Gas Price: 35 Gwei
  Gas Cost: 0.00875 ETH

🔄 Receive aToken: No
────────────────────────────────────────────────────────────

📊 Liquidation Statistics:
──────────────────────────────────────────────────
Total Liquidations: 15
Total Debt Covered: 75000.00 USDC
Total Collateral Liquidated: 32.5 ETH
Unique Users Liquidated: 12
Unique Asset Pairs: 5
Runtime: 2h 15m
Average Debt per Liquidation: 5000.00 USDC
Average Collateral per Liquidation: 2.17 ETH

Top Liquidated Asset Pairs:
  WETH -> USDC: 8 liquidations
  WBTC -> USDT: 4 liquidations
  WETH -> DAI: 3 liquidations
──────────────────────────────────────────────────
```

## Troubleshooting

### No events appearing
- Check your RPC URL is correct and has access to event logs
- Verify the network selection matches your RPC endpoint
- Ensure the pool address is correct for your network
- Try increasing `HISTORICAL_BLOCKS` to query more history

### Connection errors
- Verify your RPC URL is accessible
- Check for rate limiting on your RPC provider
- Consider using a dedicated node or upgrading your RPC plan

### High memory usage
- Reduce `HISTORICAL_BLOCKS` value
- Increase `STATS_INTERVAL` to reduce logging frequency
- Disable file logging if not needed (`LOG_TO_FILE=false`)

## Development

### Project Structure
```
liquidation-monitor/
├── index.js              # Main monitoring script
├── config/
│   └── config.js        # Network configurations and settings
├── utils/
│   ├── logger.js        # Winston logger setup
│   └── formatters.js    # Formatting utilities
├── abi/
│   └── L2Pool.json      # Aave V3 Pool ABI
├── logs/                # Log files (created automatically)
├── .env.example         # Environment variables template
├── package.json         # Dependencies
└── README.md           # This file
```

### Adding New Networks

To add support for a new network, edit `config/config.js` and add the network configuration to `NETWORK_CONFIGS` object with:
- Network name
- Chain ID
- Aave V3 Pool address
- Default RPC URL

### Extending Functionality

The monitor can be extended to:
- Send alerts via webhook/email when liquidations occur
- Store events in a database for analysis
- Calculate profitability metrics
- Integrate with trading bots
- Monitor specific positions at risk

## License

MIT

## Support

For issues or questions, please check the Aave V3 documentation or create an issue in the repository.