use alloy_primitives::Address;
/// Performance benchmarks for orderbook operations
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use pranklin_engine::Engine;
use pranklin_state::{Market, PruningConfig, StateManager};
use pranklin_tx::*;

fn create_benchmark_market() -> Market {
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

fn setup_engine() -> (Engine, tempfile::TempDir) {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let state = StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();
    let mut engine = Engine::new(state);

    let market = create_benchmark_market();
    engine.state_mut().set_market(0, market).unwrap();

    (engine, temp_dir)
}

fn bench_place_order(c: &mut Criterion) {
    let mut group = c.benchmark_group("place_order");

    for num_orders in [10, 100, 1000].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_orders),
            num_orders,
            |b, &num_orders| {
                b.iter_batched(
                    || {
                        // Setup: Create fresh engine for each iteration
                        let (mut engine, _dir) = setup_engine();
                        let trader = Address::from([1u8; 20]);

                        // Fund trader
                        engine
                            .state_mut()
                            .set_balance(trader, 0, u128::MAX)
                            .unwrap();

                        (engine, trader)
                    },
                    |(mut engine, trader)| {
                        // Benchmark: Place orders
                        for i in 0..num_orders {
                            let tx = Transaction::new_raw(
                                i,
                                trader,
                                TxPayload::PlaceOrder(PlaceOrderTx {
                                    market_id: 0,
                                    is_buy: i % 2 == 0,
                                    order_type: OrderType::Limit,
                                    price: 50_000 + (i * 1000),
                                    size: 10,
                                    time_in_force: TimeInForce::GTC,
                                    reduce_only: false,
                                    post_only: false,
                                }),
                            );

                            if let TxPayload::PlaceOrder(ref order) = tx.payload {
                                let _ = black_box(engine.process_place_order(tx.from, order));
                            }
                        }
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

fn bench_order_matching(c: &mut Criterion) {
    c.bench_function("order_matching_100_orders", |b| {
        b.iter_batched(
            || {
                // Setup: Create engine with 100 maker orders
                let (mut engine, _dir) = setup_engine();
                let maker = Address::from([1u8; 20]);
                let taker = Address::from([2u8; 20]);

                // Fund traders
                engine.state_mut().set_balance(maker, 0, u128::MAX).unwrap();
                engine.state_mut().set_balance(taker, 0, u128::MAX).unwrap();

                // Place 100 maker orders
                for i in 0..100 {
                    let tx = Transaction::new_raw(
                        i,
                        maker,
                        TxPayload::PlaceOrder(PlaceOrderTx {
                            market_id: 0,
                            is_buy: false,
                            order_type: OrderType::Limit,
                            price: 50_000 + (i * 1000),
                            size: 10,
                            time_in_force: TimeInForce::GTC,
                            reduce_only: false,
                            post_only: false,
                        }),
                    );

                    if let TxPayload::PlaceOrder(ref order) = tx.payload {
                        engine.process_place_order(tx.from, order).unwrap();
                    }
                }

                (engine, taker)
            },
            |(mut engine, taker)| {
                // Benchmark: Place taker order that matches
                let tx = Transaction::new_raw(
                    0,
                    taker,
                    TxPayload::PlaceOrder(PlaceOrderTx {
                        market_id: 0,
                        is_buy: true,
                        order_type: OrderType::Market,
                        price: 0,
                        size: 50,
                        time_in_force: TimeInForce::IOC,
                        reduce_only: false,
                        post_only: false,
                    }),
                );

                if let TxPayload::PlaceOrder(ref order) = tx.payload {
                    let _ = black_box(engine.process_place_order(tx.from, order));
                }
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_orderbook_rebuild(c: &mut Criterion) {
    let mut group = c.benchmark_group("orderbook_rebuild");

    for num_orders in [100, 1000, 10000].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_orders),
            num_orders,
            |b, &num_orders| {
                let temp_dir = tempfile::TempDir::new().unwrap();

                // Phase 1: Create orders
                {
                    let state =
                        StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();
                    let mut engine = Engine::new(state);

                    let market = create_benchmark_market();
                    engine.state_mut().set_market(0, market).unwrap();

                    let trader = Address::from([1u8; 20]);
                    engine
                        .state_mut()
                        .set_balance(trader, 0, u128::MAX)
                        .unwrap();

                    for i in 0..num_orders {
                        let tx = Transaction::new_raw(
                            i,
                            trader,
                            TxPayload::PlaceOrder(PlaceOrderTx {
                                market_id: 0,
                                is_buy: i % 2 == 0,
                                order_type: OrderType::Limit,
                                price: 50_000 + ((i % 100) * 1000),
                                size: 10,
                                time_in_force: TimeInForce::GTC,
                                reduce_only: false,
                                post_only: false,
                            }),
                        );

                        if let TxPayload::PlaceOrder(ref order) = tx.payload {
                            engine.process_place_order(tx.from, order).unwrap();
                        }
                    }

                    engine.state_mut().begin_block(1);
                    engine.state_mut().commit().unwrap();
                }

                // Phase 2: Benchmark rebuild
                b.iter(|| {
                    let state =
                        StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();
                    let mut engine = Engine::new(state);
                    engine.state_mut().rebuild_position_index().unwrap();
                    engine.rebuild_orderbook_from_state().unwrap();
                    black_box(());
                });
            },
        );
    }

    group.finish();
}

fn bench_state_operations(c: &mut Criterion) {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let state = StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();
    let mut engine = Engine::new(state);

    let address = Address::from([1u8; 20]);

    c.bench_function("state_set_balance", |b| {
        b.iter(|| {
            engine
                .state_mut()
                .set_balance(black_box(address), black_box(0), black_box(1_000_000))
                .unwrap();
        });
    });

    c.bench_function("state_get_balance", |b| {
        b.iter(|| {
            let _ = black_box(engine.state().get_balance(black_box(address), black_box(0)));
        });
    });

    let mut block_height = 0u64;
    c.bench_function("state_commit", |b| {
        b.iter(|| {
            block_height += 1;
            engine.state_mut().begin_block(black_box(block_height));
            engine
                .state_mut()
                .set_balance(address, 0, block_height as u128)
                .unwrap();
            let _ = black_box(engine.state_mut().commit());
        });
    });
}

criterion_group!(
    benches,
    bench_place_order,
    bench_order_matching,
    bench_orderbook_rebuild,
    bench_state_operations
);
criterion_main!(benches);
