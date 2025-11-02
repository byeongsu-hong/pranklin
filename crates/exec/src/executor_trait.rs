use alloy_primitives::B256;
use pranklin_engine::EngineError;
use pranklin_tx::Transaction;
use pranklin_types::TxReceipt;

/// Trait for transaction execution strategies
///
/// This trait abstracts different execution strategies:
/// - Sequential: Execute transactions one by one (current implementation)
/// - Parallel: Execute independent transactions concurrently (Block-STM)
/// - Validation-only: Verify transactions without state changes
///
/// This enables flexibility in choosing execution strategy based on:
/// - Workload characteristics (high/low conflict)
/// - Hardware capabilities (CPU cores, memory)
/// - Node type (sequencer vs validator vs query node)
pub trait TxExecutor: Send + Sync {
    /// Execute a single transaction
    ///
    /// This is the core method that processes a transaction and updates state.
    /// Returns a receipt with execution details or an error.
    fn execute_tx(&mut self, tx: Transaction) -> Result<TxReceipt, EngineError>;

    /// Execute multiple transactions in a batch
    ///
    /// The default implementation executes transactions sequentially,
    /// but parallel executors can override this to execute concurrently.
    ///
    /// Returns results in the same order as input transactions.
    fn execute_batch(&mut self, txs: Vec<Transaction>) -> Vec<Result<TxReceipt, EngineError>> {
        txs.into_iter().map(|tx| self.execute_tx(tx)).collect()
    }

    /// Validate a transaction without executing it
    ///
    /// Performs all checks (signature, nonce, balance) without modifying state.
    /// Useful for:
    /// - Mempool validation
    /// - Dry-run simulation
    /// - Query nodes
    fn validate_tx(&self, tx: &Transaction) -> Result<(), EngineError>;

    /// Get current block height
    fn block_height(&self) -> u64;

    /// Begin a new block
    ///
    /// Called before executing transactions in a block.
    /// Allows executors to prepare for batch execution.
    fn begin_block(&mut self, height: u64, timestamp: u64);

    /// End the current block
    ///
    /// Called after all transactions in a block are executed.
    /// Allows executors to finalize state, flush events, etc.
    fn end_block(&mut self) -> Result<B256, EngineError>;
}

/// Execution statistics for monitoring
#[derive(Debug, Clone, Default)]
pub struct ExecutionStats {
    /// Total transactions executed
    pub total_txs: u64,
    /// Successful transactions
    pub successful_txs: u64,
    /// Failed transactions
    pub failed_txs: u64,
    /// Total execution time (microseconds)
    pub total_time_us: u64,
    /// Average execution time per transaction (microseconds)
    pub avg_time_us: u64,
}

impl ExecutionStats {
    /// Record a transaction execution
    pub fn record(&mut self, success: bool, time_us: u64) {
        self.total_txs += 1;
        if success {
            self.successful_txs += 1;
        } else {
            self.failed_txs += 1;
        }
        self.total_time_us += time_us;
        self.avg_time_us = self.total_time_us / self.total_txs;
    }

    /// Get success rate (0.0 to 1.0)
    pub fn success_rate(&self) -> f64 {
        match self.total_txs {
            0 => 0.0,
            n => self.successful_txs as f64 / n as f64,
        }
    }

    /// Get transactions per second (requires elapsed time)
    pub fn tps(&self, elapsed_secs: f64) -> f64 {
        if elapsed_secs > 0.0 {
            self.total_txs as f64 / elapsed_secs
        } else {
            0.0
        }
    }
}

/// Execution mode for different strategies
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    /// Sequential execution (current)
    Sequential,
    /// Parallel execution with Block-STM
    Parallel {
        /// Number of worker threads
        workers: usize,
        /// Enable optimistic execution
        optimistic: bool,
    },
    /// Validation-only (no state changes)
    ValidationOnly,
}

impl Default for ExecutionMode {
    fn default() -> Self {
        ExecutionMode::Sequential
    }
}

impl ExecutionMode {
    /// Create parallel mode with default settings
    pub const fn parallel(workers: usize) -> Self {
        Self::Parallel {
            workers,
            optimistic: false,
        }
    }

    /// Create optimistic parallel mode
    pub const fn parallel_optimistic(workers: usize) -> Self {
        Self::Parallel {
            workers,
            optimistic: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_stats() {
        let mut stats = ExecutionStats::default();

        // Record some executions
        stats.record(true, 100);
        stats.record(true, 200);
        stats.record(false, 150);

        assert_eq!(stats.total_txs, 3);
        assert_eq!(stats.successful_txs, 2);
        assert_eq!(stats.failed_txs, 1);
        assert_eq!(stats.total_time_us, 450);
        assert_eq!(stats.avg_time_us, 150);
        assert_eq!(stats.success_rate(), 2.0 / 3.0);
    }

    #[test]
    fn test_execution_stats_tps() {
        let mut stats = ExecutionStats::default();

        for _ in 0..100 {
            stats.record(true, 1000);
        }

        // 100 transactions in 10 seconds = 10 TPS
        assert_eq!(stats.tps(10.0), 10.0);
    }

    #[test]
    fn test_execution_mode() {
        let seq = ExecutionMode::default();
        assert_eq!(seq, ExecutionMode::Sequential);

        let par = ExecutionMode::parallel(8);
        if let ExecutionMode::Parallel {
            workers,
            optimistic,
        } = par
        {
            assert_eq!(workers, 8);
            assert!(!optimistic);
        } else {
            panic!("Expected parallel mode");
        }

        let opt = ExecutionMode::parallel_optimistic(16);
        if let ExecutionMode::Parallel {
            workers,
            optimistic,
        } = opt
        {
            assert_eq!(workers, 16);
            assert!(optimistic);
        } else {
            panic!("Expected optimistic parallel mode");
        }
    }
}
