use alloy_primitives::B256;
use pranklin_engine::{EventStore, RocksDbEventStore};
use pranklin_types::{Address, DomainEvent, Event};
use tempfile::TempDir;

/// Helper to create a test event
fn create_test_event(block_height: u64, tx_hash: B256, event_index: u32) -> DomainEvent {
    DomainEvent {
        block_height,
        tx_hash,
        event_index,
        timestamp: 1234567890 + block_height,
        event: Event::BalanceChanged {
            address: Address::repeat_byte((block_height + event_index as u64) as u8),
            asset_id: 0,
            old_balance: 0,
            new_balance: 1000,
            reason: pranklin_types::BalanceChangeReason::Deposit,
        },
    }
}

/// Helper to create a random-ish tx hash
fn create_tx_hash(seed: u8) -> B256 {
    B256::repeat_byte(seed)
}

#[test]
fn test_rocksdb_event_store_basic() {
    let temp_dir = TempDir::new().unwrap();
    let mut store = RocksDbEventStore::new(temp_dir.path(), 10).unwrap();

    // Create test events
    let tx_hash1 = create_tx_hash(1);
    let events = vec![
        create_test_event(1, tx_hash1, 0),
        create_test_event(1, tx_hash1, 1),
        create_test_event(1, tx_hash1, 2),
    ];

    // Append events
    store.append(events.clone()).unwrap();
    store.flush().unwrap();

    // Retrieve by tx hash
    let retrieved = store.get_by_tx(tx_hash1).unwrap();
    assert_eq!(retrieved.len(), 3);
    assert_eq!(retrieved[0].event_index, 0);
    assert_eq!(retrieved[1].event_index, 1);
    assert_eq!(retrieved[2].event_index, 2);
}

#[test]
fn test_rocksdb_event_store_block_range() {
    let temp_dir = TempDir::new().unwrap();
    let mut store = RocksDbEventStore::new(temp_dir.path(), 10).unwrap();

    // Create events across multiple blocks
    let mut events = Vec::new();
    for block in 1..=5 {
        for event_idx in 0..3 {
            events.push(create_test_event(
                block,
                create_tx_hash(block as u8),
                event_idx,
            ));
        }
    }

    store.append(events).unwrap();
    store.flush().unwrap();

    // Query block range [2, 4]
    let retrieved = store.get_by_block_range(2, 4).unwrap();
    assert_eq!(retrieved.len(), 9); // 3 blocks * 3 events each

    // Verify all events are in the correct range
    for event in retrieved {
        assert!(event.block_height >= 2 && event.block_height <= 4);
    }
}

#[test]
fn test_rocksdb_event_store_batching() {
    let temp_dir = TempDir::new().unwrap();
    let buffer_limit = 5;
    let mut store = RocksDbEventStore::new(temp_dir.path(), buffer_limit).unwrap();

    // Add events below buffer limit (should not auto-flush)
    let events1 = vec![
        create_test_event(1, create_tx_hash(10), 0),
        create_test_event(1, create_tx_hash(11), 1),
    ];
    store.append(events1).unwrap();

    // Add more events to exceed buffer limit (should auto-flush)
    let events2 = vec![
        create_test_event(2, create_tx_hash(20), 0),
        create_test_event(2, create_tx_hash(21), 1),
        create_test_event(2, create_tx_hash(22), 2),
    ];
    store.append(events2).unwrap();

    // Verify all events are persisted
    let retrieved = store.get_by_block_range(1, 2).unwrap();
    assert_eq!(retrieved.len(), 5);
}

