use alloy_primitives::{Address, U256};
use crate::models::UserPosition;

// Event types for internal messaging
#[derive(Debug, Clone)]
pub enum BotEvent {
    UserPositionChanged(Address),
    PriceUpdate(Address, U256, U256), // asset address, old_price, new_price
    LiquidationOpportunity(Address),  // user address
    DatabaseSync(Vec<UserPosition>),
    OraclePriceChanged(Address, U256), // asset address, new price
}