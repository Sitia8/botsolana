use anyhow::{anyhow, Result};
use serde::Deserialize;
use std::fs;

#[derive(Debug, Deserialize, Clone)]
pub struct BotConfig {
    pub helius_api_key: String,
    /// Optional Triton/Yellowstone X-Token for authenticated gRPC access
    #[serde(default)]
    pub yellowstone_token: Option<String>,
    pub jupiter_api_url: String,
    pub wallet_keypair: String,
    pub symbols: Vec<String>,
    pub model_path: String,
    pub anchor_cluster: String,
    pub anchor_program_id: String,
    /// Trade size in base units (e.g. 1 SOL). Defaults to 1.0
    #[serde(default)]
    pub trade_amount: Option<f64>,
    /// Allowed slippage in basis points (1 bp = 0.01%). Defaults to 50 (0.5%)
    #[serde(default)]
    pub slippage_bps: Option<u64>,
    /// Max seconds to wait for tx confirmation. Defaults to 30s
    #[serde(default)]
    pub tx_confirm_secs: Option<u64>,
}

impl BotConfig {
    pub fn from_file(path: &str) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        Ok(toml::from_str(&content).map_err(|e| anyhow!(e))?)
    }
}
