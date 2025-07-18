# Architecture Overview

Technical deep-dive into the Aave v3 Liquidation Bot architecture, components, and implementation details.

## 🏗️ System Architecture

The liquidation bot is built as a distributed system with three main components that work together to provide real-time liquidation capabilities:

```
                    ┌─────────────────────────────────────────────────┐
                    │                 Rust Bot                        │
                    │  ┌──────────────────────────────────────────┐   │
                    │  │           Event Monitoring               │   │
                    │  │  • WebSocket Subscriptions             │   │
                    │  │  • HTTP Polling Fallback               │   │
                    │  │  • Oracle Price Tracking               │   │
                    │  └──────────────────────────────────────────┘   │
                    │  ┌──────────────────────────────────────────┐   │
                    │  │        Decision Engine                   │   │
                    │  │  • Health Factor Analysis               │   │
                    │  │  • Profitability Calculation           │   │
                    │  │  • Risk Assessment                      │   │
                    │  └──────────────────────────────────────────┘   │
                    │  ┌──────────────────────────────────────────┐   │
                    │  │       Execution Engine                   │   │
                    │  │  • Transaction Management               │   │
                    │  │  • Gas Optimization                     │   │
                    │  │  • Error Recovery                       │   │
                    │  └──────────────────────────────────────────┘   │
                    └─────────────────────────────────────────────────┘
                                              │
                    ┌─────────────────────────────────────────────────┐
                    │                 Blockchain                      │
                    │  ┌──────────────────────────────────────────┐   │
                    │  │         Smart Contract                   │   │
                    │  │  • Flash Loan Receiver                  │   │
                    │  │  • Liquidation Logic                    │   │
                    │  │  • Uniswap Integration                  │   │
                    │  │  • Profit Extraction                    │   │
                    │  └──────────────────────────────────────────┘   │
                    │  ┌──────────────────────────────────────────┐   │
                    │  │           Aave Protocol                  │   │
                    │  │  • L2Pool (Gas Optimized)               │   │
                    │  │  • Flash Loan Provider                  │   │
                    │  │  • Liquidation Engine                   │   │
                    │  └──────────────────────────────────────────┘   │
                    └─────────────────────────────────────────────────┘
                                              │
                    ┌─────────────────────────────────────────────────┐
                    │                  Database                       │
                    │  ┌──────────────────────────────────────────┐   │
                    │  │          Data Layer                      │   │
                    │  │  • User Position Tracking               │   │
                    │  │  • Event History                        │   │
                    │  │  • Performance Metrics                  │   │
                    │  │  • Configuration State                  │   │
                    │  └──────────────────────────────────────────┘   │
                    └─────────────────────────────────────────────────┘
```

## 🦀 Rust Bot Architecture

### Core Components

#### 1. Main Bot (`src/bot.rs`)
```rust
pub struct LiquidationBot<P> {
    provider: Arc<P>,                                    // HTTP provider
    ws_provider: Arc<dyn Provider>,                     // WebSocket provider  
    signer: PrivateKeySigner,                           // Transaction signer
    config: BotConfig,                                  // Configuration
    pool_contract: ContractInstance<...>,               // Aave pool interface
    db_pool: Pool<Sqlite>,                              // Database connection
    user_positions: Arc<DashMap<Address, UserPosition>>, // In-memory cache
    processing_users: Arc<SyncRwLock<HashSet<Address>>>, // Concurrency control
    event_tx: mpsc::UnboundedSender<BotEvent>,          // Event channel
    price_feeds: Arc<DashMap<Address, PriceFeed>>,      // Oracle data
    liquidation_assets: HashMap<Address, LiquidationAssetConfig>, // Asset configs
}
```

**Key Responsibilities:**
- Coordinate all bot components
- Manage event processing pipeline
- Handle configuration and state
- Provide transaction signing capabilities

#### 2. Event Monitoring (`src/monitoring/`)

##### WebSocket Monitoring (`websocket.rs`)
```rust
pub async fn start_event_monitoring<P>(
    provider: Arc<P>,
    ws_provider: Arc<dyn Provider>,
    ws_url: &str,
    event_tx: mpsc::UnboundedSender<BotEvent>,
) -> Result<()>
```

**Event Flow:**
1. **WebSocket Connection** → Establishes persistent connection
2. **Event Subscription** → Subscribes to Aave Pool events
3. **Event Processing** → Extracts user addresses and triggers updates
4. **Fallback Handling** → Switches to HTTP polling if WebSocket fails

