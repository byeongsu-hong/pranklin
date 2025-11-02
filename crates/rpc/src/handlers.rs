use crate::{RpcError, RpcState, types::*};
use axum::{Json, extract::State};
use pranklin_tx::{Transaction, TxPayload};

/// Helper to decode hex transaction
fn decode_hex_tx(hex: &str) -> Result<Vec<u8>, RpcError> {
    hex::decode(hex.trim_start_matches("0x"))
        .map_err(|e| RpcError::InvalidRequest(format!("Invalid hex: {}", e)))
}

/// Helper to decode and verify transaction
async fn decode_and_verify_tx(state: &RpcState, hex: &str) -> Result<Transaction, RpcError> {
    let tx_bytes = decode_hex_tx(hex)?;
    let tx = Transaction::decode(&tx_bytes)
        .map_err(|e| RpcError::InvalidRequest(format!("Invalid transaction: {}", e)))?;

    let auth = state.auth.read().await;
    auth.verify_transaction(&tx)
        .map_err(|e| RpcError::AuthError(format!("Signature verification failed: {}", e)))?;

    Ok(tx)
}

/// Helper trait to convert state errors to RPC errors with context
trait StateResultExt<T> {
    fn into_rpc_error(self, context: &str) -> Result<T, RpcError>;
}

impl<T> StateResultExt<T> for Result<T, pranklin_state::StateError> {
    fn into_rpc_error(self, context: &str) -> Result<T, RpcError> {
        self.map_err(|e| RpcError::StateError(format!("{}: {}", context, e)))
    }
}

/// Health check handler
pub async fn health() -> &'static str {
    "OK"
}

/// Submit transaction handler
pub async fn submit_transaction(
    State(state): State<RpcState>,
    Json(req): Json<SubmitTxRequest>,
) -> Result<Json<SubmitTxResponse>, RpcError> {
    let tx = decode_and_verify_tx(&state, &req.tx).await?;
    let tx_hash = tx.hash();

    let mut mempool = state.mempool.write().await;
    mempool
        .add(tx)
        .map_err(|e| RpcError::MempoolError(format!("Failed to add to mempool: {}", e)))?;

    Ok(Json(tx_hash.into()))
}

/// Get transaction status handler
pub async fn get_transaction_status(
    State(state): State<RpcState>,
    Json(req): Json<GetTxStatusRequest>,
) -> Result<Json<TxStatus>, RpcError> {
    let mempool = state.mempool.read().await;

    let status = if mempool.get(&req.tx_hash).is_some() {
        TxStatus::pending(req.tx_hash)
    } else {
        // In a full implementation, check historical transactions
        TxStatus::not_found(req.tx_hash)
    };

    Ok(Json(status))
}

/// Get balance handler
pub async fn get_balance(
    State(state): State<RpcState>,
    Json(req): Json<GetBalanceRequest>,
) -> Result<Json<GetBalanceResponse>, RpcError> {
    let engine = state.engine.read().await;
    let balance = engine
        .state()
        .get_balance(req.address, req.asset_id)
        .into_rpc_error("Failed to get balance")?;

    Ok(Json(balance.into()))
}

/// Get nonce handler
pub async fn get_nonce(
    State(state): State<RpcState>,
    Json(req): Json<GetNonceRequest>,
) -> Result<Json<GetNonceResponse>, RpcError> {
    let engine = state.engine.read().await;
    let nonce = engine
        .state()
        .get_nonce(req.address)
        .into_rpc_error("Failed to get nonce")?;

    Ok(Json(nonce.into()))
}

/// Get position handler
pub async fn get_position(
    State(state): State<RpcState>,
    Json(req): Json<GetPositionRequest>,
) -> Result<Json<Option<PositionInfo>>, RpcError> {
    let engine = state.engine.read().await;
    let position = engine
        .state()
        .get_position(req.address, req.market_id)
        .into_rpc_error("Failed to get position")?
        .map(|p| PositionInfo::from_position(p, req.market_id));

    Ok(Json(position))
}

/// Get positions handler
pub async fn get_positions(
    State(_state): State<RpcState>,
    Json(_req): Json<GetNonceRequest>, // Reusing for address
) -> Result<Json<GetPositionsResponse>, RpcError> {
    // In a full implementation, iterate through all markets
    Ok(Json(GetPositionsResponse {
        positions: Vec::new(),
    }))
}

