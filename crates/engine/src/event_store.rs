use crate::{DomainEvent, Event, EventStoreKey};
use alloy_primitives::{Address, B256};
use std::collections::HashMap;
use std::sync::Arc;

/// Check if an event involves a specific address
fn event_involves_address(event: &Event, address: Address) -> bool {
    match event {
        Event::BalanceChanged { address: addr, .. } => *addr == address,
        Event::Transfer { from, to, .. } => *from == address || *to == address,
        Event::OrderPlaced { owner, .. } => *owner == address,
        Event::OrderCancelled { owner, .. } => *owner == address,
        Event::OrderFilled { maker, taker, .. } => *maker == address || *taker == address,
        Event::PositionOpened { trader, .. } => *trader == address,
        Event::PositionClosed { trader, .. } => *trader == address,
        Event::PositionModified { trader, .. } => *trader == address,
        Event::PositionLiquidated {
            trader, liquidator, ..
        } => *trader == address || liquidator.map(|l| l == address).unwrap_or(false),
        Event::FundingPaid { trader, .. } => *trader == address,
        Event::BridgeDeposit { user, .. } => *user == address,
        Event::BridgeWithdraw { user, .. } => *user == address,
        Event::AgentSet { account, agent, .. } => *account == address || *agent == address,
        Event::AgentRemoved { account, agent } => *account == address || *agent == address,
        _ => false,
    }
}

/// Event store trait for flexible persistence
///
/// Implementations can use:
/// - In-memory (Vec) - for testing
/// - RocksDB - for local persistence
/// - PostgreSQL/ClickHouse - for analytics
pub trait EventStore: Send + Sync {
    /// Append events to the store
    fn append(&mut self, events: Vec<DomainEvent>) -> Result<(), EventStoreError>;

    /// Get events by block height range
    fn get_by_block_range(
        &self,
        from_height: u64,
        to_height: u64,
    ) -> Result<Vec<DomainEvent>, EventStoreError>;

    /// Get events by transaction hash
    fn get_by_tx(&self, tx_hash: B256) -> Result<Vec<DomainEvent>, EventStoreError>;

    /// Get events for specific address (involves full scan)
    fn get_by_address(
        &self,
        address: Address,
        limit: usize,
    ) -> Result<Vec<DomainEvent>, EventStoreError>;

    /// Get total event count
    fn count(&self) -> Result<u64, EventStoreError>;
}

/// Event store errors
#[derive(Debug, thiserror::Error)]
pub enum EventStoreError {
    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Not found")]
    NotFound,

    #[error("Other error: {0}")]
    Other(String),

    #[error("RocksDB error: {0}")]
    RocksDbError(String),
}

/// In-memory event store (for testing and development)
#[derive(Debug, Clone, Default)]
pub struct InMemoryEventStore {
    events: Vec<DomainEvent>,
    by_tx: HashMap<B256, Vec<usize>>,   // tx_hash -> event indices
    by_block: HashMap<u64, Vec<usize>>, // block_height -> event indices
}

impl InMemoryEventStore {
    /// Clear all events (for testing)
    pub fn clear(&mut self) {
        self.events.clear();
        self.by_tx.clear();
        self.by_block.clear();
    }

    /// Get all events (for testing)
    pub fn all_events(&self) -> &[DomainEvent] {
        &self.events
    }
}

impl EventStore for InMemoryEventStore {
    fn append(&mut self, events: Vec<DomainEvent>) -> Result<(), EventStoreError> {
        for event in events {
            let idx = self.events.len();

            // Index by tx hash
            self.by_tx.entry(event.tx_hash).or_default().push(idx);

            // Index by block
            self.by_block
                .entry(event.block_height)
                .or_default()
                .push(idx);

            self.events.push(event);
        }

        Ok(())
    }

    fn get_by_block_range(
        &self,
        from_height: u64,
        to_height: u64,
    ) -> Result<Vec<DomainEvent>, EventStoreError> {
        let mut result = Vec::new();

        for height in from_height..=to_height {
            if let Some(indices) = self.by_block.get(&height) {
                for &idx in indices {
                    result.push(self.events[idx].clone());
                }
            }
        }

        Ok(result)
    }

