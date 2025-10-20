use crate::client::RpcClient;
use crate::config::{LoadTestConfig, LoadTestMode, TestScenario};
use crate::generator;
use crate::metrics::{LoadTestResults, MetricsCollector};
use crate::scenarios;
use crate::setup::AccountSetup;
use crate::wallet::Wallet;
use anyhow::Result;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;

/// Run the load test with the given configuration
pub async fn run_load_test(config: LoadTestConfig) -> Result<LoadTestResults> {
    // Check server health first
    let client = Arc::new(RpcClient::new(config.rpc_url.clone()));
    tracing::info!("🔍 Checking server health...");
    match client.health().await {
        Ok(status) => tracing::info!("  ✓ Server is healthy: {}", status),
        Err(e) => {
            tracing::error!("  ✗ Server health check failed: {}", e);
            return Err(anyhow::anyhow!("Server is not responding"));
        }
    }

    // Initialize wallets
    tracing::info!("💰 Generating {} wallets...", config.num_wallets);
    let wallets: Vec<Arc<Wallet>> = (0..config.num_wallets)
        .map(|_| Arc::new(Wallet::new_random()))
        .collect();
    tracing::info!("  ✓ Wallets generated");

    // Account setup phase (if operator mode is enabled)
    if config.operator_mode {
        tracing::info!("\n🔧 PHASE 1: Account Initialization");
        let setup = AccountSetup::new(client.clone());

        tracing::info!(
            "⚠️  Bridge operator address: {:?}",
            setup.operator_address()
        );
        tracing::info!(
            "   Make sure this address is authorized as a bridge operator on the server!"
        );
        tracing::info!("   Waiting 3 seconds before proceeding...");
        tokio::time::sleep(Duration::from_secs(3)).await;

        setup
            .initialize_wallets(&wallets, config.asset_id, config.initial_balance)
            .await?;

        // Verify balances
        let verified = setup
            .verify_balances(&wallets, config.asset_id, config.initial_balance)
            .await?;

        if verified < 5 {
            tracing::warn!(
                "⚠️  Only {}/10 wallets verified with correct balance",
                verified
            );
            tracing::warn!("   Continuing anyway, but results may be affected...");
        }
    }

    // Initialize metrics
    let metrics = MetricsCollector::new();

    // Start periodic stats printing
    let metrics_clone = metrics.clone();
    let stats_handle = tokio::spawn(async move {
        loop {
            sleep(Duration::from_secs(5)).await;
            metrics_clone.print_stats().await;
        }
    });

    // Run the appropriate test scenario
    tracing::info!("\n🎯 PHASE 2: Load Testing");

    match config.scenario {
        TestScenario::Standard => {
            // Original load test modes
            match config.mode {
                LoadTestMode::Sustained => {
                    run_sustained_load(config, client, wallets, metrics.clone()).await?
                }
                LoadTestMode::Ramp => {
                    run_ramp_load(config, client, wallets, metrics.clone()).await?
                }
                LoadTestMode::Burst => {
                    run_burst_load(config, client, wallets, metrics.clone()).await?
                }
                LoadTestMode::Stress => {
                    run_stress_load(config, client, wallets, metrics.clone()).await?
                }
            }
        }
        TestScenario::OrderSpam => {
            scenarios::run_order_spam_scenario(
                client,
                wallets,
                config.market_id,
                Duration::from_secs(config.duration_secs),
                config.num_workers,
                metrics.clone(),
            )
            .await?
        }
        TestScenario::OrderMatching => {
            scenarios::run_order_matching_scenario(
                client,
                wallets,
                config.market_id,
                Duration::from_secs(config.duration_secs),
                config.num_workers,
                metrics.clone(),
            )
            .await?
        }
        TestScenario::Aggressive => {
            scenarios::run_aggressive_matching_scenario(
                client,
                wallets,
                config.market_id,
                Duration::from_secs(config.duration_secs),
                config.num_workers,
                metrics.clone(),
            )
            .await?
        }
    }

    // Stop stats printing
    stats_handle.abort();

    // Get final results
    let results = metrics.get_results().await;
    Ok(results)
}

/// Run sustained constant load
async fn run_sustained_load(
    config: LoadTestConfig,
    client: Arc<RpcClient>,
    wallets: Vec<Arc<Wallet>>,
    metrics: MetricsCollector,
) -> Result<()> {
    tracing::info!("▶️  Running sustained load test...");

    let start = Instant::now();
    let duration = Duration::from_secs(config.duration_secs);

    // Calculate delay between requests per worker
    let requests_per_worker_per_sec = config.target_tps as f64 / config.num_workers as f64;
    let delay_between_requests = Duration::from_secs_f64(1.0 / requests_per_worker_per_sec);

    // Spawn workers
    let mut handles = Vec::new();
    for worker_id in 0..config.num_workers {
        let client = client.clone();
        let wallets = wallets.clone();
        let metrics = metrics.clone();
        let config = config.clone();

        let handle = tokio::spawn(async move {
            let mut last_request = Instant::now();

            while start.elapsed() < duration {
                // Wait for the appropriate time
                let now = Instant::now();
                if now < last_request + delay_between_requests {
                    sleep(last_request + delay_between_requests - now).await;
                }
                last_request = Instant::now();

                // Select a wallet (round-robin)
                let wallet_idx = worker_id % wallets.len();
                let wallet = &wallets[wallet_idx];

                // Send transaction
                send_transaction(&client, wallet, &config, &metrics).await;
            }
        });

        handles.push(handle);
    }

    // Wait for all workers to complete
    for handle in handles {
        let _ = handle.await;
    }

    Ok(())
}

