use anyhow::{Context, Result};
use clap::Parser;
use dotenvy::dotenv;
use liquidation_bot::sim::tenderly::{estimate_profit, Tenderly, TxInput};

/// Simulate a liquidation tx on Base at a specific block using Tenderly.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// Block number to fork at
    #[arg(long)]
    block: u64,
    /// From address (bot EOA)
    #[arg(long)]
    from: String,
    /// To address (protocol/contract)
    #[arg(long)]
    to: String,
    /// Hex calldata (0x…)
    #[arg(long)]
    input: String,
    /// Gas limit (decimal). Optional; let Tenderly estimate if omitted.
    #[arg(long)]
    gas: Option<u64>,
    /// Gas price in wei (decimal). Required for net gas cost reporting.
    #[arg(long)]
    gas_price: Option<u128>,
    /// ETH value in wei (decimal)
    #[arg(long, default_value_t = 0u128)]
    value: u128,
    /// Address to attribute PnL to (defaults to FROM)
    #[arg(long)]
    pnl_address: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    let args = Args::parse();
    let t = Tenderly::new()?;

    let tx = TxInput {
        from: args.from.clone(),
        to: args.to.clone(),
        input: args.input.clone(),
        gas: args.gas.map(|g| g.to_string()),
        gas_price: args.gas_price.map(|p| p.to_string()),
        value: Some(args.value.to_string()),
    };

    let res = t.simulate(&tx, args.block).await?;
    if res.status != "succeeded" {
        let em = res.error_message.unwrap_or_else(|| "unknown".to_string());
        return Err(anyhow::anyhow!("simulation failed: {}", em));
    }

    let pnl_addr = args.pnl_address.as_ref().unwrap_or(&args.from);
    let (deltas, gas_used) = estimate_profit(&res, pnl_addr);
    println!("status: {}", res.status);
    println!("block: {}", args.block);
    println!("gas_used: {}", gas_used);

    if let (Some(gp), true) = (args.gas_price, gas_used > 0) {
        let gas_cost_wei = (gp as u128) * (gas_used as u128);
        println!("gas_cost_wei: {}", gas_cost_wei);
    } else {
        println!("gas_cost_wei: unknown (provide --gas-price)");
    }

    println!("balance_changes for {}:", pnl_addr);
    for bc in deltas {
        let sym = bc.symbol.unwrap_or_else(|| "NATIVE/TOKEN".to_string());
        let token = bc.contract_address.unwrap_or_else(|| "0x0".to_string());
        println!(
            "- token: {} ({}) raw_delta: {} decimals: {:?}",
            sym, token, bc.raw, bc.decimals
        );
    }

    Ok(())
}