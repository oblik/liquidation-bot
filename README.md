# Aave v3 Liquidation Bot

A production-ready Aave v3 liquidation bot for Base network, built with Rust and modern Ethereum libraries. The bot provides real-time monitoring, profitable liquidation detection, and automated execution using flash loans.

## 🎯 Features

- **🔴 Real-Time Monitoring**: WebSocket-based event listening with HTTP polling fallback
- **💰 Profitability Engine**: Advanced profit calculations including gas, fees, and slippage
- **⚡ Flash Loan Liquidations**: Atomic liquidations via deployed smart contract
- **📊 Database Persistence**: SQLite/PostgreSQL support for position tracking
- **🔧 Base Mainnet Optimized**: Fully optimized for Base network with L2Pool integration
- **🛡️ Production Hardened**: Comprehensive error handling and recovery mechanisms
- **🚀 PostgreSQL Migration**: Built-in migration tool for upgrading from SQLite to PostgreSQL

## 📋 Quick Start

### Prerequisites
- Rust 1.70+
- Foundry (for smart contract development)
- Base mainnet RPC access (HTTP + WebSocket)
- Private key with sufficient ETH for gas fees
- PostgreSQL

⚠️ **Important**: This bot operates exclusively on Base mainnet for all environments including development and testing.

### 1. Installation
```bash
git clone <repository>
cd liquidation-bot
cargo build --release

# Install Foundry if not already installed
curl -L https://foundry.paradigm.xyz | bash
foundryup
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

# Database
DATABASE_URL=postgresql://username:password@localhost/liquidation_bot

RUST_LOG=info
```

### 3. Deploy Smart Contract (if needed)
```bash
# Compile contracts
forge build

# Deploy to Base mainnet
forge script script/Deploy.s.sol --rpc-url $RPC_URL --private-key $PRIVATE_KEY --broadcast --verify
```

### 4. Run the Bot
```bash
# Production mode
cargo run --release

# Debug mode
RUST_LOG=debug cargo run
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

## 🛠️ Development

### Code Quality Tools

This project uses Rust's standard linting and formatting tools to maintain high code quality:

```bash
# Format code
cargo fmt

# Run linting
cargo clippy --all-targets --all-features -- -D warnings

# Run all quality checks
./scripts/check.sh

# Auto-fix issues where possible
./scripts/fix.sh
```

### Configuration

- **`clippy.toml`**: Clippy linting rules optimized for financial software
- **`rustfmt.toml`**: Code formatting configuration for consistency
- **`.github/workflows/ci.yml`**: CI pipeline with automated quality checks

### Pre-commit Requirements

All code must pass these checks before being committed:
- ✅ Code formatting (`cargo fmt`)
- ✅ Clippy linting with no warnings
- ✅ All tests passing
- ✅ Successful release build

## 🤝 Contributing

Please read [CONTRIBUTING.md](CONTRIBUTING.md) for detailed development guidelines including:

- Development setup and prerequisites
- Code quality standards and tools
- Testing strategies and requirements
- Pull request workflow
- Security considerations for financial software

Quick contribution steps:
1. Follow the [roadmap](docs/ROADMAP.md) for current priorities
2. Check [bugs-fixed.md](bugs-fixed.md) for resolved issues
3. Ensure all quality checks pass: `./scripts/check.sh`
4. Add comprehensive tests for new features
5. Update documentation as needed

## 📄 License

[License information to be added]
