//! Streaming market data using the public Yellowstone gRPC endpoint.
//!
//! This first iteration keeps the API identical to the previous `LaserStream` so
//! that the rest of the codebase requires minimal changes: it exposes a
//! `connect()` async method that returns an `impl Stream<Item = TradeMsg>`.
//!
//! Under the hood we connect to `https://solana-yellowstone-grpc.publicnode.com:10000`
//! and subscribe to **account updates** for the OpenBook Event Queue of the
//! SOL/USDC market. Parsing the event queue into individual trades will be
//! tackled incrementally; for now the stream simply yields the **mid-price**
//! derived from the best bid/ask whenever the event queue changes so that the
//! rest of the pipeline (ML, strategy, etc.) keeps working.
//!
//! Dependencies:
//!   - yellowstone-grpc-client (async gRPC client)
//!   - tokio-stream (wrap mpsc receiver)
//!
//! Future work: replace the naive mid-price extraction with proper decoding of
//! `Event::Fill` using the `openbook-dex` crate so that we have real trade size
//! and side information.

use anyhow::{anyhow, Result};
use byteorder::{ByteOrder, LittleEndian};
use futures_util::{Stream, StreamExt};
use std::pin::Pin;
use std::collections::HashMap;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;


use yellowstone_grpc_proto::geyser::{subscribe_update, SubscribeRequest, SubscribeRequestFilterAccounts};

use crate::data::TradeMsg;

/// Hard-coded SOL/USDC OpenBook **event queue** account (v1) on mainnet.
/// NOTE: if this ever changes you can move the value to the config file.
const SOL_USDC_EVENT_QUEUE: &str = "HxTJgEMDh8Jo6CQwwht6v7qAbKLFXrrHWEM5E9MJ4tSE";
/// SOL/USDC bids and asks order book accounts (slab v1)
const SOL_USDC_BIDS: &str = "9krN9TPCvQhTWZAxkVtxDC6VqeoLyzmKcqJxw5jZA7Ve";
const SOL_USDC_ASKS: &str = "EpGvXiuQgmEYBLETymFczwa3oYuoFkyeDXovvrSM7g1D";
/// Each price lot equals this many USDC per SOL (approx).
const PRICE_LOT_MULT: f64 = 0.0001;

pub struct GrpcStream {
    endpoint: String,
    event_queue: Pubkey,
    x_token: Option<String>,
}

impl GrpcStream {
    /// Create a new GrpcStream targeting the public Yellowstone endpoint.
    pub fn from_config(cfg: &crate::config::BotConfig) -> Self {
        Self {
            endpoint: "https://solana-yellowstone-grpc.publicnode.com:443".to_string(),
            event_queue: Pubkey::from_str(SOL_USDC_EVENT_QUEUE)
                .expect("valid SOL/USDC event queue pubkey"),
            x_token: cfg.yellowstone_token.clone(),
        }
    }

    /// Connect and return an async stream of `TradeMsg`.
    pub async fn connect(&self) -> Result<Pin<Box<dyn Stream<Item = TradeMsg> + Send>>> {
        // Build the gRPC client using the updated Yellowstone builder API
        let tls_cfg = yellowstone_grpc_client::ClientTlsConfig::new();
        let mut builder = yellowstone_grpc_client::GeyserGrpcClient::build_from_shared(self.endpoint.clone())?
            .tls_config(tls_cfg)?;
        if let Some(ref token) = self.x_token {
            builder = builder.x_token(token.clone())?;
        }
        let mut client = builder.connect().await?;

        // Build SubscribeRequest filtering on the single event queue account.
        let sub_req = {
            let filter_accounts = SubscribeRequestFilterAccounts {
                account: vec![self.event_queue.to_string()],
                owner: vec![],
                filters: vec![],
                nonempty_txn_signature: Some(false),
            };
            let mut req = SubscribeRequest::default();
            req.accounts = {
                let mut map = HashMap::new();
                map.insert("event_queue".to_string(), filter_accounts.clone());
                // also subscribe to bids & asks for context features
                let mut bids_filter = filter_accounts.clone();
                bids_filter.account = vec![Pubkey::from_str(SOL_USDC_BIDS).unwrap().to_string()];
                map.insert("bids".to_string(), bids_filter);
                let mut asks_filter = filter_accounts;
                asks_filter.account = vec![Pubkey::from_str(SOL_USDC_ASKS).unwrap().to_string()];
                map.insert("asks".to_string(), asks_filter);
                map
            };
            req
        };

        // We will forward parsed `TradeMsg` through an mpsc channel.
        let (tx, rx) = mpsc::channel::<TradeMsg>(4096);

        // Spawn background task handling the gRPC stream.
        tokio::spawn(async move {
            match client.subscribe_once(sub_req).await {
                Ok(mut stream) => {
                    // Keep running best bid/ask across updates
                    let mut best_bid: Option<f64> = None;
                    let mut best_ask: Option<f64> = None;
            
                    while let Some(update_res) = stream.next().await {
                        match update_res {
                            Ok(update) => {
                                if let Some(subscribe_update::UpdateOneof::Account(acct)) = update.update_oneof {
                                    if let Some(info) = acct.account {
                                        let pk = acct.pubkey.clone();
                                         if pk == self.event_queue.to_string() {
                                             if let Some((price, size, side)) = decode_last_fill(&info.data) {
                                                 let spread_now = if let (Some(bid), Some(ask)) = (best_bid, best_ask) { ask - bid } else { 0.0 };
                                                 let _ = tx.send(TradeMsg {
                                                     price,
                                                     size,
                                                     side: side.to_string(),
                                                     ts: chrono::Utc::now().timestamp_millis(),
                                                     spread: spread_now,
                                                 }).await;
                                                 log::info!("fill {} size {} (spread {})", price, size, spread_now);
                                             }
                                         } else if pk == Pubkey::from_str(SOL_USDC_BIDS).unwrap().to_string() {
                                             if let Some(p) = decode_best_price(&info.data, true) { best_bid = Some(p); }
                                         } else if pk == Pubkey::from_str(SOL_USDC_ASKS).unwrap().to_string() {
                                             if let Some(p) = decode_best_price(&info.data, false) { best_ask = Some(p); }
                                         }   }
                                        }
                                    }
                                }
                            }
                            Err(err) => {
                                log::error!("gRPC stream item error: {err}");
                            }
                        }
                    }
                }
                Err(err) => {
                    log::error!("gRPC subscribe_once error: {err}");
                }
            }
        });

        Ok(Box::pin(ReceiverStream::new(rx)))
    }
}