/// Get order handler
pub async fn get_order(
    State(state): State<RpcState>,
    Json(req): Json<GetOrderRequest>,
) -> Result<Json<Option<OrderInfo>>, RpcError> {
    let engine = state.engine.read().await;
    let order = engine
        .state()
        .get_order(req.order_id)
        .into_rpc_error("Failed to get order")?
        .map(Into::into);

    Ok(Json(order))
}

/// List orders handler
pub async fn list_orders(
    State(_state): State<RpcState>,
    Json(_req): Json<ListOrdersRequest>,
) -> Result<Json<ListOrdersResponse>, RpcError> {
    // In a full implementation, query all orders for the account
    Ok(Json(ListOrdersResponse { orders: Vec::new() }))
}

/// Get market info handler
pub async fn get_market_info(
    State(state): State<RpcState>,
    Json(req): Json<GetMarketInfoRequest>,
) -> Result<Json<Option<MarketInfo>>, RpcError> {
    let engine = state.engine.read().await;
    let market = engine
        .state()
        .get_market(req.market_id)
        .into_rpc_error("Failed to get market")?
        .map(Into::into);

    Ok(Json(market))
}

/// Get funding rate handler
pub async fn get_funding_rate(
    State(state): State<RpcState>,
    Json(req): Json<GetFundingRateRequest>,
) -> Result<Json<FundingRateInfo>, RpcError> {
    let engine = state.engine.read().await;
    let funding = engine
        .state()
        .get_funding_rate(req.market_id)
        .into_rpc_error("Failed to get funding rate")?;

    Ok(Json(funding.into()))
}

/// Set agent handler
pub async fn set_agent(
    State(state): State<RpcState>,
    Json(req): Json<SetAgentRequest>,
) -> Result<Json<SuccessResponse>, RpcError> {
    let tx = decode_and_verify_tx(&state, &req.tx).await?;

    let mut auth = state.auth.write().await;
    match &tx.payload {
        TxPayload::SetAgent(set_agent) => {
            auth.set_agent(tx.from, set_agent.agent, set_agent.permissions);
            Ok(Json(SuccessResponse::ok()))
        }
        _ => Err(RpcError::InvalidRequest(
            "Invalid transaction payload".to_string(),
        )),
    }
}

/// Remove agent handler
pub async fn remove_agent(
    State(state): State<RpcState>,
    Json(req): Json<RemoveAgentRequest>,
) -> Result<Json<SuccessResponse>, RpcError> {
    let tx = decode_and_verify_tx(&state, &req.tx).await?;

    let mut auth = state.auth.write().await;
    match &tx.payload {
        TxPayload::RemoveAgent(remove_agent) => {
            auth.remove_agent(tx.from, remove_agent.agent);
            Ok(Json(SuccessResponse::ok()))
        }
        _ => Err(RpcError::InvalidRequest(
            "Invalid transaction payload".to_string(),
        )),
    }
}

/// List agents handler
pub async fn list_agents(
    State(_state): State<RpcState>,
    Json(_req): Json<ListAgentsRequest>,
) -> Result<Json<ListAgentsResponse>, RpcError> {
    // In a full implementation, query agents from auth service
    Ok(Json(ListAgentsResponse { agents: Vec::new() }))
}

/// Get asset info handler
pub async fn get_asset_info(
    State(state): State<RpcState>,
    Json(req): Json<GetAssetInfoRequest>,
) -> Result<Json<Option<AssetInfo>>, RpcError> {
    let engine = state.engine.read().await;
    let asset = engine
        .state()
        .get_asset(req.asset_id)
        .into_rpc_error("Failed to get asset")?
        .map(Into::into);

    Ok(Json(asset))
}

/// List all assets handler
pub async fn list_assets(
    State(state): State<RpcState>,
) -> Result<Json<ListAssetsResponse>, RpcError> {
    let engine = state.engine.read().await;
    let asset_ids = engine
        .state()
        .list_all_assets()
        .into_rpc_error("Failed to list assets")?;

    let assets = asset_ids
        .into_iter()
        .filter_map(|id| engine.state().get_asset(id).ok().flatten().map(Into::into))
        .collect();

    Ok(Json(ListAssetsResponse { assets }))
}

/// Check bridge operator handler
pub async fn check_bridge_operator(
    State(state): State<RpcState>,
    Json(req): Json<CheckBridgeOperatorRequest>,
) -> Result<Json<CheckBridgeOperatorResponse>, RpcError> {
    let engine = state.engine.read().await;
    let is_operator = engine
        .state()
        .is_bridge_operator(req.address)
        .into_rpc_error("Failed to check bridge operator")?;

    Ok(Json(is_operator.into()))
}