    fn get_by_tx(&self, tx_hash: B256) -> Result<Vec<DomainEvent>, EventStoreError> {
        if let Some(indices) = self.by_tx.get(&tx_hash) {
            let events = indices
                .iter()
                .map(|&idx| self.events[idx].clone())
                .collect();
            Ok(events)
        } else {
            Ok(Vec::new())
        }
    }

    fn get_by_address(
        &self,
        address: Address,
        limit: usize,
    ) -> Result<Vec<DomainEvent>, EventStoreError> {
        let mut result = Vec::new();
        let mut count = 0;

        // Full scan (inefficient but acceptable for in-memory store)
        for event in self.events.iter().rev() {
            if count >= limit {
                break;
            }

            if event_involves_address(&event.event, address) {
                result.push(event.clone());
                count += 1;
            }
        }

        Ok(result)
    }

    fn count(&self) -> Result<u64, EventStoreError> {
        Ok(self.events.len() as u64)
    }
}

// ============================================================================
// RocksDB Event Store
// ============================================================================

/// RocksDB-backed event store for persistent event logging
///
/// This implementation provides:
/// - Durable event storage on disk
/// - Efficient indexing by block height and transaction hash
/// - Batched writes for performance
///
/// # Key Layout (using EventStoreKey enum with borsh serialization)
/// - `EventStoreKey::Event { block_height, tx_hash, event_index }` -> DomainEvent (Borsh serialized)
/// - `EventStoreKey::BlockIndex { block_height, tx_hash, event_index }` -> empty (index)
/// - `EventStoreKey::TxIndex { tx_hash, event_index }` -> empty (index)
/// - `EventStoreKey::MetaCount` -> u64 (total event count)
///
pub struct RocksDbEventStore {
    db: Arc<rocksdb::DB>,
    write_buffer: Vec<DomainEvent>,
    buffer_limit: usize,
}

impl RocksDbEventStore {
    /// Create a new RocksDB event store
    ///
    /// # Arguments
    /// - `path`: Directory path for RocksDB storage
    /// - `buffer_limit`: Number of events to buffer before auto-flushing (default: 1000)
    pub fn new<P: AsRef<std::path::Path>>(
        path: P,
        buffer_limit: usize,
    ) -> Result<Self, EventStoreError> {
        let mut opts = rocksdb::Options::default();
        opts.create_if_missing(true);
        opts.set_compression_type(rocksdb::DBCompressionType::Lz4);

        let db = rocksdb::DB::open(&opts, path)
            .map_err(|e| EventStoreError::RocksDbError(e.to_string()))?;

        Ok(Self {
            db: Arc::new(db),
            write_buffer: Vec::with_capacity(buffer_limit),
            buffer_limit,
        })
    }

    /// Flush buffered events to disk
    pub fn flush(&mut self) -> Result<(), EventStoreError> {
        if self.write_buffer.is_empty() {
            return Ok(());
        }

        let buffer_len = self.write_buffer.len();
        let mut batch = rocksdb::WriteBatch::default();

        for event in self.write_buffer.drain(..) {
            // Serialize event
            let value = borsh::to_vec(&event)
                .map_err(|e| EventStoreError::SerializationError(e.to_string()))?;

            // Primary key: Event { block_height, tx_hash, event_index }
            let event_key = EventStoreKey::Event {
                block_height: event.block_height,
                tx_hash: event.tx_hash,
                event_index: event.event_index,
            }
            .to_bytes();
            batch.put(event_key, value);

            // Block index: BlockIndex { block_height, tx_hash, event_index }
            let block_idx_key = EventStoreKey::BlockIndex {
                block_height: event.block_height,
                tx_hash: event.tx_hash,
                event_index: event.event_index,
            }
            .to_bytes();
            batch.put(block_idx_key, b"");

            // TX index: TxIndex { tx_hash, event_index }
            let tx_idx_key = EventStoreKey::TxIndex {
                tx_hash: event.tx_hash,
                event_index: event.event_index,
            }
            .to_bytes();
            batch.put(tx_idx_key, b"");
        }

        // Increment event count
        let count = self.count()? + buffer_len as u64;
        batch.put(EventStoreKey::MetaCount.to_bytes(), count.to_le_bytes());

        self.db
            .write(batch)
            .map_err(|e| EventStoreError::RocksDbError(e.to_string()))?;

        Ok(())
    }

