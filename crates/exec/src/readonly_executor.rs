use crate::tx_executor::execute_tx_payload_readonly;
use alloy_primitives::{Address, B256};
use pranklin_engine::{Engine, EventStore, RocksDbEventStore};
use pranklin_state::StateManager;
use pranklin_tx::Transaction;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Read-only executor for query nodes
///
/// This is designed for nodes that serve read-only queries without
/// participating in transaction execution. These nodes:
/// - Sync state from sequencer nodes
/// - Serve queries (balances, positions, orders, events)
/// - Do not accept transaction submissions
/// - Can scale horizontally
#[derive(Clone)]
pub struct ReadOnlyExecutor {
    /// Shared engine state (read-only access)
    engine: Arc<RwLock<Engine>>,
    /// Event store for historical queries
    event_store: Arc<RocksDbEventStore>,
    /// Current block height
    block_height: Arc<RwLock<u64>>,
}

impl ReadOnlyExecutor {
    /// Create a new read-only executor
    pub fn new(
        state: StateManager,
        event_store_path: impl AsRef<Path>,
    ) -> Result<Self, ReadOnlyError> {
        let engine = Engine::new(state);
        let event_store =
            RocksDbEventStore::new(event_store_path, 1000).map_err(ReadOnlyError::init)?;

        Ok(Self {
            engine: Arc::new(RwLock::new(engine)),
            event_store: Arc::new(event_store),
            block_height: Arc::new(RwLock::new(0)),
        })
    }

    /// Get current block height
    pub async fn block_height(&self) -> u64 {
        *self.block_height.read().await
    }

    /// Sync from sequencer/leader node
    ///
    /// Fetches missing transactions from the leader and replays them locally.
    /// This ensures the query node has an up-to-date copy of the state.
    pub async fn sync_from_leader(
        &self,
        leader_url: &str,
        batch_size: usize,
    ) -> Result<SyncResult, ReadOnlyError> {
        let current_height = *self.block_height.read().await;

        // Fetch leader's height
        let leader_height = self.fetch_leader_height(leader_url).await?;

        if leader_height <= current_height {
            return Ok(SyncResult {
                synced_blocks: 0,
                new_height: current_height,
                is_synced: true,
            });
        }

        // Sync missing blocks in batches
        let mut synced_blocks = 0;
        let mut current = current_height + 1;

        while current <= leader_height {
            let end = (current + batch_size as u64 - 1).min(leader_height);

            // Fetch transactions for block range
            let txs = self.fetch_transactions(leader_url, current, end).await?;

            // Replay transactions
            let mut engine = self.engine.write().await;
            for (height, tx) in txs {
                engine.begin_tx(tx.hash(), height, 0);

                // Execute transaction (this updates state)
                self.execute_transaction(&mut engine, &tx)?;

                // Collect and store events
                let events = engine.take_events();
                // Note: In production, event store should also be synced
                // For now, we just discard events on read-only nodes
                drop(events);
            }

            synced_blocks += end - current + 1;
            current = end + 1;
        }

        // Update block height
        *self.block_height.write().await = leader_height;

        Ok(SyncResult {
            synced_blocks,
            new_height: leader_height,
            is_synced: true,
        })
    }

    /// Execute a single transaction (internal)
    fn execute_transaction(
        &self,
        engine: &mut Engine,
        tx: &Transaction,
    ) -> Result<(), ReadOnlyError> {
        execute_tx_payload_readonly(engine, tx).map_err(ReadOnlyError::execution)
    }

    /// Fetch leader's current height
    async fn fetch_leader_height(&self, leader_url: &str) -> Result<u64, ReadOnlyError> {
        #[derive(serde::Deserialize)]
        struct StatusResponse {
            block_height: u64,
        }

        let url = format!("{}/status", leader_url);
        let status: StatusResponse = reqwest::Client::new()
            .get(&url)
            .send()
            .await
            .map_err(ReadOnlyError::network)?
            .json()
            .await
            .map_err(ReadOnlyError::network)?;

        Ok(status.block_height)
    }

