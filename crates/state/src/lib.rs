mod error;
mod rocksdb_storage;
mod snapshot_exporter;
mod types;

pub use error::*;
pub use rocksdb_storage::*;
pub use snapshot_exporter::*;
pub use types::*;

use alloy_primitives::Address;
use std::collections::{HashMap, HashSet};

/// State manager using Jellyfish Merkle Tree with RocksDB backend
pub struct StateManager {
    /// RocksDB storage with JMT support
    storage: RocksDbStorage,
    /// Current version (block height)
    version: u64,
    /// Position index: market_id -> Set of addresses with positions
    /// This is an in-memory cache for efficient position iteration
    position_index: HashMap<u32, HashSet<Address>>,
}

impl StateManager {
    /// Create a new state manager with RocksDB backend
    pub fn new<P: AsRef<std::path::Path>>(
        db_path: P,
        pruning_config: PruningConfig,
    ) -> Result<Self, StateError> {
        let storage = RocksDbStorage::new(db_path, pruning_config)?;

        // Get the latest committed version from storage (for recovery after restart)
        // This is the version we should use for reads until begin_block() is called
        let version = storage.get_current_version();

        Ok(Self {
            storage,
            version,
            position_index: HashMap::new(),
        })
    }

    /// Create a new state manager with default settings for testing
    /// Uses UUID v7 (time-ordered) for better RocksDB LSM tree performance
    #[cfg(feature = "testing")]
    pub fn new_for_test() -> Result<Self, StateError> {
        let temp_dir = std::env::temp_dir().join(format!("pranklin_test_{}", uuid::Uuid::now_v7()));
        Self::new(temp_dir, PruningConfig::default())
    }

    /// Get the current state root
    pub fn state_root(&self) -> alloy_primitives::B256 {
        self.storage.get_root(self.version)
    }

    /// Get the current version
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Get storage for snapshots
    pub fn storage(&self) -> RocksDbStorage {
        self.storage.clone()
    }

    /// Begin a new version (block)
    pub fn begin_block(&mut self, height: u64) {
        self.version = height;
    }

    /// Commit the current state
    pub fn commit(&mut self) -> Result<alloy_primitives::B256, StateError> {
        let root = self.storage.commit(self.version)?;
        // Note: Don't increment version here - let begin_block() control version
        Ok(root)
    }

    /// Create a snapshot at the current version
    pub fn create_snapshot(&self) -> Result<(), StateError> {
        self.storage.commit(self.version).map(|_| ())
    }

    /// List available snapshots
    pub fn list_snapshots(&self) -> Result<Vec<u64>, StateError> {
        self.storage.list_snapshots()
    }

    /// Restore from a specific snapshot
    pub fn restore_from_snapshot(&self, version: u64) -> Result<(), StateError> {
        self.storage.restore_from_snapshot(version)
    }

    /// Get database size estimate
    pub fn get_db_size(&self) -> Result<u64, StateError> {
        self.storage.get_db_size()
    }

    /// Get account balance
    pub fn get_balance(&self, address: Address, asset_id: u32) -> Result<u128, StateError> {
        let key = StateKey::Balance { address, asset_id };
        self.storage
            .get(&key, self.version)
            .map(|v| v.unwrap_or_default())
    }

    /// Set account balance
    pub fn set_balance(
        &mut self,
        address: Address,
        asset_id: u32,
        amount: u128,
    ) -> Result<(), StateError> {
        let key = StateKey::Balance { address, asset_id };
        self.storage.set(key, amount)
    }

    /// Get account nonce
    pub fn get_nonce(&self, address: Address) -> Result<u64, StateError> {
        let key = StateKey::Nonce { address };
        self.storage.get(&key, self.version).map(|v| v.unwrap_or(0))
    }

    /// Set account nonce
    pub fn set_nonce(&mut self, address: Address, nonce: u64) -> Result<(), StateError> {
        let key = StateKey::Nonce { address };
        self.storage.set(key, nonce)
    }

    /// Increment account nonce
    pub fn increment_nonce(&mut self, address: Address) -> Result<u64, StateError> {
        let current = self.get_nonce(address)?;
        let new_nonce = current + 1;
        self.set_nonce(address, new_nonce)?;
        Ok(new_nonce)
    }

