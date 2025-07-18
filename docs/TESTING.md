# Testing Guide

Comprehensive guide to testing the Aave v3 liquidation bot, including unit tests, integration tests, and simulation frameworks.

## 🧪 Testing Overview

The liquidation bot includes multiple layers of testing to ensure reliability and correctness:

1. **Unit Tests** - Individual component testing
2. **Integration Tests** - End-to-end workflow testing  
3. **Profitability Tests** - Economic calculation validation
4. **Simulation Tests** - Real-world scenario modeling
5. **Load Tests** - Performance and stress testing

## 🚀 Quick Testing

### Run All Tests
```bash
# Run complete test suite
cargo test

# Run with output for debugging
cargo test -- --nocapture

# Run specific test module
cargo test liquidation::profitability::tests
```

### Interactive Testing
```bash
# Run liquidation scenario simulator
cargo run --bin test_liquidation

# This shows realistic profit calculations with detailed breakdowns
```

## 📊 Profitability Tests

The most critical tests validate the economic logic that determines whether liquidations are profitable.

### Test Categories

#### 1. Basic Profitability Scenarios
```bash
cargo test liquidation::profitability::tests -- --nocapture
```

**Available Tests:**
- `test_profitable_liquidation_scenario` - Low gas, high profit
- `test_unprofitable_high_gas_scenario` - High gas making liquidation unprofitable  
- `test_small_liquidation_rejection` - Small amounts below thresholds
- `test_same_asset_liquidation` - WETH→WETH (no swap slippage)
- `test_realistic_mainnet_scenario` - Real-world profit margins

#### 2. Edge Case Testing
```bash
cargo test test_edge_case_calculations -- --nocapture
```

**Covers:**
- Zero debt scenarios
- Maximum liquidation amounts
- Extreme gas prices
- Market volatility conditions

### Example Test Output

```bash
🧪 PROFITABLE LIQUIDATION TEST:
   User: 0x1234...5678
   Health Factor: 0.960000000000000000
   
   💰 LIQUIDATION ANALYSIS:
   Debt to cover: 50.000000000000000000 ETH
   Expected collateral: 52.500000000000000000 ETH
   Liquidation bonus: 2.500000000000000000 ETH (5.00%)
   
   💸 COST BREAKDOWN:
   Flash loan fee: 0.025000000000000000 ETH (0.05%)
   Gas cost: 0.000960000000000000 ETH (10 gwei)
   Swap slippage: 0.525000000000000000 ETH (1.00%)
   Total costs: 0.550960000000000000 ETH
   
   ✅ NET PROFIT: 1.949040000000000000 ETH
   Profit threshold: 0.010000000000000000 ETH
   Profitable: true ✅
```

## 🔬 Unit Tests

### Component-Specific Tests

#### Database Tests
```bash
cargo test database::tests -- --nocapture
```

Tests include:
- Connection establishment
- Table creation and migration
- User position CRUD operations
- Event logging functionality
- Data integrity and constraints

#### Configuration Tests
```bash
cargo test config::tests -- --nocapture
```

Validates:
- Environment variable parsing
- Default value handling
- Input validation and sanitization
- Network-specific configurations

#### Asset Management Tests
```bash
cargo test liquidation::assets::tests -- --nocapture
```

Verifies:
- Asset configuration loading
- Best liquidation pair selection
- Asset ID resolution
- Collateral/debt compatibility

### Mock Testing