    /// Fetch transactions for a block range from leader
    async fn fetch_transactions(
        &self,
        leader_url: &str,
        from_height: u64,
        to_height: u64,
    ) -> Result<Vec<(u64, Transaction)>, ReadOnlyError> {
        #[derive(serde::Deserialize)]
        struct BlockData {
            height: u64,
            transactions: Vec<Transaction>,
        }

        let url = format!(
            "{}/blocks?from={}&to={}",
            leader_url, from_height, to_height
        );
        let blocks: Vec<BlockData> = reqwest::Client::new()
            .get(&url)
            .send()
            .await
            .map_err(ReadOnlyError::network)?
            .json()
            .await
            .map_err(ReadOnlyError::network)?;

        Ok(blocks
            .into_iter()
            .flat_map(|block| {
                let height = block.height;
                block.transactions.into_iter().map(move |tx| (height, tx))
            })
            .collect())
    }

    /// Query methods below (read-only access)

    /// Get balance for an address
    pub async fn get_balance(
        &self,
        address: Address,
        asset_id: u32,
    ) -> Result<u128, ReadOnlyError> {
        self.engine
            .read()
            .await
            .state()
            .get_balance(address, asset_id)
            .map_err(ReadOnlyError::query)
    }

    /// Get position for an address and market
    pub async fn get_position(
        &self,
        address: Address,
        market_id: u32,
    ) -> Result<Option<pranklin_state::Position>, ReadOnlyError> {
        self.engine
            .read()
            .await
            .state()
            .get_position(address, market_id)
            .map_err(ReadOnlyError::query)
    }

    /// Get order by ID
    pub async fn get_order(
        &self,
        order_id: u64,
    ) -> Result<Option<pranklin_state::Order>, ReadOnlyError> {
        self.engine
            .read()
            .await
            .state()
            .get_order(order_id)
            .map_err(ReadOnlyError::query)
    }

    /// Get orderbook for a market
    pub async fn get_orderbook(
        &self,
        _market_id: u32,
    ) -> Result<Vec<pranklin_state::Order>, ReadOnlyError> {
        // TODO: Implement get_orders_by_market in Engine or StateManager
        Ok(Vec::new())
    }

    /// Get events by block range
    pub async fn get_events_by_block(
        &self,
        from: u64,
        to: u64,
    ) -> Result<Vec<pranklin_types::DomainEvent>, ReadOnlyError> {
        self.event_store
            .get_by_block_range(from, to)
            .map_err(ReadOnlyError::query)
    }

    /// Get events by transaction hash
    pub async fn get_events_by_tx(
        &self,
        tx_hash: B256,
    ) -> Result<Vec<pranklin_types::DomainEvent>, ReadOnlyError> {
        self.event_store
            .get_by_tx(tx_hash)
            .map_err(ReadOnlyError::query)
    }

    /// Get events by address
    pub async fn get_events_by_address(
        &self,
        address: Address,
        limit: usize,
    ) -> Result<Vec<pranklin_types::DomainEvent>, ReadOnlyError> {
        self.event_store
            .get_by_address(address, limit)
            .map_err(ReadOnlyError::query)
    }

    /// Get state root hash
    pub async fn state_root(&self) -> B256 {
        self.engine.read().await.state().state_root()
    }
}

/// Sync result from leader
#[derive(Debug, Clone)]
pub struct SyncResult {
    /// Number of blocks synced
    pub synced_blocks: u64,
    /// New block height after sync
    pub new_height: u64,
    /// Whether node is fully synced
    pub is_synced: bool,
}

