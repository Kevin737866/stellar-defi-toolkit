use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Extension, Query};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio::sync::RwLock;

/// A real-time arbitrage opportunity update.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArbitrageOpportunityUpdate {
    pub opportunity_id: u64,
    pub asset_pair: String,
    pub potential_profit: u64,
    pub risk_score: u32,
    pub timestamp: u64,
}

/// A price update event for a single asset.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PriceUpdate {
    pub asset: String,
    pub price: u64,
    pub decimals: u32,
    pub timestamp: u64,
}

/// Shared broadcaster that pushes price updates to all WebSocket clients.
#[derive(Clone)]
pub struct PriceBroadcaster {
    tx: broadcast::Sender<PriceUpdate>,
    last_prices: Arc<RwLock<HashMap<String, PriceUpdate>>>,
}

/// Shared arbitrage opportunity stream.
#[derive(Clone)]
pub struct ArbitrageBroadcaster {
    tx: broadcast::Sender<ArbitrageOpportunityUpdate>,
    history: Arc<RwLock<Vec<ArbitrageOpportunityUpdate>>>,
}

impl ArbitrageBroadcaster {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(256);
        Self { tx, history: Arc::new(RwLock::new(Vec::new())) }
    }

    pub async fn push(&self, update: ArbitrageOpportunityUpdate) {
        let mut history = self.history.write().await;
        history.push(update.clone());
        if history.len() > 10_000 { history.drain(..history.len() - 10_000); }
        let _ = self.tx.send(update);
    }

    pub async fn history(&self) -> Vec<ArbitrageOpportunityUpdate> { self.history.read().await.clone() }
    pub fn subscribe(&self) -> broadcast::Receiver<ArbitrageOpportunityUpdate> { self.tx.subscribe() }
}

impl PriceBroadcaster {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(256);
        Self {
            tx,
            last_prices: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Push a price update to all subscribers and record it as the latest price.
    pub async fn push(&self, update: PriceUpdate) {
        let mut prices = self.last_prices.write().await;
        prices.insert(update.asset.clone(), update.clone());
        let _ = self.tx.send(update);
    }

    /// Return a snapshot of the latest known prices.
    pub async fn snapshot(&self) -> HashMap<String, PriceUpdate> {
        self.last_prices.read().await.clone()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<PriceUpdate> {
        self.tx.subscribe()
    }
}

#[derive(Deserialize)]
struct SubscribeMessage {
    #[serde(rename = "type")]
    msg_type: String,
    assets: Vec<String>,
}

#[derive(Deserialize)]
pub struct ArbitrageQuery {
    pub asset_pair: Option<String>,
    pub min_profit: Option<u64>,
    pub max_risk: Option<u32>,
}

/// WebSocket upgrade handler at `/ws/prices`.
/// WebSocket upgrade handler at `/ws/arbitrage`.
pub async fn arbitrage_ws_handler(
    ws: WebSocketUpgrade,
    Query(query): Query<ArbitrageQuery>,
    Extension(broadcaster): Extension<ArbitrageBroadcaster>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_arbitrage_socket(socket, broadcaster, query))
}

async fn handle_arbitrage_socket(socket: WebSocket, broadcaster: ArbitrageBroadcaster, query: ArbitrageQuery) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = broadcaster.subscribe();
    let history = broadcaster.history().await;
    for update in history {
        if matches_arbitrage(&update, &query) {
            if sender.send(Message::Text(serde_json::to_string(&update).unwrap().into())).await.is_err() { return; }
        }
    }
    while let Some(Ok(message)) = receiver.next().await {
        if matches!(message, Message::Close(_)) { break; }
        match rx.recv().await {
            Ok(update) if matches_arbitrage(&update, &query) => {
                if sender.send(Message::Text(serde_json::to_string(&update).unwrap().into())).await.is_err() { break; }
            }
            Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
}

fn matches_arbitrage(update: &ArbitrageOpportunityUpdate, query: &ArbitrageQuery) -> bool {
    query.asset_pair.as_ref().map(|pair| pair == &update.asset_pair).unwrap_or(true)
        && query.min_profit.map(|profit| update.potential_profit >= profit).unwrap_or(true)
        && query.max_risk.map(|risk| update.risk_score <= risk).unwrap_or(true)
}

/// WebSocket upgrade handler at `/ws/prices`.
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
    Extension(broadcaster): Extension<PriceBroadcaster>,
) -> impl IntoResponse {
    let api_key = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok().map(|s| s.to_string()))
        .or_else(|| params.get("x-api-key").cloned());

    ws.on_upgrade(move |socket| handle_socket(socket, broadcaster, api_key))
}

async fn handle_socket(
    socket: WebSocket,
    broadcaster: PriceBroadcaster,
    api_key: Option<String>,
) {
    let is_premium = api_key
        .as_deref()
        .map(|k| k == "premium-api-key")
        .unwrap_or(false);

    let (mut sender, mut receiver) = socket.split();
    let mut rx = broadcaster.subscribe();

    // Send snapshot of last known prices on connect (supports reconnection)
    let snapshot_prices: Vec<serde_json::Value> = broadcaster
        .snapshot()
        .await
        .values()
        .map(|p| {
            serde_json::json!({
                "asset": p.asset,
                "price": p.price,
                "decimals": p.decimals,
                "timestamp": p.timestamp,
            })
        })
        .collect();

    if let Ok(msg) = serde_json::to_string(&serde_json::json!({
        "type": "snapshot",
        "prices": snapshot_prices,
    })) {
        let _ = sender.send(Message::Text(msg.into())).await;
    }

    let throttle_ms: u64 = std::env::var("PRICE_PUSH_INTERVAL_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);

    let mut last_push = tokio::time::Instant::now();

    loop {
        tokio::select! {
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(sub) = serde_json::from_str::<SubscribeMessage>(&text) {
                            if sub.msg_type == "subscribe" {
                                let prices = broadcaster.snapshot().await;
                                for asset in &sub.assets {
                                    if let Some(price) = prices.get(asset) {
                                        if let Ok(msg) = serde_json::to_string(&serde_json::json!({
                                            "type": "price",
                                            "asset": price.asset,
                                            "price": price.price,
                                            "decimals": price.decimals,
                                            "timestamp": price.timestamp,
                                        })) {
                                            let _ = sender.send(Message::Text(msg.into())).await;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
            price = rx.recv() => {
                match price {
                    Ok(update) => {
                        if is_premium || last_push.elapsed() >= Duration::from_millis(throttle_ms) {
                            if let Ok(msg) = serde_json::to_string(&serde_json::json!({
                                "type": "price",
                                "asset": update.asset,
                                "price": update.price,
                                "decimals": update.decimals,
                                "timestamp": update.timestamp,
                            })) {
                                if sender.send(Message::Text(msg.into())).await.is_err() {
                                    break;
                                }
                            }
                            last_push = tokio::time::Instant::now();
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
}
