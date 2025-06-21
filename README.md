# Aave v3 Liquidation Bot for Base Network

This is an Aave v3 liquidation bot for the Base network, written in Rust using the modern [alloy](https://github.com/alloy-rs/alloy) Ethereum libraries. The bot monitors user health factors and can execute profitable liquidations using flash loans.

## ✅ Current Status

**Phase 1: Core Infrastructure (COMPLETED)**
- [x] **Smart Contract (`AaveLiquidator.sol`)**: Deployed and ready for flash loan liquidations.
- [x] **Rust Bot Foundation**: Basic health factor checking and configuration.

**Phase 2: Event Monitoring (COMPLETED)**
- [x] **WebSocket Subscriptions**: Real-time monitoring of all Aave Pool events.
- [x] **Dynamic User Discovery**: Automatically detects and monitors all active Aave users.
- [x] **Database Integration**: Persists user positions and bot events to a database (SQLite/PostgreSQL).
- [x] **Oracle Price Monitoring**: Directly monitor Chainlink price feeds to react instantly to market volatility (core implementation).
- [x] **Profitability Calculation**: Complete implementation with liquidation bonus, flash loan fees, gas costs, and slippage estimation.
- [x] **Liquidation Execution**: Full smart contract integration with transaction management and confirmation tracking.

## 🔄 Next Steps

**Phase 2 is now COMPLETE!** 🎉 The bot has full liquidation functionality with:
- ✅ **Real Profitability Calculation** with gas estimation and cost analysis
- ✅ **Complete Liquidation Execution** via smart contract integration  
- ✅ **Multi-Asset Support** for WETH, USDC, cbETH liquidation strategies

The next priorities from the [roadmap](docs/ROADMAP.md) are **Phase 3: Production Hardening & Optimization**:
- **Advanced Error Handling & Retries**
- **Dynamic Gas Price Strategy** 
- **Enhanced Multi-Asset Logic**
- **Testing & Simulation Framework**
- **Containerization & Deployment**

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

## Testing

The bot includes comprehensive testing capabilities to verify profitability calculations and simulate various liquidation scenarios.

### Unit Tests

Run the profitability calculation tests to verify the bot's decision-making logic:

```bash
# Run all profitability tests with detailed output
cargo test liquidation::profitability::tests -- --nocapture

# Run a specific test scenario
cargo test test_profitable_liquidation_scenario -- --nocapture
```

**Available Test Scenarios:**
- `test_profitable_liquidation_scenario` - Low gas, high profit liquidation
- `test_unprofitable_high_gas_scenario` - High gas making liquidation unprofitable  
- `test_small_liquidation_rejection` - Small amounts below minimum thresholds
- `test_same_asset_liquidation` - WETH->WETH liquidations (no swap slippage)
- `test_realistic_mainnet_scenario` - Real-world profit margins
- `test_edge_case_calculations` - Direct testing of calculation functions

### Interactive Demo

Run realistic liquidation scenarios with detailed profitability breakdowns:

```bash
cargo run --bin test_liquidation
```

This demo shows three scenarios:

**📊 Scenario 1: Profitable Liquidation (Low Gas)**
- User with 120 ETH collateral, 100 ETH debt, 0.96 health factor
- 10 gwei gas price
- Shows ~2.5 ETH profit after all costs

**📊 Scenario 2: Unprofitable Liquidation (High Gas)**  
- Same user position but with 1000 gwei gas price
- Gas costs (0.96 ETH) make it unprofitable vs 1 ETH minimum threshold
- Bot would skip this liquidation

**📊 Scenario 3: Realistic Mainnet Example**
- 52 ETH collateral, 45 ETH debt position  
- 25 gwei gas (typical mainnet)
- Shows actual profit margins you'd see in production

### What the Tests Show

The tests demonstrate the **real profitability logic** that considers:

- **✅ Liquidation Bonus**: 5% for WETH, 4.5% for USDC (from Aave protocol)
- **✅ Flash Loan Fees**: 0.05% charged by Aave for borrowing  
- **✅ Gas Costs**: Real gas price × gas limit (800k) × 1.2 priority fee
- **✅ DEX Slippage**: 1% estimated slippage for asset swaps
- **✅ Minimum Thresholds**: Configurable minimum profit requirements

### When to Run Tests

- **Before deploying**: Verify profitability logic is working correctly
- **After config changes**: Test new profit thresholds or gas strategies  
- **During development**: Add new asset pairs or liquidation strategies
- **For education**: Understand how liquidation economics work

### Example Test Output

```bash
🧪 PROFITABLE LIQUIDATION TEST:
   Debt to cover: 500000000000000000000 wei
   Expected collateral: 525000000000000000000 wei  
   Liquidation bonus: 25000000000000000000 wei
   Flash loan fee: 250000000000000000 wei
   Gas cost: 960000000000000 wei
   Swap slippage: 5250000000000000000 wei
   NET PROFIT: 19499040000000000000 wei
   Profitable: true
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

