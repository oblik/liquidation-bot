use alloy_contract::{ContractInstance, Interface};
use alloy_json_abi::JsonAbi;
use alloy_primitives::{Address, U256};
use alloy_provider::ProviderBuilder;
use alloy_signer_local::PrivateKeySigner;
use eyre::Result;
use serde::Deserialize;
use std::sync::Arc;
use tracing::{error, info, warn};

#[derive(Deserialize)]
struct HardhatArtifact {
    abi: JsonAbi,
}

// Configuration struct
#[derive(Debug, Clone)]
struct BotConfig {
    rpc_url: String,
    private_key: String,
    liquidator_contract: Option<Address>,
    min_profit_threshold: U256,
    gas_price_multiplier: u64,
    target_user: Option<Address>,
}

impl BotConfig {
    fn from_env() -> Result<Self> {
        dotenvy::dotenv().ok();

        let rpc_url = std::env::var("RPC_URL")
            .map_err(|_| eyre::eyre!("RPC_URL environment variable not set"))?;

        let private_key = std::env::var("PRIVATE_KEY")
            .map_err(|_| eyre::eyre!("PRIVATE_KEY environment variable not set"))?;

        let liquidator_contract = std::env::var("LIQUIDATOR_CONTRACT")
            .ok()
            .and_then(|addr| addr.parse().ok());

        let min_profit_threshold = std::env::var("MIN_PROFIT_THRESHOLD")
            .unwrap_or_else(|_| "5000000000000000000".to_string()) // 5 ETH wei default
            .parse()
            .unwrap_or(U256::from(5000000000000000000u64));

        let gas_price_multiplier = std::env::var("GAS_PRICE_MULTIPLIER")
            .unwrap_or_else(|_| "2".to_string())
            .parse()
            .unwrap_or(2);

        let target_user = std::env::var("TARGET_USER")
            .ok()
            .and_then(|addr| addr.parse().ok());

        Ok(Self {
            rpc_url,
            private_key,
            liquidator_contract,
            min_profit_threshold,
            gas_price_multiplier,
            target_user,
        })
    }
}

// Main bot struct
struct LiquidationBot<P> {
    provider: Arc<P>,
    config: BotConfig,
    pool_contract: ContractInstance<Arc<P>>,
    liquidator_contract: Option<ContractInstance<Arc<P>>>,
}

impl<P> LiquidationBot<P>
where
    P: alloy_provider::Provider,
{
    fn new(provider: Arc<P>, config: BotConfig) -> Result<Self> {
        // Load ABI of L2Pool from Hardhat artifact
        let artifact_str = include_str!("../abi/L2Pool.json");
        let artifact: HardhatArtifact = serde_json::from_str(artifact_str)?;
        let interface = Interface::new(artifact.abi);

        // Aave V3 Pool address on Base Sepolia testnet
        let pool_addr: Address = "0x07eA79F68B2B3df564D0A34F8e19D9B1e339814b".parse()?;
        let pool_contract = interface.connect(pool_addr, provider.clone());

        // For now, liquidator contract is optional
        let liquidator_contract = None;

        Ok(Self {
            provider,
            config,
            pool_contract,
            liquidator_contract,
        })
    }

    async fn check_user_health(&self, user: Address) -> Result<(U256, bool)> {
        info!("Checking health factor for user: {:?}", user);

        // Call getUserAccountData
        let args = [alloy_dyn_abi::DynSolValue::Address(user)];
        let call = self.pool_contract.function("getUserAccountData", &args)?;
        let result: Vec<alloy_dyn_abi::DynSolValue> = call.call().await?;

        let health_factor = if let Some(alloy_dyn_abi::DynSolValue::Uint(hf, _)) = result.get(5) {
            *hf
        } else {
            return Err(eyre::eyre!("Failed to parse health factor from result"));
        };

        let is_liquidatable = health_factor < U256::from(10u128.pow(18));

        info!(
            "User {:?} - Health Factor: {}, Liquidatable: {}",
            user, health_factor, is_liquidatable
        );

        Ok((health_factor, is_liquidatable))
    }

    async fn attempt_liquidation(&self, user: Address) -> Result<()> {
        if self.liquidator_contract.is_none() {
            warn!("Liquidator contract not configured, skipping liquidation");
            return Ok(());
        }

        let (health_factor, is_liquidatable) = self.check_user_health(user).await?;

        if !is_liquidatable {
            info!(
                "User {:?} is not liquidatable (HF: {})",
                user, health_factor
            );
            return Ok(());
        }

        info!("User {:?} is liquidatable! Analyzing opportunity...", user);

        // Get user account data for analysis
        let args = [alloy_dyn_abi::DynSolValue::Address(user)];
        let call = self.pool_contract.function("getUserAccountData", &args)?;
        let user_data: Vec<alloy_dyn_abi::DynSolValue> = call.call().await?;

        if let (
            Some(alloy_dyn_abi::DynSolValue::Uint(collateral, _)),
            Some(alloy_dyn_abi::DynSolValue::Uint(debt, _)),
            Some(alloy_dyn_abi::DynSolValue::Uint(available_borrows, _)),
        ) = (user_data.get(0), user_data.get(1), user_data.get(2))
        {
            info!(
                "User data - Collateral: {}, Debt: {}, Available Borrows: {}",
                collateral, debt, available_borrows
            );
        }

        // For now, we just log the opportunity
        // In a full implementation, we would:
        // 1. Determine which collateral and debt assets to use
        // 2. Calculate expected profit
        // 3. Estimate gas costs
        // 4. Execute the liquidation if profitable

        warn!("Liquidation opportunity detected but not executing (demo mode)");
        warn!("Would liquidate user {:?} with HF: {}", user, health_factor);

        Ok(())
    }

    async fn run_monitoring_loop(&self) -> Result<()> {
        info!("Starting liquidation bot monitoring loop...");

        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(10));

        loop {
            interval.tick().await;

            if let Some(target_user) = self.config.target_user {
                if let Err(e) = self.attempt_liquidation(target_user).await {
                    error!("Error processing user {:?}: {}", target_user, e);
                }
            } else {
                // In a full implementation, we would:
                // 1. Subscribe to Aave events
                // 2. Monitor price oracle updates
                // 3. Maintain a list of at-risk users
                info!("No target user specified. Set TARGET_USER environment variable.");
            }
        }
    }
}

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
    let signer: PrivateKeySigner = config.private_key.parse()?;
    info!("Signer created from private key");

    // Build provider with signer
    let url = url::Url::parse(&config.rpc_url)?;
    let provider = ProviderBuilder::new().wallet(signer).connect_http(url);

    let provider = Arc::new(provider);
    info!("Provider connected to: {}", config.rpc_url);

    // Create bot instance
    let bot = LiquidationBot::new(provider, config)?;

    // Test basic functionality
    if let Some(target_user) = bot.config.target_user {
        info!("Testing with target user: {:?}", target_user);
        let (health_factor, is_liquidatable) = bot.check_user_health(target_user).await?;

        if is_liquidatable {
            warn!(
                "🚨 TARGET USER IS LIQUIDATABLE! Health Factor: {}",
                health_factor
            );
        } else {
            info!(
                "✅ Target user is healthy. Health Factor: {}",
                health_factor
            );
        }
    }

    // Start monitoring loop
    info!("Starting monitoring loop...");
    bot.run_monitoring_loop().await?;

    Ok(())
}
