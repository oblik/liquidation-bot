# Setup Guide - Aave v3 Liquidation Bot

This guide provides detailed instructions for installing, configuring, and running the Aave v3 liquidation bot in both development and production environments.

## 📋 Prerequisites

### System Requirements
- **Rust**: Version 1.70 or higher
- **Foundry**: Latest version for smart contract development
- **Operating System**: Linux, macOS, or Windows (WSL recommended)
- **Memory**: Minimum 2GB RAM, 4GB+ recommended for production
- **Storage**: 1GB free space for dependencies and database

### Network Access
- **RPC Endpoint**: HTTP access to Base network RPC
- **WebSocket Endpoint**: WSS access for real-time monitoring (highly recommended)
- **Internet**: For package downloads and blockchain connectivity

### Accounts & Keys
- **Private Key**: Ethereum wallet with ETH for gas fees
- **RPC Provider**: Alchemy, QuickNode, or similar (public RPCs are unreliable)

## 🔧 Installation

### 1. Clone Repository
```bash
git clone <repository-url>
cd liquidation-bot
```

### 2. Install Rust Dependencies
```bash
# Build the project
cargo build --release

# Verify installation
cargo check
```

### 3. Install Foundry
```bash
# Install Foundry if not already installed
curl -L https://foundry.paradigm.xyz | bash
foundryup

# Verify Foundry installation
forge --version
cast --version
```

### 4. Initialize Database
```bash
# Database will be automatically created on first run
# For PostgreSQL, ensure the database exists first
createdb liquidation_bot  # If using PostgreSQL
```

⚠️ **Important**: This bot operates on Base mainnet for all environments. Ensure you have sufficient ETH for gas fees and use appropriate profit thresholds for testing.

## ⚙️ Configuration

### Environment Variables

Create a `.env` file in the project root:

```bash
# ===========================================
# NETWORK CONFIGURATION (Required)
# ===========================================

# HTTP RPC endpoint for transaction submission
RPC_URL=https://mainnet.base.org

# WebSocket RPC endpoint for real-time event monitoring
# If not provided, will auto-derive from RPC_URL
WS_URL=wss://mainnet.base.org

# ===========================================  
# SECURITY (Required)
# ===========================================

# Private key for the bot wallet (KEEP SECURE!)
PRIVATE_KEY=0x1234567890abcdef...

# ===========================================
# LIQUIDATION SETTINGS (Optional)
# ===========================================

# Your deployed AaveLiquidator contract address
LIQUIDATOR_CONTRACT=0x...

# Minimum profit threshold in wei (default: 0.01 ETH)
MIN_PROFIT_THRESHOLD=10000000000000000

# Health factor threshold for "at risk" detection (default: 1.1)
HEALTH_FACTOR_THRESHOLD=1100000000000000000

# Gas price multiplier for competitive bidding (default: 2)
GAS_PRICE_MULTIPLIER=2

# ===========================================
# DATABASE (Optional)
# ===========================================

# SQLite (default for development)
DATABASE_URL=sqlite:liquidation_bot.db

# PostgreSQL (recommended for production)
# DATABASE_URL=postgresql://username:password@localhost/liquidation_bot

# ===========================================
# MONITORING (Optional)
# ===========================================

# Health factor monitoring interval in seconds (default: 5)
MONITORING_INTERVAL_SECS=5

# Target user for focused monitoring (leave empty for all users)
TARGET_USER=

# ===========================================
# LOGGING (Optional)
# ===========================================

# Logging level: error, warn, info, debug, trace
RUST_LOG=info
```

### Network-Specific Configuration

#### Base Mainnet (Production & Development)
```bash
RPC_URL=https://mainnet.base.org
WS_URL=wss://mainnet.base.org
# Or use a dedicated provider (recommended):
# RPC_URL=https://base-mainnet.g.alchemy.com/v2/YOUR_API_KEY
# WS_URL=wss://base-mainnet.g.alchemy.com/v2/YOUR_API_KEY
```

### Database Setup

