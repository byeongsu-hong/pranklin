use jmt::KeyHash;
use pranklin_types::Address;
use serde::{Deserialize, Serialize};
use sha2::Digest;

// Re-export from pranklin-types
pub use pranklin_types::{Asset, FundingRate, Market, Order, OrderStatus, Position};

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
        let hash = sha2::Sha256::digest(&bytes);
        KeyHash(hash.into())
    }
}

impl From<StateKey> for KeyHash {
    fn from(key: StateKey) -> Self {
        key.hash()
    }
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
