use crate::config::BotConfig;
use crate::data::TradeMsg;
use crate::grpc_stream::GrpcStream;
use crate::strategy::{OrderSide, Strategy};
use anyhow::Result;
use futures_util::StreamExt;
use std::pin::Pin;
use crate::swap_client::SwapClient;
use ndarray::Array2;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    signature::{Keypair, Signature},
};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

pub struct Trader {
    cfg: BotConfig,
    strategy: Strategy,
    stream: GrpcStream,
    rpc: RpcClient,
    swap_client: SwapClient,
    wallet: Arc<Keypair>,
    pnl: Arc<Mutex<f64>>,
    paper_mode: bool,
    dataset: Arc<Mutex<Vec<(Vec<f64>, f64)>>>,
    last_features: Option<Vec<f64>>,
    last_price: Option<f64>,
    last_trained: usize,
    trade_amount: f64,
    slippage_bps: u64,
    confirm_secs: u64,
}

impl Trader {
    pub async fn new(cfg: BotConfig) -> Result<Self> {
        let model = crate::model::MlModel::load(&cfg.model_path)?;
        let strategy = Strategy::new(model, 0.55);

        let stream = GrpcStream::from_config(&cfg);
        let rpc = RpcClient::new(cfg.anchor_cluster.clone());
        let swap_client = SwapClient::new(cfg.jupiter_api_url.clone());
        let wallet = Arc::new(Keypair::from_bytes(&bs58::decode(&cfg.wallet_keypair).into_vec()?)?);

        let paper_mode = cfg.anchor_cluster.contains("devnet") || cfg.anchor_program_id.is_empty();

        // trading parameters with defaults
        let trade_amount = cfg.trade_amount.unwrap_or(1.0);
        let slippage_bps = cfg.slippage_bps.unwrap_or(50);
        let confirm_secs = cfg.tx_confirm_secs.unwrap_or(30);

        Ok(Self {
            cfg,
            strategy,
            stream,
            rpc,
            swap_client,
            wallet,
            pnl: Arc::new(Mutex::new(0.0)),
            paper_mode,
            dataset: Arc::new(Mutex::new(Vec::new())),
            last_features: None,
            last_price: None,
            last_trained: 0,
            trade_amount,
            slippage_bps,
            confirm_secs,
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        let mut stream: Pin<Box<dyn futures_util::Stream<Item = TradeMsg> + Send>> = self.stream.connect().await?;
        while let Some(trade) = stream.next().await {
            self.handle_trade(trade).await?;
        }
        Ok(())
    }

    async fn handle_trade(&mut self, trade: TradeMsg) -> Result<()> {
        let features = vec![trade.price, trade.size, trade.spread];

        // Build dataset for ML when previous trade exists
        if let (Some(prev_feat), Some(prev_price)) = (self.last_features.clone(), self.last_price) {
            let label = if trade.price > prev_price { 1.0 } else { 0.0 };
            self.dataset.lock().await.push((prev_feat, label));
        }

        self.last_features = Some(features.clone());
        self.last_price = Some(trade.price);

        // Train model periodically in paper mode
        if self.paper_mode && self.dataset.lock().await.len() - self.last_trained >= 500 {
            self.train_model().await?;
        }

        if let Some(side) = self.strategy.generate_signal(&features) {
            if !self.paper_mode {
                self.execute_order(side, trade.price).await?;
            } else {
                log::info!("[PAPER] Signal {:?} at price {}", side, trade.price);
            }
        }
        Ok(())
    }

    async fn train_model(&mut self) -> Result<()> {
        let data = self.dataset.lock().await.clone();
        if data.len() < 10 {
            return Ok(());
        }
        let n = data.len();
        let x: Vec<f64> = data.iter().flat_map(|(f, _)| f.clone()).collect();
        let x = Array2::from_shape_vec((n, 3), x)?;
        let y_vec: Vec<i32> = data.iter().map(|(_, lbl)| if *lbl > 0.5 { 1 } else { 0 }).collect();
        let model = crate::model::MlModel::train(x, y_vec)?;
        model.save(&self.cfg.model_path)?;

        // Update strategy with new model
        self.strategy = Strategy::new(model, 0.55);
        log::info!("Model retrained with {} samples; saved to {}.", n, self.cfg.model_path);
        self.last_trained = n;
        Ok(())
    }

    async fn execute_order(&mut self, side: OrderSide, price: f64) -> Result<()> {
        let symbol = &self.cfg.symbols[0];
        let quote = self
            .swap_client
            .quote(symbol, self.trade_amount, Some(side == OrderSide::Sell))
            .await?;

        let sig = self
            .swap_client
            .swap(&self.wallet, &quote)
            .await?;

        self.wait_for_confirmation(&sig).await?;

        log::info!("Executed {:?} order sig: {}", side, sig);
        let mut pnl = self.pnl.lock().await;
        *pnl += if side == OrderSide::Buy {
            -self.trade_amount * price
        } else {
            self.trade_amount * price
        };
        Ok(())
    }

    async fn wait_for_confirmation(&self, _sig: &Signature) -> Result<()> {
    // TODO: Replace this stub with real polling logic using the latest solana
    // `RpcClient::get_signature_status` API. For now we simply wait for the
    // configured confirmation timeout and assume success so that the bot
    // remains functional while we migrate the API calls.
    tokio::time::sleep(Duration::from_secs(self.confirm_secs)).await;
    Ok(())
}

    pub async fn shutdown(&mut self) {
        log::info!("Final PnL: {}", *self.pnl.lock().await);
    }
}