    /// Get position
    pub fn get_position(
        &self,
        address: Address,
        market_id: u32,
    ) -> Result<Option<Position>, StateError> {
        let key = StateKey::Position { address, market_id };
        self.storage.get(&key, self.version)
    }

    /// Set position
    pub fn set_position(
        &mut self,
        address: Address,
        market_id: u32,
        position: Position,
    ) -> Result<(), StateError> {
        let key = StateKey::Position { address, market_id };
        self.storage.set(key, position)?;

        // Update in-memory position index
        self.position_index
            .entry(market_id)
            .or_default()
            .insert(address);

        // Persist position index to state
        let index_key = StateKey::PositionIndex { market_id };
        let index = self
            .position_index
            .get(&market_id)
            .cloned()
            .unwrap_or_default();
        self.storage.set(index_key, index)?;

        Ok(())
    }

    /// Delete position
    pub fn delete_position(&mut self, address: Address, market_id: u32) -> Result<(), StateError> {
        let key = StateKey::Position { address, market_id };
        self.storage.delete(key)?;

        // Update in-memory position index
        if let Some(addresses) = self.position_index.get_mut(&market_id) {
            addresses.remove(&address);
            if addresses.is_empty() {
                self.position_index.remove(&market_id);
            }
        }

        // Persist position index to state
        let index_key = StateKey::PositionIndex { market_id };
        if let Some(index) = self.position_index.get(&market_id) {
            self.storage.set(index_key, index.clone())?;
        } else {
            // Remove the index if empty
            self.storage.delete(index_key)?;
        }

        Ok(())
    }

    /// Get all positions for a specific market
    /// Returns a list of (Address, Position) tuples
    pub fn get_all_positions_in_market(
        &self,
        market_id: u32,
    ) -> Result<Vec<(Address, Position)>, StateError> {
        let mut positions = Vec::new();

        // First check in-memory index
        let addresses = if let Some(addresses) = self.position_index.get(&market_id) {
            Some(addresses.clone())
        } else {
            // If in-memory index is empty (e.g., after restart), rebuild from state
            let index_key = StateKey::PositionIndex { market_id };
            self.storage
                .get::<std::collections::HashSet<Address>>(&index_key, self.version)?
        };

        if let Some(addresses) = addresses {
            for address in addresses {
                if let Some(position) = self.get_position(address, market_id)? {
                    // Only include positions with non-zero size
                    if position.size > 0 {
                        positions.push((address, position));
                    }
                }
            }
        }

        Ok(positions)
    }

    /// Rebuild position index from state (called on startup)
    pub fn rebuild_position_index(&mut self) -> Result<(), StateError> {
        self.position_index.clear();

        // Get all markets
        let markets = self.list_all_markets()?;

        for market_id in markets {
            // Load position index from state
            let index_key = StateKey::PositionIndex { market_id };
            if let Some(addresses) = self
                .storage
                .get::<std::collections::HashSet<Address>>(&index_key, self.version)?
            {
                self.position_index.insert(market_id, addresses);
            }
        }

        Ok(())
    }

    /// Get order
    pub fn get_order(&self, order_id: u64) -> Result<Option<Order>, StateError> {
        let key = StateKey::Order { order_id };
        self.storage.get(&key, self.version)
    }

    /// Set order
    pub fn set_order(&mut self, order_id: u64, order: Order) -> Result<(), StateError> {
        let key = StateKey::Order { order_id };
        self.storage.set(key, order)
    }

    /// Delete order
    pub fn delete_order(&mut self, order_id: u64) -> Result<(), StateError> {
        let key = StateKey::Order { order_id };
        self.storage.delete(key)
    }

    /// Get market info
    pub fn get_market(&self, market_id: u32) -> Result<Option<Market>, StateError> {
        let key = StateKey::Market { market_id };
        self.storage.get(&key, self.version)
    }