The bot uses mocks for external dependencies during testing:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use mockall::mock;

    mock! {
        Provider {}
        
        #[async_trait]
        impl Provider for Provider {
            async fn get_gas_price(&self) -> Result<u128, Error>;
            async fn get_block_number(&self) -> Result<u64, Error>;
        }
    }

    #[tokio::test]
    async fn test_gas_estimation() {
        let mut mock_provider = MockProvider::new();
        mock_provider
            .expect_get_gas_price()
            .returning(|| Ok(20_000_000_000)); // 20 gwei
            
        // Test with mocked provider
        let result = calculate_gas_cost(&mock_provider).await;
        assert!(result.is_ok());
    }
}
```

## 🎭 Simulation Testing

### Interactive Liquidation Simulator

Run realistic liquidation scenarios:

```bash
cargo run --bin test_liquidation
```

#### Scenario 1: Profitable Liquidation (Low Gas)
- **User Position**: 120 ETH collateral, 100 ETH debt
- **Health Factor**: 0.96 (liquidatable)
- **Gas Price**: 10 gwei (low)
- **Expected Outcome**: ~2.5 ETH profit

#### Scenario 2: Unprofitable Liquidation (High Gas)  
- **User Position**: Same as above
- **Gas Price**: 1000 gwei (extremely high)
- **Expected Outcome**: Rejected due to gas costs

#### Scenario 3: Realistic Mainnet Example
- **User Position**: 52 ETH collateral, 45 ETH debt
- **Gas Price**: 25 gwei (typical mainnet)
- **Expected Outcome**: Modest but profitable liquidation

### Custom Scenario Testing

Create your own test scenarios by modifying the test binary:

```rust
// src/bin/test_liquidation.rs
async fn test_custom_scenario() -> Result<()> {
    let user_position = UserPosition {
        address: "0x1234567890123456789012345678901234567890".parse()?,
        total_collateral_base: U256::from_str("100000000000000000000")?, // 100 ETH
        total_debt_base: U256::from_str("85000000000000000000")?, // 85 ETH
        health_factor: U256::from_str("980000000000000000")?, // 0.98
        // ... other fields
    };

    // Test with your parameters
    let opportunity = calculate_liquidation_profitability(
        provider,
        &user_position,
        &weth_config,
        &usdc_config,
        U256::from_str("5000000000000000")?, // 0.005 ETH threshold
    ).await?;

    println!("Custom scenario result: profitable = {}", 
             opportunity.profit_threshold_met);
    
    Ok(())
}
```

## 🏗️ Integration Tests

### End-to-End Testing

Full workflow tests that exercise the complete liquidation pipeline:

```bash
cargo test integration::tests -- --nocapture
```

#### Test Environment Setup
```rust
#[tokio::test]
async fn test_complete_liquidation_workflow() {
    // 1. Setup test environment
    let config = create_test_config();
    let db_pool = setup_test_database().await;
    let mock_provider = setup_mock_provider();
    
    // 2. Initialize bot components
    let bot = LiquidationBot::new(mock_provider, config, test_signer).await?;
    
    // 3. Simulate user position update
    let user = create_test_user_at_risk();
    bot.update_user_position(user).await?;
    
    // 4. Trigger liquidation opportunity
    let opportunity = bot.detect_liquidation_opportunity(user.address).await?;
    assert!(opportunity.is_some());
    
    // 5. Verify profitability calculation
    let profit = calculate_expected_profit(&opportunity.unwrap()).await?;
    assert!(profit > MIN_PROFIT_THRESHOLD);
    
    // 6. Simulate liquidation execution
    let result = bot.execute_liquidation(&opportunity.unwrap()).await;
    assert!(result.is_ok());
}
```

### Database Integration Tests

Test database operations with real database instances:

```bash
# Test with SQLite
DATABASE_URL=sqlite::memory: cargo test database::integration

