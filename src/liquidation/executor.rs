use alloy_contract::{ContractInstance, Interface};
use alloy_network::EthereumWallet;
use alloy_primitives::{Address, U256};
use alloy_provider::{Provider, ProviderBuilder};
use alloy_signer_local::PrivateKeySigner;
use eyre::Result;
use std::sync::Arc;
use tracing::{error, info, warn};

use crate::models::{LiquidationAssetConfig, LiquidationOpportunity, LiquidationParams};

/// Liquidation executor that interfaces with the deployed smart contract
pub struct LiquidationExecutor<P> {
    provider: Arc<P>,
    signer: PrivateKeySigner,
    liquidator_contract: ContractInstance<alloy_transport::BoxTransport, Arc<P>>,
    contract_address: Address,
    asset_configs: std::collections::HashMap<Address, LiquidationAssetConfig>,
    contract_interface: Interface,
    rpc_url: String,
}

impl<P> LiquidationExecutor<P>
where
    P: Provider,
{
    /// Create a new liquidation executor
    pub fn new(
        provider: Arc<P>,
        signer: PrivateKeySigner,
        contract_address: Address,
        asset_configs: std::collections::HashMap<Address, LiquidationAssetConfig>,
        rpc_url: String,
    ) -> Result<Self> {
        // Load the ABI from deployment info or hardcoded
        let liquidator_abi = get_liquidator_abi()?;
        let interface = Interface::new(liquidator_abi);
        let liquidator_contract = interface
            .clone()
            .connect(contract_address, provider.clone());

        Ok(Self {
            provider,
            signer,
            liquidator_contract,
            contract_address,
            asset_configs,
            contract_interface: interface,
            rpc_url,
        })
    }

    /// Execute a liquidation transaction
    pub async fn execute_liquidation(
        &self,
        opportunity: &LiquidationOpportunity,
    ) -> Result<String> {
        info!(
            "🚀 Executing liquidation for user: {} (profit: {} wei)",
            opportunity.user, opportunity.estimated_profit
        );

        // Get asset IDs - in a real implementation, you'd look these up from your asset configs
        let collateral_asset_id = self.get_asset_id(opportunity.collateral_asset)?;
        let debt_asset_id = self.get_asset_id(opportunity.debt_asset)?;

        let params = LiquidationParams {
            user: opportunity.user,
            collateral_asset: opportunity.collateral_asset,
            debt_asset: opportunity.debt_asset,
            debt_to_cover: opportunity.debt_to_cover,
            collateral_asset_id,
            debt_asset_id,
            receive_a_token: false, // Receive underlying assets, not aTokens
        };

        // Call the liquidate function on the smart contract
        let tx_hash = self.call_liquidate_function(&params).await?;

        info!("✅ Liquidation transaction submitted: {}", tx_hash);

        // Wait for transaction confirmation
        self.wait_for_confirmation(&tx_hash).await?;

        info!("🎉 Liquidation confirmed: {}", tx_hash);

        Ok(tx_hash)
    }

    /// Call the liquidate function on the smart contract
    async fn call_liquidate_function(&self, params: &LiquidationParams) -> Result<String> {
        info!(
            "Calling liquidate function with params: user={}, collateral={}, debt={}, amount={}",
            params.user, params.collateral_asset, params.debt_asset, params.debt_to_cover
        );

        // Prepare function call arguments
        let args = vec![
            alloy_dyn_abi::DynSolValue::Address(params.user),
            alloy_dyn_abi::DynSolValue::Address(params.collateral_asset),
            alloy_dyn_abi::DynSolValue::Address(params.debt_asset),
            alloy_dyn_abi::DynSolValue::Uint(params.debt_to_cover, 256),
            alloy_dyn_abi::DynSolValue::Bool(params.receive_a_token),
            alloy_dyn_abi::DynSolValue::Uint(U256::from(params.collateral_asset_id), 16),
            alloy_dyn_abi::DynSolValue::Uint(U256::from(params.debt_asset_id), 16),
        ];

        // Create the function call
        let _call = self.liquidator_contract.function("liquidate", &args)?;

        // Get current gas price and apply multiplier
        let gas_price = self.provider.get_gas_price().await?;
        let adjusted_gas_price = gas_price * 2; // 2x multiplier for faster inclusion

        info!(
            "📝 Preparing liquidation transaction: user={}, collateral={}, debt={}, amount={}",
            params.user, params.collateral_asset, params.debt_asset, params.debt_to_cover
        );
        info!(
            "⛽ Gas price: {} wei (2x multiplier applied)",
            adjusted_gas_price
        );

        // Since the provider should now have wallet integration (created with with_recommended_fillers().wallet()),
        // we can send the transaction directly
        // Note: The actual transaction sending requires the provider to have wallet integration

        // Create a wallet-enabled provider for sending the transaction
        let wallet = EthereumWallet::from(self.signer.clone());

        // Create a signing provider with wallet integration
        let signing_provider = ProviderBuilder::new()
            .with_recommended_fillers()
            .wallet(wallet)
            .on_http(
                url::Url::parse(&self.rpc_url)
                    .map_err(|e| eyre::eyre!("Invalid RPC URL: {}", e))?,
            );

        // Create a new contract instance with the signing provider
        let signing_contract = self
            .contract_interface
            .clone()
            .connect(self.contract_address, Arc::new(signing_provider));

        // Build and send the transaction using the signing-enabled contract
        let tx_builder = signing_contract
            .function("liquidate", &args)?
            .gas_price(adjusted_gas_price)
            .gas(500000); // Set reasonable gas limit for liquidation

        // Send the transaction
        info!("🚀 Sending liquidation transaction...");

        let pending_tx = tx_builder.send().await.map_err(|e| {
            error!("Failed to send transaction: {}", e);
            eyre::eyre!("Transaction send failed: {}", e)
        })?;

        // Get the transaction hash
        let tx_hash = *pending_tx.tx_hash();
        info!("🚀 Liquidation transaction sent! TX hash: {}", tx_hash);

        // Wait for confirmation
        info!("⏳ Waiting for transaction confirmation...");
        let receipt = pending_tx
            .get_receipt()
            .await
            .map_err(|e| eyre::eyre!("Failed to get transaction receipt: {}", e))?;

        // Check if the transaction was successful
        if receipt.status() {
            info!(
                "✅ Liquidation transaction confirmed! Gas used: {:?}",
                receipt.gas_used
            );
        } else {
            error!("❌ Liquidation transaction failed on-chain");
            return Err(eyre::eyre!("Transaction reverted on-chain"));
        }

        Ok(format!("{:#x}", tx_hash))
    }

    /// Wait for transaction confirmation
    async fn wait_for_confirmation(&self, tx_hash: &str) -> Result<()> {
        info!("⏳ Waiting for transaction confirmation: {}", tx_hash);

        // Parse tx hash
        let hash: alloy_primitives::TxHash = tx_hash.parse()?;

        // Wait for confirmation with timeout
        let mut attempts = 0;
        const MAX_ATTEMPTS: u32 = 60; // 2 minutes with 2-second intervals

        while attempts < MAX_ATTEMPTS {
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

            match self.provider.get_transaction_receipt(hash).await {
                Ok(Some(receipt)) => {
                    if receipt.status() {
                        info!(
                            "✅ Transaction confirmed successfully in block: {:?}",
                            receipt.block_number
                        );
                        return Ok(());
                    } else {
                        error!("❌ Transaction failed!");
                        return Err(eyre::eyre!("Transaction failed"));
                    }
                }
                Ok(None) => {
                    // Transaction still pending
                    attempts += 1;
                    if attempts % 15 == 0 {
                        info!(
                            "⏳ Still waiting for confirmation... ({}/{})",
                            attempts, MAX_ATTEMPTS
                        );
                    }
                }
                Err(e) => {
                    warn!("Failed to get transaction receipt: {}", e);
                    attempts += 1;
                }
            }
        }

        Err(eyre::eyre!("Transaction confirmation timeout"))
    }

    /// Get asset ID for L2Pool encoding - uses asset configuration lookup
    fn get_asset_id(&self, asset_address: Address) -> Result<u16> {
        // Look up asset configuration to get dynamically fetched asset ID
        if let Some(asset_config) = self.asset_configs.get(&asset_address) {
            Ok(asset_config.asset_id)
        } else {
            error!(
                "Asset address {:#x} not found in asset configurations. Available assets: {:?}",
                asset_address,
                self.asset_configs.keys().collect::<Vec<_>>()
            );
            Err(eyre::eyre!("Unknown asset address: {:#x}", asset_address))
        }
    }

    /// Check if the contract is properly configured
    pub async fn verify_contract_setup(&self) -> Result<()> {
        info!("🔍 Verifying liquidator contract setup...");

        // Call getPool() to verify contract is configured
        let args = vec![];
        let call = self.liquidator_contract.function("getPool", &args)?;
        let result = call.call().await?;

        if let Some(pool_address) = result.first() {
            if let Some(addr) = pool_address.as_address() {
                info!("✅ Contract pool address: {:?}", addr);

                // Verify it matches expected Base mainnet pool
                let expected_pool: Address =
                    "0xA238Dd80C259a72e81d7e4664a9801593F98d1c5".parse()?;
                if addr == expected_pool {
                    info!("✅ Pool address verification successful");
                } else {
                    warn!(
                        "⚠️ Pool address mismatch - expected: {:?}, got: {:?}",
                        expected_pool, addr
                    );
                }
            }
        }

        Ok(())
    }
}

