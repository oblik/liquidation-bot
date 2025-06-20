use alloy_json_abi::JsonAbi;
use alloy_primitives::{Address, U256};
use chrono::{DateTime, Utc};
use serde::Deserialize;

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

impl UserPosition {
    pub fn is_liquidatable(&self) -> bool {
        self.health_factor < U256::from(10u128.pow(18))
    }
    
    pub fn is_healthy(&self) -> bool {
        !self.is_at_risk && !self.is_liquidatable()
    }
}