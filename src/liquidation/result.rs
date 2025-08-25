use alloy_primitives::{Address, U256};

/// Represents the outcome of a liquidation attempt
#[derive(Debug, Clone)]
pub enum LiquidationResult {
    /// Liquidation was successfully executed on-chain
    Executed {
        tx_hash: String,
        profit: U256,
        user: Address,
    },

    /// No liquidation was needed (user is safe, no profitable pairs, etc.)
    NotNeeded {
        reason: LiquidationSkipReason,
        user: Address,
    },
}

/// Reasons why a liquidation was not executed
#[derive(Debug, Clone)]
pub enum LiquidationSkipReason {
    /// User position not found in database
    UserNotFound,

    /// User has no collateral assets
    NoCollateral,

    /// User has no debt to liquidate
    NoDebt,

    /// No profitable liquidation pair found
    NoProfitablePair,

    /// Liquidation rejected due to insufficient profit
    InsufficientProfit {
        estimated_profit: U256,
        min_threshold: U256,
    },

    /// Liquidation was only simulated (no contract/signer available)
    SimulationOnly { estimated_profit: U256 },
}

impl LiquidationResult {
    /// Check if the liquidation was actually executed
    pub fn was_executed(&self) -> bool {
        matches!(self, LiquidationResult::Executed { .. })
    }

    /// Get the transaction hash if executed
    pub fn tx_hash(&self) -> Option<&str> {
        match self {
            LiquidationResult::Executed { tx_hash, .. } => Some(tx_hash),
            _ => None,
        }
    }
}
