# Liquidation Bot with Circuit Breaker Protection

An intelligent liquidation bot for Aave v3 on Base mainnet with advanced circuit breaker protection for extreme market conditions.

## 🚨 NEW FEATURE: Circuit Breaker for Extreme Market Conditions

The bot now includes a sophisticated circuit breaker system that automatically halts operations during black-swan events, preventing erratic behavior and excessive gas spending.

### Circuit Breaker Features

- **🔍 Real-time Market Monitoring**: Continuously monitors price volatility, liquidation frequency, and gas prices
- **⚡ Automatic Triggering**: Activates when extreme conditions are detected (configurable thresholds)
- **🛡️ Safe Mode Operation**: Gracefully suspends liquidations while maintaining system integrity
- **🔄 Smart Recovery**: Automatic transition through testing phases before resuming normal operations
- **📊 Comprehensive Reporting**: Detailed statistics and health scoring
- **🚨 Alert System**: Configurable notifications for circuit breaker activations

### Quick Setup

Add these environment variables to enable circuit breaker protection:

```bash
# Enable circuit breaker (recommended for production)
CIRCUIT_BREAKER_ENABLED=true

# Conservative settings for high safety
MAX_PRICE_VOLATILITY_THRESHOLD=5.0          # 5% price volatility triggers protection
MAX_LIQUIDATIONS_PER_MINUTE=5               # Max 5 liquidations per minute
CIRCUIT_BREAKER_MONITORING_WINDOW_SECS=300  # 5-minute monitoring window
CIRCUIT_BREAKER_COOLDOWN_SECS=600           # 10-minute cooldown period
MAX_GAS_PRICE_MULTIPLIER=3                  # 3x gas price spike triggers protection
```

### Circuit Breaker States

- **🟢 Closed**: Normal operation, all liquidations allowed
- **🔴 Open**: Emergency mode, all liquidations blocked
- **🟡 Half-Open**: Testing recovery, limited operations
- **⚪ Disabled**: Manual override (emergency use only)

### Black Swan Protection Examples

1. **Flash Crash**: 20% ETH drop triggers volatility protection
2. **Gas Spike**: Network congestion causes 10x gas prices → automatic halt
3. **Liquidation Cascade**: Flood of liquidations triggers rate limiting

## Original Features

### Features

- **Real-time monitoring** of user positions on Aave v3
- **WebSocket-based event listening** for immediate liquidation opportunities
- **Profitable liquidation detection** with gas cost considerations
- **Database persistence** for position tracking and analysis
- **Flexible asset configuration** supporting dynamic loading from protocol
- **Comprehensive logging** and error handling
- **Health factor monitoring** with configurable thresholds

### Architecture

The bot consists of several key components:

1. **Event Monitoring**: WebSocket connections to track protocol events
2. **Position Scanner**: Periodic scanning of user positions
3. **Liquidation Engine**: Profit calculation and execution logic
4. **Circuit Breaker**: Market condition monitoring and protection
5. **Database Layer**: SQLite for data persistence

### Configuration

The bot is configured through environment variables:

```bash
# Basic Configuration
RPC_URL=https://mainnet.base.org
WS_URL=wss://mainnet.base.org
PRIVATE_KEY=your_private_key_here
DATABASE_URL=sqlite:liquidation_bot.db

# Liquidation Settings
MIN_PROFIT_THRESHOLD=10000000000000000  # 0.01 ETH minimum profit
HEALTH_FACTOR_THRESHOLD=1100000000000000000  # 1.1 health factor
GAS_PRICE_MULTIPLIER=2

# Circuit Breaker Settings (NEW)
CIRCUIT_BREAKER_ENABLED=true
MAX_PRICE_VOLATILITY_THRESHOLD=10.0
MAX_LIQUIDATIONS_PER_MINUTE=10
CIRCUIT_BREAKER_MONITORING_WINDOW_SECS=300
CIRCUIT_BREAKER_COOLDOWN_SECS=300
MAX_GAS_PRICE_MULTIPLIER=5
```

### Asset Loading

The bot supports multiple asset loading methods:

- **Dynamic**: Load from Aave protocol (recommended)
- **Hardcoded**: Use predefined configurations
- **File-based**: Load from external JSON configuration

### Usage

1. Clone the repository
2. Set up environment variables in `.env`
3. Run the bot: `cargo run`

### Circuit Breaker Monitoring

The bot provides comprehensive monitoring:

```
🔒 Circuit Breaker Status Report:
   State: Closed
   Health Score: 85/100
   Conditions Improving: true
   Total Activations: 3
   Liquidations Blocked: 15
   Current Volatility: 2.5% (max: 10.0%)
   Liquidations/min: 2 (max: 10)
   Gas Multiplier: 2x (max: 5x)
```

### Safety Features

- **Circuit breaker protection** against extreme market conditions
- **Gas price monitoring** to prevent expensive transactions
- **Profit threshold enforcement** to ensure profitability
- **Health factor validation** before liquidation attempts
- **Comprehensive error handling** with graceful recovery

### Documentation

- [Circuit Breaker Guide](docs/circuit_breaker.md) - Complete circuit breaker documentation
- [Configuration Reference](docs/configuration.md) - All configuration options
- [API Reference](docs/api.md) - Bot API and monitoring endpoints

### Development

Run tests with:
```bash
cargo test
```

The circuit breaker includes comprehensive test coverage for all states and conditions.

### Risk Management

The circuit breaker provides multiple layers of protection:

1. **Volatility Protection**: Halts during sudden price movements
2. **Rate Limiting**: Prevents participation in liquidation cascades  
3. **Gas Protection**: Avoids expensive transactions during network congestion
4. **Manual Override**: Emergency controls for operators

### Contributing

Contributions are welcome! Please ensure any new features include:

- Comprehensive tests
- Documentation updates  
- Circuit breaker compatibility
- Safety considerations

### License

This project is licensed under the MIT License.