    /// Get database handle for advanced operations
    pub fn db(&self) -> &Arc<rocksdb::DB> {
        &self.db
    }
}

impl EventStore for RocksDbEventStore {
    fn append(&mut self, events: Vec<DomainEvent>) -> Result<(), EventStoreError> {
        self.write_buffer.extend(events);

        // Auto-flush if buffer is full
        if self.write_buffer.len() >= self.buffer_limit {
            self.flush()?;
        }

        Ok(())
    }

    fn get_by_block_range(
        &self,
        from_height: u64,
        to_height: u64,
    ) -> Result<Vec<DomainEvent>, EventStoreError> {
        let mut result = Vec::new();

        for height in from_height..=to_height {
            let prefix = EventStoreKey::prefix_for_block(height);
            let iter = self.db.prefix_iterator(&prefix);

            for item in iter {
                let (key, _) = item.map_err(|e| EventStoreError::RocksDbError(e.to_string()))?;

                // Try to deserialize the key
                let storage_key = match EventStoreKey::from_bytes(&key) {
                    Ok(k) => k,
                    Err(_) => break, // Invalid key, stop iteration
                };

                // Verify it's a BlockIndex key with the correct block height
                match storage_key {
                    EventStoreKey::BlockIndex {
                        block_height,
                        tx_hash,
                        event_index,
                    } if block_height == height => {
                        // Construct event key
                        let event_key = EventStoreKey::Event {
                            block_height,
                            tx_hash,
                            event_index,
                        }
                        .to_bytes();

                        // Get event data
                        if let Some(value) = self
                            .db
                            .get(event_key)
                            .map_err(|e| EventStoreError::RocksDbError(e.to_string()))?
                        {
                            let event: DomainEvent = borsh::from_slice(value.as_ref())
                                .map_err(|e| EventStoreError::SerializationError(e.to_string()))?;
                            result.push(event);
                        }
                    }
                    EventStoreKey::BlockIndex { block_height, .. } if block_height != height => {
                        // Moved past our block height
                        break;
                    }
                    _ => break, // Wrong key type, stop iteration
                }
            }
        }

        Ok(result)
    }

    fn get_by_tx(&self, tx_hash: B256) -> Result<Vec<DomainEvent>, EventStoreError> {
        let mut result = Vec::new();
        let prefix = EventStoreKey::prefix_for_tx(tx_hash);
        let iter = self.db.prefix_iterator(&prefix);

        for item in iter {
            let (key, _) = item.map_err(|e| EventStoreError::RocksDbError(e.to_string()))?;

            // Try to deserialize the key
            let storage_key = match EventStoreKey::from_bytes(&key) {
                Ok(k) => k,
                Err(_) => break, // Invalid key, stop iteration
            };

            // Verify it's a TxIndex key with the correct tx_hash
            match storage_key {
                EventStoreKey::TxIndex {
                    tx_hash: key_tx_hash,
                    event_index,
                } if key_tx_hash == tx_hash => {
                    // We need to find the event, but we don't know the block_height
                    // Scan events with this tx_hash
                    let event_prefix = EventStoreKey::prefix_for_events();
                    let event_iter = self.db.prefix_iterator(&event_prefix);

                    for event_item in event_iter {
                        let (event_key_bytes, event_value) =
                            event_item.map_err(|e| EventStoreError::RocksDbError(e.to_string()))?;

                        // Check if this is the event we're looking for
                        if let Ok(EventStoreKey::Event {
                            tx_hash: event_tx_hash,
                            event_index: event_idx,
                            ..
                        }) = EventStoreKey::from_bytes(&event_key_bytes)
                            && event_tx_hash == tx_hash
                            && event_idx == event_index
                        {
                            let event: DomainEvent = borsh::from_slice(event_value.as_ref())
                                .map_err(|e| EventStoreError::SerializationError(e.to_string()))?;
                            result.push(event);
                            break; // Found the event, move to next index
                        }
                    }
                }
                EventStoreKey::TxIndex {
                    tx_hash: key_tx_hash,
                    ..
                } if key_tx_hash != tx_hash => {
                    // Moved past our tx_hash
                    break;
                }
                _ => break, // Wrong key type, stop iteration
            }
        }

        // Sort by event_index
        result.sort_by_key(|e| e.event_index);
        Ok(result)
    }

