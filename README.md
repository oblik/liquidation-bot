# Aave v3 Liquidation Bot

A production-ready Aave v3 liquidation bot for Base network, built with Rust and modern Ethereum libraries. The bot provides real-time monitoring, profitable liquidation detection, and automated execution using flash loans.

## 🎯 Features

- **🔴 Real-Time Monitoring**: WebSocket-based event listening with HTTP polling fallback
- **💰 Profitability Engine**: Advanced profit calculations including gas, fees, and slippage
- **⚡ Flash Loan Liquidations**: Atomic liquidations via deployed smart contract
- **📊 Database Persistence**: SQLite/PostgreSQL support for position tracking
- **🔧 Multi-Network Support**: Base mainnet and Sepolia testnet ready
- **🛡️ Production Hardened**: Comprehensive error handling and recovery mechanisms

## 📋 Quick Start

### Prerequisites
- Rust 1.70+
- Node.js and npm (for smart contracts)
- Base network RPC access (HTTP + WebSocket)
- Private key with ETH for gas fees

### 1. Installation
```bash
git clone <repository>
cd liquidation-bot
cargo build --release
npm install
```

### 2. Configuration
Create a `.env` file:
```bash
# Network Configuration
RPC_URL=https://mainnet.base.org
WS_URL=wss://mainnet.base.org

# Security
PRIVATE_KEY=your_private_key_here

# Optional Configuration
LIQUIDATOR_CONTRACT=0xYourDeployedContractAddress
MIN_PROFIT_THRESHOLD=10000000000000000  # 0.01 ETH in wei
DATABASE_URL=sqlite:liquidation_bot.db
RUST_LOG=info
```

### 3. Deploy Smart Contract (if needed)
```bash
npm run compile
npm run deploy  # Automatically detects network
```

### 4. Run the Bot
```bash
# Production mode
cargo run --release

# Debug mode
RUST_LOG=debug cargo run

# Test liquidation scenarios
cargo run --bin test_liquidation
```

## 🏗️ Architecture

The bot consists of three main components:

1. **Rust Bot** (`src/`) - Real-time monitoring and decision engine
2. **Smart Contract** (`contracts-foundry/`) - Flash loan liquidation execution
3. **Database** - Position tracking and event persistence

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   Rust Bot      │    │  Smart Contract  │    │    Database     │
│                 │    │                  │    │                 │
│ • Event Monitor │◄──►│ • Flash Loans    │    │ • User Positions│
│ • Profitability │    │ • Liquidations   │    │ • Events Log    │
│ • Decision Logic│    │ • Uniswap Swaps  │    │ • History       │
└─────────────────┘    └──────────────────┘    └─────────────────┘
```

## 📊 Current Status

**✅ Phase 2: Complete** - Production-ready liquidation system
- Real-time WebSocket monitoring with fallback
- Complete profitability calculations
- Smart contract integration with flash loans
- Database persistence and user tracking
- Oracle price monitoring
- Multi-asset liquidation support

**🔄 Phase 3: In Progress** - Production hardening and optimization
- Enhanced error handling and retry mechanisms
- Dynamic gas pricing strategies
- Expanded asset support
- Testing and simulation framework

## 🧪 Testing

The bot includes comprehensive testing capabilities:

```bash
# Run profitability calculation tests
cargo test liquidation::profitability::tests -- --nocapture

# Interactive liquidation scenarios
cargo run --bin test_liquidation

# Test specific scenarios
cargo test test_profitable_liquidation_scenario -- --nocapture
```

Example test output shows real profitability logic considering liquidation bonuses, flash loan fees, gas costs, and DEX slippage.

## 🌐 Network Support

| Network | Chain ID | Status | Aave Pool Address |
|---------|----------|--------|-------------------|
| Base Mainnet | 8453 | ✅ Production | `0xA238Dd80C259a72e81d7e4664a9801593F98d1c5` |
| Base Sepolia | 84532 | ✅ Testnet | `0x07eA79F68B2B3df564D0A34F8e19D9B1e339814b` |

## 📚 Documentation

- **[Setup Guide](docs/SETUP.md)** - Detailed installation and configuration
- **[Configuration Reference](docs/CONFIGURATION.md)** - Complete environment variable guide  
- **[Architecture Overview](docs/ARCHITECTURE.md)** - Technical implementation details
- **[Testing Guide](docs/TESTING.md)** - Testing and simulation documentation
- **[Roadmap](docs/ROADMAP.md)** - Development phases and future features

## 🔧 Key Configuration

Essential environment variables:

```bash
# Network (Required)
RPC_URL=https://mainnet.base.org          # HTTP RPC endpoint
WS_URL=wss://mainnet.base.org            # WebSocket for real-time events

# Security (Required)  
PRIVATE_KEY=your_private_key_here         # Bot wallet private key

# Liquidation (Optional)
LIQUIDATOR_CONTRACT=0x...                 # Your deployed contract
MIN_PROFIT_THRESHOLD=10000000000000000    # Minimum profit in wei (0.01 ETH)
HEALTH_FACTOR_THRESHOLD=1100000000000000000 # At-risk threshold (1.1)
```

## 🚨 Recent Bug Fixes

Several critical issues have been resolved:

- **Memory Leak**: Fixed unbounded task spawning in processing guards
- **Address Mismatch**: Made contract addresses network-configurable  
- **WebSocket Fallback**: Implemented getLogs-based polling for reliability

See `bugs-fixed.md` for detailed technical information.

## 🛡️ Security

- Private key management with hardware wallet support
- Reentrancy guards and access controls in smart contracts
- Slippage protection and deadline controls for swaps
- Comprehensive input validation and error handling

## 📈 Performance

- **Real-time**: WebSocket events processed within milliseconds
- **Efficient**: Concurrent architecture handles multiple users simultaneously  
- **Resilient**: Automatic fallback to HTTP polling if WebSocket fails
- **Cost-effective**: L2Pool optimization provides 60%+ gas savings

## 🤝 Contributing

1. Follow the [roadmap](docs/ROADMAP.md) for current priorities
2. Check [bugs-fixed.md](bugs-fixed.md) for resolved issues
3. Add tests for new features
4. Update documentation as needed

## 📄 License

[License information to be added]
