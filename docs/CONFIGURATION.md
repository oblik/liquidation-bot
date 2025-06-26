# Configuration Guide - WebSocket Event Monitoring

This guide covers all configuration options for the Aave v3 Liquidation Bot with WebSocket-based event monitoring capabilities.

## Environment Variables

### Network Configuration

```bash
# Base Sepolia Testnet RPC URL (HTTP)
RPC_URL=https://sepolia.base.org

# WebSocket URL for real-time event monitoring (automatically derived if not set)
WS_URL=wss://sepolia.base.org

# Base Mainnet alternatives:
# RPC_URL=https://mainnet.base.org
# WS_URL=wss://mainnet.base.org
```

**Note**: If `WS_URL` is not specified, the bot will automatically convert the HTTP URL to WebSocket by replacing `http://` with `ws://` and `https://` with `wss://`.

### Security & Authentication

```bash
# Private key for the bot wallet (KEEP SECURE!)
PRIVATE_KEY=your_private_key_here
```

⚠️ **Security Warning**: Never commit your private key to version control. Keep it secure and consider using hardware wallets or key management services in production.

### Contract Addresses

```bash
# Your deployed AaveLiquidator contract address
LIQUIDATOR_CONTRACT=0x4818d1cb788C733Ae366D6d1D463EB48A0544528

# Target user address for focused monitoring (optional, for testing)
TARGET_USER=0x1234567890123456789012345678901234567890
```

### Database Configuration

```bash
# SQLite database file (default for development)
DATABASE_URL=sqlite:liquidation_bot.db

# PostgreSQL for production (recommended)
DATABASE_URL=postgresql://username:password@localhost/liquidation_bot
```

The bot automatically creates the following tables:
- `user_positions` - Real-time user health factor tracking
- `liquidation_events` - Historical liquidation records  
- `monitoring_events` - Bot activity and status logs

### Bot Behavior Settings

```bash
# Minimum profit threshold in wei (default: 5 ETH)
MIN_PROFIT_THRESHOLD=5000000000000000000

# Gas price multiplier for competitive bidding (default: 2x)
GAS_PRICE_MULTIPLIER=2

# Health factor threshold for "at risk" alerts (default: 1.1 = 110%)
HEALTH_FACTOR_THRESHOLD=1100000000000000000

# Monitoring intervals in seconds (default: 5)
MONITORING_INTERVAL_SECS=5
```

### Logging Configuration

```bash
# Standard operation
RUST_LOG=info

# Debug WebSocket events and position updates
RUST_LOG=debug

# Maximum verbosity for development
RUST_LOG=trace
```

## Configuration Examples

### Development Setup (Base Sepolia)

```bash
RPC_URL=https://sepolia.base.org
PRIVATE_KEY=your_test_private_key
LIQUIDATOR_CONTRACT=0x4818d1cb788C733Ae366D6d1D463EB48A0544528
TARGET_USER=0x1234567890123456789012345678901234567890
DATABASE_URL=sqlite:liquidation_bot.db
HEALTH_FACTOR_THRESHOLD=1200000000000000000
RUST_LOG=debug
```

### Production Setup (Base Mainnet)

```bash
RPC_URL=https://mainnet.base.org
WS_URL=wss://mainnet.base.org
PRIVATE_KEY=your_production_private_key
LIQUIDATOR_CONTRACT=your_mainnet_contract_address
DATABASE_URL=postgresql://user:pass@localhost/liquidation_bot
MIN_PROFIT_THRESHOLD=1000000000000000000
GAS_PRICE_MULTIPLIER=3
HEALTH_FACTOR_THRESHOLD=1100000000000000000
MONITORING_INTERVAL_SECS=3
RUST_LOG=info
```

## Network-Specific Information

### Base Sepolia Testnet
- **Chain ID**: 84532
- **Aave Pool**: `0xA37D7E3d3CaD89b44f9a08A96fE01a9F39Bd7794`
- **Block Time**: ~2 seconds
- **Test Liquidator**: `0x4818d1cb788C733Ae366D6d1D463EB48A0544528`

