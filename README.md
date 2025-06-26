# Aave v3 Liquidation Bot for Base Network

This is an Aave v3 liquidation bot for the Base network, written in Rust using the modern [alloy](https://github.com/alloy-rs/alloy) Ethereum libraries. The bot monitors user health factors and executes profitable liquidations using flash loans.

## ✅ Current Status - **PRODUCTION READY**

**All Phases Complete** 🎉
- [x] **Smart Contract (`AaveLiquidator.sol`)**: Deployed and tested for flash loan liquidations.
- [x] **Rust Bot Foundation**: Complete health factor monitoring and execution.
- [x] **Real-Time Event Monitoring**: WebSocket subscriptions to all Aave Pool events.
- [x] **Dynamic User Discovery**: Automatically detects and monitors all active Aave users.
- [x] **Database Integration**: Full SQLite/PostgreSQL support with position tracking.
- [x] **Oracle Price Monitoring**: Chainlink price feed integration for market volatility response.
- [x] **Complete Profitability Calculation**: Real calculations including gas, fees, and slippage.
- [x] **Full Liquidation Execution**: Smart contract integration with transaction management.
- [x] **Base Mainnet Migration**: Ready for production deployment.

## 🏗️ **Architecture Overview**

### **Three-Stage Monitoring Pipeline**
1. **Discovery Module** (`src/monitoring/discovery.rs`)
   - Initial user detection via historical event scanning
   - One-time startup process (scans last 50k blocks)
   - Populates database with existing Aave users

2. **Scanner Module** (`src/monitoring/scanner.rs`) 
   - Continuous health factor monitoring
   - Periodic position updates (every 30 seconds)
   - Event-driven position changes from WebSocket

3. **Opportunity Module** (`src/liquidation/opportunity.rs`)
   - Liquidation profitability analysis  
   - Smart contract execution
   - Transaction confirmation and logging

### **Why Multiple Modules Log At-Risk Users:**
- **Discovery**: "Found this user during startup scan"
- **Scanner**: "This user's position changed, still at-risk" 
- **Opportunity**: "This user is now liquidatable (HF < 1.0)"

Each serves a different purpose in the liquidation pipeline.

## 🚀 **Quick Start**

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
WS_URL=wss://mainnet.base.org
PRIVATE_KEY=your_private_key_here

# Liquidation Settings
LIQUIDATOR_CONTRACT=0xYourDeployedContractAddress
MIN_PROFIT_THRESHOLD=1000000000000000000  # 1 ETH in wei
GAS_PRICE_MULTIPLIER=2

# Optional
TARGET_USER=0x1234567890123456789012345678901234567890
RUST_LOG=info
```

### 3. Deploy Smart Contract
```bash
# Compile contracts
npm run compile

# Deploy to Base mainnet
npm run deploy
# Update .env with the deployed contract address
```

### 4. Run the Bot
```bash
# Production mode
cargo run --release

# Debug mode with verbose logging
RUST_LOG=debug cargo run
```

## 🧪 **Testing & Validation**

### Profitability Testing
```bash
# Run profitability calculation tests
cargo test liquidation::profitability::tests -- --nocapture

# Interactive liquidation demo
cargo run --bin test_liquidation
```

### Test Scenarios Included:
- ✅ Profitable liquidation (low gas)
- ✅ Unprofitable liquidation (high gas)  
- ✅ Small position rejection
- ✅ Same-asset liquidation
- ✅ Realistic mainnet scenarios

## 🎯 **Key Features**

### **Smart Contract Features**
- **Flash Loan Integration**: Borrows assets via Aave for zero-capital liquidations
- **L2Pool Optimization**: Uses Base-specific encoding for reduced gas costs
- **Uniswap V3 Swaps**: Automatically converts collateral to debt assets
- **Security**: Owner-only functions, reentrancy guards, slippage protection

### **Rust Bot Features**
- **Real-Time Monitoring**: WebSocket subscriptions to Aave events
- **Multi-Asset Support**: WETH, USDC, cbBTC, USDbC on Base mainnet
- **Intelligent Profitability**: Accounts for gas, fees, slippage, liquidation bonus
- **Database Persistence**: Position tracking and liquidation history
- **Concurrent Architecture**: High-performance async processing
- **Error Resilience**: Graceful fallbacks and retry logic

## 📊 **Supported Assets (Base Mainnet)**

| Asset | Address | Liquidation Bonus | Status |
|-------|---------|-------------------|--------|
| WETH | `0x4200000000000000000000000000000000000006` | 5.0% | ✅ Active |
| USDC | `0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913` | 4.5% | ✅ Active |
| cbBTC | `0xcbb7c0000ab88b473b1f5afd9ef808440eed33bf` | 7.5% | ✅ Active |
| USDbC | `0xd9aAEc86B65D86f6A7B5B1b0c42FFA531710b6CA` | 4.5% | ✅ Active |

## ⚠️ **Production Considerations**

### **MEV Protection** (Important!)
- Consider private mempool submission
- Monitor for liquidation competition
- Implement competitive gas strategies

### **Rate Limiting**
- Use paid RPC providers (Alchemy, Infura)
- Implement multiple endpoint failover
- Monitor RPC call quotas

### **Risk Management**
- Set maximum liquidation amounts
- Implement daily profit/loss limits
- Add emergency shutdown mechanisms

## 📋 **Network Information**

**Base Mainnet**
- Chain ID: 8453
- RPC: `https://mainnet.base.org`
- Aave Pool: `0xA238Dd80C259a72e81d7e4664a9801593F98d1c5`
- Block Time: ~2 seconds

## 📖 **Documentation**

- **[Setup Guide](SETUP.md)**: Detailed installation and configuration
- **[Configuration](docs/CONFIGURATION.md)**: Complete environment variable reference
- **[Migration Summary](MAINNET_MIGRATION_SUMMARY.md)**: Base mainnet deployment guide
- **[Liquidation Implementation](LIQUIDATION_IMPLEMENTATION.md)**: Technical implementation details

## 📈 **Expected Performance**

**Typical Liquidation Opportunity:**
- Liquidation Bonus: 4.5-7.5% depending on asset
- Flash Loan Fee: 0.05% of borrowed amount
- Gas Cost: ~0.01-0.02 ETH (depending on network conditions)
- DEX Slippage: ~1% for asset swaps
- **Net Profit Margin**: 2-5% of liquidated amount

## 🎉 **Status: Production Ready**

Your liquidation bot is architecturally sound and ready for mainnet deployment. The multi-module design ensures comprehensive monitoring while the smart contract integration provides real execution capability.

**Next Steps:**
1. Deploy to mainnet with your funded wallet
2. Monitor performance and profitability
3. Consider MEV protection for competitive advantage
4. Scale with multiple RPC endpoints and advanced gas strategies

