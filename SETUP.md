# Liquidation Bot Setup Guide

## Overview

This is an Aave v3 liquidation bot for Base network written in Rust using alloy-rs. The bot monitors user health factors and can execute profitable liquidations using flash loans.

## Current Status

✅ **Completed:**
- Basic Rust workspace with alloy-rs integration
- Health factor monitoring for target users
- Smart contract for flash loan liquidations (`AaveLiquidator.sol`)
- Comprehensive logging and error handling
- Modular architecture ready for expansion

🔄 **In Progress:**
- Contract deployment and integration
- Event-driven monitoring system
- Profitability estimation
- Database persistence

## Quick Start

### 1. Prerequisites

- Rust 1.70+ installed
- Node.js and npm (for contract compilation)
- Base network RPC access
- Private key with some ETH for gas

### 2. Environment Setup

Create a `.env` file with the following variables:

```bash
# Base RPC URL (required)
RPC_URL=https://mainnet.base.org

# Private key for the bot wallet (required)
# WARNING: Keep this secure and never commit to version control
PRIVATE_KEY=your_private_key_here

# Target user address to monitor (optional, for testing)
TARGET_USER=0x1234567890123456789012345678901234567890

# Liquidator contract address (optional, set after deployment)
LIQUIDATOR_CONTRACT=0xYourLiquidatorContractAddressHere

# Minimum profit threshold in wei (optional, default: 5 ETH)
MIN_PROFIT_THRESHOLD=5000000000000000000

# Gas price multiplier for competitive bidding (optional, default: 2)
GAS_PRICE_MULTIPLIER=2

# Logging level (optional)
RUST_LOG=info
```

### 3. Install Dependencies

```bash
# Install Rust dependencies
cargo build

# Install Node.js dependencies for contract compilation
npm install
```

### 4. Deploy Smart Contract

```bash
# Compile contracts
npm run compile

# Deploy to Base mainnet
npm run deploy

# Copy the deployed contract address to your .env file as LIQUIDATOR_CONTRACT
```

### 5. Run the Bot

```bash
# Run in monitoring mode
cargo run --release

# Run with debug logging
RUST_LOG=debug cargo run
```

## Architecture

The bot consists of several key components:

### 1. Smart Contract (`AaveLiquidator.sol`)
- Implements `IFlashLoanReceiver` for Aave flash loans
- Executes atomic liquidations with Uniswap swaps
- Uses L2Pool encoding for gas efficiency on Base
- Includes profit withdrawal and security features

### 2. Monitoring System (`src/main.rs`)
- Checks user health factors via Aave Pool contract
- Configurable monitoring intervals
- Structured logging with tracing

### 3. Configuration
- Environment-based configuration
- Support for multiple networks (Base mainnet/testnet)
- Flexible profit thresholds and gas strategies

## Key Features

### Flash Loan Liquidations
The `AaveLiquidator` contract can:
- Borrow assets via Aave flash loans
- Execute liquidations using L2Pool (gas optimized)
- Swap collateral via Uniswap V3
- Automatically repay flash loans
- Extract and secure profits

### Monitoring
- Real-time health factor monitoring
- Support for specific target users (testing)
- Extensible to event-driven monitoring
- Comprehensive logging

### Safety Features
- Owner-only contract functions
- Reentrancy protection
- Slippage protection on swaps
- Minimum profit thresholds
- Emergency withdrawal functions

## Development Roadmap

### Phase 1: Core Infrastructure ✅
- [x] Smart contract development
- [x] Basic Rust integration
- [x] Health factor monitoring
- [x] Configuration system

### Phase 2: Event Monitoring 🔄
- [ ] WebSocket event subscriptions
- [ ] Oracle price monitoring
- [ ] User position tracking
- [ ] Database persistence

### Phase 3: Advanced Features
- [ ] Profit estimation algorithms
- [ ] Gas optimization strategies
- [ ] Multi-asset liquidations
- [ ] Performance metrics

### Phase 4: Production
- [ ] Comprehensive testing
- [ ] Security audits
- [ ] Deployment automation
- [ ] Monitoring dashboards

## Network Information

### Base Mainnet
- **RPC URL:** `https://mainnet.base.org`
- **Chain ID:** 8453
- **Aave Pool:** `0xA238Dd80C259a72e81d7e4664a9801593F98d1c5`
- **Block Time:** ~2 seconds

### Base Sepolia (Testnet)
- **RPC URL:** `https://sepolia.base.org`
- **Chain ID:** 84532
- **Aave Pool:** `0x07eA79F68B2B3df564D0A34F8e19D9B1e339814b`
- **Deployed Liquidator Contract:** `0x4818d1cb788C733Ae366D6d1D463EB48A0544528`

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

## Troubleshooting

### Common Issues

1. **RPC Connection Errors**
   - Verify RPC_URL is correct and accessible
   - Check network connectivity
   - Try alternative RPC endpoints

2. **Transaction Failures**
   - Ensure sufficient ETH balance for gas
   - Check gas price settings
   - Verify contract addresses

3. **Health Factor Parsing**
   - Confirm target user has active positions
   - Check Aave protocol status
   - Verify ABI compatibility

### Logs and Debugging

Set `RUST_LOG=debug` for detailed logging:
```bash
RUST_LOG=debug cargo run
```

## Contributing

This project follows the roadmap outlined in `docs/ROADMAP.md`. Key areas for contribution:

1. **Event System:** Implement WebSocket-based event monitoring
2. **Database:** Add PostgreSQL/SQLite integration for persistence
3. **Profitability:** Enhance profit calculation algorithms
4. **Testing:** Add comprehensive test coverage
5. **Documentation:** Improve setup and operational guides

## Support

For issues and questions:
1. Check the troubleshooting section above
2. Review the comprehensive research in `docs/liquidation-bot-research.md`
3. Examine the roadmap in `docs/ROADMAP.md` 