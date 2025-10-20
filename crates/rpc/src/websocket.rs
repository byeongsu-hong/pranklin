use axum::extract::ws::{Message, WebSocket};
use axum::{
    extract::{State, ws::WebSocketUpgrade},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::RpcState;

/// WebSocket message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsMessage {
    /// Subscribe to a channel
    Subscribe {
        channel: String,
    },
    /// Unsubscribe from a channel
    Unsubscribe {
        channel: String,
    },
    /// Order book update
    OrderBookUpdate {
        market_id: u32,
        bids: Vec<(u64, u64)>, // (price, size)
        asks: Vec<(u64, u64)>,
    },
    /// Trade update
    TradeUpdate {
        market_id: u32,
        price: u64,
        size: u64,
        is_buy: bool,
        timestamp: u64,
    },
    /// Position update
    PositionUpdate {
        market_id: u32,
        trader: String,
        size: u64,
        entry_price: u64,
        is_long: bool,
    },
    /// Funding rate update
    FundingUpdate {
        market_id: u32,
        rate: i64,
    },
    /// Liquidation event
    LiquidationEvent {
        market_id: u32,
        trader: String,
        size: u64,
        price: u64,
    },
    /// Ping/Pong for connection keepalive
    Ping,
    Pong,
    /// Error message
    Error {
        message: String,
    },
}

/// WebSocket broadcaster for sending updates to connected clients
#[derive(Clone)]
pub struct WsBroadcaster {
    tx: broadcast::Sender<WsMessage>,
}

impl WsBroadcaster {
    /// Create a new WebSocket broadcaster
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    /// Broadcast a message to all connected clients
    pub fn broadcast(&self, msg: WsMessage) {
        let _ = self.tx.send(msg);
    }

    /// Subscribe to receive broadcasts
    pub fn subscribe(&self) -> broadcast::Receiver<WsMessage> {
        self.tx.subscribe()
    }
}

/// WebSocket upgrade handler
pub async fn ws_handler(ws: WebSocketUpgrade, State(state): State<RpcState>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

/// Handle WebSocket connection
async fn handle_socket(socket: WebSocket, _state: RpcState) {
    let (mut sender, mut receiver) = socket.split();

    // For now, we'll create a simple echo/subscription system
    // In production, you'd integrate this with the broadcaster

    let mut subscriptions: Vec<String> = Vec::new();

    // Spawn a task to handle outgoing messages
    let mut outgoing_rx = tokio::sync::mpsc::unbounded_channel::<WsMessage>().1;

    tokio::spawn(async move {
        while let Some(msg) = outgoing_rx.recv().await {
            let json = serde_json::to_string(&msg).unwrap();
            if sender.send(Message::Text(json.into())).await.is_err() {
                break;
            }
        }
    });

    // Handle incoming messages
    while let Some(Ok(msg)) = receiver.next().await {
        match msg {
            Message::Text(text) => {
                // Parse incoming message
                let parsed: Result<WsMessage, _> = serde_json::from_str(&text);

                match parsed {
                    Ok(WsMessage::Subscribe { channel }) => {
                        subscriptions.push(channel.clone());
                        tracing::info!("Client subscribed to channel: {}", channel);
                    }
                    Ok(WsMessage::Unsubscribe { channel }) => {
                        subscriptions.retain(|c| c != &channel);
                        tracing::info!("Client unsubscribed from channel: {}", channel);
                    }
                    Ok(WsMessage::Ping) => {
                        // Respond with Pong
                        // In production, send via outgoing channel
                    }
                    _ => {
                        tracing::warn!("Received unexpected message type");
                    }
                }
            }
            Message::Close(_) => {
                tracing::info!("WebSocket connection closed");
                break;
            }
            _ => {}
        }
    }
}

/// Helper function to broadcast orderbook updates
pub async fn broadcast_orderbook_update(
    broadcaster: &WsBroadcaster,
    market_id: u32,
    bids: Vec<(u64, u64)>,
    asks: Vec<(u64, u64)>,
) {
    broadcaster.broadcast(WsMessage::OrderBookUpdate {
        market_id,
        bids,
        asks,
    });
}

/// Helper function to broadcast trade updates
pub async fn broadcast_trade_update(
    broadcaster: &WsBroadcaster,
    market_id: u32,
    price: u64,
    size: u64,
    is_buy: bool,
    timestamp: u64,
) {
    broadcaster.broadcast(WsMessage::TradeUpdate {
        market_id,
        price,
        size,
        is_buy,
        timestamp,
    });
}

/// Helper function to broadcast liquidation events
pub async fn broadcast_liquidation(
    broadcaster: &WsBroadcaster,
    market_id: u32,
    trader: String,
    size: u64,
    price: u64,
) {
    broadcaster.broadcast(WsMessage::LiquidationEvent {
        market_id,
        trader,
        size,
        price,
    });
}

// Re-export futures trait for split
use futures_util::{SinkExt, StreamExt};

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_broadcaster() {
        let broadcaster = WsBroadcaster::new(100);

        let mut rx = broadcaster.subscribe();

        // Broadcast a message
        broadcaster.broadcast(WsMessage::Ping);

        // Receive the message
        let msg = rx.recv().await.unwrap();
        assert!(matches!(msg, WsMessage::Ping));
    }

    #[tokio::test]
    async fn test_message_serialization() {
        let msg = WsMessage::OrderBookUpdate {
            market_id: 0,
            bids: vec![(50000, 100), (49900, 200)],
            asks: vec![(50100, 150), (50200, 250)],
        };

        let json = serde_json::to_string(&msg).unwrap();
        let parsed: WsMessage = serde_json::from_str(&json).unwrap();

        match parsed {
            WsMessage::OrderBookUpdate { market_id, .. } => {
                assert_eq!(market_id, 0);
            }
            _ => panic!("Wrong message type"),
        }
    }
}
