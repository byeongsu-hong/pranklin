/// End-to-end TPS (Transactions Per Second) benchmarks
///
/// This measures the full transaction execution pipeline:
/// - Transaction decoding
/// - Signature verification
/// - Nonce checking
/// - Order placement/matching
/// - Position updates
/// - State updates
/// - State commitment to JMT/RocksDB
use alloy_primitives::Address;
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;
use std::time::Duration;
use pranklin_auth::AuthService;
use pranklin_engine::Engine;
use pranklin_state::{Market, PruningConfig, StateManager};
use pranklin_tx::*;

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

/// Setup engine with multiple traders
fn setup_engine_with_traders(num_traders: usize) -> (Engine, AuthService, tempfile::TempDir) {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let state = StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();
    let mut engine = Engine::new(state);
    let auth = AuthService::new();

    // Create market
    let market = create_market();
    engine.state_mut().set_market(0, market).unwrap();

    // Fund traders
    for i in 0..num_traders {
        let trader = Address::from([(i % 255) as u8; 20]);
        engine
            .state_mut()
            .set_balance(trader, 0, u128::MAX)
            .unwrap();
    }

    // Commit initial state
    engine.state_mut().begin_block(1);
    engine.state_mut().commit().unwrap();

    (engine, auth, temp_dir)
}

