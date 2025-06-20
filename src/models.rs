use alloy_primitives::{Address, U256};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use alloy_json_abi::JsonAbi;
use alloy_sol_types::{sol, SolEvent};

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