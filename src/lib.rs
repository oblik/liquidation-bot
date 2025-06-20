pub mod bot;
pub mod config;
pub mod database;
pub mod events;
pub mod models;
pub mod monitoring;
pub mod liquidation;

// Re-export main types for convenience
pub use bot::LiquidationBot;
pub use config::BotConfig;
pub use events::BotEvent;
pub use models::{HardhatArtifact, UserPosition};

pub type Result<T> = eyre::Result<T>;