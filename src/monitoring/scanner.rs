use alloy_primitives::Address;
use eyre::Result;
use sqlx::{Pool, Sqlite};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::info;
use crate::config::BotConfig;
use crate::database::get_at_risk_users;
use crate::events::BotEvent;

pub struct PeriodicScanner {
    config: Arc<BotConfig>,
    db_pool: Pool<Sqlite>,
    event_tx: mpsc::UnboundedSender<BotEvent>,
}

impl PeriodicScanner {
    pub fn new(
        config: Arc<BotConfig>,
        db_pool: Pool<Sqlite>,
        event_tx: mpsc::UnboundedSender<BotEvent>,
    ) -> Self {
        Self {
            config,
            db_pool,
            event_tx,
        }
    }

    pub async fn start_scanning(&self) -> Result<()> {
        info!("Starting periodic position scan...");

        let mut interval = tokio::time::interval(
            tokio::time::Duration::from_secs(self.config.monitoring_interval_secs * 6), // Slower than event-driven updates
        );

        loop {
            interval.tick().await;

            // Get at-risk users from database
            let at_risk_users = match get_at_risk_users(&self.db_pool).await {
                Ok(users) => users,
                Err(e) => {
                    tracing::error!("Failed to get at-risk users: {}", e);
                    continue;
                }
            };

            info!("Scanning {} at-risk users", at_risk_users.len());

            for user_addr in at_risk_users {
                let _ = self.event_tx.send(BotEvent::UserPositionChanged(user_addr));
            }

            // If we have a specific target user, always check them
            if let Some(target_user) = self.config.target_user {
                let _ = self
                    .event_tx
                    .send(BotEvent::UserPositionChanged(target_user));
            }
        }
    }
}