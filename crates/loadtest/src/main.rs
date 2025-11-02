mod client;
mod config;
mod generator;
mod metrics;
mod runner;
mod scenarios;
mod setup;
mod traits;
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
    tracing::info!("  Duration: {}s, TPS Target: {}", config.duration_secs, config.target_tps);

    let results = runner::run_load_test(config).await?;
    results.log_summary();

    Ok(())
}
