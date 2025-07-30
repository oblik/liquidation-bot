use liquidation_bot::circuit_breaker::{CircuitBreaker, CircuitBreakerState};
use liquidation_bot::config::BotConfig;
use alloy_primitives::U256;
use std::time::Duration;
use tokio::time::sleep;

/// Demo configuration for circuit breaker testing
fn create_demo_config() -> BotConfig {
    BotConfig {
        rpc_url: "http://localhost:8545".to_string(),
        ws_url: "ws://localhost:8546".to_string(),
        private_key: "0x0000000000000000000000000000000000000000000000000000000000000001".to_string(),
        liquidator_contract: None,
        min_profit_threshold: U256::from(1000000000000000000u64), // 1 ETH
        gas_price_multiplier: 2,
        target_user: None,
        database_url: "sqlite::memory:".to_string(),
        health_factor_threshold: U256::from(1100000000000000000u64), // 1.1
        monitoring_interval_secs: 60,
        asset_loading_method: liquidation_bot::config::AssetLoadingMethod::Hardcoded,
        at_risk_scan_limit: Some(100),
        full_rescan_interval_minutes: 30,
        archive_zero_debt_users: false,
        zero_debt_cooldown_hours: 24,
        safe_health_factor_threshold: U256::from(10000000000000000000u64), // 10.0
        circuit_breaker_enabled: true,
        max_price_volatility_threshold: 5.0, // 5% for demo
        max_liquidations_per_minute: 3, // Low threshold for demo
        circuit_breaker_monitoring_window_secs: 60,
        circuit_breaker_cooldown_secs: 10, // Short cooldown for demo
        min_gas_price_multiplier: 1,
        max_gas_price_multiplier: 3, // Low threshold for demo
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing for demo
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    println!("🚀 Circuit Breaker Demo Starting...\n");

    // Scenario 1: Normal Market Conditions
    println!("📊 Scenario 1: Normal Market Conditions");
    demo_normal_conditions().await?;

    println!("\n" + "=".repeat(50) + "\n");

    // Scenario 2: Price Volatility Trigger
    println!("📊 Scenario 2: Extreme Price Volatility");
    demo_price_volatility().await?;

    println!("\n" + "=".repeat(50) + "\n");

    // Scenario 3: Liquidation Flood
    println!("📊 Scenario 3: Liquidation Flood");
    demo_liquidation_flood().await?;

    println!("\n" + "=".repeat(50) + "\n");

    // Scenario 4: Gas Price Spike
    println!("📊 Scenario 4: Gas Price Spike");
    demo_gas_spike().await?;

    println!("\n" + "=".repeat(50) + "\n");

    // Scenario 5: Recovery Process
    println!("📊 Scenario 5: Recovery Process");
    demo_recovery_process().await?;

    println!("\n🎉 Circuit Breaker Demo Complete!");
    Ok(())
}

async fn demo_normal_conditions() -> Result<(), Box<dyn std::error::Error>> {
    let config = create_demo_config();
    let circuit_breaker = CircuitBreaker::new(config);

    println!("Initial state: {:?}", circuit_breaker.get_state());
    println!("Liquidations allowed: {}", circuit_breaker.is_liquidation_allowed());

    // Record normal market data
    let base_price = U256::from(50000 * 10u128.pow(18)); // $50,000
    circuit_breaker.record_market_data(Some(base_price), false, Some(2)).await?;

    // Small price change (under threshold)
    let normal_price = U256::from(50200 * 10u128.pow(18)); // $50,200 (+0.4%)
    circuit_breaker.record_market_data(Some(normal_price), false, Some(2)).await?;

    // One liquidation (under threshold)
    circuit_breaker.record_market_data(None, true, Some(2)).await?;

    sleep(Duration::from_millis(100)).await;

    println!("After normal activity:");
    println!("  State: {:?}", circuit_breaker.get_state());
    println!("  Health Score: {}/100", circuit_breaker.get_health_score());
    circuit_breaker.log_status();

    Ok(())
}

async fn demo_price_volatility() -> Result<(), Box<dyn std::error::Error>> {
    let config = create_demo_config();
    let circuit_breaker = CircuitBreaker::new(config);

    // Record initial price
    let initial_price = U256::from(50000 * 10u128.pow(18)); // $50,000
    circuit_breaker.record_market_data(Some(initial_price), false, Some(2)).await?;

    println!("Recording extreme price volatility...");
    
    // Record volatile price change (10% drop, above 5% threshold)
    let crash_price = U256::from(45000 * 10u128.pow(18)); // $45,000 (-10%)
    circuit_breaker.record_market_data(Some(crash_price), false, Some(2)).await?;

    sleep(Duration::from_millis(200)).await;

    println!("After price crash:");
    println!("  State: {:?}", circuit_breaker.get_state());
    println!("  Health Score: {}/100", circuit_breaker.get_health_score());
    
    if circuit_breaker.get_state() == CircuitBreakerState::Open {
        println!("  ✅ Circuit breaker correctly activated due to price volatility!");
    }

    let stats = circuit_breaker.get_stats();
    println!("  Volatility triggers: {}", stats.volatility_triggers);

    Ok(())
}

async fn demo_liquidation_flood() -> Result<(), Box<dyn std::error::Error>> {
    let config = create_demo_config();
    let circuit_breaker = CircuitBreaker::new(config);

    println!("Recording liquidation flood...");

    // Record multiple liquidations quickly (above threshold of 3/minute)
    for i in 1..=5 {
        println!("  Recording liquidation {}/5", i);
        circuit_breaker.record_market_data(None, true, Some(2)).await?;
        sleep(Duration::from_millis(100)).await;
    }

    sleep(Duration::from_millis(200)).await;

    println!("After liquidation flood:");
    println!("  State: {:?}", circuit_breaker.get_state());
    println!("  Health Score: {}/100", circuit_breaker.get_health_score());
    
    if circuit_breaker.get_state() == CircuitBreakerState::Open {
        println!("  ✅ Circuit breaker correctly activated due to liquidation flood!");
    }

    let stats = circuit_breaker.get_stats();
    println!("  Liquidation flood triggers: {}", stats.liquidation_flood_triggers);

    Ok(())
}

async fn demo_gas_spike() -> Result<(), Box<dyn std::error::Error>> {
    let config = create_demo_config();
    let circuit_breaker = CircuitBreaker::new(config);

    println!("Recording gas price spike...");

    // Record high gas price (5x, above threshold of 3x)
    circuit_breaker.record_market_data(None, false, Some(5)).await?;

    sleep(Duration::from_millis(200)).await;

    println!("After gas spike:");
    println!("  State: {:?}", circuit_breaker.get_state());
    println!("  Health Score: {}/100", circuit_breaker.get_health_score());
    
    if circuit_breaker.get_state() == CircuitBreakerState::Open {
        println!("  ✅ Circuit breaker correctly activated due to gas spike!");
    }

    let stats = circuit_breaker.get_stats();
    println!("  Gas spike triggers: {}", stats.gas_spike_triggers);

    Ok(())
}

async fn demo_recovery_process() -> Result<(), Box<dyn std::error::Error>> {
    let config = create_demo_config();
    let circuit_breaker = CircuitBreaker::new(config);

    // First trigger the circuit breaker
    println!("Triggering circuit breaker...");
    for _ in 0..5 {
        circuit_breaker.record_market_data(None, true, Some(2)).await?;
    }
    
    sleep(Duration::from_millis(100)).await;
    println!("Circuit breaker state: {:?}", circuit_breaker.get_state());

    // Wait for transition to half-open (cooldown is 10 seconds in demo)
    println!("Waiting for cooldown period (10 seconds)...");
    sleep(Duration::from_secs(11)).await;

    println!("After cooldown:");
    println!("  State: {:?}", circuit_breaker.get_state());
    
    if circuit_breaker.get_state() == CircuitBreakerState::HalfOpen {
        println!("  ✅ Successfully transitioned to half-open state!");
    }

    // Record normal conditions to trigger recovery
    println!("Recording normal market conditions...");
    circuit_breaker.record_market_data(None, false, Some(2)).await?;

    sleep(Duration::from_millis(200)).await;

    println!("After normal conditions:");
    println!("  State: {:?}", circuit_breaker.get_state());
    
    if circuit_breaker.get_state() == CircuitBreakerState::Closed {
        println!("  ✅ Successfully recovered to normal operation!");
    }

    println!("  Conditions improving: {}", circuit_breaker.are_conditions_improving());
    println!("  Final health score: {}/100", circuit_breaker.get_health_score());

    Ok(())
}

/// Example of manual control
#[allow(dead_code)]
async fn demo_manual_control() -> Result<(), Box<dyn std::error::Error>> {
    let config = create_demo_config();
    let circuit_breaker = CircuitBreaker::new(config);

    println!("Manual Control Demo:");
    
    // Manual disable
    println!("Manually disabling circuit breaker...");
    circuit_breaker.disable().await?;
    println!("  State: {:?}", circuit_breaker.get_state());
    println!("  Liquidations allowed: {}", circuit_breaker.is_liquidation_allowed());

    // Manual enable
    println!("Manually enabling circuit breaker...");
    circuit_breaker.enable().await?;
    println!("  State: {:?}", circuit_breaker.get_state());
    println!("  Liquidations allowed: {}", circuit_breaker.is_liquidation_allowed());

    // Reset
    println!("Resetting circuit breaker...");
    circuit_breaker.reset().await?;
    println!("  State: {:?}", circuit_breaker.get_state());

    Ok(())
}

/// Example of status monitoring
#[allow(dead_code)]
fn demo_status_monitoring(circuit_breaker: &CircuitBreaker) {
    println!("Status Monitoring Demo:");
    
    let report = circuit_breaker.get_status_report();
    println!("Full Status Report:");
    println!("  State: {:?}", report.state);
    println!("  Total Activations: {}", report.stats.total_activations);
    println!("  Liquidations Blocked: {}", report.stats.total_liquidations_blocked);
    println!("  Current Volatility: {:?}%", report.current_conditions.current_volatility_percent);
    println!("  Liquidations/min: {}", report.current_conditions.current_liquidations_per_minute);
    println!("  Gas Multiplier: {:?}x", report.current_conditions.current_gas_multiplier);
    println!("  Data Points: {}", report.current_conditions.data_points_count);
    
    println!("Thresholds:");
    println!("  Max Volatility: {}%", report.thresholds.max_price_volatility_threshold);
    println!("  Max Liquidations/min: {}", report.thresholds.max_liquidations_per_minute);
    println!("  Max Gas Multiplier: {}x", report.thresholds.max_gas_price_multiplier);
}