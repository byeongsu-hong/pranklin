/// Performance benchmarks for state layer (JMT + RocksDB)
use alloy_primitives::Address;
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use pranklin_state::{Market, Order, OrderStatus, Position, PruningConfig, StateManager};

/// Create benchmark market
fn create_market() -> Market {
    Market {
        id: 0,
        symbol: "BTC-PERP".to_string(),
        base_asset_id: 1,
        quote_asset_id: 0,
        tick_size: 1000,
        price_decimals: 2,
        size_decimals: 6,
        min_order_size: 1,
        max_order_size: 1_000_000_000,
        max_leverage: 20,
        initial_margin_bps: 1000,
        maintenance_margin_bps: 500,
        liquidation_fee_bps: 100,
        funding_interval: 3600,
        max_funding_rate_bps: 100,
    }
}

/// Benchmark: Set balance (write to pending buffer)
fn bench_set_balance(c: &mut Criterion) {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let mut state = StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();

    let address = Address::from([1u8; 20]);

    c.bench_function("jmt_set_balance", |b| {
        b.iter(|| {
            state
                .set_balance(black_box(address), black_box(0), black_box(1_000_000u128))
                .unwrap();
        });
    });
}

/// Benchmark: Get balance (read from JMT)
fn bench_get_balance(c: &mut Criterion) {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let mut state = StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();

    let address = Address::from([1u8; 20]);
    state.set_balance(address, 0, 1_000_000u128).unwrap();
    state.begin_block(1);
    state.commit().unwrap();

    c.bench_function("jmt_get_balance", |b| {
        b.iter(|| {
            let _ = black_box(state.get_balance(black_box(address), black_box(0)).unwrap());
        });
    });
}

