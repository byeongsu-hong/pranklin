use crate::client::RpcClient;
use crate::metrics::MetricsCollector;
use crate::wallet::Wallet;
use anyhow::Result;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use pranklin_tx::*;

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
    let mut handles = Vec::new();

    for _worker_id in 0..num_workers {
        let client = client.clone();
        let wallets = wallets.clone();
        let metrics = metrics.clone();
        let start_time = start;

        let handle = tokio::spawn(async move {
            while start_time.elapsed() < duration {
                // Select a random wallet
                let wallet_idx = fastrand::usize(0..wallets.len());
                let wallet = &wallets[wallet_idx];

                // Place an order
                let order_result = place_random_order(&client, wallet, market_id, &metrics).await;

                // If placement succeeded, immediately try to cancel it
                if let Some(order_id) = order_result {
                    // Small delay before canceling
                    let delay = fastrand::u64(10..50);
                    sleep(Duration::from_millis(delay)).await;

                    cancel_order(&client, wallet, order_id, &metrics).await;
                }

                // Brief pause before next iteration
                let pause = fastrand::u64(5..20);
                sleep(Duration::from_millis(pause)).await;
            }
        });

        handles.push(handle);
    }

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

    // Divide workers into buyers and sellers
    let num_buyers = num_workers / 2;
    let num_sellers = num_workers - num_buyers;

    let start = Instant::now();
    let mut handles = Vec::new();

    // Price range for matching
    let base_price = 50000_000000u64; // $50,000 with 6 decimals
    let price_spread = 100_000000u64; // $100 spread

    // Spawn buyer workers
    for worker_id in 0..num_buyers {
        let client = client.clone();
        let wallets = wallets.clone();
        let metrics = metrics.clone();
        let start_time = start;

        let handle = tokio::spawn(async move {
            while start_time.elapsed() < duration {
                let wallet = &wallets[worker_id % wallets.len()];

                // Place buy order at or above base price (likely to match)
                let price = base_price + fastrand::u64(0..price_spread);
                let size = fastrand::u64(1_000000..10_000000); // 1-10 contracts

                let payload = TxPayload::PlaceOrder(PlaceOrderTx {
                    market_id,
                    is_buy: true,
                    order_type: OrderType::Limit,
                    price,
                    size,
                    time_in_force: TimeInForce::IOC, // Immediate or cancel for matching
                    reduce_only: false,
                    post_only: false,
                });

                send_transaction(&client, wallet, payload, &metrics).await;

                let delay = fastrand::u64(1..10);
                sleep(Duration::from_millis(delay)).await;
            }
        });

        handles.push(handle);
    }

    // Spawn seller workers
    for worker_id in 0..num_sellers {
        let client = client.clone();
        let wallets = wallets.clone();
        let metrics = metrics.clone();
        let start_time = start;

        let handle = tokio::spawn(async move {
            while start_time.elapsed() < duration {
                let wallet = &wallets[(num_buyers + worker_id) % wallets.len()];

                // Place sell order at or below base price (likely to match)
                let price = base_price - fastrand::u64(0..price_spread);
                let size = fastrand::u64(1_000000..10_000000);

                let payload = TxPayload::PlaceOrder(PlaceOrderTx {
                    market_id,
                    is_buy: false,
                    order_type: OrderType::Limit,
                    price,
                    size,
                    time_in_force: TimeInForce::IOC,
                    reduce_only: false,
                    post_only: false,
                });

                send_transaction(&client, wallet, payload, &metrics).await;

                let delay = fastrand::u64(1..10);
                sleep(Duration::from_millis(delay)).await;
            }
        });

        handles.push(handle);
    }

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
    let mut handles = Vec::new();

    // First phase: Build up the orderbook with limit orders (10 seconds)
    let build_duration = Duration::from_secs(10);

    tracing::info!("  Phase 1: Building orderbook depth...");

    for worker_id in 0..num_workers / 2 {
        let client = client.clone();
        let wallets = wallets.clone();
        let metrics = metrics.clone();
        let start_time = start;

        let handle = tokio::spawn(async move {
            let is_buyer = worker_id % 2 == 0;

            let phase_start = Instant::now();
            while phase_start.elapsed() < build_duration && start_time.elapsed() < duration {
                let wallet = &wallets[worker_id % wallets.len()];

                // Create limit orders with post-only to build depth
                let base_price = 50000_000000u64;
                let price_offset = fastrand::u64(0..1000_000000); // $0-$1000 offset

                let price = if is_buyer {
                    base_price - price_offset // Buy below mid
                } else {
                    base_price + price_offset // Sell above mid
                };

                let size = fastrand::u64(5_000000..50_000000);
                let payload = TxPayload::PlaceOrder(PlaceOrderTx {
                    market_id,
                    is_buy: is_buyer,
                    order_type: OrderType::Limit,
                    price,
                    size,
                    time_in_force: TimeInForce::PostOnly,
                    reduce_only: false,
                    post_only: true,
                });

                send_transaction(&client, wallet, payload, &metrics).await;
                sleep(Duration::from_millis(20)).await;
            }
        });

        handles.push(handle);
    }

    // Wait for orderbook building phase
    for handle in handles {
        let _ = handle.await;
    }

    tracing::info!("  Phase 2: Aggressive market orders...");

    let mut handles = Vec::new();

    // Second phase: Spam market orders to match against the book
    for worker_id in 0..num_workers {
        let client = client.clone();
        let wallets = wallets.clone();
        let metrics = metrics.clone();
        let start_time = start;

        let handle = tokio::spawn(async move {
            while start_time.elapsed() < duration {
                let wallet = &wallets[worker_id % wallets.len()];

                // Alternate between market buys and sells
                let is_buy = fastrand::bool();

                let size = fastrand::u64(1_000000..20_000000);
                let payload = TxPayload::PlaceOrder(PlaceOrderTx {
                    market_id,
                    is_buy,
                    order_type: OrderType::Market,
                    price: 0, // Market order
                    size,
                    time_in_force: TimeInForce::IOC,
                    reduce_only: false,
                    post_only: false,
                });

                send_transaction(&client, wallet, payload, &metrics).await;

                // High frequency
                let delay = fastrand::u64(1..5);
                sleep(Duration::from_millis(delay)).await;
            }
        });

        handles.push(handle);
    }

    for handle in handles {
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
    let payload = TxPayload::PlaceOrder(PlaceOrderTx {
        market_id,
        is_buy: fastrand::bool(),
        order_type: OrderType::Limit,
        price: fastrand::u64(45000_000000..55000_000000),
        size: fastrand::u64(1_000000..10_000000),
        time_in_force: TimeInForce::GTC,
        reduce_only: false,
        post_only: false,
    });

    let start = Instant::now();

    match wallet.create_signed_transaction(payload) {
        Ok(tx) => match client.submit_transaction(&tx).await {
            Ok(_) => {
                metrics.record_success(start.elapsed()).await;
                // In a real scenario, we'd parse the response to get actual order ID
                // For now, return a simulated order ID
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
            metrics
                .record_failure(format!("TX creation failed: {}", e))
                .await;
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
    let payload = TxPayload::CancelOrder(CancelOrderTx { order_id });

    send_transaction(client, wallet, payload, metrics).await;
}

/// Helper: Send a transaction and record metrics
async fn send_transaction(
    client: &RpcClient,
    wallet: &Wallet,
    payload: TxPayload,
    metrics: &MetricsCollector,
) {
    let start = Instant::now();

    match wallet.create_signed_transaction(payload) {
        Ok(tx) => match client.submit_transaction(&tx).await {
            Ok(_) => {
                metrics.record_success(start.elapsed()).await;
            }
            Err(e) => {
                metrics.record_failure(format!("{}", e)).await;
            }
        },
        Err(e) => {
            metrics.record_failure(format!("TX creation: {}", e)).await;
        }
    }
}