/// Run ramp load (gradually increasing)
async fn run_ramp_load(
    config: LoadTestConfig,
    client: Arc<RpcClient>,
    wallets: Vec<Arc<Wallet>>,
    metrics: MetricsCollector,
) -> Result<()> {
    tracing::info!("📈 Running ramp load test...");

    let start = Instant::now();
    let duration = Duration::from_secs(config.duration_secs);
    let ramp_duration = Duration::from_secs(config.ramp_up_secs);

    // Spawn workers
    let mut handles = Vec::new();
    for worker_id in 0..config.num_workers {
        let client = client.clone();
        let wallets = wallets.clone();
        let metrics = metrics.clone();
        let config = config.clone();

        let handle = tokio::spawn(async move {
            while start.elapsed() < duration {
                let elapsed = start.elapsed();

                // Calculate current target TPS based on ramp progress
                let current_tps = if elapsed < ramp_duration {
                    let progress = elapsed.as_secs_f64() / ramp_duration.as_secs_f64();
                    (config.target_tps as f64 * progress).max(1.0)
                } else {
                    config.target_tps as f64
                };

                let requests_per_worker_per_sec = current_tps / config.num_workers as f64;
                let delay = Duration::from_secs_f64(1.0 / requests_per_worker_per_sec);

                let wallet_idx = worker_id % wallets.len();
                let wallet = &wallets[wallet_idx];

                send_transaction(&client, wallet, &config, &metrics).await;

                sleep(delay).await;
            }
        });

        handles.push(handle);
    }

    for handle in handles {
        let _ = handle.await;
    }

    Ok(())
}

/// Run burst load (periodic spikes)
async fn run_burst_load(
    config: LoadTestConfig,
    client: Arc<RpcClient>,
    wallets: Vec<Arc<Wallet>>,
    metrics: MetricsCollector,
) -> Result<()> {
    tracing::info!("💥 Running burst load test...");

    let start = Instant::now();
    let duration = Duration::from_secs(config.duration_secs);
    let burst_duration = Duration::from_secs(config.burst_duration_secs);
    let burst_interval = Duration::from_secs(config.burst_interval_secs);

    let mut in_burst = false;
    let mut next_burst = start;

    while start.elapsed() < duration {
        let now = Instant::now();

        if now >= next_burst {
            in_burst = !in_burst;
            if in_burst {
                tracing::info!("💥 BURST START");
                next_burst = now + burst_duration;
            } else {
                tracing::info!("💤 BURST END - cooling down");
                next_burst = now + (burst_interval - burst_duration);
            }
        }

        if in_burst {
            // During burst: maximum load
            let mut handles = Vec::new();
            for worker_id in 0..config.num_workers {
                let client = client.clone();
                let wallets = wallets.clone();
                let metrics = metrics.clone();
                let config = config.clone();

                let handle = tokio::spawn(async move {
                    let wallet_idx = worker_id % wallets.len();
                    let wallet = &wallets[wallet_idx];
                    send_transaction(&client, wallet, &config, &metrics).await;
                });

                handles.push(handle);
            }

            for handle in handles {
                let _ = handle.await;
            }

            // Small delay to prevent overwhelming
            sleep(Duration::from_millis(10)).await;
        } else {
            // Outside burst: idle
            sleep(Duration::from_millis(100)).await;
        }
    }

    Ok(())
}

/// Run stress test (maximum throughput)
async fn run_stress_load(
    config: LoadTestConfig,
    client: Arc<RpcClient>,
    wallets: Vec<Arc<Wallet>>,
    metrics: MetricsCollector,
) -> Result<()> {
    tracing::info!("🔥 Running stress test (maximum throughput)...");

    let start = Instant::now();
    let duration = Duration::from_secs(config.duration_secs);

    // Spawn workers that send as fast as possible
    let mut handles = Vec::new();
    for worker_id in 0..config.num_workers {
        let client = client.clone();
        let wallets = wallets.clone();
        let metrics = metrics.clone();
        let config = config.clone();

        let handle = tokio::spawn(async move {
            while start.elapsed() < duration {
                let wallet_idx = worker_id % wallets.len();
                let wallet = &wallets[wallet_idx];

                send_transaction(&client, wallet, &config, &metrics).await;

                // Tiny delay to allow other tasks to run
                tokio::task::yield_now().await;
            }
        });

        handles.push(handle);
    }

    for handle in handles {
        let _ = handle.await;
    }

    Ok(())
}

/// Send a single transaction
async fn send_transaction(
    client: &RpcClient,
    wallet: &Wallet,
    config: &LoadTestConfig,
    metrics: &MetricsCollector,
) {
    let start = Instant::now();

    // Generate transaction
    let tx = match generator::generate_transaction(
        wallet,
        config.tx_type,
        config.market_id,
        config.asset_id,
    ) {
        Ok(tx) => tx,
        Err(e) => {
            metrics
                .record_failure(format!("Transaction generation failed: {}", e))
                .await;
            return;
        }
    };

    // Submit transaction
    match client.submit_transaction(&tx).await {
        Ok(_response) => {
            let latency = start.elapsed();
            metrics.record_success(latency).await;
        }
        Err(e) => {
            let error_msg = format!("{}", e);
            metrics.record_failure(error_msg).await;
        }
    }
}
