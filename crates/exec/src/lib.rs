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
mod proto;
mod server;
mod tx_executor;

pub use proto::pb;
pub use server::{
    PranklinExecutorService, SharedComponents, new_server_with_components,
    new_server_with_components_and_snapshots,
};
