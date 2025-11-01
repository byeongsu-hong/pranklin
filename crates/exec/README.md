# Execution - EV-Node Protobuf Definitions

This crate implements protobuf definitions from the [EV-Node](https://github.com/evstack/ev-node) repository using prost + tonic for use in Rust.

## Features

- ✅ **prost 0.14** - Latest version of protobuf serialization
- ✅ **tonic 0.14** - Latest version of gRPC runtime (service implementation available)
- ✅ **EV-Node Compatible** - Uses official EV-Node proto definitions

## Included Protobuf Definitions

### execution.proto

- `ExecutorService` - Execution layer interface
  - `InitChain` - Blockchain initialization
  - `GetTxs` - Transaction queries
  - `ExecuteTxs` - Transaction execution
  - `SetFinal` - Block finalization

### Message Types

- `InitChainRequest`, `InitChainResponse`
- `GetTxsRequest`, `GetTxsResponse`
- `ExecuteTxsRequest`, `ExecuteTxsResponse`
- `SetFinalRequest`, `SetFinalResponse`
- `Batch` - Transaction batch
- `State` - Blockchain state
- `Header`, `SignedHeader` - Block headers
- `Vote` - Consensus vote

## Usage Example

```rust
use execution::*;

fn main() {
    // Create InitChainRequest
    let init_request = InitChainRequest {
        genesis_time: Some(prost_types::Timestamp {
            seconds: 1609459200,
            nanos: 0,
        }),
        initial_height: 1,
        chain_id: "my-chain".to_string(),
    };

    // Create ExecuteTxsRequest
    let execute_request = ExecuteTxsRequest {
        txs: vec![b"tx1".to_vec(), b"tx2".to_vec()],
        block_height: 100,
        timestamp: Some(prost_types::Timestamp::default()),
        prev_state_root: vec![0u8; 32],
    };
}
```

For more examples, see `examples/basic_usage.rs`.

## Build

```bash
cargo build
```

## Run Examples

```bash
cargo run --example basic_usage
```

## License

Apache-2.0 (same as EV-Node)
