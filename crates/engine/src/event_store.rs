use crate::{DomainEvent, Event};
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
/// # Key Layout
/// - `event:{block_height}:{tx_hash}:{event_index}` -> DomainEvent (Borsh serialized)
/// - `block_idx:{block_height}:{tx_hash}:{event_index}` -> empty (index)
/// - `tx_idx:{tx_hash}:{event_index}` -> empty (index)
/// - `meta:count` -> u64 (total event count)
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

        let mut batch = rocksdb::WriteBatch::default();

        for event in self.write_buffer.drain(..) {
            // Serialize event
            let value = borsh::to_vec(&event)
                .map_err(|e| EventStoreError::SerializationError(e.to_string()))?;

            // Primary key: event:{block}:{tx}:{idx}
            let event_key = format!(
                "event:{}:{}:{}",
                event.block_height,
                hex::encode(event.tx_hash),
                event.event_index
            );
            batch.put(event_key.as_bytes(), &value);

            // Block index: block_idx:{block}:{tx}:{idx}
            let block_idx_key = format!(
                "block_idx:{}:{}:{}",
                event.block_height,
                hex::encode(event.tx_hash),
                event.event_index
            );
            batch.put(block_idx_key.as_bytes(), b"");

            // TX index: tx_idx:{tx}:{idx}
            let tx_idx_key = format!(
                "tx_idx:{}:{}",
                hex::encode(event.tx_hash),
                event.event_index
            );
            batch.put(tx_idx_key.as_bytes(), b"");
        }

        // Increment event count
        let count = self.count()? + self.write_buffer.len() as u64;
        batch.put(b"meta:count", count.to_le_bytes());

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
            let prefix = format!("block_idx:{}:", height);
            let iter = self.db.prefix_iterator(prefix.as_bytes());

            for item in iter {
                let (key, _) = item.map_err(|e| EventStoreError::RocksDbError(e.to_string()))?;

                // Parse key: block_idx:{block}:{tx}:{idx}
                let key_str = String::from_utf8_lossy(&key);

                // Verify it actually matches our prefix (RocksDB prefix_iterator can overmatch)
                if !key_str.starts_with(&prefix) {
                    break;
                }

                let parts: Vec<&str> = key_str.split(':').collect();
                if parts.len() != 4 {
                    continue;
                }

                // Double-check the parsed height matches (prevents "1" matching "10", "100", etc.)
                if let Ok(parsed_height) = parts[1].parse::<u64>() {
                    if parsed_height != height {
                        continue;
                    }
                } else {
                    continue;
                }

                // Construct event key
                let event_key = format!("event:{}:{}:{}", parts[1], parts[2], parts[3]);

                // Get event data
                if let Some(value) = self
                    .db
                    .get(event_key.as_bytes())
                    .map_err(|e| EventStoreError::RocksDbError(e.to_string()))?
                {
                    let event: DomainEvent = borsh::from_slice(value.as_ref())
                        .map_err(|e| EventStoreError::SerializationError(e.to_string()))?;
                    result.push(event);
                }
            }
        }

        Ok(result)
    }

    fn get_by_tx(&self, tx_hash: B256) -> Result<Vec<DomainEvent>, EventStoreError> {
        let mut result = Vec::new();
        let tx_hex = hex::encode(tx_hash);
        let prefix = format!("tx_idx:{}:", tx_hex);
        let iter = self.db.prefix_iterator(prefix.as_bytes());

        for item in iter {
            let (key, _) = item.map_err(|e| EventStoreError::RocksDbError(e.to_string()))?;

            let key_str = String::from_utf8_lossy(&key);
            if !key_str.starts_with(&prefix) {
                break; // Prefix iteration done
            }

            // Parse key: tx_idx:{tx}:{idx}
            let parts: Vec<&str> = key_str.split(':').collect();
            if parts.len() != 3 {
                continue;
            }

            // Verify the tx hash matches exactly
            if parts[1] != tx_hex {
                continue;
            }

            // Now find the event by scanning with pattern
            // We need to find event:{block}:{tx_hex}:{idx}
            let event_idx = parts[2];

            // Scan events with this tx_hash
            let event_prefix = "event:".to_string();
            let event_iter = self.db.prefix_iterator(event_prefix.as_bytes());

            for event_item in event_iter {
                let (event_key, event_value) =
                    event_item.map_err(|e| EventStoreError::RocksDbError(e.to_string()))?;

                let event_key_str = String::from_utf8_lossy(&event_key);
                let event_parts: Vec<&str> = event_key_str.split(':').collect();

                // event:{block}:{tx}:{idx}
                if event_parts.len() == 4 && event_parts[2] == tx_hex && event_parts[3] == event_idx
                {
                    let event: DomainEvent = borsh::from_slice(event_value.as_ref())
                        .map_err(|e| EventStoreError::SerializationError(e.to_string()))?;
                    result.push(event);
                    break; // Found the event, move to next index
                }
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

        let prefix = b"event:";
        let iter = self.db.prefix_iterator(prefix);

        for item in iter {
            let (key, value) = item.map_err(|e| EventStoreError::RocksDbError(e.to_string()))?;

            // Verify the key actually starts with "event:" to avoid other prefixes
            let key_str = String::from_utf8_lossy(&key);
            if !key_str.starts_with("event:") {
                break; // We've moved past the event: prefix
            }

            let event: DomainEvent = borsh::from_slice(value.as_ref())
                .map_err(|e| EventStoreError::SerializationError(e.to_string()))?;

            if event_involves_address(&event.event, address) {
                result.push(event);
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
            .get(b"meta:count")
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
