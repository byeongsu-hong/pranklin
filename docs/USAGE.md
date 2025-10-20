# Pranklin CLI Usage

## Basic Commands

### Start Daemon
```bash
# Local development
cargo run -- start

# With custom addresses
cargo run -- start --grpc.addr="[::1]:50051" --rpc.addr="0.0.0.0:3000"

# With debug logging
cargo run -- start --log.debug
```

### Snapshot Configuration

#### S3 Snapshots
```bash
cargo run -- start \
  --snapshot.enable \
  --snapshot.interval=10000 \
  --snapshot.s3.bucket=my-bucket \
  --snapshot.s3.region=us-west-2 \
  --snapshot.s3.prefix=snapshots
```

#### GCS Snapshots  
```bash
cargo run -- start \
  --snapshot.enable \
  --snapshot.interval=10000 \
  --snapshot.gcs.bucket=my-bucket \
  --snapshot.gcs.prefix=snapshots
```

#### Local Snapshots
```bash
cargo run -- start \
  --snapshot.enable \
  --snapshot.interval=10000 \
  --snapshot.local.path=./snapshots
```

## Features

- **LZ4 Compression**: Fast validator bootstrapping (default and only option)
- **Rate Limiting**: 100 txs per sender by default
- **Auto-Snapshot**: Configurable interval-based exports
- **Reth-Style CLI**: Clean nested configuration options

## Architecture

- **State**: RocksDB + JMT with Aptos optimizations
- **Consensus**: Rollkit gRPC integration
- **RPC**: Axum-based HTTP/JSON API
- **Mempool**: Per-sender rate limiting (no gas)