**Monitored Events:**
- `Borrow` - New loans taken
- `Supply` - Collateral deposits  
- `Repay` - Debt repayments
- `Withdraw` - Collateral withdrawals
- `LiquidationCall` - Competitive liquidations

##### Scanner (`scanner.rs`)
```rust
pub async fn run_periodic_scan<P>(
    provider: Arc<P>,
    pool_address: Address,
    db_pool: Pool<Sqlite>,
    event_tx: mpsc::UnboundedSender<BotEvent>,
    config: BotConfig,
    asset_configs: HashMap<Address, AssetConfig>,
) -> Result<()>
```

**Scanning Process:**
1. **Periodic Health Checks** → Validates user health factors
2. **Risk Assessment** → Identifies at-risk positions
3. **Database Sync** → Updates position data
4. **Opportunity Detection** → Triggers liquidation events

##### Oracle Monitoring (`oracle.rs`)
```rust
pub async fn start_oracle_monitoring<P>(
    provider: Arc<P>,
    ws_provider: Arc<dyn Provider>,
    ws_url: &str,
    event_tx: mpsc::UnboundedSender<BotEvent>,
    asset_configs: HashMap<Address, AssetConfig>,
    price_feeds: Arc<DashMap<Address, PriceFeed>>,
) -> Result<()>
```

**Price Monitoring:**
- **Chainlink Integration** → Direct price feed monitoring
- **Threshold Detection** → Configurable price change alerts
- **User Reassessment** → Trigger health factor recalculation

#### 3. Liquidation Engine (`src/liquidation/`)

##### Profitability Calculator (`profitability.rs`)
```rust
pub async fn calculate_liquidation_profitability<P>(
    provider: Arc<P>,
    user_position: &UserPosition,
    collateral_asset: &LiquidationAssetConfig,
    debt_asset: &LiquidationAssetConfig,
    min_profit_threshold: U256,
) -> Result<LiquidationOpportunity>
```

**Calculation Components:**
1. **Liquidation Bonus** → Protocol-defined bonus (5% for WETH)
2. **Flash Loan Fee** → Aave's 0.05% fee
3. **Gas Costs** → Dynamic gas price × estimated gas limit
4. **DEX Slippage** → Estimated 1% slippage for swaps
5. **Net Profit** → Total reward minus all costs

##### Execution Engine (`executor.rs`)
```rust
pub struct LiquidationExecutor<P> {
    provider: Arc<P>,
    signer: PrivateKeySigner,
    liquidator_contract: ContractInstance<...>,
    contract_address: Address,
}
```

**Execution Flow:**
1. **Opportunity Validation** → Verify profitability and health factor
2. **Asset Resolution** → Map addresses to L2Pool asset IDs
3. **Transaction Construction** → Build liquidation parameters
4. **Gas Estimation** → Calculate optimal gas price and limit
5. **Transaction Submission** → Send to network with monitoring
6. **Confirmation Tracking** → Wait for transaction confirmation

#### 4. Database Layer (`src/database.rs`)

##### Schema Design
```sql
-- User position tracking
CREATE TABLE user_positions (
    address TEXT PRIMARY KEY,
    total_collateral_base TEXT NOT NULL,
    total_debt_base TEXT NOT NULL,
    health_factor TEXT NOT NULL,
    last_updated DATETIME NOT NULL,
    is_at_risk BOOLEAN NOT NULL DEFAULT FALSE
);

-- Liquidation history
CREATE TABLE liquidation_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_address TEXT NOT NULL,
    collateral_asset TEXT NOT NULL,
    debt_asset TEXT NOT NULL,
    profit TEXT NOT NULL,
    tx_hash TEXT,
    timestamp DATETIME NOT NULL
);

-- Bot activity logs
CREATE TABLE monitoring_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_type TEXT NOT NULL,
    user_address TEXT,
    health_factor TEXT,
    timestamp DATETIME NOT NULL,
    details TEXT
);
```

**Database Operations:**
- **Position Updates** → Real-time health factor tracking
- **Event Logging** → Comprehensive activity history
- **Performance Metrics** → Success rates and profitability analysis
- **State Persistence** → Recovery from bot restarts

#### 5. Configuration Management (`src/config.rs`)

