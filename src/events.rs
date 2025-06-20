use alloy_primitives::Address;
use alloy_sol_types::sol;
use crate::models::UserPosition;

// Define Aave events using sol! macro for type safety
sol! {
    #[derive(Debug)]
    pub struct AaveEvents {
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
    }
}

// Event types for internal messaging
#[derive(Debug, Clone)]
pub enum BotEvent {
    UserPositionChanged(Address),
    PriceUpdate(Address),            // asset address
    LiquidationOpportunity(Address), // user address
    DatabaseSync(Vec<UserPosition>),
}