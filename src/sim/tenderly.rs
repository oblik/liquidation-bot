use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use std::env;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TxInput {
    pub from: String,       // 0x…
    pub to: String,         // 0x…
    pub input: String,      // 0x calldata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gas: Option<String>,        // hex or decimal wei limit; if None, let Tenderly estimate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gas_price: Option<String>,  // decimal wei as string
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,      // decimal wei as string
}

#[derive(Debug, Serialize)]
struct SimulateReq<'a> {
    network_id: &'a str,         // "base-mainnet"
    block_number: u64,
    from: &'a str,
    to: &'a str,
    input: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    gas: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    gas_price: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
pub struct BalanceChange {
    pub address: String,
    pub decimals: Option<u8>,
    pub symbol: Option<String>,
    pub raw: String, // signed decimal string in smallest unit
    #[serde(default)]
    pub contract_address: Option<String>, // None for native
}

#[derive(Debug, Deserialize)]
pub struct SimGas {
    pub gas_used: String, // decimal
}

#[derive(Debug, Deserialize)]
pub struct SimResult {
    pub status: String, // "succeeded" | "failed"
    #[serde(default)]
    pub gas: Option<SimGas>,
    #[serde(default)]
    pub balance_changes: Vec<BalanceChange>,
    #[serde(default)]
    pub error_message: Option<String>,
}

#[derive(thiserror::Error, Debug)]
pub enum SimError {
    #[error("simulation failed: {0}")]
    Failed(String),
}

pub struct Tenderly {
    client: reqwest::Client,
    base_url: String,
    network: String,
}

impl Tenderly {
    pub fn new() -> Result<Self> {
        let account = env::var("TENDERLY_ACCOUNT").context("TENDERLY_ACCOUNT missing")?;
        let project = env::var("TENDERLY_PROJECT").context("TENDERLY_PROJECT missing")?;
        let pat = env::var("TENDERLY_ACCESS_KEY").context("TENDERLY_ACCESS_KEY missing")?;
        let network = env::var("TENDERLY_NETWORK").unwrap_or_else(|_| "base-mainnet".to_string());

        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", pat)).unwrap(),
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()?;

        let base_url = format!(
            "https://api.tenderly.co/api/v1/account/{}/project/{}/simulate",
            account, project
        );
        Ok(Self { client, base_url, network })
    }

    pub async fn simulate(&self, tx: &TxInput, block_number: u64) -> Result<SimResult> {
        let body = SimulateReq {
            network_id: &self.network,
            block_number,
            from: &tx.from,
            to: &tx.to,
            input: &tx.input,
            gas: tx.gas.as_deref(),
            gas_price: tx.gas_price.as_deref(),
            value: tx.value.as_deref(),
        };

        let resp = self
            .client
            .post(&self.base_url)
            .json(&body)
            .send()
            .await
            .context("tenderly http error")?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("tenderly error: {}", text));
        }
        let out: SimResult = resp.json().await.context("decode tenderly json")?;
        Ok(out)
    }
}

/// Compute profit (in raw token units) for `bot_addr` by summing balance deltas from the simulation.
/// Returns (per-asset vector, gas_used as u128).
pub fn estimate_profit(sim: &SimResult, bot_addr: &str) -> (Vec<BalanceChange>, u128) {
    let addr_lc = bot_addr.to_lowercase();
    let mut deltas = vec![];
    for bc in sim.balance_changes.iter() {
        if bc.address.to_lowercase() == addr_lc {
            deltas.push(bc.clone());
        }
    }
    let gas_used = sim
        .gas
        .as_ref()
        .and_then(|g| g.gas_used.parse::<u128>().ok())
        .unwrap_or(0);
    (deltas, gas_used)
}