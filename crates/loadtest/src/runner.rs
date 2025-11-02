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

/// Send a single transaction
async fn send_transaction(
    client: &RpcClient,
    wallet: &Wallet,
    config: &LoadTestConfig,
    metrics: &MetricsCollector,
) {
    let start = Instant::now();

    let result = match generator::generate_transaction(
        wallet,
        config.tx_type,
        config.market_id,
        config.asset_id,
    ) {
        Ok(tx) => client.submit_transaction(&tx).await,
        Err(e) => Err(e),
    };

    match result {
        Ok(_) => metrics.record_success(start.elapsed()).await,
        Err(e) => metrics.record_failure(e.to_string()).await,
    }
}

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

    tracing::info!("💰 Generating {} wallets...", config.num_wallets);
    let wallets = (0..config.num_wallets)
        .map(|_| Arc::new(Wallet::new_random()))
        .collect::<Vec<_>>();
    tracing::info!("  ✓ Wallets generated");

    if config.operator_mode {
        setup_accounts(&client, &wallets, config.asset_id, config.initial_balance).await?;
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
    let delay_per_worker =
        Duration::from_secs_f64(config.num_workers as f64 / config.target_tps as f64);

    let handles = (0..config.num_workers)
        .map(|worker_id| {
            let (client, wallets, metrics, config) = (
                client.clone(),
                wallets.clone(),
                metrics.clone(),
                config.clone(),
            );
            let wallet = wallets[worker_id % wallets.len()].clone();

            tokio::spawn(async move {
                let mut last_request = Instant::now();
                while start.elapsed() < duration {
                    if let Some(wait_time) =
                        (last_request + delay_per_worker).checked_duration_since(Instant::now())
                    {
                        sleep(wait_time).await;
                    }
                    last_request = Instant::now();
                    send_transaction(&client, &wallet, &config, &metrics).await;
                }
            })
        })
        .collect::<Vec<_>>();

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

    let handles = (0..config.num_workers)
        .map(|worker_id| {
            let (client, wallets, metrics, config) = (
                client.clone(),
                wallets.clone(),
                metrics.clone(),
                config.clone(),
            );
            let wallet = wallets[worker_id % wallets.len()].clone();

            tokio::spawn(async move {
                while start.elapsed() < duration {
                    let elapsed = start.elapsed();
                    let current_tps = if elapsed < ramp_duration {
                        (config.target_tps as f64 * elapsed.as_secs_f64()
                            / ramp_duration.as_secs_f64())
                        .max(1.0)
                    } else {
                        config.target_tps as f64
                    };

                    let delay = Duration::from_secs_f64(config.num_workers as f64 / current_tps);
                    send_transaction(&client, &wallet, &config, &metrics).await;
                    sleep(delay).await;
                }
            })
        })
        .collect::<Vec<_>>();

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
            tracing::info!(
                "{}",
                if in_burst {
                    "💥 BURST START"
                } else {
                    "💤 BURST END - cooling down"
                }
            );
            next_burst = now
                + if in_burst {
                    burst_duration
                } else {
                    burst_interval - burst_duration
                };
        }

        if in_burst {
            let handles = (0..config.num_workers)
                .map(|worker_id| {
                    let (client, wallets, metrics, config) = (
                        client.clone(),
                        wallets.clone(),
                        metrics.clone(),
                        config.clone(),
                    );
                    let wallet = wallets[worker_id % wallets.len()].clone();
                    tokio::spawn(async move {
                        send_transaction(&client, &wallet, &config, &metrics).await;
                    })
                })
                .collect::<Vec<_>>();

            for handle in handles {
                let _ = handle.await;
            }
            sleep(Duration::from_millis(10)).await;
        } else {
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

    let handles = (0..config.num_workers)
        .map(|worker_id| {
            let (client, wallets, metrics, config) = (
                client.clone(),
                wallets.clone(),
                metrics.clone(),
                config.clone(),
            );
            let wallet = wallets[worker_id % wallets.len()].clone();

            tokio::spawn(async move {
                while start.elapsed() < duration {
                    send_transaction(&client, &wallet, &config, &metrics).await;
                    tokio::task::yield_now().await;
                }
            })
        })
        .collect::<Vec<_>>();

    for handle in handles {
        let _ = handle.await;
    }

    Ok(())
}

/// Setup accounts with initial balances
async fn setup_accounts(
    client: &Arc<RpcClient>,
    wallets: &[Arc<Wallet>],
    asset_id: u32,
    initial_balance: u128,
) -> Result<()> {
    tracing::info!("\n🔧 PHASE 1: Account Initialization");
    let setup = AccountSetup::new(client.clone());

    tracing::info!(
        "⚠️  Bridge operator address: {:?}",
        setup.operator_address()
    );
    tracing::info!("   Make sure this address is authorized as a bridge operator on the server!");
    tracing::info!("   Waiting 3 seconds before proceeding...");
    tokio::time::sleep(Duration::from_secs(3)).await;

    setup
        .initialize_wallets(wallets, asset_id, initial_balance)
        .await?;

    let verified = setup
        .verify_balances(wallets, asset_id, initial_balance)
        .await?;
    if verified < 5 {
        tracing::warn!(
            "⚠️  Only {}/10 wallets verified with correct balance",
            verified
        );
        tracing::warn!("   Continuing anyway, but results may be affected...");
    }

    Ok(())
}
