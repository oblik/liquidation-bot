use alloy_contract::{ContractInstance, Interface};
use alloy_primitives::{Address, U256};
use alloy_provider::Provider;
use alloy_signer_local::PrivateKeySigner;
use eyre::Result;
use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;
use std::sync::Arc;
use tracing::{error, info, warn};

use crate::models::{LiquidationOpportunity, LiquidationParams};

/// Liquidation executor that interfaces with the deployed smart contract
pub struct LiquidationExecutor<P> {
    provider: Arc<P>,
    signer: PrivateKeySigner,
    liquidator_contract: ContractInstance<alloy_transport::BoxTransport, Arc<P>>,
    contract_address: Address,
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
    ) -> Result<Self> {
        // Load the ABI from deployment info or hardcoded
        let liquidator_abi = get_liquidator_abi()?;
        let interface = Interface::new(liquidator_abi);
        let liquidator_contract = interface.connect(contract_address, provider.clone());

        Ok(Self {
            provider,
            signer,
            liquidator_contract,
            contract_address,
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

        // Create transaction request
        let call = self.liquidator_contract.function("liquidate", &args)?;
        let _tx_req = call.into_transaction_request();

        // Get gas price for logging
        let gas_price_u128 = self.provider.get_gas_price().await?;

        // For now, let's create the transaction bytes directly
        // TODO: Implement proper transaction signing when alloy APIs are clearer
        warn!("🚧 Transaction signing implementation needed");
        warn!(
            "Would execute liquidation with gas price: {}",
            gas_price_u128 * 2
        );
        warn!(
            "Parameters: user={}, collateral={}, debt={}, amount={}",
            params.user, params.collateral_asset, params.debt_asset, params.debt_to_cover
        );

        // Return a mock transaction hash for now
        let mock_tx_hash = format!("0x{:064x}", DefaultHasher::new().finish());

        Ok(mock_tx_hash)
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

    /// Get asset ID for L2Pool encoding - this would be looked up from your asset config
    fn get_asset_id(&self, asset_address: Address) -> Result<u16> {
        // Hardcoded asset IDs for Base Sepolia - in production, use a proper mapping
        match format!("{:?}", asset_address).as_str() {
            "0x4200000000000000000000000000000000000006" => Ok(0), // WETH
            "0x036CbD53842c5426634e7929541eC2318f3dCF7e" => Ok(1), // USDC
            "0x2Ae3F1Ec7F1F5012CFEab0185bfc7aa3cf0DEc22" => Ok(2), // cbETH
            _ => {
                error!("Unknown asset address: {:?}", asset_address);
                Err(eyre::eyre!("Unknown asset address"))
            }
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

                // Verify it matches expected Base Sepolia pool
                let expected_pool: Address =
                    "0x07eA79F68B2B3df564D0A34F8e19D9B1e339814b".parse()?;
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
