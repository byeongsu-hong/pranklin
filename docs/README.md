# Pranklin Core Documentation

Welcome to the Pranklin Core documentation. This directory contains comprehensive documentation for the Pranklin perpetual futures DEX engine.

## 📚 Documentation Index

### Getting Started

- [Usage Guide](USAGE.md) - How to use the Pranklin Core engine
- [Project Status](PROJECT_STATUS.md) - Current implementation status and roadmap

### Core Documentation

- [API Reference](API.md) - Complete REST and WebSocket API documentation
- [Authentication](AUTHENTICATION.md) - Authentication and signature verification
- [Deployment Guide](DEPLOYMENT.md) - Production deployment instructions
- [Orderbook Recovery](ORDERBOOK_RECOVERY.md) - Orderbook recovery architecture
- [Tick System](TICK_SYSTEM.md) - Tick-based order management system

## 🏗️ System Architecture

Pranklin Core is a high-performance perpetual futures DEX engine built with Rust. Key components:

- **State Management**: RocksDB + Jellyfish Merkle Tree
- **Matching Engine**: Price-time priority orderbook
- **Liquidation**: Advanced partial liquidation with insurance fund
- **Risk Management**: Real-time margin checks and ADL
- **Execution**: gRPC server for Rollkit/ABCI integration
- **RPC**: REST API + WebSocket for real-time updates

## 🚀 Quick Start

```bash
# Build the project
cargo build --release

# Run tests
cargo test

# Run example
cargo run --example simple_trade
```

## 📖 Core Concepts

### Trading

- **Perpetual Futures**: No expiry, continuous trading
- **Leverage**: Up to 20x (configurable per market)
- **Margin**: Cross-margin model with isolated positions

### Liquidation

- **Partial Liquidation**: Protects trader equity
- **Insurance Fund**: Covers bad debt
- **ADL (Auto-Deleveraging)**: Last resort mechanism
- **Liquidator Incentives**: Rewards for timely liquidations

### Risk Management

- **Initial Margin**: Required to open positions (10% default)
- **Maintenance Margin**: Required to keep positions open (5% default)
- **Risk Index**: Priority queue for liquidation monitoring

## 🔧 Configuration

Markets can be configured with:

- `max_leverage`: Maximum leverage allowed (e.g., 20x)
- `initial_margin_bps`: Initial margin ratio in basis points (e.g., 1000 = 10%)
- `maintenance_margin_bps`: Maintenance margin ratio (e.g., 500 = 5%)
- `liquidation_fee_bps`: Liquidation fee (e.g., 100 = 1%)

## 📝 API Reference

### Engine API

```rust
// Create engine
let engine = Engine::new(state);

// Place order
engine.process_place_order(&tx, &order)?;

// Execute liquidation
engine.liquidate_with_incentive(trader, market_id, mark_price, liquidator)?;
```

### State API

```rust
// Get position
let position = state.get_position(address, market_id)?;

// Update balance
state.set_balance(address, asset_id, amount)?;
```

## 🧪 Testing

Run the comprehensive test suite:

```bash
# All tests
cargo test

# Specific module
cargo test --package pranklin-engine

# Integration tests
cargo test --test integration_tests
```

## 📈 Performance

- **Throughput**: 10,000+ orders/sec
- **Latency**: <1ms for order matching
- **State Size**: Efficient with RocksDB compression
- **Memory**: In-memory position index for O(1) access

## 🔐 Security

- **EIP-712**: Typed structured data signing
- **Nonce Management**: Replay attack prevention
- **Input Validation**: Comprehensive checks
- **Rate Limiting**: DDoS protection

## 🌐 Deployment

See [Project Status](PROJECT_STATUS.md) for deployment guides and production readiness checklist.

## 🤝 Contributing

1. Read the documentation
2. Run tests: `cargo test`
3. Check lints: `cargo clippy`
4. Format code: `cargo fmt`

## 📄 License

See LICENSE file in the project root.