```rust
#[derive(Debug, Clone)]
pub struct BotConfig {
    pub rpc_url: String,
    pub ws_url: String,
    pub private_key: String,
    pub liquidator_contract: Option<Address>,
    pub min_profit_threshold: U256,
    pub gas_price_multiplier: u64,
    pub health_factor_threshold: U256,
    pub monitoring_interval_secs: u64,
    // ... additional configuration
}
```

## 📜 Smart Contract Architecture

### AaveLiquidator Contract (`contracts-foundry/AaveLiquidator.sol`)

```solidity
contract AaveLiquidator is IFlashLoanReceiver, Ownable, ReentrancyGuard {
    using SafeERC20 for IERC20;

    // Immutable addresses set during deployment
    address private immutable POOL_ADDRESS;
    address private immutable ADDRESSES_PROVIDER_ADDRESS;
    address private immutable SWAP_ROUTER;
    address private immutable DATA_PROVIDER;

    // Configurable parameters
    uint256 public maxSlippage = 500;        // 5% default
    uint256 public swapDeadlineBuffer = 300; // 5 minutes
    uint24 public defaultSwapFee = 3000;     // 0.3% Uniswap fee
}
```

#### Key Functions

##### 1. Flash Loan Liquidation
```solidity
function liquidate(
    address user,
    address collateralAsset,
    address debtAsset,
    uint256 debtToCover,
    bool receiveAToken,
    uint16 collateralAssetId,
    uint16 debtAssetId
) external onlyOwner nonReentrant
```

**Execution Flow:**
1. **Parameter Validation** → Verify inputs and user health
2. **Flash Loan Request** → Borrow debt asset from Aave
3. **Liquidation Execution** → Call L2Pool.liquidationCall
4. **Collateral Swap** → Convert seized collateral to debt asset
5. **Flash Loan Repayment** → Return borrowed amount + fee
6. **Profit Extraction** → Transfer remaining balance to owner

##### 2. Flash Loan Callback
```solidity
function executeOperation(
    address[] calldata assets,
    uint256[] calldata amounts,
    uint256[] calldata premiums,
    address initiator,
    bytes calldata params
) external override returns (bool)
```

**Security Features:**
- **Caller Verification** → Only Aave Pool can call
- **Initiator Check** → Only self-initiated flash loans
- **Reentrancy Protection** → Guards against recursive calls
- **Slippage Protection** → Maximum acceptable price impact

#### Gas Optimization

##### L2Pool Integration
The contract uses Aave's L2Pool for optimized liquidations:

```solidity
// L2Pool encoding saves gas by using packed bytes32 arguments
bytes32 args1 = bytes32(
    (uint256(collateralAssetId) << 240) |
        (uint256(debtAssetId) << 224) |
        uint256(uint160(user))
);
bytes32 args2 = bytes32(
    (debtToCover << 128) | (receiveAToken ? 1 : 0)
);
IL2Pool(POOL_ADDRESS).liquidationCall(args1, args2);
```

**Gas Savings:**
- **60%+ reduction** compared to standard Pool interface
- **Optimized for L2** networks like Base
- **Batch operations** for multiple liquidations

## 🔄 Event Processing Pipeline

### Real-Time Event Flow

```
Blockchain Event → WebSocket → Event Parser → User Update → Database → Opportunity Detection
      ↓                ↓            ↓             ↓           ↓              ↓
   Borrow/Supply → WS Listener → Extract User → Health Check → Persist → Liquidation Queue
```

#### 1. Event Detection
- **WebSocket Subscription** → Real-time event streaming
- **Event Filtering** → Focus on position-changing events
- **User Extraction** → Identify affected users

#### 2. Position Updates  
- **Health Factor Calculation** → Query Aave for latest data
- **Risk Assessment** → Compare against thresholds
- **Change Detection** → Only update if significant change

#### 3. Opportunity Processing
- **Profitability Analysis** → Calculate expected profit
- **Asset Selection** → Choose optimal collateral/debt pair
- **Execution Decision** → Validate against minimum thresholds

### Concurrent Processing

The bot uses Rust's tokio for high-performance concurrency:

```rust
// Multiple concurrent tasks
tokio::try_join!(
    websocket::start_event_monitoring(...),     // Real-time events
    oracle::start_oracle_monitoring(...),       // Price monitoring  
    scanner::run_periodic_scan(...),            // Health checks
    bot.run_event_processor(),                  // Event processing
    scanner::start_status_reporter(...),        // Status reporting
)?;
```

**Concurrency Benefits:**
- **Non-blocking I/O** → Efficient network operations
- **Parallel Processing** → Multiple users simultaneously
- **Event-driven** → React immediately to blockchain events
- **Resource Efficient** → Single-threaded async runtime

