use alloy_primitives::{Address, B256};
use serde::{Deserialize, Serialize};

/// Submit transaction request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitTxRequest {
    /// Transaction bytes
    pub tx: String, // hex-encoded transaction
}

/// Submit transaction response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitTxResponse {
    /// Transaction hash
    pub tx_hash: B256,
}

/// Get transaction status request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetTxStatusRequest {
    /// Transaction hash
    pub tx_hash: B256,
}

/// Transaction status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxStatus {
    /// Transaction hash
    pub tx_hash: B256,
    /// Status: "pending", "confirmed", "failed"
    pub status: String,
    /// Block height (if confirmed)
    pub block_height: Option<u64>,
    /// Error message (if failed)
    pub error: Option<String>,
}

/// Get balance request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetBalanceRequest {
    /// Account address
    pub address: Address,
    /// Asset identifier
    pub asset_id: u32,
}

/// Get balance response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetBalanceResponse {
    /// Balance
    pub balance: u128,
}

/// Get nonce request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetNonceRequest {
    /// Account address
    pub address: Address,
}

/// Get nonce response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetNonceResponse {
    /// Current nonce
    pub nonce: u64,
}

/// Get position request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetPositionRequest {
    /// Account address
    pub address: Address,
    /// Market ID
    pub market_id: u32,
}

/// Position info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionInfo {
    /// Market ID
    pub market_id: u32,
    /// Position size
    pub size: u64,
    /// Entry price
    pub entry_price: u64,
    /// Long or short
    pub is_long: bool,
    /// Margin
    pub margin: u128,
    /// Unrealized PnL
    pub unrealized_pnl: u128,
    /// Is profit
    pub is_profit: bool,
}

/// Get positions response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetPositionsResponse {
    /// List of positions
    pub positions: Vec<PositionInfo>,
}

/// Get order request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetOrderRequest {
    /// Order ID
    pub order_id: u64,
}

/// Order info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderInfo {
    /// Order ID
    pub id: u64,
    /// Market ID
    pub market_id: u32,
    /// Owner
    pub owner: Address,
    /// Buy or sell
    pub is_buy: bool,
    /// Price
    pub price: u64,
    /// Original size
    pub original_size: u64,
    /// Remaining size
    pub remaining_size: u64,
    /// Created at (block height)
    pub created_at: u64,
}

/// List orders request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListOrdersRequest {
    /// Account address
    pub address: Address,
    /// Optional market filter
    pub market_id: Option<u32>,
}

/// List orders response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListOrdersResponse {
    /// Orders
    pub orders: Vec<OrderInfo>,
}

/// Get market info request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetMarketInfoRequest {
    /// Market ID
    pub market_id: u32,
}

/// Market info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketInfo {
    /// Market ID
    pub id: u32,
    /// Symbol
    pub symbol: String,
    /// Base asset ID
    pub base_asset_id: u32,
    /// Quote asset ID
    pub quote_asset_id: u32,
    /// Price decimals
    pub price_decimals: u8,
    /// Size decimals
    pub size_decimals: u8,
    /// Minimum order size
    pub min_order_size: u64,
    /// Maximum leverage
    pub max_leverage: u32,
}

/// Get funding rate request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetFundingRateRequest {
    /// Market ID
    pub market_id: u32,
}

/// Funding rate info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FundingRateInfo {
    /// Current rate (in basis points)
    pub rate: i64,
    /// Last update timestamp
    pub last_update: u64,
    /// Cumulative index
    pub index: i128,
    /// Mark price
    pub mark_price: u64,
    /// Oracle price
    pub oracle_price: u64,
}

/// Set agent request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetAgentRequest {
    /// Signed transaction
    pub tx: String, // hex-encoded
}

/// Set agent response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetAgentResponse {
    /// Success
    pub success: bool,
}

/// Remove agent request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveAgentRequest {
    /// Signed transaction
    pub tx: String, // hex-encoded
}

/// Remove agent response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveAgentResponse {
    /// Success
    pub success: bool,
}

/// List agents request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListAgentsRequest {
    /// Account address
    pub address: Address,
}

/// Agent info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    /// Agent address
    pub agent: Address,
    /// Permissions
    pub permissions: u64,
}

/// List agents response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListAgentsResponse {
    /// Agents
    pub agents: Vec<AgentInfo>,
}

/// Get asset info request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetAssetInfoRequest {
    /// Asset ID
    pub asset_id: u32,
}

/// Asset info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetInfo {
    /// Asset ID
    pub id: u32,
    /// Symbol (e.g., "USDC")
    pub symbol: String,
    /// Name (e.g., "USD Coin")
    pub name: String,
    /// Decimals
    pub decimals: u8,
    /// Is collateral
    pub is_collateral: bool,
    /// Collateral weight (basis points)
    pub collateral_weight_bps: u32,
}

/// List assets response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListAssetsResponse {
    /// Assets
    pub assets: Vec<AssetInfo>,
}

/// Set bridge operator request (requires admin permissions)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetBridgeOperatorRequest {
    /// Operator address
    pub operator: Address,
    /// Whether to grant or revoke operator status
    pub is_operator: bool,
}

/// Set bridge operator response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetBridgeOperatorResponse {
    /// Success
    pub success: bool,
}

/// Check bridge operator request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckBridgeOperatorRequest {
    /// Address to check
    pub address: Address,
}

/// Check bridge operator response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckBridgeOperatorResponse {
    /// Whether the address is a bridge operator
    pub is_operator: bool,
}
