mod error;
mod handlers;
mod metrics;
mod middleware;
mod types;
mod websocket;

pub use error::*;
pub use metrics::*;
pub use middleware::*;
pub use types::*;
pub use websocket::*;

use axum::{
    Router,
    routing::{get, post},
};
use pranklin_auth::AuthService;
use pranklin_engine::Engine;
use pranklin_mempool::Mempool;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;

/// RPC server state
#[derive(Clone)]
pub struct RpcState {
    /// Authentication service
    pub auth: Arc<RwLock<AuthService>>,
    /// Mempool
    pub mempool: Arc<RwLock<Mempool>>,
    /// Engine
    pub engine: Arc<RwLock<Engine>>,
    /// Metrics collector (optional)
    pub metrics: Option<Arc<Metrics>>,
    /// WebSocket broadcaster (optional)
    pub ws_broadcaster: Option<Arc<WsBroadcaster>>,
}

impl RpcState {
    /// Create a new RPC state
    pub fn new(auth: AuthService, mempool: Mempool, engine: Engine) -> Self {
        Self {
            auth: Arc::new(RwLock::new(auth)),
            mempool: Arc::new(RwLock::new(mempool)),
            engine: Arc::new(RwLock::new(engine)),
            metrics: None,
            ws_broadcaster: None,
        }
    }

    /// Create a new RPC state with metrics
    pub fn new_with_metrics(
        auth: AuthService,
        mempool: Mempool,
        engine: Engine,
        metrics: Metrics,
    ) -> Self {
        Self {
            auth: Arc::new(RwLock::new(auth)),
            mempool: Arc::new(RwLock::new(mempool)),
            engine: Arc::new(RwLock::new(engine)),
            metrics: Some(Arc::new(metrics)),
            ws_broadcaster: None,
        }
    }

    /// Create a new RPC state with all features
    pub fn new_with_features(
        auth: AuthService,
        mempool: Mempool,
        engine: Engine,
        metrics: Option<Metrics>,
        ws_broadcaster: Option<WsBroadcaster>,
    ) -> Self {
        Self {
            auth: Arc::new(RwLock::new(auth)),
            mempool: Arc::new(RwLock::new(mempool)),
            engine: Arc::new(RwLock::new(engine)),
            metrics: metrics.map(Arc::new),
            ws_broadcaster: ws_broadcaster.map(Arc::new),
        }
    }

    /// Create RPC state from shared components
    pub fn new_from_shared(
        auth: Arc<RwLock<AuthService>>,
        mempool: Arc<RwLock<Mempool>>,
        engine: Arc<RwLock<Engine>>,
    ) -> Self {
        Self {
            auth,
            mempool,
            engine,
            metrics: None,
            ws_broadcaster: None,
        }
    }

    /// Create RPC state from shared components with metrics
    pub fn new_from_shared_with_metrics(
        auth: Arc<RwLock<AuthService>>,
        mempool: Arc<RwLock<Mempool>>,
        engine: Arc<RwLock<Engine>>,
        metrics: Metrics,
    ) -> Self {
        Self {
            auth,
            mempool,
            engine,
            metrics: Some(Arc::new(metrics)),
            ws_broadcaster: None,
        }
    }
}

/// Metrics endpoint handler
async fn metrics_handler(axum::extract::State(state): axum::extract::State<RpcState>) -> String {
    state
        .metrics
        .as_ref()
        .and_then(|m| m.export().ok())
        .unwrap_or_else(|| "# Metrics not enabled\n".to_string())
}

/// Create RPC server router
pub fn create_router(state: RpcState) -> Router {
    Router::new()
        // Health check
        .route("/health", get(handlers::health))
        // Metrics endpoint
        .route("/metrics", get(metrics_handler))
        // WebSocket endpoint
        .route("/ws", get(websocket::ws_handler))
        // Transaction endpoints
        .route("/tx/submit", post(handlers::submit_transaction))
        .route("/tx/status", post(handlers::get_transaction_status))
        // Query endpoints
        .route("/account/balance", post(handlers::get_balance))
        .route("/account/nonce", post(handlers::get_nonce))
        .route("/account/position", post(handlers::get_position))
        .route("/account/positions", post(handlers::get_positions))
        // Order endpoints
        .route("/order/get", post(handlers::get_order))
        .route("/order/list", post(handlers::list_orders))
        // Market endpoints
        .route("/market/info", post(handlers::get_market_info))
        .route("/market/funding", post(handlers::get_funding_rate))
        // Agent endpoints
        .route("/agent/set", post(handlers::set_agent))
        .route("/agent/remove", post(handlers::remove_agent))
        .route("/agent/list", post(handlers::list_agents))
        // Asset endpoints
        .route("/asset/info", post(handlers::get_asset_info))
        .route("/asset/list", get(handlers::list_assets))
        // Bridge operator endpoints
        .route(
            "/bridge/check_operator",
            post(handlers::check_bridge_operator),
        )
        // Middleware
        .layer(CorsLayer::permissive())
        .with_state(state)
}

/// Start the RPC server
pub async fn start_server(state: RpcState, addr: &str) -> Result<(), RpcError> {
    let router = create_router(state);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| RpcError::ServerError(e.to_string()))?;

    tracing::info!("RPC server listening on {}", addr);

    axum::serve(listener, router)
        .await
        .map_err(|e| RpcError::ServerError(e.to_string()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rpc_state() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let auth = AuthService::new();
        let mempool = Mempool::default();
        let state = pranklin_state::StateManager::new(
            temp_dir.path(),
            pranklin_state::PruningConfig::default(),
        )
        .unwrap();
        let engine = Engine::new(state);

        let state = RpcState::new(auth, mempool, engine);
        assert!(state.auth.try_read().is_ok());
    }
}
