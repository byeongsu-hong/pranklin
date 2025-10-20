use crate::{RpcError, RpcState, types::*};
use axum::{Json, extract::State};

/// Health check handler
pub async fn health() -> &'static str {
    "OK"
}

/// Submit transaction handler
pub async fn submit_transaction(
    State(state): State<RpcState>,
    Json(req): Json<SubmitTxRequest>,
) -> Result<Json<SubmitTxResponse>, RpcError> {
    // Decode transaction
    let tx_bytes = hex::decode(req.tx.trim_start_matches("0x"))
        .map_err(|e| RpcError::InvalidRequest(format!("Invalid hex: {}", e)))?;

    let tx = pranklin_tx::Transaction::decode(&tx_bytes)
        .map_err(|e| RpcError::InvalidRequest(format!("Invalid transaction: {}", e)))?;

    // Verify transaction signature
    let auth = state.auth.read().await;
    auth.verify_transaction(&tx)
        .map_err(|e| RpcError::AuthError(format!("Signature verification failed: {}", e)))?;
    drop(auth);

    // Add to mempool
    let tx_hash = tx.hash();
    let mut mempool = state.mempool.write().await;
    mempool
        .add(tx)
        .map_err(|e| RpcError::MempoolError(format!("Failed to add to mempool: {}", e)))?;

    Ok(Json(SubmitTxResponse { tx_hash }))
}

/// Get transaction status handler
pub async fn get_transaction_status(
    State(state): State<RpcState>,
    Json(req): Json<GetTxStatusRequest>,
) -> Result<Json<TxStatus>, RpcError> {
    let mempool = state.mempool.read().await;

    // Check if transaction is in mempool
    if mempool.get(&req.tx_hash).is_some() {
        return Ok(Json(TxStatus {
            tx_hash: req.tx_hash,
            status: "pending".to_string(),
            block_height: None,
            error: None,
        }));
    }

    // In a full implementation, check historical transactions
    Ok(Json(TxStatus {
        tx_hash: req.tx_hash,
        status: "not_found".to_string(),
        block_height: None,
        error: Some("Transaction not found".to_string()),
    }))
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
        .map_err(|e| RpcError::StateError(format!("Failed to get balance: {}", e)))?;

    Ok(Json(GetBalanceResponse { balance }))
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
        .map_err(|e| RpcError::StateError(format!("Failed to get nonce: {}", e)))?;

    Ok(Json(GetNonceResponse { nonce }))
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
        .map_err(|e| RpcError::StateError(format!("Failed to get position: {}", e)))?;

    let info = position.map(|p| PositionInfo {
        market_id: req.market_id,
        size: p.size,
        entry_price: p.entry_price,
        is_long: p.is_long,
        margin: p.margin,
        unrealized_pnl: 0, // Calculate in full implementation
        is_profit: true,
    });

    Ok(Json(info))
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
        .map_err(|e| RpcError::StateError(format!("Failed to get order: {}", e)))?;

    let info = order.map(|o| OrderInfo {
        id: o.id,
        market_id: o.market_id,
        owner: o.owner,
        is_buy: o.is_buy,
        price: o.price,
        original_size: o.original_size,
        remaining_size: o.remaining_size,
        created_at: o.created_at,
    });

    Ok(Json(info))
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
        .map_err(|e| RpcError::StateError(format!("Failed to get market: {}", e)))?;

    let info = market.map(|m| MarketInfo {
        id: m.id,
        symbol: m.symbol,
        base_asset_id: m.base_asset_id,
        quote_asset_id: m.quote_asset_id,
        price_decimals: m.price_decimals,
        size_decimals: m.size_decimals,
        min_order_size: m.min_order_size,
        max_leverage: m.max_leverage,
    });

    Ok(Json(info))
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
        .map_err(|e| RpcError::StateError(format!("Failed to get funding rate: {}", e)))?;

    Ok(Json(FundingRateInfo {
        rate: funding.rate,
        last_update: funding.last_update,
        index: funding.index,
        mark_price: funding.mark_price,
        oracle_price: funding.oracle_price,
    }))
}

