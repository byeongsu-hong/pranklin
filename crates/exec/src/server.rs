use crate::constants::*;
// Error types are not needed here as we use std::result::Result
use crate::proto::pb::{
    ExecuteTxsRequest, ExecuteTxsResponse, GetTxsRequest, GetTxsResponse, InitChainRequest,
    InitChainResponse, SetFinalRequest, SetFinalResponse,
    executor_service_server::{ExecutorService, ExecutorServiceServer},
};
use crate::tx_executor::execute_tx_batch;
use pranklin_auth::AuthService;
use pranklin_engine::Engine;
use pranklin_mempool::Mempool;
use pranklin_state::{SnapshotExporter, StateManager};
use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::{Request, Status};

type RpcResponse<T> = std::result::Result<tonic::Response<T>, Status>;

/// Shared components for RPC/gRPC servers
pub type SharedComponents = (
    Arc<RwLock<AuthService>>,
    Arc<RwLock<Mempool>>,
    Arc<RwLock<Engine>>,
);

/// Get default assets configuration
fn default_assets() -> Vec<pranklin_state::Asset> {
    vec![
        pranklin_state::Asset {
            id: 0,
            symbol: "USDC".to_string(),
            name: "USD Coin".to_string(),
            decimals: 6,
            is_collateral: true,
            collateral_weight_bps: 10000, // 100%
        },
        pranklin_state::Asset {
            id: 1,
            symbol: "USDT".to_string(),
            name: "Tether USD".to_string(),
            decimals: 6,
            is_collateral: true,
            collateral_weight_bps: 9800, // 98%
        },
        pranklin_state::Asset {
            id: 2,
            symbol: "DAI".to_string(),
            name: "Dai Stablecoin".to_string(),
            decimals: 18,
            is_collateral: true,
            collateral_weight_bps: 9500, // 95%
        },
    ]
}

/// Pranklin executor service implementation
#[derive(Clone)]
pub struct PranklinExecutorService {
    /// Authentication service
    auth: Arc<RwLock<AuthService>>,
    /// Mempool
    mempool: Arc<RwLock<Mempool>>,
    /// Engine
    engine: Arc<RwLock<Engine>>,
    /// Max bytes per block
    max_bytes: u64,
    /// Database path (for snapshots)
    db_path: String,
    /// Optional snapshot exporter
    snapshot_exporter: Option<Arc<RwLock<SnapshotExporter>>>,
}

impl PranklinExecutorService {
    /// Create executor service with optional snapshot exporter
    pub fn new(
        db_path: impl Into<String>,
        snapshot_config: Option<pranklin_state::SnapshotExporterConfig>,
    ) -> Self {
        let db_path = db_path.into();
        let pruning_config = pranklin_state::PruningConfig::default();
        let state =
            StateManager::new(&db_path, pruning_config).expect("Failed to initialize StateManager");
        let engine = Engine::new(state);

        // Initialize snapshot exporter if configured
        let snapshot_exporter = snapshot_config.map(|config| {
            let exporter = SnapshotExporter::new(config);
            Arc::new(RwLock::new(exporter))
        });

        Self {
            auth: Arc::new(RwLock::new(AuthService::new())),
            mempool: Arc::new(RwLock::new(Mempool::default())),
            engine: Arc::new(RwLock::new(engine)),
            max_bytes: DEFAULT_MAX_BYTES,
            db_path,
            snapshot_exporter,
        }
    }

    /// Get references for RPC server
    pub fn get_components(&self) -> SharedComponents {
        (self.auth.clone(), self.mempool.clone(), self.engine.clone())
    }

    /// Initialize default assets in the system
    pub async fn initialize_assets(&self) -> std::result::Result<(), String> {
        let mut engine = self.engine.write().await;
        let state = engine.state_mut();
        default_assets()
            .into_iter()
            .try_for_each(|asset| state.set_asset(asset.id, asset).map_err(|e| e.to_string()))
    }

    /// Initialize bridge operators
    pub async fn initialize_bridge_operators(
        &self,
        operators: &[alloy_primitives::Address],
    ) -> std::result::Result<(), String> {
        let mut engine = self.engine.write().await;
        let state = engine.state_mut();
        operators.iter().try_for_each(|op| {
            state
                .set_bridge_operator(*op, true)
                .map_err(|e| e.to_string())
        })
    }

    /// Validate chain initialization request
    fn validate_init_chain(&self, req: &InitChainRequest) -> std::result::Result<u64, Status> {
        if req.chain_id.is_empty() {
            return Err(Status::invalid_argument("Chain ID is required"));
        }

        Ok(match req.initial_height {
            0 => {
                tracing::warn!(
                    "Initial height is 0, using default height of {}",
                    GENESIS_HEIGHT
                );
                GENESIS_HEIGHT
            }
            height => height,
        })
    }

    /// Get transactions from mempool with size limit
    async fn fetch_txs_from_mempool(&self) -> Vec<Vec<u8>> {
        let ready_txs = self.mempool.read().await.ready_txs(MAX_TXS_PER_BLOCK);
        tracing::debug!("Retrieved {} transactions from mempool", ready_txs.len());

        let (txs, total_size) = ready_txs
            .into_iter()
            .map(|tx| tx.encode())
            .scan(0u64, |total, encoded| {
                let tx_size = encoded.len() as u64;
                if *total + tx_size > self.max_bytes {
                    None
                } else {
                    *total += tx_size;
                    Some((encoded, *total))
                }
            })
            .fold((Vec::new(), 0u64), |(mut txs, _), (encoded, total)| {
                txs.push(encoded);
                (txs, total)
            });

        tracing::debug!(
            "Returning {} transactions ({} bytes)",
            txs.len(),
            total_size
        );
        txs
    }