    fn get_by_address(
        &self,
        address: Address,
        limit: usize,
    ) -> Result<Vec<DomainEvent>, EventStoreError> {
        // Full scan - inefficient but necessary without address indexing
        let mut result = Vec::new();

        let prefix = EventStoreKey::prefix_for_events();
        let iter = self.db.prefix_iterator(&prefix);

        for item in iter {
            let (key, value) = item.map_err(|e| EventStoreError::RocksDbError(e.to_string()))?;

            // Verify the key is an Event key
            if let Ok(EventStoreKey::Event { .. }) = EventStoreKey::from_bytes(&key) {
                let event: DomainEvent = borsh::from_slice(value.as_ref())
                    .map_err(|e| EventStoreError::SerializationError(e.to_string()))?;

                if event_involves_address(&event.event, address) {
                    result.push(event);
                }
            } else {
                break; // We've moved past the event keys
            }
        }

        // Sort by block height descending, then take limit
        result.sort_by(|a, b| b.block_height.cmp(&a.block_height));
        result.truncate(limit);

        Ok(result)
    }

    fn count(&self) -> Result<u64, EventStoreError> {
        match self
            .db
            .get(EventStoreKey::MetaCount.to_bytes())
            .map_err(|e| EventStoreError::RocksDbError(e.to_string()))?
        {
            Some(bytes) => {
                let count_bytes: [u8; 8] = bytes
                    .try_into()
                    .map_err(|_| EventStoreError::Other("Invalid count format".to_string()))?;
                Ok(u64::from_le_bytes(count_bytes))
            }
            None => Ok(0),
        }
    }
}

impl Drop for RocksDbEventStore {
    fn drop(&mut self) {
        // Flush remaining events on drop
        let _ = self.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BalanceChangeReason, Event};

    #[test]
    fn test_in_memory_event_store() {
        let mut store = InMemoryEventStore::default();

        let event = DomainEvent::new(
            100,
            B256::ZERO,
            0,
            1234567890,
            Event::BalanceChanged {
                address: Address::ZERO,
                asset_id: 0,
                old_balance: 1000,
                new_balance: 2000,
                reason: BalanceChangeReason::Deposit,
            },
        );

        store.append(vec![event.clone()]).unwrap();

        assert_eq!(store.count().unwrap(), 1);

        let events = store.get_by_block_range(100, 100).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].block_height, 100);

        let tx_events = store.get_by_tx(B256::ZERO).unwrap();
        assert_eq!(tx_events.len(), 1);
    }

    #[test]
    fn test_event_filtering_by_address() {
        let mut store = InMemoryEventStore::default();
        let addr1 = Address::with_last_byte(1);
        let addr2 = Address::with_last_byte(2);

        // Event for addr1
        store
            .append(vec![DomainEvent::new(
                100,
                B256::ZERO,
                0,
                1234567890,
                Event::BalanceChanged {
                    address: addr1,
                    asset_id: 0,
                    old_balance: 0,
                    new_balance: 1000,
                    reason: BalanceChangeReason::Deposit,
                },
            )])
            .unwrap();

        // Event for addr2
        store
            .append(vec![DomainEvent::new(
                101,
                B256::ZERO,
                0,
                1234567891,
                Event::BalanceChanged {
                    address: addr2,
                    asset_id: 0,
                    old_balance: 0,
                    new_balance: 2000,
                    reason: BalanceChangeReason::Deposit,
                },
            )])
            .unwrap();

        let addr1_events = store.get_by_address(addr1, 10).unwrap();
        assert_eq!(addr1_events.len(), 1);

        let addr2_events = store.get_by_address(addr2, 10).unwrap();
        assert_eq!(addr2_events.len(), 1);
    }
}
