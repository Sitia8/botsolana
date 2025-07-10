use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use tokio_tungstenite::connect_async;

#[derive(Debug, Deserialize, Clone)]
pub struct TradeMsg {
    pub price: f64,
    pub size: f64,
    pub side: String,
    pub ts: i64,
    /// Optional spread (best ask - best bid) in USDC. Zero if unknown.
    #[serde(default)]
    pub spread: f64,
}

pub struct LaserStream {
    url: String,
}

impl LaserStream {
    /// Create a new LaserStream selecting the right host depending on the
    /// cluster URL (devnet or mainnet).
    pub fn new(api_key: &str, cluster_url: &str) -> Self {
        let host = if cluster_url.contains("devnet") {
            "wss://devnet.helius-rpc.com"
        } else {
            // covers mainnet-beta and any other cluster supported by Helius
            "wss://mainnet.helius-rpc.com"
        };
        Self {
            url: format!("{host}/?api-key={api_key}")
        }
    }

    pub async fn connect(&self, symbols: &[String]) -> Result<impl futures_util::Stream<Item = TradeMsg>> {
        log::info!("Connecting to Helius WS: {}", self.url);
        let (ws, _) = match connect_async(&self.url).await {
            Ok(ok) => ok,
            Err(e) => {
                log::error!("WebSocket connection error: {}", e);
                return Err(e.into());
            }
        };
        let (mut write, read) = ws.split();
        let sub_msg = json!({
            "type": "subscribe",
            "channels": [{"name": "trades", "symbols": symbols}]
        })
        .to_string();
        log::info!("Sending subscription: {}", sub_msg);
        write
            .send(tokio_tungstenite::tungstenite::Message::Text(sub_msg))
            .await?;
        Ok(read.filter_map(|msg| async move {
            match msg.ok()?.into_text().ok().and_then(|t| serde_json::from_str::<TradeMsg>(&t).ok()) {
                Some(tm) => Some(tm),
                None => None,
            }
        }))
    }
}