/// Set agent handler
pub async fn set_agent(
    State(state): State<RpcState>,
    Json(req): Json<SetAgentRequest>,
) -> Result<Json<SetAgentResponse>, RpcError> {
    // Decode and verify transaction
    let tx_bytes = hex::decode(req.tx.trim_start_matches("0x"))
        .map_err(|e| RpcError::InvalidRequest(format!("Invalid hex: {}", e)))?;

    let tx = pranklin_tx::Transaction::decode(&tx_bytes)
        .map_err(|e| RpcError::InvalidRequest(format!("Invalid transaction: {}", e)))?;

    // Verify signature
    let mut auth = state.auth.write().await;
    auth.verify_transaction(&tx)
        .map_err(|e| RpcError::AuthError(format!("Signature verification failed: {}", e)))?;

    // Extract set agent payload
    if let pranklin_tx::TxPayload::SetAgent(set_agent) = &tx.payload {
        auth.set_agent(tx.from, set_agent.agent, set_agent.permissions);
        Ok(Json(SetAgentResponse { success: true }))
    } else {
        Err(RpcError::InvalidRequest(
            "Invalid transaction payload".to_string(),
        ))
    }
}

/// Remove agent handler
pub async fn remove_agent(
    State(state): State<RpcState>,
    Json(req): Json<RemoveAgentRequest>,
) -> Result<Json<RemoveAgentResponse>, RpcError> {
    // Similar to set_agent
    let tx_bytes = hex::decode(req.tx.trim_start_matches("0x"))
        .map_err(|e| RpcError::InvalidRequest(format!("Invalid hex: {}", e)))?;

    let tx = pranklin_tx::Transaction::decode(&tx_bytes)
        .map_err(|e| RpcError::InvalidRequest(format!("Invalid transaction: {}", e)))?;

    let mut auth = state.auth.write().await;
    auth.verify_transaction(&tx)
        .map_err(|e| RpcError::AuthError(format!("Signature verification failed: {}", e)))?;

    if let pranklin_tx::TxPayload::RemoveAgent(remove_agent) = &tx.payload {
        auth.remove_agent(tx.from, remove_agent.agent);
        Ok(Json(RemoveAgentResponse { success: true }))
    } else {
        Err(RpcError::InvalidRequest(
            "Invalid transaction payload".to_string(),
        ))
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
        .map_err(|e| RpcError::StateError(format!("Failed to get asset: {}", e)))?;

    let info = asset.map(|a| AssetInfo {
        id: a.id,
        symbol: a.symbol,
        name: a.name,
        decimals: a.decimals,
        is_collateral: a.is_collateral,
        collateral_weight_bps: a.collateral_weight_bps,
    });

    Ok(Json(info))
}

/// List all assets handler
pub async fn list_assets(
    State(state): State<RpcState>,
) -> Result<Json<ListAssetsResponse>, RpcError> {
    let engine = state.engine.read().await;
    let asset_ids = engine
        .state()
        .list_all_assets()
        .map_err(|e| RpcError::StateError(format!("Failed to list assets: {}", e)))?;

    let mut assets = Vec::new();
    for asset_id in asset_ids {
        if let Some(asset) = engine
            .state()
            .get_asset(asset_id)
            .map_err(|e| RpcError::StateError(format!("Failed to get asset: {}", e)))?
        {
            assets.push(AssetInfo {
                id: asset.id,
                symbol: asset.symbol,
                name: asset.name,
                decimals: asset.decimals,
                is_collateral: asset.is_collateral,
                collateral_weight_bps: asset.collateral_weight_bps,
            });
        }
    }

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
        .map_err(|e| RpcError::StateError(format!("Failed to check bridge operator: {}", e)))?;

    Ok(Json(CheckBridgeOperatorResponse { is_operator }))
}
