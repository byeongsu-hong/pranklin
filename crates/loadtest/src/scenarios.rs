use crate::client::RpcClient;
use crate::generator::{OrderGenerator, TxGenerator};
use crate::metrics::MetricsCollector;
use crate::wallet::Wallet;
use anyhow::Result;
use pranklin_tx::*;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;

/// Helper: Send a transaction and record metrics
async fn send_tx(
    client: &RpcClient,
    wallet: &Wallet,
    payload: TxPayload,
    metrics: &MetricsCollector,
) {
    let start = Instant::now();
    match wallet.create_signed_transaction(payload) {
        Ok(tx) => match client.submit_transaction(&tx).await {
            Ok(_) => metrics.record_success(start.elapsed()).await,
            Err(e) => metrics.record_failure(format!("{}", e)).await,
        },
        Err(e) => metrics.record_failure(format!("TX creation: {}", e)).await,
    }
}

/// Scenario: Order spamming - rapid submit & cancel
pub async fn run_order_spam_scenario(
    client: Arc<RpcClient>,
    wallets: Vec<Arc<Wallet>>,
    market_id: u32,
    duration: Duration,
    num_workers: usize,
    metrics: MetricsCollector,
) -> Result<()> {
    tracing::info!("📝 Running order spam scenario (submit & cancel)");

    let start = Instant::now();
    let handles = (0..num_workers)
        .map(|_| {
            let client = client.clone();
            let wallets = wallets.clone();
            let metrics = metrics.clone();

            tokio::spawn(async move {
                while start.elapsed() < duration {
                    let wallet = &wallets[fastrand::usize(0..wallets.len())];
                    let order_id = place_random_order(&client, wallet, market_id, &metrics).await;

                    if let Some(id) = order_id {
                        sleep(Duration::from_millis(fastrand::u64(10..50))).await;
                        cancel_order(&client, wallet, id, &metrics).await;
                    }

                    sleep(Duration::from_millis(fastrand::u64(5..20))).await;
                }
            })
        })
        .collect::<Vec<_>>();

    for handle in handles {
        let _ = handle.await;
    }

    Ok(())
}

/// Scenario: Order matching spam - create matching buy/sell orders
pub async fn run_order_matching_scenario(
    client: Arc<RpcClient>,
    wallets: Vec<Arc<Wallet>>,
    market_id: u32,
    duration: Duration,
    num_workers: usize,
    metrics: MetricsCollector,
) -> Result<()> {
    tracing::info!("🔄 Running order matching scenario");

    let (num_buyers, num_sellers) = (num_workers / 2, num_workers - num_workers / 2);
    let base_price = 50000_000000u64;
    let price_spread = 100_000000u64;
    let start = Instant::now();

    let spawn_trader = |is_buyer: bool, worker_id: usize| {
        let (client, wallets, metrics) = (client.clone(), wallets.clone(), metrics.clone());

        tokio::spawn(async move {
            while start.elapsed() < duration {
                let wallet = &wallets[worker_id % wallets.len()];
                let price = if is_buyer {
                    base_price + fastrand::u64(0..price_spread)
                } else {
                    base_price - fastrand::u64(0..price_spread)
                };

                let payload = OrderGenerator::with_params(
                    market_id,
                    is_buyer,
                    OrderType::Limit,
                    price,
                    fastrand::u64(1_000000..10_000000),
                    TimeInForce::IOC,
                );

                send_tx(&client, wallet, payload, &metrics).await;
                sleep(Duration::from_millis(fastrand::u64(1..10))).await;
            }
        })
    };

    let handles: Vec<_> = (0..num_buyers)
        .map(|id| spawn_trader(true, id))
        .chain((0..num_sellers).map(|id| spawn_trader(false, num_buyers + id)))
        .collect();

    for handle in handles {
        let _ = handle.await;
    }

    Ok(())
}

/// Scenario: Aggressive orderbook depth building + matching
pub async fn run_aggressive_matching_scenario(
    client: Arc<RpcClient>,
    wallets: Vec<Arc<Wallet>>,
    market_id: u32,
    duration: Duration,
    num_workers: usize,
    metrics: MetricsCollector,
) -> Result<()> {
    tracing::info!("⚡ Running aggressive matching scenario");

    let start = Instant::now();
    let build_duration = Duration::from_secs(10);
    let base_price = 50000_000000u64;

    tracing::info!("  Phase 1: Building orderbook depth...");

    let build_handles = (0..num_workers / 2).map(|worker_id| {
        let (client, wallets, metrics) = (client.clone(), wallets.clone(), metrics.clone());

        tokio::spawn(async move {
            let is_buyer = worker_id % 2 == 0;
            let phase_start = Instant::now();

            while phase_start.elapsed() < build_duration && start.elapsed() < duration {
                let wallet = &wallets[worker_id % wallets.len()];
                let price_offset = fastrand::u64(0..1000_000000);
                let price = if is_buyer {
                    base_price - price_offset
                } else {
                    base_price + price_offset
                };

                let payload = OrderGenerator::with_params(
                    market_id,
                    is_buyer,
                    OrderType::Limit,
                    price,
                    fastrand::u64(5_000000..50_000000),
                    TimeInForce::PostOnly,
                );

                send_tx(&client, wallet, payload, &metrics).await;
                sleep(Duration::from_millis(20)).await;
            }
        })
    });

    for handle in build_handles {
        let _ = handle.await;
    }

    tracing::info!("  Phase 2: Aggressive market orders...");

    let market_handles = (0..num_workers).map(|worker_id| {
        let (client, wallets, metrics) = (client.clone(), wallets.clone(), metrics.clone());

        tokio::spawn(async move {
            while start.elapsed() < duration {
                let wallet = &wallets[worker_id % wallets.len()];
                let payload = OrderGenerator::with_params(
                    market_id,
                    fastrand::bool(),
                    OrderType::Market,
                    0,
                    fastrand::u64(1_000000..20_000000),
                    TimeInForce::IOC,
                );

                send_tx(&client, wallet, payload, &metrics).await;
                sleep(Duration::from_millis(fastrand::u64(1..5))).await;
            }
        })
    });

    for handle in market_handles {
        let _ = handle.await;
    }

    Ok(())
}

/// Helper: Place a random order and return the order ID (simulated)
async fn place_random_order(
    client: &RpcClient,
    wallet: &Wallet,
    market_id: u32,
    metrics: &MetricsCollector,
) -> Option<u64> {
    let payload = OrderGenerator::new(market_id).generate();
    let start = Instant::now();

    match wallet.create_signed_transaction(payload) {
        Ok(tx) => match client.submit_transaction(&tx).await {
            Ok(_) => {
                metrics.record_success(start.elapsed()).await;
                Some(fastrand::u64(1..1_000_000))
            }
            Err(e) => {
                metrics
                    .record_failure(format!("Place order failed: {}", e))
                    .await;
                None
            }
        },
        Err(e) => {
            metrics.record_failure(format!("TX creation: {}", e)).await;
            None
        }
    }
}

/// Helper: Cancel an order
async fn cancel_order(
    client: &RpcClient,
    wallet: &Wallet,
    order_id: u64,
    metrics: &MetricsCollector,
) {
    send_tx(
        client,
        wallet,
        TxPayload::CancelOrder(CancelOrderTx { order_id }),
        metrics,
    )
    .await;
}