/// Very rough helper that looks at the first 16 bytes of the account to read the
/// best bid/ask price lots and compute the mid-price. This is **NOT** precise –
/// it’s only meant to keep the pipeline functional until we implement full
/// `EventQueue` decoding.
/// Decode the most recent `Fill` event in the OpenBook EventQueue and
/// return `(price, size, side)` if at least one fill is available.
/// We read the queue header to locate the last written node and parse it
/// according to Serum/OpenBook layout. Errors are ignored and logged because
/// malformed data should not bring the whole stream down.
fn decode_last_fill(raw: &[u8]) -> Option<(f64, f64, &'static str)> {
    // Layout constants
    const HEADER_LEN: usize = 5 + 8 + 4 + 4 + 4 + 4; // account flags (5) + padding + head + padding + count + padding + seq + padding
    const NODE_SIZE: usize = 88; // FillEvent size

    if raw.len() < HEADER_LEN {
        return None;
    }
    // head and count are little-endian u32 located right after the account-flags (5+3 pad =8)
    let head_off = 8;
    let count_off = 16;
    let head = LittleEndian::read_u32(&raw[head_off..head_off + 4]) as usize;
    let count = LittleEndian::read_u32(&raw[count_off..count_off + 4]) as usize;

    // capacity of circular buffer
    let capacity = (raw.len() - HEADER_LEN) / NODE_SIZE;
    if capacity == 0 || count == 0 {
        return None;
    }
    // Index of last element written
    let last_idx = (head + count - 1) % capacity;
    let node_off = HEADER_LEN + last_idx * NODE_SIZE;
    if node_off + NODE_SIZE > raw.len() {
        return None;
    }
    let node = &raw[node_off..node_off + NODE_SIZE];

    // event_flags byte 0
    let flags = node[0];
    let fill_flag = flags & 0x1 != 0;
    if !fill_flag {
        return None;
    }
    let bid_flag = flags & 0x4 != 0; // third bit
    let side = if bid_flag { "bid" } else { "ask" };

    // native_quantity_paid (qty user paid) is at offset 16
    let qty_paid = LittleEndian::read_u64(&node[16..24]) as f64;
    // native_quantity_released at 8
    let qty_received = LittleEndian::read_u64(&node[8..16]) as f64;

    // For SOL/USDC we treat qty_paid as USDC volume and qty_received as SOL size (for ask fill); need price
    // Price lots per SOL: price = qty_paid / qty_received, fallback
    if qty_received == 0.0 {
        return None;
    }
    let price = qty_paid / qty_received / 1_000_000f64; // assuming USDC has 6 decimals
    let size = qty_received / 1_000_000f64; // SOL has 9 decimals; approximate
    Some((price, size, side))
}

fn decode_best_price(raw: &[u8], _is_bid: bool) -> Option<f64> {
    if raw.len() < 8 {
        return None;
    }
    let price_lots = LittleEndian::read_u64(&raw[0..8]);
    if price_lots == 0 {
        return None;
    }
    Some(price_lots as f64 * PRICE_LOT_MULT)
}

fn extract_mid_price(raw: &[u8]) -> Result<f64> {
    if raw.len() < 16 {
        return Err(anyhow!("account data too short"));
    }
    // For now assume little-endian u64 bid price lots followed by ask price lots.
    let bid_lots = u64::from_le_bytes(raw[0..8].try_into()?);
    let ask_lots = u64::from_le_bytes(raw[8..16].try_into()?);
    if bid_lots == 0 || ask_lots == 0 {
        return Err(anyhow!("invalid lots"));
    }
    // Each lot on OpenBook SOL/USDC equals 0.0001 SOL; convert to SOL price in USDC
    // This is a simplification; proper decoding will use `quote_lot_size/base_lot_size`.
    let bid = bid_lots as f64 * 0.0001;
    let ask = ask_lots as f64 * 0.0001;
    Ok((bid + ask) / 2.0)
}