/// Get the liquidator contract ABI
fn get_liquidator_abi() -> Result<alloy_json_abi::JsonAbi> {
    // For now, create a minimal ABI with the liquidate function
    // In production, you'd load this from the contract artifact or hardcode the full ABI
    let abi_json = r#"[
        {
            "inputs": [
                {"internalType": "address", "name": "user", "type": "address"},
                {"internalType": "address", "name": "collateralAsset", "type": "address"},
                {"internalType": "address", "name": "debtAsset", "type": "address"},
                {"internalType": "uint256", "name": "debtToCover", "type": "uint256"},
                {"internalType": "bool", "name": "receiveAToken", "type": "bool"},
                {"internalType": "uint16", "name": "collateralAssetId", "type": "uint16"},
                {"internalType": "uint16", "name": "debtAssetId", "type": "uint16"}
            ],
            "name": "liquidate",
            "outputs": [],
            "stateMutability": "nonpayable",
            "type": "function"
        },
        {
            "inputs": [],
            "name": "getPool",
            "outputs": [{"internalType": "address", "name": "", "type": "address"}],
            "stateMutability": "view",
            "type": "function"
        }
    ]"#;

    let abi: alloy_json_abi::JsonAbi = serde_json::from_str(abi_json)?;
    Ok(abi)
}
