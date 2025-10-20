# Pranklin Core

High-performance perpetual futures DEX engine built with Rust.

## 🚀 Features

- **Orderbook Matching**: Price-time priority with IOC, FOK, Post-Only support
- **Advanced Liquidation**: Partial liquidation, insurance fund, ADL
- **Risk Management**: Real-time margin monitoring and position tracking
- **State Management**: RocksDB + Jellyfish Merkle Tree with snapshots
- **Rollkit Integration**: Full ABCI compliance for Rollkit/Celestia
- **RPC Server**: REST API + WebSocket with rate limiting and circuit breaker
- **Production Ready**: Comprehensive monitoring, logging, and error handling

## 📖 Documentation

All documentation is in the [`docs/`](docs/) directory:

- **[Getting Started](docs/USAGE.md)** - Quick start guide
- **[Project Status](docs/PROJECT_STATUS.md)** - Implementation status
- **[API Reference](docs/API.md)** - Complete API documentation
- **[Deployment Guide](docs/DEPLOYMENT.md)** - Production deployment
- **[Authentication](docs/AUTHENTICATION.md)** - EIP-712 and agent system

See [`docs/README.md`](docs/README.md) for the full documentation index.

## 🏗️ Architecture

Pranklin consists of three main components that can run as a unified node (like Cosmos+Tendermint):

```text
┌─────────────────────────────────────────────────────┐
│              Pranklin Unified Node                  │
│  ┌──────────────────────────────────────────────┐   │
│  │  Sequencer (Go - EV-Node)                    │   │
│  │  • Consensus & Block Production              │   │
│  │  • P2P Networking                            │   │
│  └──────────────┬───────────────────────────────┘   │
│                 │ gRPC                              │
│  ┌──────────────▼───────────────────────────────┐   │
│  │  Execution Layer (Rust)                      │   │
│  │  • Trading Engine (Orderbook, Liquidations)  │   │
│  │  • State Management (RocksDB + JMT)          │   │
│  │  • Mempool & Authentication                  │   │
│  └──────────────────────────────────────────────┘   │
│                                                     │
│  ┌──────────────────────────────────────────────┐   │
│  │  Data Availability (local-da)                │   │
│  │  • Block data publishing                     │   │
│  └──────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────┘
```

### Project Structure

```text
pranklin-core/
├── sequencer/       # Sequencer node (Go)
│   ├── cmd/         # CLI commands (node, start, init)
│   └── grpc/        # gRPC client for execution layer
├── crates/
│   ├── engine/      # Core matching and liquidation engine
│   ├── state/       # State management (RocksDB + JMT)
│   ├── tx/          # Transaction types and validation
│   ├── auth/        # Authentication (EIP-712, agents)
│   ├── mempool/     # Transaction mempool
│   ├── exec/        # gRPC execution server
│   ├── rpc/         # RPC server (REST + WebSocket)
│   ├── app/         # Main application
│   └── loadtest/    # Load testing tools
└── docs/            # Documentation
```

## ⚡ Quick Start

### Option 1: Unified Node (Recommended)

Run everything as a single unified node:

```bash
# 1. Build all components
cd sequencer && make build-all

# 2. Initialize
./bin/pranklin-sequencer init --root-dir ~/.pranklin-sequencer

# 3. Run unified node (from sequencer directory)
./bin/pranklin-sequencer node --root-dir ~/.pranklin-sequencer
```

That's it! All components (DA + Execution + Sequencer) start automatically.

### Option 2: Development Mode

For development, you can run components separately:

```bash
# Build
cargo build --release

# Run tests
cargo test

# Run example
cargo run --example simple_trade

# Start RPC server only
cargo run --bin pranklin-rpc

# Start execution layer
cargo run --bin pranklin-app -- start
```

## 🧪 Testing

```bash
# All tests
cargo test

# Specific package
cargo test --package pranklin-engine

# With output
cargo test -- --nocapture
```

## 🔧 Development

```bash
# Check code
cargo check

# Run clippy
cargo clippy --all-targets

# Format code
cargo fmt

# Build docs
cargo doc --open
```

## 📊 Performance

- **Throughput**: 10,000+ orders/sec
- **Latency**: <1ms order matching
- **State**: Efficient RocksDB with compression
- **Memory**: In-memory position index

## 🔐 Security

- EIP-712 structured data signing
- Nonce management (replay protection)
- Input validation
- Rate limiting & circuit breaker

## 📄 License

Copyright © 2025 Pranklin Core Team