# Test with PostgreSQL (requires running instance)
DATABASE_URL=postgresql://test:test@localhost/test_db cargo test database::integration
```

### Network Integration Tests

Test against real blockchain networks:

```bash
# Requires mainnet RPC access
RPC_URL=https://mainnet.base.org cargo test network::integration::tests
```

⚠️ **Important**: These tests run on Base mainnet. Ensure you have sufficient ETH for gas fees and use appropriate test parameters.

## 📈 Performance Testing

### Load Testing

Simulate high-volume scenarios:

```bash
cargo test performance::load_tests -- --nocapture
```

#### Concurrent User Processing Test
```rust
#[tokio::test]
async fn test_concurrent_user_processing() {
    let user_count = 1000;
    let bot = setup_test_bot().await;
    
    // Create many users simultaneously
    let users: Vec<_> = (0..user_count)
        .map(|i| create_test_user(i))
        .collect();
    
    // Process all users concurrently
    let start = Instant::now();
    let results = future::join_all(
        users.iter().map(|user| bot.process_user_update(user.address))
    ).await;
    let duration = start.elapsed();
    
    // Verify performance metrics
    assert!(duration < Duration::from_secs(10)); // Should complete within 10s
    assert!(results.iter().all(|r| r.is_ok())); // All should succeed
    
    println!("Processed {} users in {:?}", user_count, duration);
}
```

#### Memory Usage Testing
```rust
#[tokio::test]
async fn test_memory_usage_under_load() {
    let initial_memory = get_memory_usage();
    
    // Simulate extended operation
    for _ in 0..10000 {
        let user = create_test_user_position();
        process_user_position(user).await?;
    }
    
    let final_memory = get_memory_usage();
    let memory_growth = final_memory - initial_memory;
    
    // Should not have excessive memory growth
    assert!(memory_growth < 100_000_000); // Less than 100MB growth
}
```

### Benchmark Tests

Performance benchmarks for critical operations:

```bash
cargo bench
```

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_profitability_calculation(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    c.bench_function("profitability_calculation", |b| {
        b.to_async(&rt).iter(|| async {
            let user_position = create_benchmark_user();
            let result = calculate_liquidation_profitability(
                black_box(provider),
                black_box(&user_position),
                black_box(&collateral_config),
                black_box(&debt_config),
                black_box(min_threshold),
            ).await;
            black_box(result)
        })
    });
}

criterion_group!(benches, bench_profitability_calculation);
criterion_main!(benches);
```

## 🔍 Test Data Management

### Test User Generation

Create realistic test users with various risk profiles:

```rust
pub fn create_test_users() -> Vec<UserPosition> {
    vec![
        // Healthy user
        UserPosition {
            health_factor: U256::from_str("1500000000000000000").unwrap(), // 1.5
            total_collateral_base: U256::from_str("100000000000000000000").unwrap(),
            total_debt_base: U256::from_str("60000000000000000000").unwrap(),
            is_at_risk: false,
            // ...
        },
        // At-risk user  
        UserPosition {
            health_factor: U256::from_str("1050000000000000000").unwrap(), // 1.05
            total_collateral_base: U256::from_str("100000000000000000000").unwrap(),
            total_debt_base: U256::from_str("90000000000000000000").unwrap(),
            is_at_risk: true,
            // ...
        },
        // Liquidatable user
        UserPosition {
            health_factor: U256::from_str("980000000000000000").unwrap(), // 0.98
            total_collateral_base: U256::from_str("100000000000000000000").unwrap(),
            total_debt_base: U256::from_str("95000000000000000000").unwrap(),
            is_at_risk: true,
            // ...
        },
    ]
}
```

### Market Condition Simulation

Test different market scenarios:

```rust
pub struct MarketCondition {
    pub gas_price: u128,
    pub eth_price: U256,
    pub usdc_price: U256,
    pub network_congestion: f64,
}

pub fn create_market_scenarios() -> Vec<MarketCondition> {
    vec![
        MarketCondition {
            gas_price: 10_000_000_000,  // 10 gwei - calm
            eth_price: U256::from(2000), // $2000 ETH
            usdc_price: U256::from(1),   // $1 USDC
            network_congestion: 0.3,     // 30% congestion
        },
        MarketCondition {
            gas_price: 100_000_000_000, // 100 gwei - busy
            eth_price: U256::from(1800), // $1800 ETH (volatile)
            usdc_price: U256::from(1),   // $1 USDC
            network_congestion: 0.8,     // 80% congestion
        },
        MarketCondition {
            gas_price: 500_000_000_000, // 500 gwei - extreme
            eth_price: U256::from(1500), // $1500 ETH (crash)
            usdc_price: U256::from(1),   // $1 USDC
            network_congestion: 0.95,    // 95% congestion
        },
    ]
}
```

