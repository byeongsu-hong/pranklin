use alloy_primitives::Address;
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use pranklin_engine::{BlockStmScheduler, DependencyGraph};
use pranklin_tx::{OrderType, PlaceOrderTx, TimeInForce, Transaction, TransferTx, TxPayload};
use std::hint::black_box;

fn bench_dependency_graph_independent(c: &mut Criterion) {
    let mut group = c.benchmark_group("dependency_graph_independent");

    for size in [10, 50, 100, 500, 1000] {
        group.throughput(Throughput::Elements(size));

        // Create independent transactions (different accounts)
        let txs: Vec<Transaction> = (0..size)
            .map(|i| {
                Transaction::new_raw(
                    1,
                    Address::from([i as u8; 20]),
                    TxPayload::Transfer(TransferTx {
                        to: Address::from([(i + 1) as u8; 20]),
                        asset_id: 0,
                        amount: 1000,
                    }),
                )
            })
            .collect();

        group.bench_with_input(BenchmarkId::from_parameter(size), &txs, |b, txs| {
            b.iter(|| {
                let graph = DependencyGraph::build(black_box(txs));
                black_box(graph);
            });
        });
    }

    group.finish();
}

fn bench_dependency_graph_sequential(c: &mut Criterion) {
    let mut group = c.benchmark_group("dependency_graph_sequential");

    for size in [10, 50, 100, 500, 1000] {
        group.throughput(Throughput::Elements(size));

        // Create sequential transactions (same account)
        let addr = Address::from([1u8; 20]);
        let txs: Vec<Transaction> = (0..size)
            .map(|i| {
                Transaction::new_raw(
                    i + 1,
                    addr,
                    TxPayload::Transfer(TransferTx {
                        to: Address::from([2u8; 20]),
                        asset_id: 0,
                        amount: 1000,
                    }),
                )
            })
            .collect();

        group.bench_with_input(BenchmarkId::from_parameter(size), &txs, |b, txs| {
            b.iter(|| {
                let graph = DependencyGraph::build(black_box(txs));
                black_box(graph);
            });
        });
    }

    group.finish();
}

fn bench_dependency_graph_mixed(c: &mut Criterion) {
    let mut group = c.benchmark_group("dependency_graph_mixed");

    for size in [10, 50, 100, 500, 1000] {
        group.throughput(Throughput::Elements(size));

        // Create mixed workload (50% same market, 50% different markets)
        let txs: Vec<Transaction> = (0..size)
            .map(|i| {
                let market_id = if i % 2 == 0 { 0 } else { (i % 10) as u32 };
                Transaction::new_raw(
                    1,
                    Address::from([i as u8; 20]),
                    TxPayload::PlaceOrder(PlaceOrderTx {
                        market_id,
                        is_buy: i % 2 == 0,
                        order_type: OrderType::Limit,
                        price: 50000,
                        size: 10,
                        time_in_force: TimeInForce::GTC,
                        reduce_only: false,
                        post_only: false,
                    }),
                )
            })
            .collect();

        group.bench_with_input(BenchmarkId::from_parameter(size), &txs, |b, txs| {
            b.iter(|| {
                let graph = DependencyGraph::build(black_box(txs));
                black_box(graph);
            });
        });
    }

    group.finish();
}

fn bench_independent_groups(c: &mut Criterion) {
    let mut group = c.benchmark_group("independent_groups");

    for size in [10, 50, 100, 500, 1000] {
        group.throughput(Throughput::Elements(size));

        let txs: Vec<Transaction> = (0..size)
            .map(|i| {
                Transaction::new_raw(
                    1,
                    Address::from([i as u8; 20]),
                    TxPayload::Transfer(TransferTx {
                        to: Address::from([(i + 1) as u8; 20]),
                        asset_id: 0,
                        amount: 1000,
                    }),
                )
            })
            .collect();

        let graph = DependencyGraph::build(&txs);

        group.bench_with_input(BenchmarkId::from_parameter(size), &graph, |b, graph| {
            b.iter(|| {
                let groups = black_box(graph).independent_groups();
                black_box(groups);
            });
        });
    }

    group.finish();
}

fn bench_parallelism_analysis(c: &mut Criterion) {
    let mut group = c.benchmark_group("parallelism_analysis");

    for size in [10, 50, 100, 500, 1000] {
        group.throughput(Throughput::Elements(size));

        let scheduler = BlockStmScheduler::with_workers(8);

        // Independent transactions (best case)
        let independent_txs: Vec<Transaction> = (0..size)
            .map(|i| {
                Transaction::new_raw(
                    1,
                    Address::from([i as u8; 20]),
                    TxPayload::Transfer(TransferTx {
                        to: Address::from([(i + 1) as u8; 20]),
                        asset_id: 0,
                        amount: 1000,
                    }),
                )
            })
            .collect();

        group.bench_with_input(
            BenchmarkId::new("independent", size),
            &independent_txs,
            |b, txs| {
                b.iter(|| {
                    let analysis = scheduler.analyze_block(black_box(txs));
                    black_box(analysis);
                });
            },
        );

        // Sequential transactions (worst case)
        let addr = Address::from([1u8; 20]);
        let sequential_txs: Vec<Transaction> = (0..size)
            .map(|i| {
                Transaction::new_raw(
                    i + 1,
                    addr,
                    TxPayload::Transfer(TransferTx {
                        to: Address::from([2u8; 20]),
                        asset_id: 0,
                        amount: 1000,
                    }),
                )
            })
            .collect();

        group.bench_with_input(
            BenchmarkId::new("sequential", size),
            &sequential_txs,
            |b, txs| {
                b.iter(|| {
                    let analysis = scheduler.analyze_block(black_box(txs));
                    black_box(analysis);
                });
            },
        );
    }

    group.finish();
}

fn bench_parallelism_score(c: &mut Criterion) {
    let mut group = c.benchmark_group("parallelism_score");

    for size in [10, 50, 100, 500, 1000] {
        group.throughput(Throughput::Elements(size));

        let txs: Vec<Transaction> = (0..size)
            .map(|i| {
                Transaction::new_raw(
                    1,
                    Address::from([i as u8; 20]),
                    TxPayload::Transfer(TransferTx {
                        to: Address::from([(i + 1) as u8; 20]),
                        asset_id: 0,
                        amount: 1000,
                    }),
                )
            })
            .collect();

        let graph = DependencyGraph::build(&txs);

        group.bench_with_input(BenchmarkId::from_parameter(size), &graph, |b, graph| {
            b.iter(|| {
                let score = black_box(graph).parallelism_score();
                black_box(score);
            });
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_dependency_graph_independent,
    bench_dependency_graph_sequential,
    bench_dependency_graph_mixed,
    bench_independent_groups,
    bench_parallelism_analysis,
    bench_parallelism_score
);
criterion_main!(benches);
