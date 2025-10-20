# Pranklin Sequencer

Pranklin perpetual DEX sequencer integrated with [EV-Node](https://github.com/evstack/ev-node) consensus framework.

## Architecture

```text
┌───────────────────────────────────────────────────────┐
│              EV-Node Consensus (Go)                   │
│  ┌────────────┐  ┌────────────┐  ┌─────────────────┐  │
│  │ Consensus  │  │ Sequencer  │  │  DA Client      │  │
│  │  Layer     │  │  (Single)  │  │  (local-da)     │  │
│  └──────┬─────┘  └──────┬─────┘  └────────┬────────┘  │
│         │               │                 │           │
│         └───────────────┴─────────────────┘           │
│                         │                             │
│                    gRPC Client                        │
└─────────────────────────┼─────────────────────────────┘
                          │
                          ▼
┌───────────────────────────────────────────────────────┐
│           Pranklin Execution Layer (Rust)             │
│  ┌──────────┐  ┌─────────┐  ┌────────┐  ┌──────────┐  │
│  │  Engine  │  │ Mempool │  │  Auth  │  │  State   │  │
│  │ (Trading)│  │         │  │        │  │ (RocksDB)│  │
│  └──────────┘  └─────────┘  └────────┘  └──────────┘  │
│                    gRPC Server (port 50051)           │
└───────────────────────────────────────────────────────┘
```

## Quick Start

### Option A: Unified Node (Recommended) 🎉

Run all components (DA + Execution + Sequencer) as a single unified node, similar to how Cosmos embeds Tendermint.

**1. Build both execution and sequencer:**

```bash
# Build execution layer (Rust)
cd pranklin-core
cargo build --release --bin pranklin-app

# Build sequencer (Go)
cd sequencer
make build
```

**2. Initialize:**

```bash
cd sequencer
./bin/pranklin-sequencer init --root-dir ~/.pranklin-sequencer
```

**3. Run unified node:**

```bash
./bin/pranklin-sequencer node \
  --root-dir ~/.pranklin-sequencer \
  --bridge-operators 0x742d35Cc6634C0532925a3b844Bc454e4438f44e

# Default paths:
# - local-da: from PATH
# - pranklin-app: ../target/release/pranklin-app
```

That's it! All components start automatically:

- ✅ Local DA layer
- ✅ Execution layer (gRPC + RPC)
- ✅ Sequencer with consensus

Use `Ctrl+C` to gracefully shutdown all components.

### Option B: Separate Components (Advanced)

Run each component in a separate terminal for development.

**Terminal 1 - DA Layer:**

```bash
local-da
```

**Terminal 2 - Execution:**

```bash
cargo run --release --bin pranklin-app -- start \
  --grpc.addr 0.0.0.0:50051 \
  --rpc.addr 0.0.0.0:3000
```

**Terminal 3 - Sequencer:**

```bash
cd sequencer
make init

./bin/pranklin-sequencer start \
  --root-dir ~/.pranklin-sequencer \
  --grpc-executor-url localhost:50051 \
  --da.address http://localhost:7980
```

## Configuration

### Unified Node Command

Key flags for `pranklin-sequencer node`:

- `--local-da-binary`: Path to local-da binary (default: `local-da`)
- `--local-da-port`: Port for local-da (default: `7980`)
- `--execution-binary`: Path to pranklin-app binary (default: `pranklin-app`)
- `--execution-grpc-addr`: Execution gRPC address (default: `0.0.0.0:50051`)
- `--execution-rpc-addr`: Execution RPC address (default: `0.0.0.0:3000`)
- `--execution-db-path`: Execution database path (default: `./data/pranklin_db`)
- `--bridge-operators`: Bridge operator addresses (comma-separated)
- `--root-dir`: Config and data directory (default: `~/.pranklin-sequencer`)

### Legacy Start Command

Key flags for `pranklin-sequencer start`:

- `--grpc-executor-url`: Pranklin execution gRPC address (default: `localhost:50051`)
  - Uses standard gRPC with HTTP/2
  - Compatible with tonic gRPC server
- `--da.address`: DA layer HTTP address (default: `http://localhost:7980`)
- `--root-dir`: Config and data directory (default: `~/.pranklin-sequencer`)

See EV-Node documentation for more configuration options.

### HTTP/2 Support

The gRPC client automatically uses HTTP/2 for communication with the execution layer:

- ✅ HTTP/2 multiplexing for concurrent requests
- ✅ Header compression (HPACK)
- ✅ Binary framing protocol
- ✅ Connection reuse and flow control

## Development

### Testing

```bash
go test ./...
```

### With Docker

```bash
cd sequencer
docker-compose up
```

## References

- [EV-Node](https://github.com/evstack/ev-node)
- [Pranklin Execution](../../crates/exec/README.md)
- [Protocol Documentation](../../docs/README.md)