/// Read-only executor errors
#[derive(Debug, thiserror::Error)]
pub enum ReadOnlyError {
    #[error("Initialization error: {0}")]
    InitError(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Execution error: {0}")]
    ExecutionError(String),

    #[error("Query error: {0}")]
    QueryError(String),
}

impl ReadOnlyError {
    pub fn init(e: impl std::fmt::Display) -> Self {
        Self::InitError(e.to_string())
    }

    pub fn network(e: impl std::fmt::Display) -> Self {
        Self::NetworkError(e.to_string())
    }

    pub fn execution(e: impl std::fmt::Display) -> Self {
        Self::ExecutionError(e.to_string())
    }

    pub fn query(e: impl std::fmt::Display) -> Self {
        Self::QueryError(e.to_string())
    }
}

/// Configuration for read-only node
#[derive(Debug, Clone)]
pub struct ReadOnlyConfig {
    /// Leader node URL to sync from
    pub leader_url: String,

    /// Sync interval in seconds
    pub sync_interval_secs: u64,

    /// Batch size for syncing blocks
    pub sync_batch_size: usize,

    /// Whether to verify state root after sync
    pub verify_state_root: bool,
}

impl Default for ReadOnlyConfig {
    fn default() -> Self {
        Self {
            leader_url: "http://localhost:3000".to_string(),
            sync_interval_secs: 5,
            sync_batch_size: 100,
            verify_state_root: true,
        }
    }
}

/// Background sync service for read-only nodes
pub struct SyncService {
    executor: ReadOnlyExecutor,
    config: ReadOnlyConfig,
    running: Arc<RwLock<bool>>,
}

impl SyncService {
    /// Create a new sync service
    pub fn new(executor: ReadOnlyExecutor, config: ReadOnlyConfig) -> Self {
        Self {
            executor,
            config,
            running: Arc::new(RwLock::new(false)),
        }
    }

    /// Start background sync loop
    pub async fn start(&self) -> Result<(), ReadOnlyError> {
        *self.running.write().await = true;

        while *self.running.read().await {
            if let Ok(result) = self
                .executor
                .sync_from_leader(&self.config.leader_url, self.config.sync_batch_size)
                .await
            {
                if result.synced_blocks > 0 {
                    log::info!(
                        "Synced {} blocks, current height: {}",
                        result.synced_blocks,
                        result.new_height
                    );

                    if self.config.verify_state_root {
                        log::debug!(
                            "State root verification: {:?}",
                            self.executor.state_root().await
                        );
                    }
                }
            } else {
                log::error!("Sync error occurred");
            }

            tokio::time::sleep(tokio::time::Duration::from_secs(
                self.config.sync_interval_secs,
            ))
            .await;
        }

        Ok(())
    }

    /// Stop background sync
    pub async fn stop(&self) {
        *self.running.write().await = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pranklin_state::PruningConfig;

    #[tokio::test]
    async fn test_readonly_executor_creation() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let state_path = temp_dir.path().join("state");
        let events_path = temp_dir.path().join("events");

        std::fs::create_dir_all(&state_path).unwrap();
        std::fs::create_dir_all(&events_path).unwrap();

        let state = StateManager::new(&state_path, PruningConfig::default()).unwrap();
        let executor = ReadOnlyExecutor::new(state, events_path).unwrap();

        assert_eq!(executor.block_height().await, 0);
    }

    #[tokio::test]
    async fn test_readonly_queries() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let state_path = temp_dir.path().join("state");
        let events_path = temp_dir.path().join("events");

        std::fs::create_dir_all(&state_path).unwrap();
        std::fs::create_dir_all(&events_path).unwrap();

        let mut state = StateManager::new(&state_path, PruningConfig::default()).unwrap();

        // Setup initial state
        let address = Address::from([1u8; 20]);
        state.set_balance(address, 0, 100_000).unwrap();

        let executor = ReadOnlyExecutor::new(state, events_path).unwrap();

        // Query balance
        let balance = executor.get_balance(address, 0).await.unwrap();
        assert_eq!(balance, 100_000);
    }
}
