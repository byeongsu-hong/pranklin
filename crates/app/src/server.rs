use alloy_primitives::Address;
use pranklin_exec::{PranklinExecutorService, pb::executor_service_server::ExecutorServiceServer};
use tonic::transport::Server;
use tower::ServiceBuilder;
use tracing_subscriber::EnvFilter;

use crate::config::StartConfig;

/// Log snapshot configuration
fn log_snapshot_config(interval: u64, provider: &pranklin_state::CloudProvider) {
    tracing::info!("  📸 Auto-snapshot: every {} blocks", interval);
    match provider {
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
}

/// Convert content-type header between Connect-RPC and gRPC formats
fn convert_content_type<T, F>(
    mut msg: T,
    get_headers: F,
    from_type: &str,
    to_type: String,
    log_msg: &str,
) -> T
where
    F: Fn(&mut T) -> &mut http::HeaderMap,
{
    let headers = get_headers(&mut msg);
    if let Some(content_type) = headers.get("content-type") {
        if let Ok(ct_str) = content_type.to_str() {
            if ct_str.contains(from_type) {
                tracing::debug!("{}", log_msg);
                if let Ok(header_value) = http::HeaderValue::from_str(&to_type) {
                    headers.insert("content-type", header_value);
                }
            }
        }
    }
    msg
}

/// Convert Connect-RPC content-type to gRPC
fn convert_connect_to_grpc<T>(req: http::Request<T>) -> http::Request<T> {
    convert_content_type(
        req,
        |r| r.headers_mut(),
        "application/proto",
        "application/grpc".to_string(),
        "Request: Connect-RPC → gRPC",
    )
}

/// Convert gRPC content-type to Connect-RPC
fn convert_grpc_to_connect<T>(res: http::Response<T>) -> http::Response<T> {
    convert_content_type(
        res,
        |r| r.headers_mut(),
        "application/grpc",
        "application/proto".to_string(),
        "Response: gRPC → Connect-RPC",
    )
}

/// Initialize tracing subscriber
pub fn init_tracing(debug: bool) {
    let default_level = if debug { "debug" } else { "info" };
    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(default_level))
        .unwrap();

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}

/// Start Pranklin daemon
pub async fn start_server(config: &StartConfig) -> anyhow::Result<()> {
    log_startup_info(config);

    let snapshot_config = config.snapshot_exporter_config();
    if let Some(ref cfg) = snapshot_config {
        log_snapshot_config(cfg.auto_export_interval, &cfg.provider);
    }

    let (grpc_server, executor_service) = match snapshot_config {
        Some(cfg) => pranklin_exec::new_server_with_components_and_snapshots(&config.db_path, cfg),
        None => pranklin_exec::new_server_with_components(&config.db_path),
    };

    initialize_assets(&executor_service).await?;
    initialize_bridge_operators(&executor_service, config).await?;

    let (auth, mempool, engine) = executor_service.get_components();
    let rpc_state = pranklin_rpc::RpcState::new_from_shared(auth, mempool, engine);

    spawn_servers(grpc_server, rpc_state, &config.grpc_addr, &config.rpc_addr).await
}

fn log_startup_info(config: &StartConfig) {
    tracing::info!("🚀 Starting Pranklin daemon");
    tracing::info!("  gRPC: {}", config.grpc_addr);
    tracing::info!("  RPC:  {}", config.rpc_addr);
    tracing::info!("  DB:   {}", config.db_path);
}

async fn initialize_assets(executor_service: &PranklinExecutorService) -> anyhow::Result<()> {
    tracing::info!("📦 Initializing default assets...");
    executor_service
        .initialize_assets()
        .await
        .inspect(|_| tracing::info!("  ✓ Assets initialized: USDC, USDT, DAI"))
        .map_err(|e| anyhow::anyhow!("Failed to initialize assets: {}", e))
}

async fn initialize_bridge_operators(
    executor_service: &PranklinExecutorService,
    config: &StartConfig,
) -> anyhow::Result<()> {
    if !config.has_bridge_operators() {
        tracing::warn!(
            "⚠️  No bridge operators configured. Bridge functionality will be disabled."
        );
        tracing::warn!("    Use --bridge.operators=<addr1>,<addr2>,... to enable bridge.");
        return Ok(());
    }

    tracing::info!("🌉 Initializing bridge operators...");
    let operators = config.parse_bridge_operators()?;

    executor_service
        .initialize_bridge_operators(&operators)
        .await
        .inspect(|_| log_operators(&operators))
        .map_err(|e| anyhow::anyhow!("Failed to initialize bridge operators: {}", e))
}

fn log_operators(operators: &[Address]) {
    tracing::info!("  ✓ {} bridge operator(s) authorized", operators.len());
    operators.iter().enumerate().for_each(|(i, op)| {
        tracing::info!("    {}. {:?}", i + 1, op);
    });
}

async fn spawn_servers(
    grpc_server: ExecutorServiceServer<PranklinExecutorService>,
    rpc_state: pranklin_rpc::RpcState,
    grpc_addr: &str,
    rpc_addr: &str,
) -> anyhow::Result<()> {
    let grpc_addr_parsed = grpc_addr.parse()?;
    let rpc_addr_owned = rpc_addr.to_string();

    let grpc_handle = tokio::spawn(async move {
        Server::builder()
            .layer(
                ServiceBuilder::new()
                    .map_request(convert_connect_to_grpc)
                    .map_response(convert_grpc_to_connect)
                    .into_inner(),
            )
            .add_service(grpc_server)
            .serve(grpc_addr_parsed)
            .await
    });

    let rpc_handle =
        tokio::spawn(async move { pranklin_rpc::start_server(rpc_state, &rpc_addr_owned).await });

    tracing::info!("✅ Pranklin daemon started");
    tracing::info!("Press Ctrl+C to stop");

    tokio::select! {
        result = grpc_handle => match result {
            Ok(Ok(_)) => tracing::info!("gRPC server stopped"),
            Ok(Err(e)) => tracing::error!("gRPC server error: {}", e),
            Err(e) => tracing::error!("gRPC server task error: {}", e),
        },
        result = rpc_handle => match result {
            Ok(Ok(_)) => tracing::info!("RPC server stopped"),
            Ok(Err(e)) => tracing::error!("RPC server error: {}", e),
            Err(e) => tracing::error!("RPC server task error: {}", e),
        },
    }

    Ok(())
}
