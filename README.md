# Aave v3 Liquidation Bot for Base Network

This is a sophisticated Aave v3 liquidation bot for the Base network, written in Rust using the modern [alloy](https://github.com/alloy-rs/alloy) Ethereum libraries. The bot monitors user health factors and can execute profitable liquidations using flash loans.

## ✅ Current Status

**Phase 1: Core Infrastructure (COMPLETED)**
- [x] **Smart Contract (`AaveLiquidator.sol`)**: Deployed and ready for flash loan liquidations.
- [x] **Rust Bot Foundation**: Basic health factor checking and configuration.

**Phase 2: Event Monitoring (IN PROGRESS)**
- [x] **WebSocket Subscriptions**: Real-time monitoring of all Aave Pool events.
- [x] **Dynamic User Discovery**: Automatically detects and monitors all active Aave users.
- [x] **Database Integration**: Persists user positions and bot events to a database (SQLite/PostgreSQL).
- [x] **Oracle Price Monitoring**: Directly monitor Chainlink price feeds to react instantly to market volatility (core implementation).
- [ ] **Profitability Calculation**: Implement logic to calculate the exact profit of a liquidation.

## 🔄 Next Steps

The next priorities from the [roadmap](docs/ROADMAP.md) are to complete Phase 2:
- Implement **Profitability Calculation With Gas Estimation**.
- Enable full **Liquidation Execution**.
- Implement **Multi-Asset Liquidation Strategies**.

## Quick Start

### Prerequisites
- Rust 1.70+
- Node.js and npm
- Base network RPC access
- Private key with ETH for gas

### 1. Clone and Install
```bash
git clone <repository>
cd liquidation-bot
cargo build --release
npm install
```

### 2. Environment Setup
Create a `.env` file:
```bash
# Required
RPC_URL=https://mainnet.base.org
PRIVATE_KEY=your_private_key_here

# Optional
TARGET_USER=0x1234567890123456789012345678901234567890
LIQUIDATOR_CONTRACT=0xYourDeployedContractAddress
RUST_LOG=info
```

### 3. Deploy Smart Contract (Optional)
```bash
# Compile contracts
npm run compile

# Deploy to Base mainnet
npm run deploy
```

### 4. Run the Bot
```bash
# Monitor health factors
cargo run --release

# Debug mode with verbose logging
RUST_LOG=debug cargo run
```

## Architecture

### Smart Contract Features
- **Flash Loan Integration**: Borrows assets via Aave for liquidations
- **L2Pool Optimization**: Uses Base-specific encoding for reduced gas costs
- **Uniswap V3 Swaps**: Automatically converts collateral to debt assets
- **Security**: Owner-only functions, reentrancy guards, slippage protection
- **Profit Management**: Automated profit extraction and withdrawal functions

### Rust Bot Features
- **Real-Time Event Monitoring**: Subscribes to Aave Pool events (Borrow, Repay, Supply, etc.) over WebSockets for instant user activity detection.
- **Dynamic User Discovery**: Automatically discovers and tracks all users interacting with the Aave protocol, not just a single target.
- **Database Integration**: Uses SQLite (or PostgreSQL) to persist user positions, at-risk users, and bot events for analysis and statefulness.
- **Concurrent Architecture**: Employs a multi-tasking `tokio` architecture for high-performance, non-blocking operations.
- **Intelligent Fallback**: Automatically reverts to HTTP polling if a WebSocket connection is not available.
- **Alloy Integration**: Modern Ethereum library with type-safe contract bindings.
- **Structured Logging**: Comprehensive `tracing` for debugging and monitoring.
- **Advanced Configuration**: Detailed environment-based setup, documented in `docs/CONFIGURATION.md`.

## Key Advantages

1. **Base Network Optimized**: Leverages L2Pool for 60%+ gas savings.
2. **Modern Tech Stack**: Uses latest Rust and Ethereum tooling (Alloy, Tokio).
3. **Real-Time & Proactive**: Instead of just polling, the bot reacts to on-chain events the moment they happen.

## Documentation

- **[Setup Guide](SETUP.md)**: Detailed installation and configuration.
- **[Configuration](docs/CONFIGURATION.md)**: In-depth guide to all environment variables.
- **[Research](docs/liquidation-bot-research.md)**: 75KB comprehensive technical analysis.
- **[Roadmap](docs/ROADMAP.md)**: Development phases and future features.
- **[Contract](contracts/AaveLiquidator.sol)**: Fully documented Solidity implementation

## Network Information

**Base Mainnet**
- Chain ID: 8453
- RPC: `https://mainnet.base.org`
- Aave Pool: `0xA238Dd80C259a72e81d7e4664a9801593F98d1c5`
- Block Time: ~2 seconds

## Example Output

```
INFO liquidation_bot: 🚀 Starting Aave v3 Liquidation Bot with Real-Time WebSocket Monitoring
INFO liquidation_bot: ✅ WebSocket connection established successfully!
INFO liquidation_bot: 🤖 Liquidation bot initialized with real-time WebSocket monitoring
INFO liquidation_bot: ✅ Successfully subscribed to Aave Pool events!
INFO liquidation_bot: 🎧 Listening for real-time Aave events...
WARN liquidation_bot: ⚠️  User 0xdb3e... is at risk. Health Factor: 1.094
INFO liquidation_bot: Scanning 1 at-risk users...
WARN liquidation_bot: ⚠️  User 0xdb3e... is at risk. Health Factor: 1.093
INFO liquidation_bot: 📊 Status Report: 3 positions tracked, 1 at risk, 0 liquidatable
```