#[test]
fn test_rocksdb_event_store_filtering_by_address() {
    let temp_dir = TempDir::new().unwrap();
    let mut store = RocksDbEventStore::new(temp_dir.path(), 10).unwrap();

    let address1 = Address::repeat_byte(1);
    let address2 = Address::repeat_byte(2);

    let events = vec![
        DomainEvent {
            block_height: 1,
            tx_hash: create_tx_hash(30),
            event_index: 0,
            timestamp: 1234567890,
            event: Event::BalanceChanged {
                address: address1,
                asset_id: 0,
                old_balance: 0,
                new_balance: 1000,
                reason: pranklin_types::BalanceChangeReason::Deposit,
            },
        },
        DomainEvent {
            block_height: 1,
            tx_hash: create_tx_hash(31),
            event_index: 1,
            timestamp: 1234567891,
            event: Event::BalanceChanged {
                address: address2,
                asset_id: 0,
                old_balance: 0,
                new_balance: 2000,
                reason: pranklin_types::BalanceChangeReason::Deposit,
            },
        },
        DomainEvent {
            block_height: 2,
            tx_hash: create_tx_hash(32),
            event_index: 0,
            timestamp: 1234567892,
            event: Event::OrderPlaced {
                market_id: 1,
                order_id: 1,
                owner: address1,
                price: 50000,
                size: 100,
                is_buy: true,
                order_type: pranklin_types::OrderType::Limit,
            },
        },
    ];

    store.append(events).unwrap();
    store.flush().unwrap();

    // Filter by address1
    let filtered = store.get_by_address(address1, 10).unwrap();
    assert_eq!(filtered.len(), 2);

    // Filter by address2
    let filtered = store.get_by_address(address2, 10).unwrap();
    assert_eq!(filtered.len(), 1);
}

#[test]
fn test_rocksdb_event_store_persistence() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().to_path_buf();

    let tx_hash = create_tx_hash(40);
    let events = vec![create_test_event(1, tx_hash, 0)];

    // Write events and drop the store
    {
        let mut store = RocksDbEventStore::new(&path, 10).unwrap();
        store.append(events).unwrap();
        store.flush().unwrap();
    }

    // Reopen the store and verify data persists
    {
        let store = RocksDbEventStore::new(&path, 10).unwrap();
        let retrieved = store.get_by_tx(tx_hash).unwrap();
        assert_eq!(retrieved.len(), 1);
        assert_eq!(retrieved[0].block_height, 1);
    }
}

#[test]
fn test_rocksdb_event_store_auto_flush_on_drop() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().to_path_buf();
    let tx_hash = create_tx_hash(50);

    // Write events without manual flush
    {
        let mut store = RocksDbEventStore::new(&path, 100).unwrap();
        let events = vec![create_test_event(1, tx_hash, 0)];
        store.append(events).unwrap();
        // Drop without calling flush() - auto-flush should happen
    }

    // Verify data was auto-flushed
    {
        let store = RocksDbEventStore::new(&path, 10).unwrap();
        let retrieved = store.get_by_tx(tx_hash).unwrap();
        assert_eq!(retrieved.len(), 1);
    }
}

#[test]
fn test_rocksdb_event_store_large_batch() {
    let temp_dir = TempDir::new().unwrap();
    let mut store = RocksDbEventStore::new(temp_dir.path(), 1000).unwrap();

    // Create a large batch of events
    let mut events = Vec::new();
    for block in 1..=100 {
        for event_idx in 0..10 {
            events.push(create_test_event(
                block,
                create_tx_hash((block % 256) as u8),
                event_idx,
            ));
        }
    }

    store.append(events).unwrap();
    store.flush().unwrap();

    // Verify all events are stored
    let retrieved = store.get_by_block_range(1, 100).unwrap();
    assert_eq!(retrieved.len(), 1000);
}

#[test]
fn test_rocksdb_event_store_ordering() {
    let temp_dir = TempDir::new().unwrap();
    let mut store = RocksDbEventStore::new(temp_dir.path(), 10).unwrap();

    let tx_hash = create_tx_hash(60);
    let events = vec![
        create_test_event(1, tx_hash, 2),
        create_test_event(1, tx_hash, 0),
        create_test_event(1, tx_hash, 1),
    ];

    store.append(events).unwrap();
    store.flush().unwrap();

    // Verify events are returned in correct order (by event_index)
    let retrieved = store.get_by_tx(tx_hash).unwrap();
    assert_eq!(retrieved.len(), 3);
    assert_eq!(retrieved[0].event_index, 0);
    assert_eq!(retrieved[1].event_index, 1);
    assert_eq!(retrieved[2].event_index, 2);
}
