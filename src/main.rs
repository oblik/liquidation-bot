use alloy_contract::{ContractInstance, Interface};
use alloy_json_abi::JsonAbi;
use alloy_primitives::{Address, U256};
use alloy_provider::ProviderBuilder;
use anyhow::Result;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let rpc_url = std::env::var("RPC_URL")?;
    let user = std::env::var("TARGET_USER").unwrap_or_default();
    let user: Address = user.parse().unwrap_or(Address::ZERO);

    let url = url::Url::parse(&rpc_url)?;
    // build provider with recommended fillers
    let provider = ProviderBuilder::new().connect_http(url);
    let provider = Arc::new(provider);

    // load ABI of L2Pool
    let abi_str = include_str!("../abi/L2Pool.json");
    let abi: JsonAbi = serde_json::from_str(abi_str)?;
    let interface = Interface::new(abi);

    // Aave V3 Pool address on Base mainnet
    let pool_addr: Address = "0xA238Dd80C259a72e81d7e4664a9801593F98d1c5".parse()?;
    let contract: ContractInstance<_, _> = interface.connect(pool_addr, provider.clone());

    // call getUserAccountData
    let args = [alloy_dyn_abi::DynSolValue::Address(user)];
    let call = contract.function("getUserAccountData", &args)?;
    let result: Vec<alloy_dyn_abi::DynSolValue> = call.call().await?;
    println!("getUserAccountData result: {:?}", result);

    if let Some(hf_val) = result.get(5) {
        if let alloy_dyn_abi::DynSolValue::Uint(hf, _) = hf_val {
            // HF is ray (1e18)
            let one = U256::from(10u64.pow(18));
            if *hf < one {
                println!("User {user:?} is undercollateralized");
            } else {
                println!("User healthy");
            }
        }
    }

    Ok(())
}
