use alloy_primitives::Address;
use jmt::KeyHash;
use serde::{Deserialize, Serialize};
use sha2::Digest;

/// State key types
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StateKey {
    /// Account balance: address + asset_id -> amount
    Balance { address: Address, asset_id: u32 },
    /// Account nonce: address -> nonce
    Nonce { address: Address },
    /// Position: address + market_id -> Position
    Position { address: Address, market_id: u32 },
    /// Order: order_id -> Order
    Order { order_id: u64 },
    /// Market info: market_id -> Market
    Market { market_id: u32 },
    /// Funding rate: market_id -> FundingRate
    FundingRate { market_id: u32 },
    /// Global order ID counter
    NextOrderId,
    /// Individual active order flag: market_id + order_id -> bool
    /// Used for fast orderbook recovery (individual keys for O(1) add/remove)
    ActiveOrder { market_id: u32, order_id: u64 },
    /// List of active order IDs for a market: market_id -> Vec<u64>
    /// Maintained alongside individual flags for fast iteration
    ActiveOrderList { market_id: u32 },
    /// List of all market IDs
    MarketList,
    /// Position index: market_id -> Set of addresses with positions
    /// Used for efficient position iteration
    PositionIndex { market_id: u32 },
    /// Latest version for a key hash: key_hash -> u64
    /// Used for O(1) version lookup instead of O(n) scan
    LatestVersion { key_hash_hex: String },
    /// Bridge operator status: address -> bool
    /// Only authorized bridge operators can process bridge deposits/withdrawals
    BridgeOperator { address: Address },
    /// Asset registry: asset_id -> AssetInfo
    /// Stores metadata about supported assets
    AssetInfo { asset_id: u32 },
    /// List of all registered asset IDs
    AssetList,
}

impl StateKey {
    /// Hash the key for use in the merkle tree
    pub fn hash(&self) -> KeyHash {
        let bytes = serde_json::to_vec(self).unwrap();
        let mut hasher = <sha2::Sha256 as sha2::Digest>::new();
        hasher.update(&bytes);
        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(result.as_slice());
        KeyHash(hash)
    }
}

/// Position information
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    /// Position size (in base units)
    pub size: u64,
    /// Entry price (in base units)
    pub entry_price: u64,
    /// Long or short
    pub is_long: bool,
    /// Margin (collateral)
    pub margin: u128,
    /// Last funding index when position was updated
    pub funding_index: u128,
}

/// Order status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderStatus {
    /// Order is active and can be matched
    Active,
    /// Order has been fully filled
    Filled,
    /// Order has been cancelled
    Cancelled,
}

/// Order information
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Order {
    /// Order ID
    pub id: u64,
    /// Market ID
    pub market_id: u32,
    /// Owner address
    pub owner: Address,
    /// Buy or sell
    pub is_buy: bool,
    /// Price
    pub price: u64,
    /// Original size
    pub original_size: u64,
    /// Remaining size
    pub remaining_size: u64,
    /// Order status
    pub status: OrderStatus,
    /// Block when order was created
    pub created_at: u64,
    /// Reduce only flag
    pub reduce_only: bool,
    /// Post only flag
    pub post_only: bool,
}

/// Market information
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Market {
    /// Market ID
    pub id: u32,
    /// Market name/symbol
    pub symbol: String,
    /// Base asset ID
    pub base_asset_id: u32,
    /// Quote asset ID
    pub quote_asset_id: u32,
    /// Tick size (minimum price increment in base units)
    ///
    /// All prices must be multiples of tick_size.
    /// Example: tick_size = 100 means prices move in increments of 100 base units
    /// (e.g., if base unit is 1 cent, tick_size=100 means $1.00 increments)
    pub tick_size: u64,
    /// Price precision (decimals for display)
    pub price_decimals: u8,
    /// Size precision (decimals)
    pub size_decimals: u8,
    /// Minimum order size
    pub min_order_size: u64,
    /// Maximum order size (prevent manipulation)
    pub max_order_size: u64,
    /// Maximum leverage
    pub max_leverage: u32,
    /// Maintenance margin ratio (in basis points)
    pub maintenance_margin_bps: u32,
    /// Initial margin ratio (in basis points)
    pub initial_margin_bps: u32,
    /// Liquidation fee (in basis points)
    pub liquidation_fee_bps: u32,
    /// Funding rate interval (in seconds)
    pub funding_interval: u64,
    /// Maximum funding rate (in basis points)
    pub max_funding_rate_bps: u32,
}

impl Market {
    /// Normalize price to nearest tick
    ///
    /// Rounds the price to the nearest valid tick boundary.
    pub fn normalize_price(&self, price: u64) -> u64 {
        if self.tick_size == 0 {
            return price; // Avoid division by zero
        }
        // Round to nearest tick
        ((price + self.tick_size / 2) / self.tick_size) * self.tick_size
    }

    /// Validate that price is on a valid tick boundary
    pub fn validate_price(&self, price: u64) -> bool {
        if self.tick_size == 0 {
            return true; // No tick restriction
        }
        price.is_multiple_of(self.tick_size)
    }

    /// Convert price to tick ID
    pub fn price_to_tick(&self, price: u64) -> u64 {
        if self.tick_size == 0 {
            return price;
        }
        price / self.tick_size
    }

    /// Convert tick ID to price
    pub fn tick_to_price(&self, tick: u64) -> u64 {
        tick * self.tick_size
    }
}

/// Funding rate information
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct FundingRate {
    /// Current funding rate (per interval, in basis points)
    pub rate: i64,
    /// Last update timestamp
    pub last_update: u64,
    /// Cumulative funding index
    pub index: i128,
    /// Mark price at last update
    pub mark_price: u64,
    /// Oracle price at last update
    pub oracle_price: u64,
}

/// Asset information
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    /// Asset ID (unique identifier)
    pub id: u32,
    /// Asset symbol (e.g., "USDC", "USDT", "ETH")
    pub symbol: String,
    /// Asset name (e.g., "USD Coin")
    pub name: String,
    /// Decimals for display
    pub decimals: u8,
    /// Whether this asset can be used as collateral
    pub is_collateral: bool,
    /// Collateral weight in basis points (e.g., 10000 = 100%, 9000 = 90%)
    /// Used for calculating collateral value
    pub collateral_weight_bps: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_key_hash() {
        let key1 = StateKey::Balance {
            address: Address::ZERO,
            asset_id: 0,
        };
        let key2 = StateKey::Balance {
            address: Address::ZERO,
            asset_id: 0,
        };
        let key3 = StateKey::Nonce {
            address: Address::ZERO,
        };

        assert_eq!(key1.hash(), key2.hash());
        assert_ne!(key1.hash(), key3.hash());
    }
}
