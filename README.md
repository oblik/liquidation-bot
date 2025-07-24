# Aave v3 Liquidation Bot

A production-ready Aave v3 liquidation bot for Base network, built with Rust and modern Ethereum libraries. The bot provides real-time monitoring, profitable liquidation detection, and automated execution using flash loans.

## 🎯 Features

- **🔴 Real-Time Monitoring**: WebSocket-based event listening with HTTP polling fallback
- **💰 Profitability Engine**: Advanced profit calculations including gas, fees, and slippage
- **⚡ Flash Loan Liquidations**: Atomic liquidations via deployed smart contract
- **📊 Database Persistence**: SQLite/PostgreSQL support for position tracking
- **🗄️ Smart Archival**: Automatic cleanup of users with zero debt to prevent database bloat
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

# Asset Loading Configuration
ASSET_LOADING_METHOD=dynamic_with_fallback  # Options: dynamic_with_fallback, fully_dynamic, hardcoded, file:assets.json

# Database
DATABASE_URL=postgresql://username:password@localhost/liquidation_bot

# User Archival Configuration (optional)
ARCHIVE_ZERO_DEBT_USERS=true               # Enable archival of users with zero debt
ZERO_DEBT_COOLDOWN_HOURS=24                # Hours to wait before archiving (default: 24)
SAFE_HEALTH_FACTOR_THRESHOLD=10000000000000000000  # 10.0 ETH - minimum health factor for archival

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

## 🎯 Asset Configuration

The bot supports multiple asset loading strategies to handle new Aave reserves without redeployment:

### Loading Methods

Configure via `ASSET_LOADING_METHOD` environment variable:

#### `dynamic_with_fallback` (Default)
- Fetches asset IDs, decimals, and liquidation bonuses from Aave protocol
- Falls back to hardcoded values if protocol calls fail
- Best balance of reliability and adaptability

#### `fully_dynamic`
- Loads ALL assets dynamically from Aave protocol
- Automatically supports new reserves without code changes
- Requires stable RPC connection

#### `file:assets.json`
- Loads asset configurations from external JSON file
- Allows customization of liquidation bonuses and other parameters
- Useful for testing or custom asset configurations

#### `hardcoded`
- Uses only hardcoded asset configurations
- Most reliable but requires redeployment for new assets

### Sample Asset Configuration File

Create `assets.json` for file-based loading:

```json
{
  "assets": [
    {
      "address": "0x4200000000000000000000000000000000000006",
      "symbol": "WETH",
      "decimals": 18,
      "liquidation_bonus": 500,
      "is_collateral": true,
      "is_borrowable": true
    },
    {
      "address": "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913",
      "symbol": "USDC",
      "decimals": 6,
      "liquidation_bonus": 450,
      "is_collateral": true,
      "is_borrowable": true
    }
  ]
}
```

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

# Scanning (Optional)
AT_RISK_SCAN_LIMIT=100                    # Max users to scan per cycle (default: unlimited)
FULL_RESCAN_INTERVAL_MINUTES=60           # Full rescan every N minutes (default: 60)
```

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
