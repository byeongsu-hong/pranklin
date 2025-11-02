//! # Pranklin Execution Layer
//!
//! EV-Node compatible execution layer implementation with gRPC interface.
//!
//! ## Features
//!
//! - ✅ **EV-Node Compatible** - Implements ExecutorService gRPC interface
//! - ✅ **Transaction Processing** - Full transaction lifecycle management
//! - ✅ **State Management** - Persistent state with RocksDB backend
//! - ✅ **Snapshot Support** - Automatic state snapshots at configurable intervals
//!
//! ## Usage
//!
//! ```rust,no_run
//! use pranklin_exec::{new_server_with_components, PranklinExecutorService};
//!
//! #[tokio::main]
//! async fn main() {
//!     let (server, service) = new_server_with_components("./data");
//!     // Use server for gRPC, service.get_components() for JSON-RPC
//! }
//! ```

mod constants;
mod error;
mod executor_trait;
mod proto;
mod readonly_executor;
mod server;
mod tx_executor;

// Core types and traits
pub use error::{Result, TxExecutionError};
pub use executor_trait::{ExecutionMode, ExecutionStats, TxExecutor};

// Server
pub use server::{
    PranklinExecutorService, SharedComponents, new_server_with_components,
    new_server_with_components_and_snapshots,
};

// Executors
pub use readonly_executor::{
    ReadOnlyConfig, ReadOnlyError, ReadOnlyExecutor, SyncResult, SyncService,
};
pub use tx_executor::{TransactionExecutor, TxExecutionStats, execute_single_tx, execute_tx_batch};

// Constants
pub use constants::{DEFAULT_MAX_BYTES, GENESIS_HEIGHT, MAX_TXS_PER_BLOCK};

// Proto
pub use proto::pb;
