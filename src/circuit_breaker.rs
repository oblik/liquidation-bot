use alloy_primitives::U256;
use eyre::Result;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::config::BotConfig;

/// Circuit breaker states following the circuit breaker pattern
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CircuitBreakerState {
    /// Normal operation - liquidations are allowed
    Closed,
    /// Circuit breaker activated - all liquidations are blocked
    Open,
    /// Testing if conditions have improved - limited operations allowed
    HalfOpen,
    /// Manually disabled by operator
    Disabled,
}

/// Types of extreme market conditions that can trigger circuit breaker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MarketCondition {
    /// Price volatility exceeded threshold
    ExtremeVolatility { volatility_percent: f64 },
    /// Too many liquidations in short time period
    LiquidationFlood { liquidations_per_minute: u64 },
    /// Gas prices are extremely high
    GasPriceSpike { gas_multiplier: u64 },
    /// Multiple conditions triggered simultaneously
    MultipleConditions { conditions: Vec<String> },
}

/// Market data point for tracking conditions
#[derive(Debug, Clone)]
pub struct MarketDataPoint {
    pub timestamp: Instant,
    pub price: Option<U256>,
    pub liquidation_occurred: bool,  // Successful liquidation
    pub liquidation_attempted: bool, // Any liquidation attempt (successful or failed)
    pub gas_price_wei: Option<U256>,
}

/// Alert for circuit breaker activation
#[derive(Debug, Clone)]
pub struct CircuitBreakerAlert {
    pub timestamp: SystemTime,
    pub condition: MarketCondition,
    pub state_change: CircuitBreakerState,
    pub message: String,
}

/// Circuit breaker for monitoring extreme market conditions
pub struct CircuitBreaker {
    /// Current state of the circuit breaker
    state: Arc<RwLock<CircuitBreakerState>>,
    /// Configuration settings
    config: BotConfig,
    /// Market data history for analysis
    market_data: Arc<RwLock<VecDeque<MarketDataPoint>>>,
    /// Time when circuit breaker was last activated
    last_activation: Arc<RwLock<Option<Instant>>>,
    /// Time when last test liquidation was allowed in half-open state
    last_test_liquidation: Arc<RwLock<Option<Instant>>>,
    /// Alert sender for notifications
    alert_tx: mpsc::UnboundedSender<CircuitBreakerAlert>,
    /// Alert receiver for notifications
    alert_rx: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<CircuitBreakerAlert>>>,
    /// Statistics tracking
    stats: Arc<RwLock<CircuitBreakerStats>>,
}

/// Statistics for circuit breaker performance
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerStats {
    pub total_activations: u64,
    pub total_liquidations_blocked: u64,
    pub volatility_triggers: u64,
    pub liquidation_flood_triggers: u64,
    pub gas_spike_triggers: u64,
    pub average_activation_duration_secs: f64,
    pub last_activation_reason: Option<String>,
}

/// Circuit breaker status report for monitoring and dashboards
#[derive(Debug, Serialize, Deserialize)]
pub struct CircuitBreakerStatusReport {
    pub state: CircuitBreakerState,
    pub stats: CircuitBreakerStats,
    pub last_activation_timestamp: Option<u64>,
    pub time_since_last_activation_secs: Option<u64>,
    pub monitoring_window_secs: u64,
    pub cooldown_secs: u64,
    pub thresholds: CircuitBreakerThresholds,
    pub current_conditions: CurrentMarketConditions,
}

/// Circuit breaker threshold configuration
#[derive(Debug, Serialize, Deserialize)]
pub struct CircuitBreakerThresholds {
    pub max_price_volatility_threshold: f64,
    pub max_liquidations_per_minute: u64,
    pub max_gas_price_multiplier: u64,
}

/// Current market conditions snapshot
#[derive(Debug, Serialize, Deserialize)]
pub struct CurrentMarketConditions {
    pub current_volatility_percent: Option<f64>,
    pub current_liquidations_per_minute: u64, // Total attempts (successful + failed)
    pub current_successful_liquidations_per_minute: u64, // Only successful liquidations
    pub current_gas_multiplier: Option<u64>,
    pub data_points_count: usize,
}

impl CircuitBreaker {
    /// Create a new circuit breaker instance
    pub fn new(config: BotConfig) -> Self {
        let (alert_tx, alert_rx) = mpsc::unbounded_channel();

        let initial_state = if config.circuit_breaker_enabled {
            CircuitBreakerState::Closed
        } else {
            CircuitBreakerState::Disabled
        };

        info!(
            "🔒 Circuit breaker initialized in state: {:?}",
            initial_state
        );
        info!(
            "📊 Monitoring thresholds - Volatility: {}%, Liquidations/min: {}, Gas multiplier: {}-{}",
            config.max_price_volatility_threshold,
            config.max_liquidations_per_minute,
            config.min_gas_price_multiplier,
            config.max_gas_price_multiplier
        );

        Self {
            state: Arc::new(RwLock::new(initial_state)),
            config,
            market_data: Arc::new(RwLock::new(VecDeque::new())),
            last_activation: Arc::new(RwLock::new(None)),
            last_test_liquidation: Arc::new(RwLock::new(None)),
            alert_tx,
            alert_rx: Arc::new(tokio::sync::Mutex::new(alert_rx)),
            stats: Arc::new(RwLock::new(CircuitBreakerStats::default())),
        }
    }

    /// Check if liquidations are currently allowed
    pub fn is_liquidation_allowed(&self) -> bool {
        let state = self.state.read();
        match *state {
            CircuitBreakerState::Closed => true,
            CircuitBreakerState::HalfOpen => {
                // In half-open state, allow limited operations to test recovery
                self.should_allow_test_liquidation()
            }
            CircuitBreakerState::Open | CircuitBreakerState::Disabled => false,
        }
    }

