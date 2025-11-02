use alloy_primitives::Address;
use std::collections::{HashMap, HashSet};

use crate::{RocksDbStorage, StateError, StateKey};

/// Position index manager
///
/// Maintains mapping: market_id -> Set<Address>
pub struct PositionIndexManager {
    /// In-memory index for fast lookup
    index: HashMap<u32, HashSet<Address>>,
    /// Storage backend
    storage: RocksDbStorage,
    /// Current version for reads
    version: u64,
}

impl PositionIndexManager {
    pub fn new(storage: RocksDbStorage, version: u64) -> Self {
        Self {
            index: HashMap::new(),
            storage,
            version,
        }
    }

    /// Add a position to the index
    pub fn add_position(&mut self, market_id: u32, address: Address) -> Result<(), StateError> {
        self.index.entry(market_id).or_default().insert(address);
        self.persist_index(market_id)
    }

    /// Remove a position from the index
    pub fn remove_position(&mut self, market_id: u32, address: Address) -> Result<(), StateError> {
        if let Some(addresses) = self.index.get_mut(&market_id) {
            addresses.remove(&address);
            if addresses.is_empty() {
                self.index.remove(&market_id);
            }
        }
        self.persist_or_delete_index(market_id)
    }

    /// Get all addresses with positions in a market
    pub fn get_addresses(&self, market_id: u32) -> Option<HashSet<Address>> {
        self.index.get(&market_id).cloned()
    }

    /// Rebuild index from state (called on recovery)
    pub fn rebuild_from_state(&mut self, markets: &[u32]) -> Result<(), StateError> {
        self.index = markets
            .iter()
            .filter_map(|&market_id| {
                let key = StateKey::PositionIndex { market_id };
                self.storage
                    .get::<HashSet<Address>>(&key, self.version)
                    .ok()
                    .flatten()
                    .map(|addresses| (market_id, addresses))
            })
            .collect();
        Ok(())
    }

    /// Update version for reads
    pub fn set_version(&mut self, version: u64) {
        self.version = version;
    }

    /// Persist index to storage
    fn persist_index(&self, market_id: u32) -> Result<(), StateError> {
        let key = StateKey::PositionIndex { market_id };
        let index = self.index.get(&market_id).cloned().unwrap_or_default();
        self.storage.set(key, index)
    }

    /// Persist or delete index based on existence
    fn persist_or_delete_index(&self, market_id: u32) -> Result<(), StateError> {
        let key = StateKey::PositionIndex { market_id };
        match self.index.get(&market_id) {
            Some(index) => self.storage.set(key, index.clone()),
            None => self.storage.delete(key),
        }
    }
}

/// Active order index manager
///
/// Maintains mapping: market_id -> Set<order_id>
pub struct OrderIndexManager {
    /// In-memory index
    index: HashMap<u32, HashSet<u64>>,
    /// Storage backend
    storage: RocksDbStorage,
    /// Current version
    version: u64,
}

impl OrderIndexManager {
    pub fn new(storage: RocksDbStorage, version: u64) -> Self {
        Self {
            index: HashMap::new(),
            storage,
            version,
        }
    }

    /// Add an active order
    pub fn add_order(&mut self, market_id: u32, order_id: u64) -> Result<(), StateError> {
        self.index.entry(market_id).or_default().insert(order_id);

        let flag_key = StateKey::ActiveOrder {
            market_id,
            order_id,
        };
        self.storage.set(flag_key, true)?;

        self.persist_order_list(market_id)
    }

    /// Remove an active order
    pub fn remove_order(&mut self, market_id: u32, order_id: u64) -> Result<(), StateError> {
        if let Some(orders) = self.index.get_mut(&market_id) {
            orders.remove(&order_id);
            if orders.is_empty() {
                self.index.remove(&market_id);
            }
        }

        let flag_key = StateKey::ActiveOrder {
            market_id,
            order_id,
        };
        self.storage.delete(flag_key)?;

        self.persist_or_delete_order_list(market_id)
    }

    /// Get all active orders for a market
    pub fn get_orders(&self, market_id: u32) -> Vec<u64> {
        self.index
            .get(&market_id)
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default()
    }

    /// Rebuild from state
    pub fn rebuild_from_state(&mut self, markets: &[u32]) -> Result<(), StateError> {
        self.index = markets
            .iter()
            .filter_map(|&market_id| {
                let list_key = StateKey::ActiveOrderList { market_id };
                self.storage
                    .get::<Vec<u64>>(&list_key, self.version)
                    .ok()
                    .flatten()
                    .map(|order_list| (market_id, order_list.into_iter().collect()))
            })
            .collect();
        Ok(())
    }

    /// Update version
    pub fn set_version(&mut self, version: u64) {
        self.version = version;
    }

    /// Persist order list to storage
    fn persist_order_list(&self, market_id: u32) -> Result<(), StateError> {
        let list_key = StateKey::ActiveOrderList { market_id };
        let order_list: Vec<u64> = self
            .index
            .get(&market_id)
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default();
        self.storage.set(list_key, order_list)
    }

    /// Persist or delete order list based on existence
    fn persist_or_delete_order_list(&self, market_id: u32) -> Result<(), StateError> {
        let list_key = StateKey::ActiveOrderList { market_id };
        match self.index.get(&market_id) {
            Some(orders) => {
                let order_list: Vec<u64> = orders.iter().copied().collect();
                self.storage.set(list_key, order_list)
            }
            None => self.storage.delete(list_key),
        }
    }
}
