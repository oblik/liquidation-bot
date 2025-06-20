use alloy_provider::ProviderBuilder;
use alloy_signer_local::PrivateKeySigner;
use eyre::Result;
use std::sync::Arc;
use tracing::info;

use liquidation_bot::{BotConfig, LiquidationBot};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    info!("Starting Aave v3 Liquidation Bot on Base");

    // Load configuration
    let config = BotConfig::from_env()?;
    info!("Configuration loaded");

    // Parse private key and create signer
    let _signer: PrivateKeySigner = config.private_key.parse()?;
    info!("Signer created from private key");

    // Build HTTP provider with signer for transactions
    let url = url::Url::parse(&config.rpc_url)?;
    let provider = ProviderBuilder::new().on_http(url).boxed();
    let provider = Arc::new(provider);
    info!("HTTP Provider connected to: {}", config.rpc_url);
    info!("WebSocket will connect to: {}", config.ws_url);

    // Create bot instance
    let bot = LiquidationBot::new(provider, config).await?;

    let using_websocket =
        bot.config.ws_url.starts_with("wss://") && !bot.config.ws_url.contains("sepolia.base.org");

    if using_websocket {
        info!("🤖 Liquidation bot initialized with real-time WebSocket monitoring");
    } else {
        info!("🤖 Liquidation bot initialized with polling-based monitoring");
    }

    // Run the bot
    bot.run().await?;

    Ok(())
}