### Base Mainnet  
- **Chain ID**: 8453
- **Aave Pool**: `0xA238Dd80C259a72e81d7e4664a9801593F98d1c5`
- **Block Time**: ~2 seconds

## WebSocket Event Monitoring Features

The bot now monitors the following Aave events in real-time:

### Core Events
- **Borrow**: When users take new loans
- **Repay**: When users repay debt
- **Supply**: When users provide collateral
- **Withdraw**: When users remove collateral
- **LiquidationCall**: When liquidations occur

### Event Processing Pipeline
1. **WebSocket Subscription** → Real-time event detection
2. **Event Parser** → Extract affected user addresses
3. **Position Update** → Refresh user health factors
4. **Database Storage** → Persist position data
5. **Opportunity Detection** → Identify liquidatable positions
6. **Alert System** → Log and notify of opportunities

## Performance Tuning

### For High-Volume Monitoring
```bash
MONITORING_INTERVAL_SECS=3
HEALTH_FACTOR_THRESHOLD=1050000000000000000  # 1.05 = 105%
GAS_PRICE_MULTIPLIER=3
RUST_LOG=info
```

### For Development/Testing
```bash
MONITORING_INTERVAL_SECS=10
HEALTH_FACTOR_THRESHOLD=1200000000000000000  # 1.2 = 120%
GAS_PRICE_MULTIPLIER=2
RUST_LOG=debug
```

## Database Schema

### user_positions
```sql
CREATE TABLE user_positions (
    address TEXT PRIMARY KEY,
    total_collateral_base TEXT NOT NULL,
    total_debt_base TEXT NOT NULL,
    available_borrows_base TEXT NOT NULL,
    current_liquidation_threshold TEXT NOT NULL,
    ltv TEXT NOT NULL,
    health_factor TEXT NOT NULL,
    last_updated DATETIME NOT NULL,
    is_at_risk BOOLEAN NOT NULL DEFAULT FALSE
);
```

### liquidation_events  
```sql
CREATE TABLE liquidation_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_address TEXT NOT NULL,
    collateral_asset TEXT NOT NULL,
    debt_asset TEXT NOT NULL,
    debt_covered TEXT NOT NULL,
    collateral_received TEXT NOT NULL,
    profit TEXT NOT NULL,
    tx_hash TEXT,
    block_number INTEGER,
    timestamp DATETIME NOT NULL
);
```

### monitoring_events
```sql
CREATE TABLE monitoring_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_type TEXT NOT NULL,
    user_address TEXT,
    asset_address TEXT,
    health_factor TEXT,
    timestamp DATETIME NOT NULL,
    details TEXT
);
```

## Troubleshooting

### WebSocket Connection Issues
- Verify `WS_URL` is accessible
- Check firewall/proxy settings
- Try alternative WebSocket endpoints

### Database Connection Problems
- Ensure SQLite file has write permissions
- For PostgreSQL, verify connection string
- Check database server status

### High Memory Usage
- Reduce `MONITORING_INTERVAL_SECS` 
- Implement periodic database cleanup
- Monitor the `user_positions` cache size

### Missing Events
- Check WebSocket connection stability
- Verify RPC endpoint reliability
- Enable debug logging to trace events

## Security Considerations

1. **Private Key Management**
   - Use environment variables, never hardcode
   - Consider hardware wallets for production
   - Regularly rotate keys and withdraw profits

2. **Database Security**
   - Use strong PostgreSQL credentials
   - Restrict database access by IP
   - Regular backups of position data

3. **Network Security**
   - Use HTTPS/WSS endpoints only
   - Monitor for unusual transaction patterns
   - Set up alerts for failed transactions

4. **Operational Security**
   - Monitor bot health continuously
   - Set up alerting for bot downtime
   - Keep the bot software updated 