use tonic::transport::Server;
use tower::ServiceBuilder;
use tracing_subscriber::EnvFilter;

use crate::config::SnapshotConfig;

/// Initialize tracing subscriber
pub fn init_tracing(debug: bool) {
    let filter = if debug {
        // In debug mode, default to "debug" but allow RUST_LOG override
        EnvFilter::try_from_default_env()
            .or_else(|_| EnvFilter::try_new("debug"))
            .unwrap()
    } else {
        // In normal mode, default to "info" but allow RUST_LOG override
        EnvFilter::try_from_default_env()
            .or_else(|_| EnvFilter::try_new("info"))
            .unwrap()
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}

/// Start Pranklin daemon
#[allow(clippy::too_many_arguments)]
pub async fn start_server(
    grpc_addr: &str,
    rpc_addr: &str,
    db_path: &str,
    chain_id: &str,
    bridge_operators: &[String],
    snapshot_enable: bool,
    snapshot_interval: u64,
    snapshot: SnapshotConfig,
) -> anyhow::Result<()> {
    tracing::info!("🚀 Starting Pranklin daemon");
    tracing::info!("  gRPC: {}", grpc_addr);
    tracing::info!("  RPC:  {}", rpc_addr);
    tracing::info!("  DB:   {}", db_path);

    // Parse snapshot config if enabled
    let snapshot_config = if snapshot_enable {
        let provider = snapshot.to_provider()?;

        tracing::info!("  📸 Auto-snapshot: every {} blocks", snapshot_interval);
        match &provider {
            pranklin_state::CloudProvider::S3(cfg) => {
                tracing::info!("  📤 Storage: S3 → s3://{}/{}", cfg.bucket, cfg.prefix);
            }
            pranklin_state::CloudProvider::GCS(cfg) => {
                tracing::info!("  📤 Storage: GCS → gs://{}/{}", cfg.bucket, cfg.prefix);
            }
            pranklin_state::CloudProvider::Local { path } => {
                tracing::info!("  📤 Storage: Local → {:?}", path);
            }
        }

        Some(pranklin_state::SnapshotExporterConfig {
            provider,
            auto_export_interval: snapshot_interval,
            chain_id: chain_id.to_string(),
        })
    } else {
        None
    };

    // Create executor service
    let (grpc_server, executor_service) = if let Some(config) = snapshot_config {
        pranklin_exec::new_server_with_components_and_snapshots(db_path, config)
    } else {
        pranklin_exec::new_server_with_components(db_path)
    };

    // Initialize default assets
    tracing::info!("📦 Initializing default assets...");
    executor_service
        .initialize_assets()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to initialize assets: {}", e))?;
    tracing::info!("  ✓ Assets initialized: USDC, USDT, DAI");

    // Initialize bridge operators if provided
    if !bridge_operators.is_empty() {
        tracing::info!("🌉 Initializing bridge operators...");

        let operators: Result<Vec<alloy_primitives::Address>, _> =
            bridge_operators.iter().map(|s| s.parse()).collect();

        let operators = operators?;

        executor_service
            .initialize_bridge_operators(&operators)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to initialize bridge operators: {}", e))?;

        tracing::info!("  ✓ {} bridge operator(s) authorized", operators.len());
        for (i, op) in operators.iter().enumerate() {
            tracing::info!("    {}. {:?}", i + 1, op);
        }
    } else {
        tracing::warn!(
            "⚠️  No bridge operators configured. Bridge functionality will be disabled."
        );
        tracing::warn!("    Use --bridge.operators=<addr1>,<addr2>,... to enable bridge.");
    }

    // Get components for RPC
    let (auth, mempool, engine) = executor_service.get_components();
    let rpc_state = pranklin_rpc::RpcState::new_from_shared(auth, mempool, engine);

    // Parse addresses
    let grpc_addr_parsed = grpc_addr.parse()?;
    let rpc_addr_str = rpc_addr.to_string();

    // Spawn gRPC server (internal communication with Rollkit sequencer)
    // No rate limiting needed - this is trusted internal traffic
    let grpc_handle = tokio::spawn(async move {
        Server::builder()
            .layer(
                ServiceBuilder::new()
                    // Convert request headers: Connect-RPC → gRPC
                    .map_request(|mut req: http::Request<_>| {
                        if let Some(content_type) = req.headers().get("content-type")
                            && let Ok(ct_str) = content_type.to_str()
                            && (ct_str.contains("application/proto")
                                || ct_str.contains("application/connect+proto"))
                        {
                            tracing::debug!("Request: Connect-RPC → gRPC");
                            req.headers_mut().insert(
                                "content-type",
                                http::HeaderValue::from_static("application/grpc"),
                            );
                        }
                        req
                    })
                    // Convert response headers: gRPC → Connect-RPC
                    .map_response(|mut res: http::Response<_>| {
                        if let Some(content_type) = res.headers().get("content-type")
                            && let Ok(ct_str) = content_type.to_str()
                            && ct_str.contains("application/grpc")
                        {
                            tracing::debug!("Response: gRPC → Connect-RPC");
                            res.headers_mut().insert(
                                "content-type",
                                http::HeaderValue::from_static("application/proto"),
                            );
                        }
                        res
                    })
                    .into_inner(),
            )
            .add_service(grpc_server)
            .serve(grpc_addr_parsed)
            .await
    });

    let rpc_handle =
        tokio::spawn(async move { pranklin_rpc::start_server(rpc_state, &rpc_addr_str).await });

    tracing::info!("✅ Pranklin daemon started");
    tracing::info!("Press Ctrl+C to stop");

    // Wait for either server to exit
    tokio::select! {
        result = grpc_handle => {
            match result {
                Ok(Ok(_)) => tracing::info!("gRPC server stopped"),
                Ok(Err(e)) => tracing::error!("gRPC server error: {}", e),
                Err(e) => tracing::error!("gRPC server task error: {}", e),
            }
        }
        result = rpc_handle => {
            match result {
                Ok(Ok(_)) => tracing::info!("RPC server stopped"),
                Ok(Err(e)) => tracing::error!("RPC server error: {}", e),
                Err(e) => tracing::error!("RPC server task error: {}", e),
            }
        }
    }

    Ok(())
}