/// Benchmark: Batch writes with commit
fn bench_batch_write_commit(c: &mut Criterion) {
    let mut group = c.benchmark_group("jmt_batch_write_commit");

    for batch_size in [10, 100, 1000].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(batch_size),
            batch_size,
            |b, &batch_size| {
                b.iter_batched(
                    || {
                        let temp_dir = tempfile::TempDir::new().unwrap();
                        let state =
                            StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();
                        (state, temp_dir)
                    },
                    |(mut state, _temp_dir)| {
                        // Write batch_size balances
                        for i in 0..batch_size {
                            let address = Address::from([i as u8; 20]);
                            state.set_balance(address, 0, i as u128 * 1000).unwrap();
                        }

                        // Commit
                        state.begin_block(1);
                        let _ = black_box(state.commit().unwrap());
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

/// Benchmark: Complex state update (positions + orders + balances)
fn bench_complex_state_update(c: &mut Criterion) {
    let mut group = c.benchmark_group("jmt_complex_update");

    for num_updates in [10, 100, 500].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_updates),
            num_updates,
            |b, &num_updates| {
                b.iter_batched(
                    || {
                        let temp_dir = tempfile::TempDir::new().unwrap();
                        let mut state =
                            StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();

                        // Setup: create market
                        let market = create_market();
                        state.set_market(0, market).unwrap();
                        state.begin_block(1);
                        state.commit().unwrap();

                        (state, temp_dir)
                    },
                    |(mut state, _temp_dir)| {
                        state.begin_block(2);

                        // Simulate complex trading activity
                        for i in 0..num_updates {
                            let trader = Address::from([(i % 255) as u8; 20]);

                            // Update balance
                            state.set_balance(trader, 0, i as u128 * 100_000).unwrap();

                            // Update position
                            let position = Position {
                                size: (i as u64 + 1) * 10,
                                entry_price: 50_000_000_000,
                                is_long: i % 2 == 0,
                                margin: 1_000_000_000,
                                funding_index: 0,
                            };
                            state.set_position(trader, 0, position).unwrap();

                            // Create order
                            let order = Order {
                                id: i as u64,
                                market_id: 0,
                                owner: trader,
                                is_buy: i % 2 == 0,
                                price: 50_000 + (i as u64 * 100),
                                original_size: 100,
                                remaining_size: 100,
                                status: OrderStatus::Active,
                                created_at: 2,
                                reduce_only: false,
                                post_only: false,
                            };
                            state.set_order(i as u64, order).unwrap();
                            state.add_active_order(0, i as u64).unwrap();
                        }

                        // Commit everything
                        let _ = black_box(state.commit().unwrap());
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

/// Benchmark: Read all positions in market (range scan)
fn bench_read_all_positions(c: &mut Criterion) {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let mut state = StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();

    // Setup: Create market and positions
    let market = create_market();
    state.set_market(0, market).unwrap();
    state.begin_block(1);
    state.commit().unwrap();

    state.begin_block(2);
    for i in 0..100 {
        let trader = Address::from([i; 20]);
        let position = Position {
            size: 100,
            entry_price: 50_000_000_000,
            is_long: i % 2 == 0,
            margin: 1_000_000_000,
            funding_index: 0,
        };
        state.set_position(trader, 0, position).unwrap();
    }
    state.commit().unwrap();

    // Rebuild position index
    state.rebuild_position_index().unwrap();

    c.bench_function("jmt_read_all_positions", |b| {
        b.iter(|| {
            let positions = state.get_all_positions_in_market(0).unwrap();
            black_box(positions);
        });
    });
}

/// Benchmark: Snapshot creation
fn bench_snapshot_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("jmt_snapshot_creation");
    group.sample_size(10); // Fewer samples for expensive operation

    for num_keys in [100, 1000, 5000].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_keys),
            num_keys,
            |b, &num_keys| {
                b.iter_batched(
                    || {
                        let temp_dir = tempfile::TempDir::new().unwrap();
                        let config = PruningConfig {
                            snapshot_interval: 10,
                            prune_before: 100000,
                            enabled: true,
                        };
                        let mut state = StateManager::new(temp_dir.path(), config).unwrap();

                        // Write many keys
                        state.begin_block(1);
                        for i in 0..num_keys {
                            let address = Address::from([(i % 255) as u8; 20]);
                            state.set_balance(address, 0, i as u128 * 1000).unwrap();
                        }
                        state.commit().unwrap();

                        (state, temp_dir)
                    },
                    |(mut state, _temp_dir)| {
                        state.begin_block(10); // Snapshot interval
                        state.set_balance(Address::ZERO, 0, 999).unwrap();
                        let _ = black_box(state.commit().unwrap());
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

/// Benchmark: State root calculation
fn bench_state_root(c: &mut Criterion) {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let mut state = StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();

    // Setup: Write some data
    state.begin_block(1);
    for i in 0..100 {
        let address = Address::from([i; 20]);
        state.set_balance(address, 0, i as u128 * 1000).unwrap();
    }
    state.commit().unwrap();

    c.bench_function("jmt_state_root", |b| {
        b.iter(|| {
            let root = state.state_root();
            black_box(root);
        });
    });
}

/// Benchmark: Version recovery (restart simulation)
fn bench_version_recovery(c: &mut Criterion) {
    let temp_dir = tempfile::TempDir::new().unwrap();

    // Phase 1: Write data
    {
        let mut state = StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();
        state.begin_block(1);
        for i in 0..100 {
            let address = Address::from([i; 20]);
            state.set_balance(address, 0, i as u128 * 1000).unwrap();
        }
        state.commit().unwrap();
    }

    c.bench_function("jmt_version_recovery", |b| {
        b.iter(|| {
            // Phase 2: Recover (simulates restart)
            let recovered_state =
                StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();
            let version = recovered_state.version();
            black_box(version);
        });
    });
}

/// Benchmark: Concurrent reads (simulate multiple queries)
fn bench_concurrent_reads(c: &mut Criterion) {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let mut state = StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();

    // Setup: Write data
    state.begin_block(1);
    for i in 0..1000 {
        let address = Address::from([i as u8; 20]);
        state.set_balance(address, 0, i as u128 * 1000).unwrap();
    }
    state.commit().unwrap();

    c.bench_function("jmt_concurrent_reads_100", |b| {
        b.iter(|| {
            // Simulate 100 concurrent balance queries
            for i in 0..100 {
                let address = Address::from([i; 20]);
                let _ = black_box(state.get_balance(address, 0).unwrap());
            }
        });
    });
}

/// Benchmark: Database size growth
fn bench_db_size(c: &mut Criterion) {
    c.bench_function("jmt_db_size_query", |b| {
        b.iter_batched(
            || {
                let temp_dir = tempfile::TempDir::new().unwrap();
                let mut state =
                    StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();

                // Write some data
                state.begin_block(1);
                for i in 0..100 {
                    let address = Address::from([i; 20]);
                    state.set_balance(address, 0, i as u128 * 1000).unwrap();
                }
                state.commit().unwrap();

                (state, temp_dir)
            },
            |(state, _temp_dir)| {
                let size = state.get_db_size().unwrap();
                black_box(size);
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

criterion_group!(
    benches,
    bench_set_balance,
    bench_get_balance,
    bench_batch_write_commit,
    bench_complex_state_update,
    bench_read_all_positions,
    bench_snapshot_creation,
    bench_state_root,
    bench_version_recovery,
    bench_concurrent_reads,
    bench_db_size,
);
criterion_main!(benches);
