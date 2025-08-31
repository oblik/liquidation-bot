use alloy_contract::{ContractInstance, Interface};
use alloy_network::EthereumWallet;
use alloy_primitives::{Address, U256};
use alloy_provider::{Provider, ProviderBuilder};
use alloy_signer_local::PrivateKeySigner;
use eyre::Result;

use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;
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
            asset_configs,
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

        // Check if we should use real execution or mock (based on environment variable)
        let use_real_execution = std::env::var("LIQUIDATION_REAL_EXECUTION")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(false);

        if use_real_execution {
            self.execute_real_transaction(params, &args).await
        } else {
            self.execute_mock_transaction(params, &args).await
        }
    }

    /// Execute real blockchain transaction with signing
    async fn execute_real_transaction(
        &self,
        params: &LiquidationParams,
        args: &[alloy_dyn_abi::DynSolValue],
    ) -> Result<String> {
        info!("🔗 EXECUTING REAL BLOCKCHAIN TRANSACTION");

        // Create the transaction request from the contract call
        let call = self.liquidator_contract.function("liquidate", args)?;
        let mut tx_req = call.into_transaction_request();

        // Get current gas price and add multiplier for competitive execution
        let base_gas_price = self.provider.get_gas_price().await?;
        let gas_price_multiplier = 2; // Use 2x gas price for faster execution
        let adjusted_gas_price = base_gas_price * gas_price_multiplier;

        // Set transaction parameters directly
        tx_req.gas_price = Some(adjusted_gas_price);
        tx_req.gas = Some(500_000); // Conservative gas limit for liquidations
        tx_req.from = Some(self.signer.address());
        tx_req.chain_id = Some(8453); // Base mainnet

        info!(
            "🔗 Preparing liquidation transaction: gas_price={}, gas_limit={}, from={:?}",
            adjusted_gas_price,
            500_000,
            self.signer.address()
        );

        // Log the transaction details
        info!("📋 Transaction parameters:");
        info!("  - Function: liquidate");
        info!("  - User: {:?}", params.user);
        info!("  - Collateral Asset: {:?}", params.collateral_asset);
        info!("  - Debt Asset: {:?}", params.debt_asset);
        info!("  - Debt to Cover: {} wei", params.debt_to_cover);
        info!("  - Gas price: {} wei", adjusted_gas_price);
        info!("  - Gas limit: 500,000");
        info!("  - From: {:?}", self.signer.address());
        info!("  - Chain ID: 8453 (Base mainnet)");

        // Get RPC URL from environment or use default Base mainnet
        let rpc_url =
            std::env::var("RPC_URL").unwrap_or_else(|_| "https://mainnet.base.org".to_string());

        info!("🔗 Setting up provider with signer for real transaction execution...");

        // Create wallet from the signer
        let wallet = EthereumWallet::from(self.signer.clone());

        // Create provider with signer using ProviderBuilder
        let signer_provider = ProviderBuilder::new()
            .with_recommended_fillers()
            .wallet(wallet)
            .on_http(rpc_url.parse()?);

        info!("✅ Signer provider created, submitting transaction...");

        // Submit the transaction using the signer provider
        let pending_tx = signer_provider.send_transaction(tx_req).await?;
        let tx_hash = *pending_tx.tx_hash();
        let tx_hash_string = format!("0x{:x}", tx_hash);

        info!(
            "🚀 REAL liquidation transaction submitted successfully: {}",
            tx_hash_string
        );
        info!(
            "📊 Transaction hash: {}, waiting for confirmation...",
            tx_hash_string
        );

        // Wait for confirmation with timeout
        match tokio::time::timeout(
            std::time::Duration::from_secs(120),
            pending_tx.get_receipt(),
        )
        .await
        {
            Ok(Ok(receipt)) => {
                if receipt.status() {
                    info!(
                        "✅ Transaction confirmed successfully in block: {:?}",
                        receipt.block_number
                    );
                    info!("🎉 Real liquidation executed on-chain!");
                } else {
                    error!("❌ Transaction failed on-chain");
                    return Err(eyre::eyre!("Transaction failed on blockchain"));
                }
            }
            Ok(Err(e)) => {
                warn!("⚠️ Could not get receipt: {}", e);
                info!("Transaction submitted, hash: {}", tx_hash_string);
            }
            Err(_) => {
                warn!("⏰ Receipt timeout - transaction may still be pending");
                info!("Transaction submitted, hash: {}", tx_hash_string);
            }
        }

        Ok(tx_hash_string)
    }

    /// Execute mock transaction for testing/simulation
    async fn execute_mock_transaction(
        &self,
        params: &LiquidationParams,
        _args: &[alloy_dyn_abi::DynSolValue],
    ) -> Result<String> {
        info!("🎭 EXECUTING MOCK TRANSACTION (simulation mode)");

        // Get current gas price and add multiplier for realistic simulation
        let base_gas_price = self.provider.get_gas_price().await?;
        let gas_price_multiplier = 2; // Use 2x gas price for faster execution
        let adjusted_gas_price = base_gas_price * gas_price_multiplier;

        // Get nonce for the signer for realistic simulation
        let nonce = self
            .provider
            .get_transaction_count(self.signer.address())
            .await?;

        info!(
            "🔗 Simulating liquidation transaction with gas price: {} ({}x multiplier), gas limit: {}, nonce: {}",
            adjusted_gas_price, gas_price_multiplier, 500_000, nonce
        );

        // Log detailed transaction parameters that would be used
        info!("📋 Transaction parameters:");
        info!("  - User: {:?}", params.user);
        info!("  - Collateral Asset: {:?}", params.collateral_asset);
        info!("  - Debt Asset: {:?}", params.debt_asset);
        info!("  - Debt to Cover: {} wei", params.debt_to_cover);
        info!("  - Gas price: {} wei", adjusted_gas_price);
        info!("  - Gas limit: 500,000");
        info!("  - From: {:?}", self.signer.address());
        info!("  - Nonce: {}", nonce);
        info!("  - Chain ID: 8453 (Base mainnet)");

        // Create deterministic mock transaction hash for testing
        let mut hasher = DefaultHasher::new();
        hasher.write_u128(adjusted_gas_price);
        hasher.write_u64(nonce);
        hasher.write(params.user.as_slice());
        hasher.write(params.collateral_asset.as_slice());
        hasher.write(params.debt_asset.as_slice());
        let mock_tx_hash = format!("0x{:064x}", hasher.finish());

        info!("🎭 Mock transaction hash generated: {}", mock_tx_hash);
        warn!("⚠️  This is a MOCK transaction - no real on-chain execution occurred");
        warn!(
            "⚠️  Set LIQUIDATION_REAL_EXECUTION=true environment variable to enable real execution"
        );

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