/// Benchmark: Place order transactions (no matching)
fn bench_tps_place_orders_no_match(c: &mut Criterion) {
    let mut group = c.benchmark_group("tps_place_orders_no_match");
    group.measurement_time(Duration::from_secs(10));

    for num_txs in [100, 1000, 5000].iter() {
        group.throughput(Throughput::Elements(*num_txs as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(num_txs),
            num_txs,
            |b, &num_txs| {
                b.iter_batched(
                    || {
                        let (engine, auth, temp_dir) = setup_engine_with_traders(num_txs);

                        // Prepare transactions
                        let mut txs = Vec::new();
                        for i in 0..num_txs {
                            let trader = Address::from([(i % 255) as u8; 20]);
                            let tx = Transaction::new(
                                0, // All have nonce 0 initially
                                trader,
                                TxPayload::PlaceOrder(PlaceOrderTx {
                                    market_id: 0,
                                    is_buy: i % 2 == 0,
                                    order_type: OrderType::Limit,
                                    price: 50_000 + ((i * 1000) as u64),
                                    size: 10,
                                    time_in_force: TimeInForce::GTC,
                                    reduce_only: false,
                                    post_only: false,
                                }),
                            );
                            txs.push(tx);
                        }

                        (engine, auth, txs, temp_dir)
                    },
                    |(mut engine, auth, txs, _temp_dir)| {
                        // Begin block
                        engine.state_mut().begin_block(2);

                        // Execute all transactions
                        for tx in &txs {
                            if let TxPayload::PlaceOrder(ref order) = tx.payload {
                                // Verify signature
                                let _ = auth.verify_transaction(tx);

                                // Check nonce
                                let current_nonce = engine.state().get_nonce(tx.from).unwrap();
                                if tx.nonce == current_nonce {
                                    // Execute
                                    let _ = engine.process_place_order(tx, order);

                                    // Increment nonce
                                    let _ = engine.state_mut().increment_nonce(tx.from);
                                }
                            }
                        }

                        // Commit block
                        let _ = black_box(engine.state_mut().commit().unwrap());
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

/// Benchmark: Place order transactions with matching
fn bench_tps_place_orders_with_match(c: &mut Criterion) {
    let mut group = c.benchmark_group("tps_place_orders_with_match");
    group.measurement_time(Duration::from_secs(10));

    for num_txs in [100, 500, 1000].iter() {
        group.throughput(Throughput::Elements(*num_txs as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(num_txs),
            num_txs,
            |b, &num_txs| {
                b.iter_batched(
                    || {
                        let (mut engine, auth, temp_dir) = setup_engine_with_traders(num_txs * 2);

                        // Phase 1: Place maker orders (buy side)
                        engine.state_mut().begin_block(2);
                        for i in 0..(num_txs / 2) {
                            let trader = Address::from([(i % 255) as u8; 20]);
                            let tx = Transaction::new(
                                0,
                                trader,
                                TxPayload::PlaceOrder(PlaceOrderTx {
                                    market_id: 0,
                                    is_buy: true,
                                    order_type: OrderType::Limit,
                                    price: 50_000,
                                    size: 10,
                                    time_in_force: TimeInForce::GTC,
                                    reduce_only: false,
                                    post_only: false,
                                }),
                            );
                            if let TxPayload::PlaceOrder(ref order) = tx.payload {
                                let _ = engine.process_place_order(&tx, order);
                                let _ = engine.state_mut().increment_nonce(tx.from);
                            }
                        }
                        engine.state_mut().commit().unwrap();

                        // Phase 2: Prepare taker orders (sell side)
                        let mut txs = Vec::new();
                        for i in 0..(num_txs / 2) {
                            let trader = Address::from([((i + num_txs / 2) % 255) as u8; 20]);
                            let tx = Transaction::new(
                                0,
                                trader,
                                TxPayload::PlaceOrder(PlaceOrderTx {
                                    market_id: 0,
                                    is_buy: false,
                                    order_type: OrderType::Limit,
                                    price: 50_000,
                                    size: 10,
                                    time_in_force: TimeInForce::IOC,
                                    reduce_only: false,
                                    post_only: false,
                                }),
                            );
                            txs.push(tx);
                        }

                        (engine, auth, txs, temp_dir)
                    },
                    |(mut engine, auth, txs, _temp_dir)| {
                        // Begin block
                        engine.state_mut().begin_block(3);

                        // Execute all taker transactions (with matching)
                        for tx in &txs {
                            if let TxPayload::PlaceOrder(ref order) = tx.payload {
                                let _ = auth.verify_transaction(tx);
                                let current_nonce = engine.state().get_nonce(tx.from).unwrap();
                                if tx.nonce == current_nonce {
                                    let _ = engine.process_place_order(tx, order);
                                    let _ = engine.state_mut().increment_nonce(tx.from);
                                }
                            }
                        }

                        // Commit block
                        let _ = black_box(engine.state_mut().commit().unwrap());
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

/// Benchmark: Mixed transaction workload
fn bench_tps_mixed_workload(c: &mut Criterion) {
    let mut group = c.benchmark_group("tps_mixed_workload");
    group.measurement_time(Duration::from_secs(10));

    for num_txs in [100, 500, 1000].iter() {
        group.throughput(Throughput::Elements(*num_txs as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(num_txs),
            num_txs,
            |b, &num_txs| {
                b.iter_batched(
                    || {
                        let (engine, auth, temp_dir) = setup_engine_with_traders(num_txs);

                        // Prepare mixed transactions: deposits, withdrawals, orders
                        let mut txs = Vec::new();
                        for i in 0..num_txs {
                            let trader = Address::from([(i % 255) as u8; 20]);

                            let payload = match i % 3 {
                                0 => TxPayload::Deposit(DepositTx {
                                    amount: 1_000_000,
                                    asset_id: 0,
                                }),
                                1 => TxPayload::PlaceOrder(PlaceOrderTx {
                                    market_id: 0,
                                    is_buy: i % 2 == 0,
                                    order_type: OrderType::Limit,
                                    price: 50_000 + ((i * 100) as u64),
                                    size: 10,
                                    time_in_force: TimeInForce::GTC,
                                    reduce_only: false,
                                    post_only: false,
                                }),
                                _ => TxPayload::Withdraw(WithdrawTx {
                                    to: trader,
                                    amount: 100_000,
                                    asset_id: 0,
                                }),
                            };

                            let tx = Transaction::new(0, trader, payload);
                            txs.push(tx);
                        }

                        (engine, auth, txs, temp_dir)
                    },
                    |(mut engine, auth, txs, _temp_dir)| {
                        engine.state_mut().begin_block(2);

                        for tx in &txs {
                            let _ = auth.verify_transaction(tx);
                            let current_nonce = engine.state().get_nonce(tx.from).unwrap();

                            if tx.nonce == current_nonce {
                                // Execute based on payload type
                                match &tx.payload {
                                    TxPayload::Deposit(deposit) => {
                                        let _ = engine.process_deposit(tx, deposit);
                                    }
                                    TxPayload::Withdraw(withdraw) => {
                                        let _ = engine.process_withdraw(tx, withdraw);
                                    }
                                    TxPayload::PlaceOrder(order) => {
                                        let _ = engine.process_place_order(tx, order);
                                    }
                                    _ => {}
                                }
                                let _ = engine.state_mut().increment_nonce(tx.from);
                            }
                        }

                        let _ = black_box(engine.state_mut().commit().unwrap());
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

/// Benchmark: Block production (multiple blocks)
fn bench_tps_block_production(c: &mut Criterion) {
    let mut group = c.benchmark_group("tps_block_production");
    group.measurement_time(Duration::from_secs(15));

    for txs_per_block in [100, 500, 1000].iter() {
        group.throughput(Throughput::Elements((*txs_per_block * 10) as u64)); // 10 blocks
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}tx_10blocks", txs_per_block)),
            txs_per_block,
            |b, &txs_per_block| {
                b.iter_batched(
                    || {
                        let (engine, auth, temp_dir) = setup_engine_with_traders(txs_per_block);
                        (engine, auth, temp_dir)
                    },
                    |(mut engine, auth, _temp_dir)| {
                        // Produce 10 blocks
                        for block_height in 2..12 {
                            engine.state_mut().begin_block(block_height);

                            // Execute transactions in this block
                            for i in 0..txs_per_block {
                                let trader =
                                    Address::from([((i + block_height as usize) % 255) as u8; 20]);
                                let tx = Transaction::new(
                                    block_height - 2, // Increment nonce per block
                                    trader,
                                    TxPayload::PlaceOrder(PlaceOrderTx {
                                        market_id: 0,
                                        is_buy: i % 2 == 0,
                                        order_type: OrderType::Limit,
                                        price: 50_000 + ((i * 100) as u64),
                                        size: 10,
                                        time_in_force: TimeInForce::GTC,
                                        reduce_only: false,
                                        post_only: false,
                                    }),
                                );

                                if let TxPayload::PlaceOrder(ref order) = tx.payload {
                                    let _ = auth.verify_transaction(&tx);
                                    let current_nonce = engine.state().get_nonce(tx.from).unwrap();
                                    if tx.nonce == current_nonce {
                                        let _ = engine.process_place_order(&tx, order);
                                        let _ = engine.state_mut().increment_nonce(tx.from);
                                    }
                                }
                            }

                            // Commit block
                            let _ = engine.state_mut().commit().unwrap();
                        }

                        black_box(());
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

/// Benchmark: High contention (many traders, same price level)
fn bench_tps_high_contention(c: &mut Criterion) {
    let mut group = c.benchmark_group("tps_high_contention");
    group.measurement_time(Duration::from_secs(10));

    for num_txs in [100, 500, 1000].iter() {
        group.throughput(Throughput::Elements(*num_txs as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(num_txs),
            num_txs,
            |b, &num_txs| {
                b.iter_batched(
                    || {
                        let (engine, auth, temp_dir) = setup_engine_with_traders(num_txs);

                        // All traders want to trade at the same price (high contention)
                        let mut txs = Vec::new();
                        for i in 0..num_txs {
                            let trader = Address::from([(i % 255) as u8; 20]);
                            let tx = Transaction::new(
                                0,
                                trader,
                                TxPayload::PlaceOrder(PlaceOrderTx {
                                    market_id: 0,
                                    is_buy: i % 2 == 0,
                                    order_type: OrderType::Limit,
                                    price: 50_000, // Same price for all
                                    size: 10,
                                    time_in_force: TimeInForce::GTC,
                                    reduce_only: false,
                                    post_only: false,
                                }),
                            );
                            txs.push(tx);
                        }

                        (engine, auth, txs, temp_dir)
                    },
                    |(mut engine, auth, txs, _temp_dir)| {
                        engine.state_mut().begin_block(2);

                        for tx in &txs {
                            if let TxPayload::PlaceOrder(ref order) = tx.payload {
                                let _ = auth.verify_transaction(tx);
                                let current_nonce = engine.state().get_nonce(tx.from).unwrap();
                                if tx.nonce == current_nonce {
                                    let _ = engine.process_place_order(tx, order);
                                    let _ = engine.state_mut().increment_nonce(tx.from);
                                }
                            }
                        }

                        let _ = black_box(engine.state_mut().commit().unwrap());
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

/// Benchmark: Orderbook with large depth
fn bench_tps_large_orderbook(c: &mut Criterion) {
    let mut group = c.benchmark_group("tps_large_orderbook");
    group.measurement_time(Duration::from_secs(15));
    group.sample_size(10);

    for orderbook_depth in [1000, 5000, 10000].iter() {
        group.throughput(Throughput::Elements(100)); // 100 new orders
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("depth_{}", orderbook_depth)),
            orderbook_depth,
            |b, &orderbook_depth| {
                b.iter_batched(
                    || {
                        let (mut engine, auth, temp_dir) =
                            setup_engine_with_traders(orderbook_depth + 100);

                        // Fill orderbook with existing orders
                        engine.state_mut().begin_block(2);
                        for i in 0..orderbook_depth {
                            let trader = Address::from([(i % 255) as u8; 20]);
                            let tx = Transaction::new(
                                0,
                                trader,
                                TxPayload::PlaceOrder(PlaceOrderTx {
                                    market_id: 0,
                                    is_buy: i % 2 == 0,
                                    order_type: OrderType::Limit,
                                    price: 50_000 + ((i / 2) as u64 * 100),
                                    size: 10,
                                    time_in_force: TimeInForce::GTC,
                                    reduce_only: false,
                                    post_only: false,
                                }),
                            );
                            if let TxPayload::PlaceOrder(ref order) = tx.payload {
                                let _ = engine.process_place_order(&tx, order);
                                let _ = engine.state_mut().increment_nonce(tx.from);
                            }
                        }
                        engine.state_mut().commit().unwrap();

                        // Prepare new orders
                        let mut txs = Vec::new();
                        for i in 0..100 {
                            let trader = Address::from([((orderbook_depth + i) % 255) as u8; 20]);
                            let tx = Transaction::new(
                                0,
                                trader,
                                TxPayload::PlaceOrder(PlaceOrderTx {
                                    market_id: 0,
                                    is_buy: i % 2 == 0,
                                    order_type: OrderType::Limit,
                                    price: 50_000 + ((i / 2) as u64 * 100),
                                    size: 10,
                                    time_in_force: TimeInForce::GTC,
                                    reduce_only: false,
                                    post_only: false,
                                }),
                            );
                            txs.push(tx);
                        }

                        (engine, auth, txs, temp_dir)
                    },
                    |(mut engine, auth, txs, _temp_dir)| {
                        engine.state_mut().begin_block(3);

                        for tx in &txs {
                            if let TxPayload::PlaceOrder(ref order) = tx.payload {
                                let _ = auth.verify_transaction(tx);
                                let current_nonce = engine.state().get_nonce(tx.from).unwrap();
                                if tx.nonce == current_nonce {
                                    let _ = engine.process_place_order(tx, order);
                                    let _ = engine.state_mut().increment_nonce(tx.from);
                                }
                            }
                        }

                        let _ = black_box(engine.state_mut().commit().unwrap());
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_tps_place_orders_no_match,
    bench_tps_place_orders_with_match,
    bench_tps_mixed_workload,
    bench_tps_block_production,
    bench_tps_high_contention,
    bench_tps_large_orderbook,
);
criterion_main!(benches);
