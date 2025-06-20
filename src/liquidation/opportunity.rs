use alloy_primitives::Address;
use dashmap::DashMap;
use eyre::Result;
use sqlx::{Pool, Sqlite};
use std::sync::Arc;
use tracing::{info, warn};
use crate::database::log_monitoring_event;
use crate::models::UserPosition;

pub struct LiquidationOpportunityHandler {
    db_pool: Pool<Sqlite>,
    user_positions: Arc<DashMap<Address, UserPosition>>,
}

impl LiquidationOpportunityHandler {
    pub fn new(db_pool: Pool<Sqlite>, user_positions: Arc<DashMap<Address, UserPosition>>) -> Self {
        Self {
            db_pool,
            user_positions,
        }
    }

    pub async fn handle_opportunity(&self, user: Address) -> Result<()> {
        info!("🎯 Processing liquidation opportunity for user: {:?}", user);

        // Get current position
        let position = match self.user_positions.get(&user) {
            Some(pos) => pos.clone(),
            None => {
                // Position not found in cache, would need to refresh
                warn!("Position not found in cache for user: {:?}", user);
                return Ok(());
            }
        };

        // Log the opportunity
        let details = format!(
            "Health Factor: {}, Debt: {}, Collateral: {}",
            position.health_factor, position.total_debt_base, position.total_collateral_base
        );

        if let Err(e) = log_monitoring_event(
            &self.db_pool,
            "liquidation_opportunity",
            Some(user),
            Some(&details),
        )
        .await
        {
            tracing::error!("Failed to log liquidation opportunity: {}", e);
        }

        // For now, just log the opportunity
        // In the future, this would call attempt_liquidation
        warn!("Liquidation opportunity detected but not executing (monitoring mode)");
        warn!(
            "User: {:?}, HF: {}, Debt: {}, Collateral: {}",
            user, position.health_factor, position.total_debt_base, position.total_collateral_base
        );

        Ok(())
    }

    // Future implementation could include:
    // - Calculate optimal liquidation parameters
    // - Execute flash loan liquidation
    // - Handle profit distribution
    // - Risk management checks
}