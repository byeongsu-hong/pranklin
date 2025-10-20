mod client;
mod config;
mod generator;
mod metrics;
mod runner;
mod scenarios;
mod setup;
mod wallet;

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    // Parse CLI arguments
    let config = config::LoadTestConfig::parse();

    tracing::info!("🚀 Starting Pranklin Load Test");
    tracing::info!("  Target: {}", config.rpc_url);
    tracing::info!("  Mode: {:?} ({} workers)", config.mode, config.num_workers);
    tracing::info!(
        "  Duration: {}s, TPS Target: {}",
        config.duration_secs,
        config.target_tps
    );

    // Run the load test
    let results = runner::run_load_test(config).await?;

    // Display results
    tracing::info!("\n📊 Load Test Results:");
    tracing::info!("  Total Requests: {}", results.total_requests);
    tracing::info!("  Successful: {}", results.successful_requests);
    tracing::info!("  Failed: {}", results.failed_requests);
    tracing::info!(
        "  Success Rate: {:.2}%",
        (results.successful_requests as f64 / results.total_requests as f64) * 100.0
    );
    tracing::info!("  Duration: {:.2}s", results.duration_secs);
    tracing::info!(
        "  Actual TPS: {:.2}",
        results.total_requests as f64 / results.duration_secs
    );

    tracing::info!("\n⏱️  Latency Statistics (ms):");
    tracing::info!("  Min: {:.2}", results.latency_min_ms);
    tracing::info!("  Max: {:.2}", results.latency_max_ms);
    tracing::info!("  Mean: {:.2}", results.latency_mean_ms);
    tracing::info!("  P50: {:.2}", results.latency_p50_ms);
    tracing::info!("  P95: {:.2}", results.latency_p95_ms);
    tracing::info!("  P99: {:.2}", results.latency_p99_ms);
    tracing::info!("  P99.9: {:.2}", results.latency_p999_ms);

    if !results.errors.is_empty() {
        tracing::warn!("\n⚠️  Errors encountered:");
        for (error, count) in results.errors.iter().take(10) {
            tracing::warn!("  [{}x] {}", count, error);
        }
        if results.errors.len() > 10 {
            tracing::warn!("  ... and {} more", results.errors.len() - 10);
        }
    }

    Ok(())
}