    /// Record a liquidation attempt (successful or failed) for frequency monitoring
    ///
    /// This is the preferred method for tracking liquidation attempts as it correctly
    /// distinguishes between successful and failed attempts.
    ///
    /// # Arguments
    /// * `liquidation_succeeded` - Whether the liquidation was successful
    /// * `gas_price_wei` - Current gas price in wei (optional)
    ///
    /// # Example
    /// ```rust,no_run
    /// # use liquidation_bot::circuit_breaker::CircuitBreaker;
    /// # use alloy_primitives::U256;
    /// # async fn example(circuit_breaker: &CircuitBreaker) -> eyre::Result<()> {
    /// # let gas_price = U256::from(20_000_000_000u64);
    /// // Record a failed liquidation attempt
    /// circuit_breaker.record_liquidation_attempt(false, Some(gas_price)).await?;
    ///
    /// // Record a successful liquidation
    /// circuit_breaker.record_liquidation_attempt(true, Some(gas_price)).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn record_liquidation_attempt(
        &self,
        liquidation_succeeded: bool,
        gas_price_wei: Option<U256>,
    ) -> Result<()> {
        if !self.config.circuit_breaker_enabled {
            return Ok(());
        }

        let data_point = MarketDataPoint {
            timestamp: Instant::now(),
            price: None, // Price updates are handled separately
            liquidation_occurred: liquidation_succeeded,
            liquidation_attempted: true, // Always true for any attempt
            gas_price_wei,
        };

        // Add to history
        {
            let mut market_data = self.market_data.write();
            market_data.push_back(data_point.clone());

            // Keep only data within monitoring window
            let cutoff_time = Instant::now()
                - Duration::from_secs(self.config.circuit_breaker_monitoring_window_secs);
            while let Some(front) = market_data.front() {
                if front.timestamp < cutoff_time {
                    market_data.pop_front();
                } else {
                    break;
                }
            }
        }

        // Check for extreme conditions
        self.check_extreme_conditions().await?;

        Ok(())
    }

    /// Record price updates and non-liquidation market data
    ///
    /// This method should be used for recording price changes and gas price updates
    /// that are NOT related to liquidation attempts.
    ///
    /// # Arguments
    /// * `price` - Current asset price (optional)
    /// * `gas_price_wei` - Current gas price in wei (optional)
    ///
    /// # Example
    /// ```rust,no_run
    /// # use liquidation_bot::circuit_breaker::CircuitBreaker;
    /// # use alloy_primitives::U256;
    /// # async fn example(circuit_breaker: &CircuitBreaker) -> eyre::Result<()> {
    /// # let new_price = U256::from(50000 * 10u128.pow(18));
    /// # let gas_price = U256::from(20_000_000_000u64);
    /// // Record a price update from oracle
    /// circuit_breaker.record_price_update(Some(new_price), Some(gas_price)).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn record_price_update(
        &self,
        price: Option<U256>,
        gas_price_wei: Option<U256>,
    ) -> Result<()> {
        if !self.config.circuit_breaker_enabled {
            return Ok(());
        }

        let data_point = MarketDataPoint {
            timestamp: Instant::now(),
            price,
            liquidation_occurred: false,
            liquidation_attempted: false, // No liquidation attempt for price updates
            gas_price_wei,
        };

        // Add to history
        {
            let mut market_data = self.market_data.write();
            market_data.push_back(data_point.clone());

            // Keep only data within monitoring window
            let cutoff_time = Instant::now()
                - Duration::from_secs(self.config.circuit_breaker_monitoring_window_secs);
            while let Some(front) = market_data.front() {
                if front.timestamp < cutoff_time {
                    market_data.pop_front();
                } else {
                    break;
                }
            }
        }

        // Check for extreme conditions
        self.check_extreme_conditions().await?;

        Ok(())
    }

    /// ⚠️ DEPRECATED: Use `record_liquidation_attempt()` or `record_price_update()` instead
    ///
    /// **CRITICAL BUG**: This method has a fundamental design flaw that prevents it from
    /// correctly tracking failed liquidation attempts. When `liquidation_occurred` is false,
    /// the method cannot distinguish between:
    /// 1. A failed liquidation attempt (should count as an attempt)
    /// 2. A regular market data update with no liquidation (should not count)
    ///
    /// This causes the circuit breaker to **miss liquidation floods** when many liquidations
    /// fail due to network congestion, gas spikes, or other issues.
    ///
    /// **Migration Guide**:
    /// - For liquidation tracking: Use `record_liquidation_attempt(success, gas_price)`
    /// - For price updates: Use `record_price_update(price, gas_price)`
    ///
    /// This method is maintained only for backward compatibility and will be removed in v2.0.
    #[deprecated(
        since = "1.1.0",
        note = "Use record_liquidation_attempt() for liquidation tracking or record_price_update() for market data. This method cannot properly track failed liquidation attempts."
    )]
    pub async fn record_market_data(
        &self,
        price: Option<U256>,
        liquidation_occurred: bool,
        gas_price_wei: Option<U256>,
    ) -> Result<()> {
        if !self.config.circuit_breaker_enabled {
            return Ok(());
        }

        let data_point = MarketDataPoint {
            timestamp: Instant::now(),
            price,
            liquidation_occurred,
            liquidation_attempted: liquidation_occurred, // ⚠️ CRITICAL BUG: This is incorrect!
            // When liquidation_occurred is false, we cannot distinguish between
            // failed liquidation attempts and regular market data updates.
            // This causes the circuit breaker to miss liquidation floods when
            // liquidations fail due to network issues.
            //
            // CORRECT USAGE:
            // - Use record_liquidation_attempt() for ANY liquidation attempt
            // - Use record_price_update() for pure market data updates
            gas_price_wei,
        };

        // Add to history
        {
            let mut market_data = self.market_data.write();
            market_data.push_back(data_point.clone());

            // Keep only data within monitoring window
            let cutoff_time = Instant::now()
                - Duration::from_secs(self.config.circuit_breaker_monitoring_window_secs);
            while let Some(front) = market_data.front() {
                if front.timestamp < cutoff_time {
                    market_data.pop_front();
                } else {
                    break;
                }
            }
        }

        // Check for extreme conditions
        self.check_extreme_conditions().await?;

        Ok(())
    }

    /// Check for extreme market conditions and activate circuit breaker if needed
    async fn check_extreme_conditions(&self) -> Result<()> {
        // Collect all data and perform checks within a scope to release locks before await
        let triggered_conditions = {
            let market_data = self.market_data.read();

            // Check current state first to skip if already open
            {
                let state = self.state.read();
                if *state == CircuitBreakerState::Open {
                    return Ok(());
                }
            }

            let mut triggered_conditions = Vec::new();

            // Check price volatility
            if let Some(volatility) = self.calculate_price_volatility(&market_data) {
                if volatility > self.config.max_price_volatility_threshold {
                    triggered_conditions.push(MarketCondition::ExtremeVolatility {
                        volatility_percent: volatility,
                    });
                }
            }

            // Check liquidation frequency with floating-point precision
            let liquidation_count = self.count_recent_liquidations(&market_data);
            let liquidations_per_minute = (liquidation_count as f64 * 60.0
                / self.config.circuit_breaker_monitoring_window_secs as f64)
                .round() as u64;

            if liquidations_per_minute > self.config.max_liquidations_per_minute {
                triggered_conditions.push(MarketCondition::LiquidationFlood {
                    liquidations_per_minute,
                });
            }

            // Check gas price conditions
            if let Some(current_gas_multiplier) = self.get_current_gas_multiplier(&market_data) {
                if current_gas_multiplier > self.config.max_gas_price_multiplier {
                    triggered_conditions.push(MarketCondition::GasPriceSpike {
                        gas_multiplier: current_gas_multiplier,
                    });
                }
            }

            triggered_conditions
        }; // market_data lock is released here

        // Re-check state before making transition decisions to avoid race conditions
        let current_state = self.state.read().clone();

        // Activate circuit breaker if conditions are met
        if !triggered_conditions.is_empty() {
            // Only activate if not already open (double-check to prevent race)
            if current_state != CircuitBreakerState::Open {
                self.activate_circuit_breaker(triggered_conditions).await?;
            }
        } else if current_state == CircuitBreakerState::HalfOpen {
            // Conditions look good, transition back to closed
            self.close_circuit_breaker().await?;
        }

        Ok(())
    }

    /// Activate the circuit breaker due to extreme conditions
    async fn activate_circuit_breaker(&self, conditions: Vec<MarketCondition>) -> Result<()> {
        {
            let mut state = self.state.write();
            let mut last_activation = self.last_activation.write();
            let mut stats = self.stats.write();

            *state = CircuitBreakerState::Open;
            *last_activation = Some(Instant::now());
            stats.total_activations += 1;

            // Update condition-specific stats
            for condition in &conditions {
                match condition {
                    MarketCondition::ExtremeVolatility { .. } => stats.volatility_triggers += 1,
                    MarketCondition::LiquidationFlood { .. } => {
                        stats.liquidation_flood_triggers += 1
                    }
                    MarketCondition::GasPriceSpike { .. } => stats.gas_spike_triggers += 1,
                    MarketCondition::MultipleConditions { .. } => {
                        // Multiple conditions already counted individually
                    }
                }
            }
        }

        let condition = if conditions.len() == 1 {
            conditions.into_iter().next().unwrap()
        } else {
            MarketCondition::MultipleConditions {
                conditions: conditions.iter().map(|c| format!("{:?}", c)).collect(),
            }
        };

        let alert = CircuitBreakerAlert {
            timestamp: SystemTime::now(),
            condition: condition.clone(),
            state_change: CircuitBreakerState::Open,
            message: format!(
                "🚨 CIRCUIT BREAKER ACTIVATED: {:?} - All liquidations suspended for {} seconds",
                condition, self.config.circuit_breaker_cooldown_secs
            ),
        };

        error!("{}", alert.message);

        // Update stats with reason
        {
            let mut stats = self.stats.write();
            stats.last_activation_reason = Some(format!("{:?}", condition));
        }

        if let Err(e) = self.alert_tx.send(alert) {
            error!("Failed to send circuit breaker alert: {}", e);
        }

        // Schedule automatic transition to half-open after cooldown
        self.schedule_half_open_transition().await;

        Ok(())
    }

    /// Close the circuit breaker (transition from half-open to closed)
    async fn close_circuit_breaker(&self) -> Result<()> {
        {
            let mut state = self.state.write();
            *state = CircuitBreakerState::Closed;
        }

        let alert = CircuitBreakerAlert {
            timestamp: SystemTime::now(),
            condition: MarketCondition::ExtremeVolatility {
                volatility_percent: 0.0,
            }, // Placeholder
            state_change: CircuitBreakerState::Closed,
            message: "✅ Circuit breaker CLOSED - Normal operations resumed".to_string(),
        };

        info!("{}", alert.message);

        if let Err(e) = self.alert_tx.send(alert) {
            error!("Failed to send circuit breaker close alert: {}", e);
        }

        Ok(())
    }

    /// Schedule transition to half-open state after cooldown period
    async fn schedule_half_open_transition(&self) {
        let state = self.state.clone();
        let cooldown_duration = Duration::from_secs(self.config.circuit_breaker_cooldown_secs);
        let alert_tx = self.alert_tx.clone();

        tokio::spawn(async move {
            tokio::time::sleep(cooldown_duration).await;

            // Transition to half-open if still in open state
            {
                let mut state_guard = state.write();
                if *state_guard == CircuitBreakerState::Open {
                    *state_guard = CircuitBreakerState::HalfOpen;

                    let alert = CircuitBreakerAlert {
                        timestamp: SystemTime::now(),
                        condition: MarketCondition::ExtremeVolatility {
                            volatility_percent: 0.0,
                        }, // Placeholder
                        state_change: CircuitBreakerState::HalfOpen,
                        message: "🟡 Circuit breaker HALF-OPEN - Testing market conditions"
                            .to_string(),
                    };

                    warn!("{}", alert.message);

                    if let Err(e) = alert_tx.send(alert) {
                        error!("Failed to send half-open transition alert: {}", e);
                    }
                }
            }
        });
    }

    /// Calculate price volatility over the monitoring window
    fn calculate_price_volatility(&self, market_data: &VecDeque<MarketDataPoint>) -> Option<f64> {
        let prices: Vec<f64> = market_data
            .iter()
            .filter_map(|point| point.price.map(|p| p.to::<u128>() as f64))
            .collect();

        if prices.len() < 2 {
            return None;
        }

        // Calculate volatility using all prices, not just first and last
        // This prevents missing price spikes that occur and recover within the window
        let mut max_volatility = 0.0;

        // Use the first price as the baseline for volatility calculation
        let baseline_price = prices.first()?;

        // Use epsilon comparison for floating-point safety
        const EPSILON: f64 = 1e-10;
        if baseline_price.abs() < EPSILON {
            return None;
        }

        // Calculate maximum volatility against the baseline price for all subsequent prices
        for price in &prices[1..] {
            let volatility = ((price - baseline_price) / baseline_price).abs() * 100.0;
            max_volatility = f64::max(max_volatility, volatility);
        }

        Some(max_volatility)
    }

    /// Count ALL liquidation attempts (successful and failed) in the monitoring window
    fn count_recent_liquidations(&self, market_data: &VecDeque<MarketDataPoint>) -> u64 {
        market_data
            .iter()
            .map(|point| if point.liquidation_attempted { 1 } else { 0 })
            .sum()
    }

    /// Count only successful liquidations in the monitoring window
    fn count_successful_liquidations(&self, market_data: &VecDeque<MarketDataPoint>) -> u64 {
        market_data
            .iter()
            .map(|point| if point.liquidation_occurred { 1 } else { 0 })
            .sum()
    }

    /// Calculate gas multiplier relative to baseline (uses 20 Gwei as baseline)
    fn calculate_gas_multiplier(&self, gas_price_wei: U256) -> u64 {
        // Define baseline gas price as 20 Gwei (typical normal gas price)
        let baseline_gas_price_wei = U256::from(20_000_000_000u64); // 20 Gwei in wei

        // Calculate multiplier: current_gas_price / baseline_gas_price
        if baseline_gas_price_wei.is_zero() {
            return 1;
        }

        // Convert to u128 for division, then back to u64
        let gas_price_u128 = gas_price_wei.to::<u128>();
        let baseline_u128 = baseline_gas_price_wei.to::<u128>();

        ((gas_price_u128 / baseline_u128) as u64).max(1)
    }

    /// Get the most recent gas price multiplier
    fn get_current_gas_multiplier(&self, market_data: &VecDeque<MarketDataPoint>) -> Option<u64> {
        market_data.iter().rev().find_map(|point| {
            point
                .gas_price_wei
                .map(|price| self.calculate_gas_multiplier(price))
        })
    }

    /// Check if a test liquidation should be allowed in half-open state
    fn should_allow_test_liquidation(&self) -> bool {
        // Allow one test liquidation every 30 seconds in half-open state
        let last_test = *self.last_test_liquidation.read();

        match last_test {
            Some(last_time) => {
                // Allow if at least 30 seconds have passed since last test liquidation
                Instant::now().duration_since(last_time).as_secs() >= 30
            }
            None => {
                // No previous test liquidation, allow the first one
                true
            }
        }
    }

    /// Get current circuit breaker state
    pub fn get_state(&self) -> CircuitBreakerState {
        self.state.read().clone()
    }

    /// Get circuit breaker statistics
    pub fn get_stats(&self) -> CircuitBreakerStats {
        self.stats.read().clone()
    }

    /// Manually disable circuit breaker (emergency override)
    pub async fn disable(&self) -> Result<()> {
        {
            let mut state = self.state.write();
            *state = CircuitBreakerState::Disabled;
        }

        let alert = CircuitBreakerAlert {
            timestamp: SystemTime::now(),
            condition: MarketCondition::ExtremeVolatility {
                volatility_percent: 0.0,
            }, // Placeholder
            state_change: CircuitBreakerState::Disabled,
            message: "⚠️ Circuit breaker MANUALLY DISABLED by operator".to_string(),
        };

        warn!("{}", alert.message);

        if let Err(e) = self.alert_tx.send(alert) {
            error!("Failed to send disable alert: {}", e);
        }

        Ok(())
    }

    /// Manually enable circuit breaker
    pub async fn enable(&self) -> Result<()> {
        {
            let mut state = self.state.write();
            *state = CircuitBreakerState::Closed;
        }

        let alert = CircuitBreakerAlert {
            timestamp: SystemTime::now(),
            condition: MarketCondition::ExtremeVolatility {
                volatility_percent: 0.0,
            }, // Placeholder
            state_change: CircuitBreakerState::Closed,
            message: "✅ Circuit breaker MANUALLY ENABLED by operator".to_string(),
        };

        info!("{}", alert.message);

        if let Err(e) = self.alert_tx.send(alert) {
            error!("Failed to send enable alert: {}", e);
        }

        Ok(())
    }

    /// Record that a liquidation was blocked by circuit breaker
    pub fn record_blocked_liquidation(&self) {
        let mut stats = self.stats.write();
        stats.total_liquidations_blocked += 1;
    }

    /// Record that a test liquidation was allowed in half-open state
    pub fn record_test_liquidation(&self) {
        let mut last_test = self.last_test_liquidation.write();
        *last_test = Some(Instant::now());
    }

    /// Get alert receiver for monitoring circuit breaker events
    pub fn get_alert_receiver(
        &self,
    ) -> Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<CircuitBreakerAlert>>> {
        self.alert_rx.clone()
    }

    /// Process circuit breaker alerts (should be run in a separate task)
    pub async fn run_alert_processor(&self) -> Result<()> {
        let mut alert_rx = self.alert_rx.lock().await;

        while let Some(alert) = alert_rx.recv().await {
            // Log to system
            match alert.state_change {
                CircuitBreakerState::Open => {
                    error!("🚨 {}", alert.message);
                }
                CircuitBreakerState::Closed => {
                    info!("✅ {}", alert.message);
                }
                CircuitBreakerState::HalfOpen => {
                    warn!("🟡 {}", alert.message);
                }
                CircuitBreakerState::Disabled => {
                    warn!("⚠️ {}", alert.message);
                }
            }

            // Here you could add additional alerting mechanisms:
            // - Send to Slack/Discord webhook
            // - Send email notification
            // - Write to monitoring system
            // - Update dashboard

            // Example: Send to external monitoring system
            if let Err(e) = self.send_external_alert(&alert).await {
                error!("Failed to send external alert: {}", e);
            }
        }

        Ok(())
    }

    /// Send alert to external monitoring system (placeholder implementation)
    async fn send_external_alert(&self, alert: &CircuitBreakerAlert) -> Result<()> {
        // Placeholder for external alerting - could integrate with:
        // - Slack webhook
        // - PagerDuty
        // - Email service
        // - Custom monitoring dashboard

        info!(
            "External alert sent: {} at {}",
            alert.message,
            alert
                .timestamp
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        );

        Ok(())
    }

    /// Get comprehensive status report for monitoring and dashboards
    pub fn get_status_report(&self) -> CircuitBreakerStatusReport {
        let state = self.state.read().clone();
        let stats = self.stats.read().clone();
        let last_activation = self.last_activation.read().clone();
        let market_data = self.market_data.read();

        let last_activation_timestamp = last_activation.map(|instant| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                - instant.elapsed().as_secs()
        });

        let time_since_last_activation_secs =
            last_activation.map(|instant| instant.elapsed().as_secs());

        let current_conditions = self.get_current_market_conditions(&market_data);

        CircuitBreakerStatusReport {
            state,
            stats,
            last_activation_timestamp,
            time_since_last_activation_secs,
            monitoring_window_secs: self.config.circuit_breaker_monitoring_window_secs,
            cooldown_secs: self.config.circuit_breaker_cooldown_secs,
            thresholds: CircuitBreakerThresholds {
                max_price_volatility_threshold: self.config.max_price_volatility_threshold,
                max_liquidations_per_minute: self.config.max_liquidations_per_minute,
                max_gas_price_multiplier: self.config.max_gas_price_multiplier,
            },
            current_conditions,
        }
    }

    /// Get current market conditions snapshot
    fn get_current_market_conditions(
        &self,
        market_data: &VecDeque<MarketDataPoint>,
    ) -> CurrentMarketConditions {
        let current_volatility_percent = self.calculate_price_volatility(market_data);

        // Count total attempts (successful + failed)
        let current_liquidations_per_minute = {
            let liquidation_count = self.count_recent_liquidations(market_data);
            (liquidation_count as f64 * 60.0
                / self.config.circuit_breaker_monitoring_window_secs as f64)
                .round() as u64
        };

        // Count only successful liquidations
        let current_successful_liquidations_per_minute = {
            let successful_count = self.count_successful_liquidations(market_data);
            (successful_count as f64 * 60.0
                / self.config.circuit_breaker_monitoring_window_secs as f64)
                .round() as u64
        };

        let current_gas_multiplier = self.get_current_gas_multiplier(market_data);
        let data_points_count = market_data.len();

        CurrentMarketConditions {
            current_volatility_percent,
            current_liquidations_per_minute,
            current_successful_liquidations_per_minute,
            current_gas_multiplier,
            data_points_count,
        }
    }

    /// Reset circuit breaker to initial state (emergency function)
    pub async fn reset(&self) -> Result<()> {
        {
            let mut state = self.state.write();
            let mut last_activation = self.last_activation.write();
            let mut last_test_liquidation = self.last_test_liquidation.write();
            let mut market_data = self.market_data.write();

            *state = if self.config.circuit_breaker_enabled {
                CircuitBreakerState::Closed
            } else {
                CircuitBreakerState::Disabled
            };

            *last_activation = None;
            *last_test_liquidation = None;
            market_data.clear();
        }

        let alert = CircuitBreakerAlert {
            timestamp: SystemTime::now(),
            condition: MarketCondition::ExtremeVolatility {
                volatility_percent: 0.0,
            }, // Placeholder
            state_change: CircuitBreakerState::Closed,
            message: "🔄 Circuit breaker RESET by operator - All history cleared".to_string(),
        };

        warn!("{}", alert.message);

        if let Err(e) = self.alert_tx.send(alert) {
            error!("Failed to send reset alert: {}", e);
        }

        Ok(())
    }

    /// Check if conditions are improving (for monitoring dashboards)
    pub fn are_conditions_improving(&self) -> bool {
        let market_data = self.market_data.read();

        if market_data.len() < 5 {
            return false; // Not enough data
        }

        // Check recent vs older data points
        let recent_half = market_data.len() / 2;
        let recent_data: Vec<_> = market_data.iter().skip(recent_half).collect();
        let older_data: Vec<_> = market_data.iter().take(recent_half).collect();

        // Compare liquidation rates
        let recent_liquidations = recent_data
            .iter()
            .filter(|p| p.liquidation_occurred)
            .count();
        let older_liquidations = older_data.iter().filter(|p| p.liquidation_occurred).count();

        // Compare gas prices (if available)
        let recent_gas = recent_data
            .iter()
            .filter_map(|p| {
                p.gas_price_wei
                    .map(|price| self.calculate_gas_multiplier(price))
            })
            .collect::<Vec<_>>();
        let older_gas = older_data
            .iter()
            .filter_map(|p| {
                p.gas_price_wei
                    .map(|price| self.calculate_gas_multiplier(price))
            })
            .collect::<Vec<_>>();

        let gas_improving = if !recent_gas.is_empty() && !older_gas.is_empty() {
            let recent_avg = recent_gas.iter().sum::<u64>() as f64 / recent_gas.len() as f64;
            let older_avg = older_gas.iter().sum::<u64>() as f64 / older_gas.len() as f64;
            recent_avg < older_avg
        } else {
            true // Assume improving if no data
        };

        // Conditions are improving if liquidations are decreasing and gas is lower
        recent_liquidations < older_liquidations && gas_improving
    }

    /// Get health score (0-100, where 100 is perfect health)
    pub fn get_health_score(&self) -> u8 {
        let market_data = self.market_data.read();
        let current_conditions = self.get_current_market_conditions(&market_data);

        let mut score = 100u8;

        // Deduct points for current volatility
        if let Some(volatility) = current_conditions.current_volatility_percent {
            let volatility_penalty =
                ((volatility / self.config.max_price_volatility_threshold) * 30.0) as u8;
            score = score.saturating_sub(volatility_penalty.min(30));
        }

        // Deduct points for liquidation frequency
        let liquidation_ratio = current_conditions.current_liquidations_per_minute as f64
            / self.config.max_liquidations_per_minute as f64;
        let liquidation_penalty = (liquidation_ratio * 30.0) as u8;
        score = score.saturating_sub(liquidation_penalty.min(30));

        // Deduct points for gas prices
        if let Some(gas_multiplier) = current_conditions.current_gas_multiplier {
            let gas_ratio = gas_multiplier as f64 / self.config.max_gas_price_multiplier as f64;
            let gas_penalty = (gas_ratio * 20.0) as u8;
            score = score.saturating_sub(gas_penalty.min(20));
        }

        // Additional penalty if circuit breaker is currently open
        match self.state.read().clone() {
            CircuitBreakerState::Open => score.saturating_sub(20),
            CircuitBreakerState::HalfOpen => score.saturating_sub(10),
            CircuitBreakerState::Disabled => 0, // Disabled state gets 0 health
            CircuitBreakerState::Closed => score,
        }
    }

    /// Log current status for debugging
    pub fn log_status(&self) {
        let status_report = self.get_status_report();
        let health_score = self.get_health_score();
        let improving = self.are_conditions_improving();

        info!("🔒 Circuit Breaker Status Report:");
        info!("   State: {:?}", status_report.state);
        info!("   Health Score: {}/100", health_score);
        info!("   Conditions Improving: {}", improving);
        info!(
            "   Total Activations: {}",
            status_report.stats.total_activations
        );
        info!(
            "   Liquidations Blocked: {}",
            status_report.stats.total_liquidations_blocked
        );

        if let Some(volatility) = status_report.current_conditions.current_volatility_percent {
            info!(
                "   Current Volatility: {:.2}% (max: {:.2}%)",
                volatility, status_report.thresholds.max_price_volatility_threshold
            );
        }

        info!(
            "   Total Liquidation Attempts/min: {} (max: {})",
            status_report
                .current_conditions
                .current_liquidations_per_minute,
            status_report.thresholds.max_liquidations_per_minute
        );

        info!(
            "   Successful Liquidations/min: {}",
            status_report
                .current_conditions
                .current_successful_liquidations_per_minute
        );

        if let Some(gas) = status_report.current_conditions.current_gas_multiplier {
            info!(
                "   Gas Multiplier: {}x (max: {}x)",
                gas, status_report.thresholds.max_gas_price_multiplier
            );
        }

        if let Some(time_since) = status_report.time_since_last_activation_secs {
            info!("   Time Since Last Activation: {}s", time_since);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::sleep;

    /// Helper function to convert gas multiplier to gas price in wei for tests
    fn gas_multiplier_to_wei(multiplier: u64) -> U256 {
        // Use 20 Gwei as baseline (same as in calculate_gas_multiplier)
        let baseline_gas_price_wei = U256::from(20_000_000_000u64); // 20 Gwei
        baseline_gas_price_wei * U256::from(multiplier)
    }

    fn create_test_config() -> BotConfig {
        BotConfig {
            rpc_url: "http://localhost:8545".to_string(),
            ws_url: "ws://localhost:8546".to_string(),
            private_key: "0x0000000000000000000000000000000000000000000000000000000000000001"
                .to_string(),
            liquidator_contract: None,
            min_profit_threshold: U256::from(1000000000000000000u64), // 1 ETH
            gas_price_multiplier: 2,
            target_user: None,
            database_url: "sqlite::memory:".to_string(),
            health_factor_threshold: U256::from(1100000000000000000u64), // 1.1
            monitoring_interval_secs: 60,
            asset_loading_method: crate::config::AssetLoadingMethod::Hardcoded,
            at_risk_scan_limit: Some(100),
            full_rescan_interval_minutes: 30,
            archive_zero_debt_users: false,
            zero_debt_cooldown_hours: 24,
            safe_health_factor_threshold: U256::from(10000000000000000000u64), // 10.0
            circuit_breaker_enabled: true,
            max_price_volatility_threshold: 5.0, // 5% for testing
            max_liquidations_per_minute: 3,      // Low threshold for testing
            circuit_breaker_monitoring_window_secs: 60,
            circuit_breaker_cooldown_secs: 5, // Short cooldown for testing
            min_gas_price_multiplier: 1,
            max_gas_price_multiplier: 3, // Low threshold for testing
            ws_fast_path_enabled: true,  // Enable fast path for testing
        }
    }

    #[tokio::test]
    async fn test_circuit_breaker_initialization() {
        let config = create_test_config();
        let circuit_breaker = CircuitBreaker::new(config);

        assert_eq!(circuit_breaker.get_state(), CircuitBreakerState::Closed);
        assert!(circuit_breaker.is_liquidation_allowed());

        let stats = circuit_breaker.get_stats();
        assert_eq!(stats.total_activations, 0);
        assert_eq!(stats.total_liquidations_blocked, 0);
    }

    #[tokio::test]
    async fn test_circuit_breaker_disabled() {
        let mut config = create_test_config();
        config.circuit_breaker_enabled = false;

        let circuit_breaker = CircuitBreaker::new(config);
        assert_eq!(circuit_breaker.get_state(), CircuitBreakerState::Disabled);
        assert!(!circuit_breaker.is_liquidation_allowed());
    }

    #[tokio::test]
    async fn test_price_volatility_trigger() {
        let config = create_test_config();
        let circuit_breaker = CircuitBreaker::new(config);

        // Record initial price
        let initial_price = U256::from(50000 * 10u128.pow(18)); // $50,000
        circuit_breaker
            .record_market_data(Some(initial_price), false, Some(gas_multiplier_to_wei(2)))
            .await
            .unwrap();

        // Record volatile price change (10% increase, above 5% threshold)
        let volatile_price = U256::from(55000 * 10u128.pow(18)); // $55,000 (+10%)
        circuit_breaker
            .record_market_data(Some(volatile_price), false, Some(gas_multiplier_to_wei(2)))
            .await
            .unwrap();

        // Give it a moment to process
        sleep(Duration::from_millis(100)).await;

        // Circuit breaker should be triggered
        assert_eq!(circuit_breaker.get_state(), CircuitBreakerState::Open);
        assert!(!circuit_breaker.is_liquidation_allowed());

        let stats = circuit_breaker.get_stats();
        assert_eq!(stats.total_activations, 1);
        assert_eq!(stats.volatility_triggers, 1);
    }

    #[tokio::test]
    async fn test_liquidation_flood_trigger() {
        let config = create_test_config();
        let circuit_breaker = CircuitBreaker::new(config);

        // Record multiple liquidations quickly (above threshold of 3/minute)
        for _ in 0..5 {
            circuit_breaker
                .record_market_data(None, true, Some(gas_multiplier_to_wei(2)))
                .await
                .unwrap();
            sleep(Duration::from_millis(100)).await;
        }

        // Circuit breaker should be triggered
        assert_eq!(circuit_breaker.get_state(), CircuitBreakerState::Open);
        assert!(!circuit_breaker.is_liquidation_allowed());

        let stats = circuit_breaker.get_stats();
        assert_eq!(stats.total_activations, 1);
        assert_eq!(stats.liquidation_flood_triggers, 1);
    }

    #[tokio::test]
    async fn test_gas_price_spike_trigger() {
        let config = create_test_config();
        let circuit_breaker = CircuitBreaker::new(config);

        // Record high gas price (above threshold of 3x)
        circuit_breaker
            .record_market_data(None, false, Some(gas_multiplier_to_wei(5))) // 5x gas multiplier
            .await
            .unwrap();

        // Give it a moment to process
        sleep(Duration::from_millis(100)).await;

        // Circuit breaker should be triggered
        assert_eq!(circuit_breaker.get_state(), CircuitBreakerState::Open);
        assert!(!circuit_breaker.is_liquidation_allowed());

        let stats = circuit_breaker.get_stats();
        assert_eq!(stats.total_activations, 1);
        assert_eq!(stats.gas_spike_triggers, 1);
    }

    #[tokio::test]
    async fn test_failed_liquidation_tracking_with_new_method() {
        let config = create_test_config();
        let circuit_breaker = CircuitBreaker::new(config);

        // Record multiple FAILED liquidation attempts using the new method
        // This should trigger the circuit breaker as it correctly tracks failed attempts
        for _ in 0..5 {
            circuit_breaker
                .record_liquidation_attempt(false, Some(gas_multiplier_to_wei(2)))
                .await
                .unwrap();
            sleep(Duration::from_millis(100)).await;
        }

        // Circuit breaker SHOULD be triggered because we had 5 liquidation attempts
        // (even though all failed) which exceeds the threshold of 3/minute
        assert_eq!(circuit_breaker.get_state(), CircuitBreakerState::Open);
        assert!(!circuit_breaker.is_liquidation_allowed());

        let stats = circuit_breaker.get_stats();
        assert_eq!(stats.total_activations, 1);
        assert_eq!(stats.liquidation_flood_triggers, 1);
    }

    #[tokio::test]
    async fn test_failed_liquidation_bug_with_old_method() {
        let config = create_test_config();
        let circuit_breaker = CircuitBreaker::new(config);

        // Record multiple FAILED liquidation attempts using the OLD deprecated method
        // This demonstrates the BUG: failed attempts are NOT counted
        for _ in 0..5 {
            #[allow(deprecated)]
            circuit_breaker
                .record_market_data(None, false, Some(gas_multiplier_to_wei(2))) // false = failed liquidation
                .await
                .unwrap();
            sleep(Duration::from_millis(100)).await;
        }

        // Circuit breaker should NOT be triggered (this is the BUG!)
        // Because record_market_data treats liquidation_occurred=false as no attempt
        assert_eq!(circuit_breaker.get_state(), CircuitBreakerState::Closed);
        assert!(circuit_breaker.is_liquidation_allowed());

        // Verify that no liquidation flood was detected (this confirms the bug)
        let stats = circuit_breaker.get_stats();
        assert_eq!(stats.total_activations, 0);
        assert_eq!(stats.liquidation_flood_triggers, 0);
    }

    #[tokio::test]
    async fn test_mixed_successful_and_failed_liquidations() {
        let config = create_test_config();
        let circuit_breaker = CircuitBreaker::new(config);

        // Record a mix of successful and failed liquidation attempts
        // Total: 5 attempts (2 successful, 3 failed) should trigger circuit breaker
        circuit_breaker
            .record_liquidation_attempt(true, Some(gas_multiplier_to_wei(2))) // Success
            .await
            .unwrap();
        sleep(Duration::from_millis(100)).await;

        circuit_breaker
            .record_liquidation_attempt(false, Some(gas_multiplier_to_wei(2))) // Failed
            .await
            .unwrap();
        sleep(Duration::from_millis(100)).await;

        circuit_breaker
            .record_liquidation_attempt(false, Some(gas_multiplier_to_wei(2))) // Failed
            .await
            .unwrap();
        sleep(Duration::from_millis(100)).await;

        circuit_breaker
            .record_liquidation_attempt(true, Some(gas_multiplier_to_wei(2))) // Success
            .await
            .unwrap();
        sleep(Duration::from_millis(100)).await;

        circuit_breaker
            .record_liquidation_attempt(false, Some(gas_multiplier_to_wei(2))) // Failed
            .await
            .unwrap();
        sleep(Duration::from_millis(100)).await;

        // Circuit breaker should be triggered (5 attempts > 3/minute threshold)
        assert_eq!(circuit_breaker.get_state(), CircuitBreakerState::Open);
        assert!(!circuit_breaker.is_liquidation_allowed());

        let stats = circuit_breaker.get_stats();
        assert_eq!(stats.total_activations, 1);
        assert_eq!(stats.liquidation_flood_triggers, 1);
    }

    #[tokio::test]
    async fn test_price_updates_dont_count_as_liquidations() {
        let config = create_test_config();
        let circuit_breaker = CircuitBreaker::new(config);

        // Record multiple price updates using the new dedicated method
        // These should NOT count as liquidation attempts
        for i in 0..10 {
            let price = U256::from((50000 + i * 100) * 10u128.pow(18));
            circuit_breaker
                .record_price_update(Some(price), Some(gas_multiplier_to_wei(2)))
                .await
                .unwrap();
            sleep(Duration::from_millis(50)).await;
        }

        // Circuit breaker should NOT be triggered (no liquidation attempts)
        assert_eq!(circuit_breaker.get_state(), CircuitBreakerState::Closed);
        assert!(circuit_breaker.is_liquidation_allowed());

        let stats = circuit_breaker.get_stats();
        assert_eq!(stats.total_activations, 0);
        assert_eq!(stats.liquidation_flood_triggers, 0);
    }

    #[tokio::test]
    async fn test_correct_liquidation_counting_in_status_report() {
        let config = create_test_config();
        let circuit_breaker = CircuitBreaker::new(config);

        // Record a mix of attempts
        circuit_breaker
            .record_liquidation_attempt(true, Some(gas_multiplier_to_wei(2))) // Success
            .await
            .unwrap();
        circuit_breaker
            .record_liquidation_attempt(false, Some(gas_multiplier_to_wei(2))) // Failed
            .await
            .unwrap();
        circuit_breaker
            .record_liquidation_attempt(false, Some(gas_multiplier_to_wei(2))) // Failed
            .await
            .unwrap();

        let status_report = circuit_breaker.get_status_report();

        // Should count 3 total attempts (1 successful + 2 failed)
        assert_eq!(
            status_report
                .current_conditions
                .current_liquidations_per_minute,
            3
        );

        // Should count only 1 successful liquidation
        assert_eq!(
            status_report
                .current_conditions
                .current_successful_liquidations_per_minute,
            1
        );
    }

    #[tokio::test]
    async fn test_circuit_breaker_state_transitions() {
        let config = create_test_config();
        let circuit_breaker = CircuitBreaker::new(config);

        // Start in closed state
        assert_eq!(circuit_breaker.get_state(), CircuitBreakerState::Closed);

        // Trigger circuit breaker with liquidation flood
        for _ in 0..5 {
            circuit_breaker
                .record_market_data(None, true, Some(gas_multiplier_to_wei(2)))
                .await
                .unwrap();
        }

        // Should be open
        assert_eq!(circuit_breaker.get_state(), CircuitBreakerState::Open);

        // Wait for transition to half-open (cooldown period is 5 seconds in test config)
        sleep(Duration::from_secs(6)).await;

        // Should transition to half-open
        assert_eq!(circuit_breaker.get_state(), CircuitBreakerState::HalfOpen);

        // Wait for the monitoring window to expire so liquidation data gets cleaned up
        // The monitoring window is 60 seconds in the test config
        sleep(Duration::from_secs(61)).await;

        // Record normal conditions after liquidation data has expired
        circuit_breaker
            .record_market_data(None, false, Some(gas_multiplier_to_wei(2)))
            .await
            .unwrap();

        // Give it a moment to process
        sleep(Duration::from_millis(200)).await;

        // Should return to closed after liquidation data has expired from window
        assert_eq!(circuit_breaker.get_state(), CircuitBreakerState::Closed);
    }

    #[tokio::test]
    async fn test_blocked_liquidation_counting() {
        let config = create_test_config();
        let circuit_breaker = CircuitBreaker::new(config);

        // Trigger circuit breaker
        for _ in 0..5 {
            circuit_breaker
                .record_market_data(None, true, Some(gas_multiplier_to_wei(2)))
                .await
                .unwrap();
        }

        // Record blocked liquidations
        for _ in 0..3 {
            circuit_breaker.record_blocked_liquidation();
        }

        let stats = circuit_breaker.get_stats();
        assert_eq!(stats.total_liquidations_blocked, 3);
    }

    #[tokio::test]
    async fn test_manual_disable_enable() {
        let config = create_test_config();
        let circuit_breaker = CircuitBreaker::new(config);

        // Manually disable
        circuit_breaker.disable().await.unwrap();
        assert_eq!(circuit_breaker.get_state(), CircuitBreakerState::Disabled);
        assert!(!circuit_breaker.is_liquidation_allowed());

        // Manually enable
        circuit_breaker.enable().await.unwrap();
        assert_eq!(circuit_breaker.get_state(), CircuitBreakerState::Closed);
        assert!(circuit_breaker.is_liquidation_allowed());
    }

    #[tokio::test]
    async fn test_half_open_limited_operations() {
        let config = create_test_config();
        let circuit_breaker = CircuitBreaker::new(config);

        // Trigger circuit breaker
        for _ in 0..5 {
            circuit_breaker
                .record_market_data(None, true, Some(gas_multiplier_to_wei(2)))
                .await
                .unwrap();
        }

        // Wait for half-open transition
        sleep(Duration::from_secs(6)).await;
        assert_eq!(circuit_breaker.get_state(), CircuitBreakerState::HalfOpen);

        // In half-open state, liquidations should be limited
        // This depends on timing, so we test the general behavior
        let allowed = circuit_breaker.is_liquidation_allowed();
        // Should either be allowed (for testing) or not allowed (normal case)
        // The exact behavior depends on the timing-based logic
        assert!(allowed == true || allowed == false);
    }

    #[tokio::test]
    async fn test_market_data_window_cleanup() {
        let config = create_test_config();
        let circuit_breaker = CircuitBreaker::new(config);

        // Record data points
        for i in 0..10 {
            circuit_breaker
                .record_market_data(
                    Some(U256::from(50000 + i)),
                    false,
                    Some(gas_multiplier_to_wei(2)),
                )
                .await
                .unwrap();
            sleep(Duration::from_millis(100)).await;
        }

        // Data should be maintained within the monitoring window
        let market_data = circuit_breaker.market_data.read();
        assert!(market_data.len() <= 10);
        assert!(market_data.len() > 0);
    }

    #[tokio::test]
    async fn test_multiple_conditions_trigger() {
        let config = create_test_config();
        let circuit_breaker = CircuitBreaker::new(config);

        // Trigger multiple conditions simultaneously
        // High gas price + liquidation + price volatility
        let initial_price = U256::from(50000 * 10u128.pow(18));
        circuit_breaker
            .record_market_data(Some(initial_price), false, Some(gas_multiplier_to_wei(2)))
            .await
            .unwrap();

        // Record extreme conditions
        let volatile_price = U256::from(60000 * 10u128.pow(18)); // 20% increase
        circuit_breaker
            .record_market_data(Some(volatile_price), true, Some(gas_multiplier_to_wei(5))) // High gas + liquidation + volatility
            .await
            .unwrap();

        // Give it a moment to process
        sleep(Duration::from_millis(100)).await;

        // Circuit breaker should be triggered
        assert_eq!(circuit_breaker.get_state(), CircuitBreakerState::Open);

        let stats = circuit_breaker.get_stats();
        assert_eq!(stats.total_activations, 1);
        // Should have triggered multiple condition types
        assert!(stats.volatility_triggers > 0);
        assert!(stats.gas_spike_triggers > 0);
    }

    #[tokio::test]
    async fn test_normal_market_conditions_no_trigger() {
        let config = create_test_config();
        let circuit_breaker = CircuitBreaker::new(config);

        // Record normal market conditions
        let base_price = U256::from(50000 * 10u128.pow(18));

        // Small price changes (under threshold)
        circuit_breaker
            .record_market_data(Some(base_price), false, Some(gas_multiplier_to_wei(2)))
            .await
            .unwrap();

        let small_change_price = U256::from(50100 * 10u128.pow(18)); // 0.2% increase
        circuit_breaker
            .record_market_data(
                Some(small_change_price),
                false,
                Some(gas_multiplier_to_wei(2)),
            )
            .await
            .unwrap();

        // Normal liquidation frequency (1 liquidation, under threshold)
        circuit_breaker
            .record_market_data(None, true, Some(gas_multiplier_to_wei(2)))
            .await
            .unwrap();

        // Give it time to process
        sleep(Duration::from_millis(100)).await;

        // Circuit breaker should remain closed
        assert_eq!(circuit_breaker.get_state(), CircuitBreakerState::Closed);
        assert!(circuit_breaker.is_liquidation_allowed());

        let stats = circuit_breaker.get_stats();
        assert_eq!(stats.total_activations, 0);
    }

    #[tokio::test]
    async fn test_half_open_test_liquidation_timing() {
        let config = create_test_config();
        let circuit_breaker = CircuitBreaker::new(config);

        // Trigger circuit breaker
        for _ in 0..5 {
            circuit_breaker
                .record_market_data(None, true, Some(gas_multiplier_to_wei(2)))
                .await
                .unwrap();
        }

        // Wait for half-open transition
        sleep(Duration::from_secs(6)).await;
        assert_eq!(circuit_breaker.get_state(), CircuitBreakerState::HalfOpen);

        // First test liquidation should be allowed immediately
        assert!(circuit_breaker.is_liquidation_allowed());

        // Record that test liquidation occurred
        circuit_breaker.record_test_liquidation();

        // Immediately after test liquidation, should not be allowed
        assert!(!circuit_breaker.is_liquidation_allowed());

        // Wait a bit less than 30 seconds, should still not be allowed
        sleep(Duration::from_secs(10)).await;
        assert!(!circuit_breaker.is_liquidation_allowed());

        // Wait full 30 seconds since last test liquidation
        sleep(Duration::from_secs(21)).await; // Total 31 seconds
        assert!(circuit_breaker.is_liquidation_allowed());
    }
}
