# Liquidation Bot Setup Guide

## Overview

This is an Aave v3 liquidation bot for the Base network, written in Rust using modern `alloy-rs` libraries. The bot uses real-time WebSocket subscriptions to monitor Aave V3 events, dynamically discovers at-risk users, and persists data to a database.

## Current Status (Phase 2: In Progress)

- ✅ **Real-Time Monitoring**: The bot connects to a WebSocket endpoint to listen to all Aave Pool events.
- ✅ **Dynamic User Discovery**: The bot automatically finds and tracks any user interacting with the Aave protocol.
- ✅ **Database Persistence**: User positions and bot activity are saved to a SQLite or PostgreSQL database.
- ✅ **Concurrent Architecture**: The bot is built on Tokio and handles multiple tasks in parallel for high performance.
- 🚧 **Next Up**: Implementing oracle price monitoring and profitability calculations.

---

## Quick Start

### 1. Prerequisites

- Rust 1.70+
- Node.js and npm (for smart contract management)
- A Base Sepolia WebSocket RPC endpoint (e.g., from [Alchemy](https://alchemy.com/), [QuickNode](https://quicknode.com/))
- A private key with some Base Sepolia ETH for gas.

### 2. Environment Setup

Create a `.env` file from the example. For a detailed explanation of every variable, see `docs/CONFIGURATION.md`.

```bash
# Get a free WebSocket endpoint from a provider like Alchemy
# This is required for real-time monitoring.
WS_URL=wss://base-sepolia.g.alchemy.com/v2/YOUR_API_KEY

# This is used for sending transactions.
RPC_URL=https://sepolia.base.org

# WARNING: Keep this secure and never commit to version control
PRIVATE_KEY=your_private_key_here

# The deployed AaveLiquidator contract on Base Sepolia
LIQUIDATOR_CONTRACT=0x4818d1cb788C733Ae366D6d1D463EB48A0544528

# Optional: Monitor a specific user for testing purposes
# Leave blank to monitor the entire Aave market
TARGET_USER=

# Path to the database file
DATABASE_URL=sqlite:liquidation_bot.db

# Log level (error, warn, info, debug)
RUST_LOG=info
```
*If you do not provide a `WS_URL`, the bot will fall back to slower HTTP polling.*

### 3. Install Dependencies

```bash
# Install Rust dependencies
cargo build

# Install Node.js dependencies
npm install
```

### 4. Run the Bot

```bash
# Run in real-time monitoring mode
cargo run --release

# Run with detailed debug logging
RUST_LOG=debug cargo run
```
The bot will connect to the WebSocket endpoint, initialize the database, and begin listening for Aave events.

## Architecture

The bot is architected for high performance and reliability.

### 1. Smart Contract (`AaveLiquidator.sol`)
- Deployed contract that can execute flash-loan-powered liquidations atomically.
- Secured with `onlyOwner` and reentrancy guards.

### 2. Monitoring System (`src/main.rs`)
- **WebSocket Listener**: Subscribes to on-chain Aave events for real-time data.
- **Dynamic User Tracking**: Discovers users from events and adds them to a monitoring list.
- **Concurrent Processing**: Uses `tokio` tasks to manage event listening, database writes, and health checks simultaneously.
- **Database Backend**: Persists all discovered user positions and bot activity, allowing state to be maintained across restarts.

### 3. Configuration System
- All settings are managed via a `.env` file.
- See `docs/CONFIGURATION.md` for a complete list of options.

## Key Features

### Monitoring
- **Real-Time Event-Driven**: Reacts instantly to on-chain events rather than relying on slow polling.
- **Market-Wide Surveillance**: Monitors all user activity on the Aave V3 Pool, not just a predefined list.
- **Persistent State**: The database ensures that knowledge of at-risk users is not lost if the bot restarts.
- **Status Reporting**: Logs regular updates on the number of users tracked and at risk.

### Flash Loan Liquidations (Ready for Execution)
- The `AaveLiquidator` contract is deployed and can be called by the bot owner to:
  - Borrow assets via Aave flash loans.
  - Execute liquidations using gas-optimized L2Pool functions.
  - Swap seized collateral via Uniswap V3 to repay the loan.
  - Extract and secure profits.

## Troubleshooting

### WebSocket Connection Errors
- **Verify `WS_URL`**: Ensure your WebSocket endpoint is correct and your API key is valid.
- **Provider Issues**: Public RPCs often don't support WebSockets. Use a dedicated provider like Alchemy.
- **Firewall**: Check for network restrictions that might block the connection.

### Database Errors
- **File Permissions**: If using SQLite, ensure the bot has write permissions in the project directory.
- **Connection String**: If using PostgreSQL, double-check your `DATABASE_URL`.
- **File Not Found**: The bot creates the SQLite file automatically, but ensure the directory is writable.

### Common Issues
- **RPC Timeouts**: Public HTTP RPC endpoints can be unreliable. Consider a dedicated provider for `RPC_URL` as well.
- **No Events Detected**: Ensure you are connected to the correct network (Base Sepolia) where there is activity. If the market is quiet, no events will be emitted.

## Contributing
This project follows the roadmap in `docs/ROADMAP.md`. Key areas for the next phase of contribution are:
1.  **Price Oracles**: Integrate Chainlink feeds.
2.  **Profitability Engine**: Build the profit calculation logic.
3.  **Execution Logic**: Implement the final transaction-sending step.
4.  **Testing**: Add a robust testing and simulation suite.

## Network Information

### Base Mainnet
- **RPC URL:** `https://mainnet.base.org`
- **Chain ID:** 8453
- **Aave Pool:** `0xA238Dd80C259a72e81d7e4664a9801593F98d1c5`
- **Block Time:** ~2 seconds

### Base Goerli (Testnet)
- **RPC URL:** `https://goerli.base.org`
- **Chain ID:** 84531

## Security Considerations

1. **Private Key Management**
   - Never commit private keys to version control
   - Use secure key storage in production
   - Consider using hardware wallets for large amounts

2. **Contract Security**
   - Owner-only functions prevent unauthorized access
   - Reentrancy guards protect against attacks
   - Slippage protection prevents sandwich attacks

3. **Operational Security**
   - Monitor bot health and performance
   - Set up alerts for unusual activity
   - Regular withdrawal of accumulated profits

## Support

For issues and questions:
1. Check the troubleshooting section above
2. Review the comprehensive research in `docs/liquidation-bot-research.md`
3. Examine the roadmap in `docs/ROADMAP.md` 