## 🐛 Error Testing

### Failure Scenario Testing

Test how the bot handles various failure conditions:

```bash
cargo test error_handling::tests -- --nocapture
```

#### Network Failure Tests
```rust
#[tokio::test]
async fn test_rpc_connection_failure() {
    let bot = setup_bot_with_failing_rpc().await;
    
    // Should gracefully handle RPC failures
    let result = bot.update_user_health_factor(test_user).await;
    assert!(result.is_err());
    
    // Should not crash the bot
    assert!(bot.is_running());
}

#[tokio::test]  
async fn test_websocket_disconnection() {
    let bot = setup_bot_with_unstable_websocket().await;
    
    // Should automatically fallback to HTTP polling
    tokio::time::sleep(Duration::from_secs(5)).await;
    
    // Should continue processing events
    assert!(bot.is_processing_events());
}
```

#### Database Failure Tests
```rust
#[tokio::test]
async fn test_database_disconnection() {
    let bot = setup_bot_with_database().await;
    
    // Simulate database failure
    drop_database_connection(&bot.db_pool).await;
    
    // Should handle gracefully and attempt reconnection
    let result = bot.save_user_position(test_user).await;
    assert!(result.is_err());
    
    // Should continue operation with in-memory cache
    assert!(bot.can_process_users());
}
```

## 📋 Test Configuration

### Environment Setup

Create separate test configurations:

```bash
# test.env
RPC_URL=https://mainnet.base.org
WS_URL=wss://mainnet.base.org
PRIVATE_KEY=0x1234567890abcdef... # Development key with sufficient ETH for testing
DATABASE_URL=sqlite::memory:
MIN_PROFIT_THRESHOLD=1000000000000000 # 0.001 ETH for testing
RUST_LOG=debug
```

### Continuous Integration

GitHub Actions workflow for automated testing:

```yaml
name: Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    
    services:
      postgres:
        image: postgres:13
        env:
          POSTGRES_PASSWORD: test
          POSTGRES_DB: test_db
        options: >-
          --health-cmd pg_isready
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5

    steps:
    - uses: actions/checkout@v2
    
    - uses: actions-rs/toolchain@v1
      with:
        toolchain: stable
        
    - name: Run unit tests
      run: cargo test --lib
      
    - name: Run integration tests
      run: cargo test --test integration
      env:
        DATABASE_URL: postgresql://postgres:test@localhost/test_db
        
    - name: Run profitability tests
      run: cargo test liquidation::profitability::tests -- --nocapture
```

## 🎯 Testing Best Practices

### 1. Test Isolation
- Each test should be independent
- Use separate database instances
- Clean up resources after tests

### 2. Realistic Data
- Use real-world asset prices and amounts
- Test with actual gas price ranges
- Simulate realistic network conditions

### 3. Edge Case Coverage
- Test boundary conditions
- Test with extreme values
- Test error conditions

### 4. Performance Validation
- Benchmark critical operations
- Test under load conditions
- Monitor memory usage

### 5. Security Testing
- Test with invalid inputs
- Test privilege escalation
- Test against common attacks

## 📊 Test Reporting

### Coverage Reports
```bash
# Install coverage tool
cargo install cargo-tarpaulin

# Generate coverage report
cargo tarpaulin --out Html --output-dir coverage

# View results
open coverage/tarpaulin-report.html
```

### Test Metrics
Track important testing metrics:
- **Code Coverage**: Aim for >90% line coverage
- **Test Duration**: Keep test suite under 2 minutes
- **Flakiness**: No flaky tests in CI
- **Performance**: Benchmark regressions

This comprehensive testing approach ensures the liquidation bot operates reliably under all conditions and provides confidence for production deployment.