#### SQLite (Development)
```bash
# Default - no additional setup required
DATABASE_URL=sqlite:liquidation_bot.db
```

#### PostgreSQL (Production)
```bash
# Install PostgreSQL
sudo apt-get install postgresql postgresql-contrib  # Ubuntu/Debian
brew install postgresql                              # macOS

# Create database and user
sudo -u postgres createuser --interactive liquidation_user
sudo -u postgres createdb liquidation_bot --owner=liquidation_user

# Set connection string
DATABASE_URL=postgresql://liquidation_user:password@localhost/liquidation_bot
```

## 🚀 Smart Contract Deployment

### 1. Set Environment Variables
Ensure your `.env` file has the required variables:
```bash
RPC_URL=https://mainnet.base.org
PRIVATE_KEY=0x...
```

### 2. Compile Contracts
```bash
forge build
```

### 3. Deploy to Base Mainnet
```bash
# Deploy with verification
forge script script/Deploy.s.sol \
  --rpc-url $RPC_URL \
  --private-key $PRIVATE_KEY \
  --broadcast \
  --verify

# Or deploy without verification
forge script script/Deploy.s.sol \
  --rpc-url $RPC_URL \
  --private-key $PRIVATE_KEY \
  --broadcast
```

### 4. Verify Deployment
After deployment, you'll see output like:
```
== Logs ==
Deploying AaveLiquidator...
AaveLiquidator deployed to: 0x1234567890123456789012345678901234567890
Pool Address: 0xA238Dd80C259a72e81d7e4664a9801593F98d1c5
Swap Router: 0x2626664c2603336E57B271c5C0b26F421741e481

## Setting up 1 EVM.
==========================
Chain 8453

Estimated gas price: 0.001000007 gwei
Estimated total gas used for script: 2841234
==========================
```

Update your `.env` file with the contract address:
```bash
LIQUIDATOR_CONTRACT=0x1234567890123456789012345678901234567890
```

## 🏃 Running the Bot

### Development Mode
```bash
# Run with debug logging
RUST_LOG=debug cargo run

# Run with trace logging (very verbose)
RUST_LOG=trace cargo run
```

### Production Mode
```bash
# Run optimized build
cargo run --release

# Run in background with logging
nohup cargo run --release > bot.log 2>&1 &

# View logs
tail -f bot.log
```

### Testing Mode
```bash
# Run liquidation scenario tests
cargo run --bin test_liquidation

# Run unit tests
cargo test

# Run profitability tests with output
cargo test liquidation::profitability::tests -- --nocapture
```

## 🔍 Verification & Health Checks

### 1. Check Database Connection
```bash
# For SQLite
sqlite3 liquidation_bot.db ".tables"

# For PostgreSQL  
psql postgresql://liquidation_user:password@localhost/liquidation_bot -c "\dt"
```

### 2. Verify Network Connectivity
```bash
# Test RPC connectivity
curl -X POST $RPC_URL \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}'

# Test WebSocket (optional - requires wscat: npm install -g wscat)
# wscat -c $WS_URL

# Alternative: Test with curl (HTTP endpoint)
curl -X POST $RPC_URL \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"net_version","params":[],"id":1}'
```

### 3. Check Bot Health
When running, the bot should output:
```
INFO liquidation_bot: 🚀 Starting Aave v3 Liquidation Bot with Real-Time WebSocket Monitoring
INFO liquidation_bot: ✅ WebSocket connection established successfully!
INFO liquidation_bot: ✅ Bot initialized with signer for transaction signing capability
INFO liquidation_bot: 🔍 Performing initial user discovery...
INFO liquidation_bot: ✅ Initial discovery completed. Found X users to monitor
```

## 🛠️ Troubleshooting

### Common Issues

#### WebSocket Connection Failed
```
Error: WebSocket connection failed
```
**Solutions:**
- Verify `WS_URL` is correct and accessible
- Try a dedicated RPC provider (Alchemy, QuickNode)
- Check firewall/proxy settings
- Bot will fallback to HTTP polling automatically