## 💾 Data Flow & State Management

### In-Memory State
```rust
// Fast access for real-time operations
user_positions: Arc<DashMap<Address, UserPosition>>     // Thread-safe HashMap
processing_users: Arc<SyncRwLock<HashSet<Address>>>     // Concurrency control
price_feeds: Arc<DashMap<Address, PriceFeed>>           // Oracle price cache
```

### Database Persistence
- **Write-through Cache** → Updates both memory and database
- **Periodic Sync** → Batch database operations for efficiency
- **Recovery State** → Restore from database on restart

### Event Communication
```rust
// Internal event bus for component communication
enum BotEvent {
    UserPositionChanged(Address),
    PriceUpdate(Address, U256, U256),
    LiquidationOpportunity(Address),
    DatabaseSync(Vec<UserPosition>),
}
```

## 🔧 Performance Optimizations

### Database Optimizations
- **Connection Pooling** → Reuse database connections
- **Batch Operations** → Group multiple updates
- **Indexes** → Fast lookups by user address
- **Prepared Statements** → Avoid SQL parsing overhead

### Network Optimizations  
- **WebSocket Persistent Connections** → Avoid reconnection overhead
- **HTTP Keep-Alive** → Reuse connections for API calls
- **Provider Rotation** → Distribute load across multiple RPC endpoints
- **Rate Limiting** → Respect provider limits

### Memory Optimizations
- **DashMap** → Lock-free concurrent HashMap
- **Arc/Rc** → Shared ownership without copying
- **Fixed-size Buffers** → Avoid dynamic allocations
- **Event Channels** → Bounded queues to prevent memory leaks

### Computational Optimizations
- **U256 Math** → Optimized big integer operations
- **Lazy Evaluation** → Calculate only when needed
- **Caching** → Store frequently accessed data
- **Parallel Processing** → Utilize multiple CPU cores

## 🛡️ Security Architecture

### Smart Contract Security
- **Access Controls** → `onlyOwner` for critical functions
- **Reentrancy Guards** → Prevent recursive attacks
- **Input Validation** → Comprehensive parameter checking
- **Slippage Protection** → Maximum acceptable price impact
- **Emergency Pause** → Circuit breaker for extreme conditions

### Bot Security
- **Private Key Management** → Secure key storage and rotation
- **RPC Endpoint Security** → HTTPS/WSS only connections
- **Database Security** → Connection encryption and access controls
- **Error Handling** → Graceful failure without data exposure
- **Audit Logging** → Comprehensive activity tracking

### Operational Security
- **Health Monitoring** → Continuous bot health checks
- **Alert Systems** → Immediate notification of issues
- **Backup Strategies** → Database and configuration backups
- **Recovery Procedures** → Documented emergency procedures

## 📊 Monitoring & Observability

### Metrics Collection
- **Performance Metrics** → Latency, throughput, success rates
- **Business Metrics** → Profitability, liquidation volume, opportunities
- **System Metrics** → Memory usage, CPU usage, database performance
- **Network Metrics** → RPC response times, WebSocket connectivity

### Logging Strategy
```rust
// Structured logging with tracing
use tracing::{info, warn, error, debug};

info!("Liquidation opportunity detected: user={:?}, profit={}", user, profit);
warn!("User {:?} at risk: health_factor={}", user, health_factor);
error!("Failed to execute liquidation: {}", error);
debug!("Event processed: {:?}", event);
```

### Health Checks
- **Database Connectivity** → Verify database operations
- **RPC Connectivity** → Test blockchain connectivity  
- **WebSocket Status** → Monitor real-time connection
- **Memory Usage** → Detect potential leaks
- **Event Processing Rate** → Ensure keeping up with blockchain

## 🚀 Deployment Architecture

### Container Strategy
```dockerfile
FROM rust:1.70-slim

# Install dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Build application
WORKDIR /app
COPY . .
RUN cargo build --release

# Runtime
CMD ["./target/release/liquidation-bot"]
```

### Production Environment
- **Container Orchestration** → Docker Compose or Kubernetes
- **Environment Management** → Separate configs for dev/staging/prod
- **Secret Management** → Encrypted environment variables
- **Backup Strategy** → Automated database backups
- **Monitoring Integration** → Prometheus/Grafana dashboards

This architecture provides a robust, scalable, and maintainable foundation for automated liquidation operations on the Aave v3 protocol.x