use alloy_json_abi::JsonAbi;
use alloy_primitives::{Address, U256};
use alloy_sol_types::sol;
use chrono::{DateTime, Utc};
use serde::Deserialize;

/// Result of a liquidation attempt
#[derive(Debug, Clone)]
pub enum LiquidationResult {
    /// Liquidation was executed on-chain with the given transaction hash
    Executed(String),
    /// No liquidation was needed (user safe, no profitable pairs, etc.)
    NotNeeded(String),
    /// Liquidation failed with an error
    Failed(String),
}

// Define Aave events using sol! macro for type safety
sol! {
    event Borrow(
        address indexed reserve,
        address user,
        address indexed onBehalfOf,
        uint256 amount,
        uint8 interestRateMode,
        uint256 borrowRate,
        uint16 indexed referralCode
    );

    event Repay(
        address indexed reserve,
        address indexed user,
        address indexed repayer,
        uint256 amount,
        bool useATokens
    );

    event Supply(
        address indexed reserve,
        address user,
        address indexed onBehalfOf,
        uint256 amount,
        uint16 indexed referralCode
    );

    event Withdraw(
        address indexed reserve,
        address indexed user,
        address indexed to,
        uint256 amount
    );

    event LiquidationCall(
        address indexed collateralAsset,
        address indexed debtAsset,
        address indexed user,
        uint256 debtToCover,
        uint256 liquidatedCollateralAmount,
        address liquidator,
        bool receiveAToken
    );

    event ReserveDataUpdated(
        address indexed reserve,
        uint256 liquidityRate,
        uint256 stableBorrowRate,
        uint256 variableBorrowRate,
        uint256 liquidityIndex,
        uint256 variableBorrowIndex
    );

    // Chainlink Price Feed events
    event AnswerUpdated(
        int256 indexed current,
        uint256 indexed roundId,
        uint256 updatedAt
    );
}

// Oracle price feed monitoring
#[derive(Debug, Clone)]
pub struct PriceFeed {
    pub asset_address: Address,
    pub feed_address: Address,
    pub asset_symbol: String,
    pub last_price: U256,
    pub last_updated: DateTime<Utc>,
    pub price_change_threshold: f64, // Percentage change to trigger recalculation
}

#[derive(Debug, Clone)]
pub struct AssetConfig {
    pub address: Address,
    pub symbol: String,
    pub chainlink_feed: Address,
    pub price_change_threshold: f64, // e.g., 0.05 for 5% change
}

#[derive(Deserialize)]
pub struct HardhatArtifact {
    pub abi: JsonAbi,
}

// User position tracking
#[derive(Debug, Clone)]
pub struct UserPosition {
    pub address: Address,
    pub total_collateral_base: U256,
    pub total_debt_base: U256,
    pub available_borrows_base: U256,
    pub current_liquidation_threshold: U256,
    pub ltv: U256,
    pub health_factor: U256,
    pub last_updated: DateTime<Utc>,
    pub is_at_risk: bool,
}

#[derive(Debug, Clone)]
pub struct LiquidationAssetConfig {
    pub address: Address,
    pub symbol: String,
    pub decimals: u8,
    pub asset_id: u16,          // For L2Pool encoding
    pub liquidation_bonus: u16, // In basis points (e.g., 500 = 5%)
    pub is_collateral: bool,
    pub is_borrowable: bool,
}

#[derive(Debug, Clone)]
pub struct LiquidationOpportunity {
    pub user: Address,
    pub collateral_asset: Address,
    pub debt_asset: Address,
    pub debt_to_cover: U256,
    pub expected_collateral_received: U256,
    pub liquidation_bonus: U256,
    pub flash_loan_fee: U256,
    pub gas_cost: U256,
    pub swap_slippage: U256,
    pub estimated_profit: U256,
    pub profit_threshold_met: bool,
}

#[derive(Debug, Clone)]
pub struct LiquidationParams {
    pub user: Address,
    pub collateral_asset: Address,
    pub debt_asset: Address,
    pub debt_to_cover: U256,
    pub collateral_asset_id: u16,
    pub debt_asset_id: u16,
    pub receive_a_token: bool,
}

#[derive(Debug, Clone)]
pub struct GasEstimate {
    pub base_fee: U256,
    pub priority_fee: U256,
    pub gas_limit: U256,
    pub total_cost: U256,
}

/// Result of a liquidation attempt to distinguish between executed vs not-needed liquidations
#[derive(Debug, Clone)]
pub enum LiquidationResult {
    /// Liquidation was executed successfully with the given transaction hash
    Executed(String),
    /// No liquidation was needed (user safe, no profitable pairs, etc.)
    NotNeeded(NotNeededReason),
    /// Liquidation failed due to an error
    Failed(String),
}

/// Reasons why a liquidation was not needed
#[derive(Debug, Clone)]
pub enum NotNeededReason {
    /// User position not found in database
    UserNotFound,
    /// User has no collateral assets
    NoCollateral,
    /// User has no debt assets
    NoDebt,
    /// No profitable liquidation pairs found
    NoProfitablePairs,
    /// Liquidation opportunity exists but doesn't meet profit threshold
    InsufficientProfit,
    /// Liquidator contract or signer not configured (simulation mode)
    SimulationMode,
}