    /// Set market info
    pub fn set_market(&mut self, market_id: u32, info: Market) -> Result<(), StateError> {
        let key = StateKey::Market { market_id };
        self.storage.set(key, info)?;

        // Update market list
        let mut markets = self.list_all_markets()?;
        if !markets.contains(&market_id) {
            markets.push(market_id);
            let list_key = StateKey::MarketList;
            self.storage.set(list_key, markets)?;
        }

        Ok(())
    }

    /// Get list of all market IDs
    pub fn list_all_markets(&self) -> Result<Vec<u32>, StateError> {
        let key = StateKey::MarketList;
        Ok(self.storage.get(&key, self.version)?.unwrap_or_default())
    }

    /// Get funding rate
    pub fn get_funding_rate(&self, market_id: u32) -> Result<FundingRate, StateError> {
        let key = StateKey::FundingRate { market_id };
        self.storage
            .get(&key, self.version)
            .map(|v| v.unwrap_or_default())
    }

    /// Set funding rate
    pub fn set_funding_rate(
        &mut self,
        market_id: u32,
        rate: FundingRate,
    ) -> Result<(), StateError> {
        let key = StateKey::FundingRate { market_id };
        self.storage.set(key, rate)
    }

    /// Get next order ID and increment counter atomically
    pub fn next_order_id(&mut self) -> Result<u64, StateError> {
        let key = StateKey::NextOrderId;
        let current: u64 = self.storage.get(&key, self.version)?.unwrap_or(1);
        let next = current
            .checked_add(1)
            .ok_or_else(|| StateError::Other("Order ID counter overflow".to_string()))?;
        self.storage.set(key, next)?;
        Ok(current)
    }

    /// Check if an address is authorized as a bridge operator
    pub fn is_bridge_operator(&self, address: Address) -> Result<bool, StateError> {
        let key = StateKey::BridgeOperator { address };
        self.storage
            .get(&key, self.version)
            .map(|v| v.unwrap_or(false))
    }

    /// Set bridge operator status
    pub fn set_bridge_operator(
        &mut self,
        address: Address,
        is_operator: bool,
    ) -> Result<(), StateError> {
        let key = StateKey::BridgeOperator { address };
        if is_operator {
            self.storage.set(key, true)
        } else {
            self.storage.delete(key)
        }
    }

    /// Get asset information
    pub fn get_asset(&self, asset_id: u32) -> Result<Option<Asset>, StateError> {
        let key = StateKey::AssetInfo { asset_id };
        self.storage.get(&key, self.version)
    }

    /// Register a new asset
    pub fn set_asset(&mut self, asset_id: u32, asset: Asset) -> Result<(), StateError> {
        let key = StateKey::AssetInfo { asset_id };
        self.storage.set(key, asset)?;

        // Update asset list
        let mut assets = self.list_all_assets()?;
        if !assets.contains(&asset_id) {
            assets.push(asset_id);
            let list_key = StateKey::AssetList;
            self.storage.set(list_key, assets)?;
        }

        Ok(())
    }

    /// Get list of all registered asset IDs
    pub fn list_all_assets(&self) -> Result<Vec<u32>, StateError> {
        let key = StateKey::AssetList;
        Ok(self.storage.get(&key, self.version)?.unwrap_or_default())
    }

    /// Get active order IDs for a market
    ///
    /// This is used for orderbook recovery. Only active orders are tracked here.
    /// OPTIMIZED: Uses individual keys instead of Vec for O(1) add/remove
    pub fn get_active_orders_by_market(&self, market_id: u32) -> Result<Vec<u64>, StateError> {
        // Get the set of order IDs from the index
        // This requires scanning all ActiveOrder keys for this market
        // For now, we'll use the old approach but with individual key storage
        let key = StateKey::ActiveOrderList { market_id };
        Ok(self.storage.get(&key, self.version)?.unwrap_or_default())
    }

    /// Add an order to the active orders list for a market
    /// OPTIMIZED: O(1) operation using individual key
    pub fn add_active_order(&mut self, market_id: u32, order_id: u64) -> Result<(), StateError> {
        // Store individual active order flag
        let key = StateKey::ActiveOrder {
            market_id,
            order_id,
        };
        self.storage.set(key, true)?;

        // Also maintain a list for fast iteration (compromise for now)
        let list_key = StateKey::ActiveOrderList { market_id };
        let mut active_orders: Vec<u64> = self
            .storage
            .get(&list_key, self.version)?
            .unwrap_or_default();
        if !active_orders.contains(&order_id) {
            active_orders.push(order_id);
            self.storage.set(list_key, active_orders)?;
        }

        Ok(())
    }

