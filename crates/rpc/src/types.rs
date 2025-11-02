use alloy_primitives::{Address, B256};
use serde::{Deserialize, Serialize};

/// Generic success response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuccessResponse {
    pub success: bool,
}

impl SuccessResponse {
    pub fn ok() -> Self {
        Self { success: true }
    }
}

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

impl From<B256> for SubmitTxResponse {
    fn from(tx_hash: B256) -> Self {
        Self { tx_hash }
    }
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

impl TxStatus {
    pub fn pending(tx_hash: B256) -> Self {
        Self {
            tx_hash,
            status: "pending".to_string(),
            block_height: None,
            error: None,
        }
    }

    pub fn not_found(tx_hash: B256) -> Self {
        Self {
            tx_hash,
            status: "not_found".to_string(),
            block_height: None,
            error: Some("Transaction not found".to_string()),
        }
    }

    pub fn confirmed(tx_hash: B256, block_height: u64) -> Self {
        Self {
            tx_hash,
            status: "confirmed".to_string(),
            block_height: Some(block_height),
            error: None,
        }
    }

    pub fn failed(tx_hash: B256, error: String) -> Self {
        Self {
            tx_hash,
            status: "failed".to_string(),
            block_height: None,
            error: Some(error),
        }
    }
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

impl From<u128> for GetBalanceResponse {
    fn from(balance: u128) -> Self {
        Self { balance }
    }
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

impl From<u64> for GetNonceResponse {
    fn from(nonce: u64) -> Self {
        Self { nonce }
    }
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

impl PositionInfo {
    pub fn from_position(position: pranklin_state::Position, market_id: u32) -> Self {
        Self {
            market_id,
            size: position.size,
            entry_price: position.entry_price,
            is_long: position.is_long,
            margin: position.margin,
            unrealized_pnl: 0,
            is_profit: true,
        }
    }
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

impl From<pranklin_state::Order> for OrderInfo {
    fn from(order: pranklin_state::Order) -> Self {
        Self {
            id: order.id,
            market_id: order.market_id,
            owner: order.owner,
            is_buy: order.is_buy,
            price: order.price,
            original_size: order.original_size,
            remaining_size: order.remaining_size,
            created_at: order.created_at,
        }
    }
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

impl From<pranklin_state::Market> for MarketInfo {
    fn from(market: pranklin_state::Market) -> Self {
        Self {
            id: market.id,
            symbol: market.symbol,
            base_asset_id: market.base_asset_id,
            quote_asset_id: market.quote_asset_id,
            price_decimals: market.price_decimals,
            size_decimals: market.size_decimals,
            min_order_size: market.min_order_size,
            max_leverage: market.max_leverage,
        }
    }
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

impl From<pranklin_state::FundingRate> for FundingRateInfo {
    fn from(funding: pranklin_state::FundingRate) -> Self {
        Self {
            rate: funding.rate,
            last_update: funding.last_update,
            index: funding.index,
            mark_price: funding.mark_price,
            oracle_price: funding.oracle_price,
        }
    }
}

/// Set agent request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetAgentRequest {
    /// Signed transaction
    pub tx: String, // hex-encoded
}

/// Remove agent request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveAgentRequest {
    /// Signed transaction
    pub tx: String, // hex-encoded
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

impl From<pranklin_state::Asset> for AssetInfo {
    fn from(asset: pranklin_state::Asset) -> Self {
        Self {
            id: asset.id,
            symbol: asset.symbol,
            name: asset.name,
            decimals: asset.decimals,
            is_collateral: asset.is_collateral,
            collateral_weight_bps: asset.collateral_weight_bps,
        }
    }
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

impl From<bool> for CheckBridgeOperatorResponse {
    fn from(is_operator: bool) -> Self {
        Self { is_operator }
    }
}
