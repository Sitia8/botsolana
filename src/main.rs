//! Solana HFT Bot using Barter-rs and Jupiter Aggregator
//! High-level architecture:
//! - Market data via Helius LaserStream WS
//! - Order routing via Jupiter Swap API
//! - Strategy & backtesting via Barter
//! - ML signal (logistic regression) via Linfa
//! - On-chain interactions via Anchor client

mod config;
mod data;
mod grpc_stream;
mod model;
mod strategy;
mod trader;
mod swap_client;

use anyhow::Result;
use config::BotConfig;
use structopt::StructOpt;
use tokio::signal;
use trader::Trader;

#[derive(StructOpt, Debug)]
#[structopt(name = "solana_hft_bot")]
struct Cli {
    /// Path to config file
    #[structopt(short, long, default_value = "bot.toml")]
    config: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let args = Cli::from_args();
    let cfg = BotConfig::from_file(&args.config)?;

    let mut trader = Trader::new(cfg).await?;

    tokio::select! {
        res = trader.run() => res?,
        _ = signal::ctrl_c() => {
            log::info!("Shutdown signal received");
        }
    }
    trader.shutdown().await;
    Ok(())
}