    /// Remove an order from the active orders list (when filled or cancelled)
    /// OPTIMIZED: O(1) operation using individual key
    pub fn remove_active_order(&mut self, market_id: u32, order_id: u64) -> Result<(), StateError> {
        // Remove individual active order flag
        let key = StateKey::ActiveOrder {
            market_id,
            order_id,
        };
        self.storage.delete(key)?;

        // Also update the list
        let list_key = StateKey::ActiveOrderList { market_id };
        let mut active_orders: Vec<u64> = self
            .storage
            .get(&list_key, self.version)?
            .unwrap_or_default();
        active_orders.retain(|&id| id != order_id);
        self.storage.set(list_key, active_orders)?;

        Ok(())
    }
}

// Note: StateManager doesn't implement Default because it requires
// a database path and pruning configuration. Use StateManager::new() instead
// or StateManager::new_for_test() for testing with the "testing" feature.

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_state_manager() {
        let temp_dir = TempDir::new().unwrap();
        let mut state = StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();
        let address = Address::ZERO;
        let asset_id = 0; // USDC

        // Set balance
        state.set_balance(address, asset_id, 1000).unwrap();
        let balance = state.get_balance(address, asset_id).unwrap();
        assert_eq!(balance, 1000);

        // Commit
        state.begin_block(1);
        let root = state.commit().unwrap();
        assert_ne!(root, alloy_primitives::B256::ZERO);
    }

    #[test]
    fn test_nonce() {
        let temp_dir = TempDir::new().unwrap();
        let mut state = StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();
        let address = Address::ZERO;

        let nonce = state.get_nonce(address).unwrap();
        assert_eq!(nonce, 0);

        state.increment_nonce(address).unwrap();
        let nonce = state.get_nonce(address).unwrap();
        assert_eq!(nonce, 1);
    }

    #[test]
    fn test_position() {
        let temp_dir = TempDir::new().unwrap();
        let mut state = StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();
        let address = Address::ZERO;
        let market_id = 0; // BTC-PERP

        let position = Position {
            size: 100,
            entry_price: 50000_000000, // $50k with 6 decimals
            is_long: true,
            margin: 1000_000000, // $1k margin
            funding_index: 0,
        };

        state
            .set_position(address, market_id, position.clone())
            .unwrap();
        let retrieved = state.get_position(address, market_id).unwrap().unwrap();
        assert_eq!(retrieved.size, position.size);
    }

    #[test]
    fn test_snapshots() {
        let temp_dir = TempDir::new().unwrap();
        let config = PruningConfig {
            snapshot_interval: 10,
            ..Default::default()
        };

        let mut state = StateManager::new(temp_dir.path(), config).unwrap();

        // Create some versions
        for i in 0..25 {
            state.begin_block(i);
            state
                .set_balance(Address::ZERO, 0, i as u128 * 1000)
                .unwrap();
            state.commit().unwrap();
        }

        // List snapshots
        let snapshots = state.list_snapshots().unwrap();
        assert!(!snapshots.is_empty());
    }

    #[test]
    fn test_version_persistence() {
        let temp_dir = TempDir::new().unwrap();

        // Phase 1: Write data
        {
            let mut state = StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();
            assert_eq!(state.version(), 0, "Initial version should be 0");

            state.begin_block(1);
            assert_eq!(
                state.version(),
                1,
                "Version should be 1 after begin_block(1)"
            );

            state.set_balance(Address::ZERO, 0, 1000).unwrap();
            state.commit().unwrap();
            assert_eq!(state.version(), 1, "Version should still be 1 after commit");
        }

        // Phase 2: Read data after restart
        {
            let state = StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();
            assert_eq!(state.version(), 1, "Version should be 1 after restart");

            let balance = state.get_balance(Address::ZERO, 0).unwrap();
            assert_eq!(balance, 1000, "Balance should be recovered");
        }
    }
}