#### Database Connection Error
```
Error: Database connection verification failed
```
**Solutions:**
- Check database is running (for PostgreSQL)
- Verify connection string format
- Ensure database and user exist
- Check file permissions (for SQLite)

#### RPC Rate Limiting
```
Error: Too Many Requests
```
**Solutions:**
- Use a dedicated RPC provider
- Increase `MONITORING_INTERVAL_SECS`
- Check provider rate limits

#### Contract Deployment Failed
```
Error: Transaction reverted
```
**Solutions:**
- Ensure sufficient ETH balance for gas
- Check network configuration
- Verify contract compilation succeeded

### Performance Optimization

#### For High-Volume Monitoring
```bash
MONITORING_INTERVAL_SECS=3
HEALTH_FACTOR_THRESHOLD=1050000000000000000  # 1.05
GAS_PRICE_MULTIPLIER=3
RUST_LOG=info
```

#### For Development/Testing  
```bash
MONITORING_INTERVAL_SECS=10
HEALTH_FACTOR_THRESHOLD=1200000000000000000  # 1.2
GAS_PRICE_MULTIPLIER=2
RUST_LOG=debug
```

## 🔒 Security Best Practices

### Private Key Management
- Never commit private keys to version control
- Use hardware wallets for production
- Consider key management services (AWS KMS, etc.)
- Regularly rotate keys and withdraw profits

### Environment Security
- Restrict `.env` file permissions: `chmod 600 .env`
- Use separate keys for testing and production
- Monitor for unusual transaction patterns
- Set up alerts for bot failures

### Database Security
- Use strong PostgreSQL credentials
- Restrict database access by IP
- Regular backups of position data
- Encrypt database connections in production

## 📊 Monitoring & Alerting

### Log Monitoring
```bash
# Monitor for errors
tail -f bot.log | grep ERROR

# Monitor liquidation opportunities
tail -f bot.log | grep "LIQUIDATION OPPORTUNITY"

# Monitor profitability decisions
tail -f bot.log | grep "profitable\|rejected"
```

### Database Monitoring
```sql
-- Check user positions count
SELECT COUNT(*) FROM user_positions;

-- Check at-risk users
SELECT COUNT(*) FROM user_positions WHERE is_at_risk = true;

-- Recent liquidation events
SELECT * FROM liquidation_events ORDER BY timestamp DESC LIMIT 10;
```

### Performance Monitoring
- Monitor memory usage: `htop` or `ps aux | grep liquidation-bot`
- Monitor database size: `du -h liquidation_bot.db`
- Monitor network connectivity uptime
- Set up alerts for bot downtime

## 🚀 Production Deployment

### Systemd Service (Linux)
Create `/etc/systemd/system/liquidation-bot.service`:
```ini
[Unit]
Description=Aave v3 Liquidation Bot
After=network.target

[Service]
Type=simple
User=ubuntu
WorkingDirectory=/home/ubuntu/liquidation-bot
Environment=PATH=/home/ubuntu/.cargo/bin:/usr/bin
ExecStart=/home/ubuntu/.cargo/bin/cargo run --release
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
```

Enable and start:
```bash
sudo systemctl enable liquidation-bot
sudo systemctl start liquidation-bot
sudo systemctl status liquidation-bot
```

### Docker Deployment
```dockerfile
FROM rust:1.70

WORKDIR /app
COPY . .
RUN cargo build --release

CMD ["cargo", "run", "--release"]
```

Build and run:
```bash
docker build -t liquidation-bot .
docker run -d --env-file .env --name liquidation-bot liquidation-bot
```

## 📞 Support

If you encounter issues:

1. Check the [Configuration Reference](CONFIGURATION.md) for detailed parameter explanations
2. Review the [Architecture Overview](ARCHITECTURE.md) for technical details
3. Check `bugs-fixed.md` for known resolved issues
4. Enable debug logging: `RUST_LOG=debug cargo run`
5. Examine database contents for user discovery issues