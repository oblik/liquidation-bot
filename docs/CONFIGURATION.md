# Configuration Reference

Complete guide to configuring the Aave v3 Liquidation Bot environment variables, parameters, and settings for optimal performance.

## 🔧 Environment Variables

### Network Configuration

```bash
# Base Mainnet (Production & Development)
RPC_URL=https://mainnet.base.org
WS_URL=wss://mainnet.base.org

# Dedicated Providers (Recommended for all environments)
RPC_URL=https://base-mainnet.g.alchemy.com/v2/YOUR_API_KEY
WS_URL=wss://base-mainnet.g.alchemy.com/v2/YOUR_API_KEY
```

**Configuration Notes:**
- If `WS_URL` is not specified, the bot automatically converts `RPC_URL` by replacing `http://` with `ws://` and `https://` with `wss://`
- WebSocket is required for real-time monitoring; HTTP polling is used as fallback
- Dedicated providers (Alchemy, QuickNode) are recommended over public endpoints

### Security & Authentication

```bash
# Private key for bot wallet (REQUIRED)
PRIVATE_KEY=0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef

# Optional: Target specific user for testing
TARGET_USER=0x1234567890123456789012345678901234567890
```

⚠️ **Security Warning**: 
- Never commit private keys to version control
- Use separate keys for testing and production
- Consider hardware wallets or key management services for production
- Regularly rotate keys and withdraw accumulated profits

### Contract Configuration

```bash
# Your deployed AaveLiquidator contract address
LIQUIDATOR_CONTRACT=0x1234567890123456789012345678901234567890
```

**Notes:**
- Contract must be deployed before running liquidations
- Use Foundry deployment: `forge script script/Deploy.s.sol --broadcast`
- Single contract deployment on Base mainnet for all environments

### Database Configuration

```bash
# SQLite (Development)
DATABASE_URL=sqlite:liquidation_bot.db

# PostgreSQL (Production)
DATABASE_URL=postgresql://username:password@localhost/liquidation_bot
```

**Database Tables Created:**
- `user_positions` - Real-time user health factor tracking
- `liquidation_events` - Historical liquidation records  
- `monitoring_events` - Bot activity and error logs

### Liquidation Behavior

```bash
# Minimum profit threshold in wei (default: 0.01 ETH)
MIN_PROFIT_THRESHOLD=10000000000000000

# Gas price multiplier for transaction priority (default: 2)
GAS_PRICE_MULTIPLIER=2

# Health factor threshold for "at risk" alerts (default: 1.1)
HEALTH_FACTOR_THRESHOLD=1100000000000000000

# Monitoring interval in seconds (default: 5)
MONITORING_INTERVAL_SECS=5
```

**Parameter Explanations:**
- `MIN_PROFIT_THRESHOLD`: Minimum expected profit before executing liquidation
- `GAS_PRICE_MULTIPLIER`: Multiplier for competitive gas pricing (1 = market rate)
- `HEALTH_FACTOR_THRESHOLD`: Health factor below which users are flagged as "at risk"
- `MONITORING_INTERVAL_SECS`: How often to perform periodic health checks

### Logging Configuration

```bash
# Standard operation
RUST_LOG=info

# Debug mode (includes WebSocket events and position updates)
RUST_LOG=debug

# Maximum verbosity (development only)
RUST_LOG=trace

# Selective logging
RUST_LOG=liquidation_bot=debug,sqlx=warn
```

## 📋 Configuration Examples

### Development Setup (Base Mainnet)

```bash
# Network
RPC_URL=https://mainnet.base.org
WS_URL=wss://mainnet.base.org

# Security
PRIVATE_KEY=0x...your_development_private_key

# Contract
LIQUIDATOR_CONTRACT=0x...your_mainnet_contract

# Behavior
MIN_PROFIT_THRESHOLD=1000000000000000    # 0.001 ETH for development
HEALTH_FACTOR_THRESHOLD=1200000000000000000  # 1.2 for safer development
GAS_PRICE_MULTIPLIER=2
MONITORING_INTERVAL_SECS=10

# Database
DATABASE_URL=sqlite:liquidation_bot_dev.db

# Logging
RUST_LOG=debug
```

### Production Setup (Base Mainnet)

```bash
# Network (use dedicated provider)
RPC_URL=https://base-mainnet.g.alchemy.com/v2/YOUR_API_KEY
WS_URL=wss://base-mainnet.g.alchemy.com/v2/YOUR_API_KEY

# Security
PRIVATE_KEY=0x...your_production_private_key

# Contract
LIQUIDATOR_CONTRACT=0x...your_mainnet_contract

# Behavior
MIN_PROFIT_THRESHOLD=10000000000000000   # 0.01 ETH minimum
HEALTH_FACTOR_THRESHOLD=1100000000000000000  # 1.1 for early detection
GAS_PRICE_MULTIPLIER=3                   # Higher priority for mainnet
MONITORING_INTERVAL_SECS=3               # Faster monitoring

# Database
DATABASE_URL=postgresql://liquidation_user:secure_password@localhost/liquidation_bot

# Logging
RUST_LOG=info
```

### High-Performance Setup

```bash
# Optimized for high-volume liquidations
MIN_PROFIT_THRESHOLD=5000000000000000    # 0.005 ETH - lower threshold
HEALTH_FACTOR_THRESHOLD=1050000000000000000  # 1.05 - more aggressive
GAS_PRICE_MULTIPLIER=4                   # Highest priority
MONITORING_INTERVAL_SECS=2               # Very fast monitoring
RUST_LOG=warn                           # Minimal logging overhead
```