    /// Handle snapshot export if configured
    async fn handle_snapshot_export(&self, block_height: u64, engine: &Engine) {
        if let Some(exporter) = &self.snapshot_exporter {
            let exporter_guard = exporter.write().await;
            if exporter_guard.should_export(block_height) {
                tracing::info!("Triggering auto-snapshot at height {}", block_height);

                // Spawn snapshot export in background
                let storage = engine.state().storage();
                let db_path = self.db_path.clone();
                let height = block_height;
                let exporter_clone = Arc::new(RwLock::new(exporter_guard.clone()));

                tokio::spawn(async move {
                    let mut exp = exporter_clone.write().await;
                    match exp
                        .export_snapshot(&storage, std::path::Path::new(&db_path), height)
                        .await
                    {
                        Ok(metadata) => {
                            tracing::info!(
                                "✅ Snapshot exported at height {}: {:.2} MB",
                                metadata.height,
                                metadata.snapshot_size as f64 / 1_000_000.0
                            );
                        }
                        Err(e) => {
                            tracing::error!("❌ Failed to export snapshot: {}", e);
                        }
                    }
                });
            }
        }
    }
}

#[tonic::async_trait]
impl ExecutorService for PranklinExecutorService {
    async fn init_chain(&self, req: Request<InitChainRequest>) -> RpcResponse<InitChainResponse> {
        let req = req.into_inner();

        tracing::info!("Initializing chain: {}", req.chain_id);
        tracing::info!("Initial height: {}", req.initial_height);

        // Validate request
        let height = self.validate_init_chain(&req)?;

        // Initialize the state at genesis
        let mut engine = self.engine.write().await;
        engine.state_mut().begin_block(height);
        tracing::debug!("Block {} initialized", height);

        // Commit genesis state
        let state_root = engine
            .state_mut()
            .commit()
            .map_err(|e| Status::internal(format!("Failed to commit genesis state: {}", e)))?;

        tracing::info!("Chain initialized with state root: {:?}", state_root);

        Ok(tonic::Response::new(InitChainResponse {
            state_root: state_root.as_slice().to_vec(),
            max_bytes: self.max_bytes,
        }))
    }

    async fn get_txs(&self, _req: Request<GetTxsRequest>) -> RpcResponse<GetTxsResponse> {
        let txs = self.fetch_txs_from_mempool().await;
        Ok(tonic::Response::new(GetTxsResponse { txs }))
    }

    async fn execute_txs(
        &self,
        req: Request<ExecuteTxsRequest>,
    ) -> RpcResponse<ExecuteTxsResponse> {
        let req = req.into_inner();

        tracing::info!("Executing block at height: {}", req.block_height);

        let mut engine = self.engine.write().await;
        let mut auth = self.auth.write().await;
        let mut mempool = self.mempool.write().await;

        // Begin new block
        engine.state_mut().begin_block(req.block_height);

        // Execute all transactions
        let result = execute_tx_batch(
            &req.txs,
            &mut engine,
            &mut auth,
            &mut mempool,
            req.block_height,
        );

        // Commit state
        let state_root = engine
            .state_mut()
            .commit()
            .map_err(|e| Status::internal(format!("Failed to commit state: {}", e)))?;

        tracing::info!(
            "Block {} committed: state_root={:?}, successful_txs={}, failed_txs={}",
            req.block_height,
            state_root,
            result.successful,
            result.failed
        );

        // Handle snapshot export
        self.handle_snapshot_export(req.block_height, &engine).await;

        Ok(tonic::Response::new(ExecuteTxsResponse {
            updated_state_root: state_root.as_slice().to_vec(),
            max_bytes: self.max_bytes,
        }))
    }

    async fn set_final(&self, req: Request<SetFinalRequest>) -> RpcResponse<SetFinalResponse> {
        let req = req.into_inner();
        tracing::info!("Finalizing block at height: {}", req.block_height);

        if req.block_height == 0 {
            tracing::warn!("Attempt to finalize block 0");
        } else {
            tracing::debug!("Block {} finalized successfully", req.block_height);
        }

        // TODO: Implement finalization logic:
        // - Mark block as finalized, prune old state, update finality-dependent logic

        Ok(tonic::Response::new(SetFinalResponse {}))
    }
}

/// Create a new executor service with access to its components
pub fn new_server_with_components(
    db_path: impl Into<String>,
) -> (
    ExecutorServiceServer<PranklinExecutorService>,
    PranklinExecutorService,
) {
    let service = PranklinExecutorService::new(db_path, None);
    let server = ExecutorServiceServer::new(service.clone());
    (server, service)
}

/// Create a new executor service with snapshot support and access to components
pub fn new_server_with_components_and_snapshots(
    db_path: impl Into<String>,
    snapshot_config: pranklin_state::SnapshotExporterConfig,
) -> (
    ExecutorServiceServer<PranklinExecutorService>,
    PranklinExecutorService,
) {
    let service = PranklinExecutorService::new(db_path, Some(snapshot_config));
    let server = ExecutorServiceServer::new(service.clone());
    (server, service)
}