## 🌐 Network Information

### Base Mainnet
- **Chain ID**: 8453
- **RPC URL**: `https://mainnet.base.org`
- **WebSocket**: `wss://mainnet.base.org`
- **Aave Pool**: `0xA238Dd80C259a72e81d7e4664a9801593F98d1c5`
- **Block Time**: ~2 seconds
- **Currency**: ETH

**Note**: All development and testing is performed on Base mainnet. Use appropriate profit thresholds and monitoring intervals for your development environment.

## 🎛️ Advanced Configuration

### Performance Tuning

#### High-Volume Monitoring
```bash
MONITORING_INTERVAL_SECS=3
HEALTH_FACTOR_THRESHOLD=1050000000000000000  # More aggressive
GAS_PRICE_MULTIPLIER=3
RUST_LOG=info  # Reduce logging overhead
```

#### Conservative Strategy
```bash
MONITORING_INTERVAL_SECS=10
HEALTH_FACTOR_THRESHOLD=1200000000000000000  # Safer threshold  
GAS_PRICE_MULTIPLIER=2
MIN_PROFIT_THRESHOLD=50000000000000000  # Higher profit requirement
```

#### Debug/Development
```bash
MONITORING_INTERVAL_SECS=5
HEALTH_FACTOR_THRESHOLD=1200000000000000000
GAS_PRICE_MULTIPLIER=2
RUST_LOG=debug
TARGET_USER=0x...  # Focus on specific user
```

### Database Optimization

#### SQLite Configuration
```bash
DATABASE_URL=sqlite:liquidation_bot.db?cache=shared&mode=rwc
```

#### PostgreSQL Configuration
```bash
# With connection pooling
DATABASE_URL=postgresql://user:pass@localhost/liquidation_bot?pool_max_conns=10&pool_min_conns=2
```

### WebSocket Configuration

The bot automatically handles WebSocket connectivity:

- **Real-time Mode**: When `WS_URL` uses `wss://` protocol
- **Polling Mode**: Fallback when WebSocket unavailable
- **Auto-retry**: Automatic reconnection on connection loss

## 🔍 Monitoring & Alerts

### Event Processing Pipeline

1. **WebSocket Subscription** → Real-time event detection
2. **Event Parser** → Extract affected user addresses  
3. **Position Update** → Refresh user health factors
4. **Database Storage** → Persist position data
5. **Opportunity Detection** → Identify liquidatable positions
6. **Profit Calculation** → Validate economic viability
7. **Execution** → Submit liquidation transaction

### Monitored Events

- **Borrow** - New loans trigger user monitoring
- **Supply** - Collateral deposits affect health factors
- **Repay** - Debt repayments improve user health
- **Withdraw** - Collateral removals create liquidation opportunities
- **LiquidationCall** - Track competitive liquidations
- **Oracle Price Updates** - Market volatility triggers reassessment

## 🛠️ Troubleshooting

### WebSocket Issues
```bash
# Test WebSocket connectivity
wscat -c $WS_URL

# Check bot logs for connection status
grep "WebSocket" bot.log

# Verify fallback to polling
grep "polling" bot.log
```

### Database Issues
```bash
# Test SQLite connectivity
sqlite3 $DATABASE_URL ".tables"

# Test PostgreSQL connectivity  
psql $DATABASE_URL -c "SELECT version();"

# Check table creation
sqlite3 $DATABASE_URL "SELECT name FROM sqlite_master WHERE type='table';"
```

### Performance Issues
```bash
# Monitor memory usage
ps aux | grep liquidation-bot

# Monitor database size
du -h liquidation_bot.db

# Check event processing rate
grep "Status Report" bot.log | tail -5
```

### Configuration Validation

The bot validates configuration on startup:

```
INFO liquidation_bot: Configuration loaded
INFO liquidation_bot: ✅ Signer ready for transaction signing
INFO liquidation_bot: HTTP Provider connected to: https://mainnet.base.org
INFO liquidation_bot: WebSocket will connect to: wss://mainnet.base.org
```

Common validation errors:
- Invalid private key format
- Unreachable RPC endpoints
- Database connection failures
- Missing required environment variables

## 📊 Configuration Impact

| Setting | Low Value | High Value | Impact |
|---------|-----------|------------|---------|
| `MIN_PROFIT_THRESHOLD` | More liquidations | Fewer, more profitable | Risk vs Reward |
| `HEALTH_FACTOR_THRESHOLD` | Later detection | Earlier warning | Timing vs Noise |
| `GAS_PRICE_MULTIPLIER` | Lower costs | Faster execution | Cost vs Speed |
| `MONITORING_INTERVAL_SECS` | Real-time | Battery saving | Responsiveness vs Resources |

## 🔒 Security Considerations

### Private Key Security
- Use dedicated wallet for bot operations
- Minimum balance required for gas fees
- Regular withdrawal of accumulated profits
- Monitor for unusual transaction patterns

### Network Security  
- Use HTTPS/WSS endpoints only
- Consider VPN for additional privacy
- Monitor for RPC rate limiting
- Have backup RPC providers configured

### Operational Security
- Monitor bot health continuously
- Set up alerting for failures
- Regular backups of database
- Keep software updated

## 📈 Optimization Guidelines

1. **Start Conservative**: Use higher thresholds initially
2. **Monitor Performance**: Track success/failure rates
3. **Adjust Gradually**: Tune parameters based on market conditions
4. **Benchmark**: Compare against manual calculations
5. **Document Changes**: Keep configuration change log

For additional details, see the [Setup Guide](SETUP.md) and [Architecture Overview](ARCHITECTURE.md). 