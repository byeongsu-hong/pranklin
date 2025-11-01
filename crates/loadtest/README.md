# Pranklin Load Testing Framework

High-performance load testing tool for REST API endpoints.

## Build

```bash
cargo build --release -p pranklin-loadtest
```

## Test Flow

1. **Account Initialization** (`--operator-mode`) - Fund wallets with bridge operator privileges
2. **Order Spam** (`--scenario order-spam`) - Repeatedly create & cancel orders
3. **Order Matching** (`--scenario order-matching`) - Simulate buy/sell order matching
4. **Aggressive Matching** (`--scenario aggressive`) - Build orderbook then bombard with market orders

## Usage

### Complete Flow Execution (Recommended)

```bash
# Step 1: Register bridge operator on the server
# First run loadtest to display the operator address
cargo run --release -p pranklin-loadtest -- \
  --operator-mode \
  --scenario order-matching \
  --num-workers 20 \
  --num-wallets 100 \
  --initial-balance 10000000000 \
  --duration-secs 60

# Add the displayed operator address to the server's --bridge.operators flag when starting
```

### Scenario-specific Execution

#### 1. Order Spam (Create & Cancel Orders)

```bash
cargo run --release -p pranklin-loadtest -- \
  --operator-mode \
  --scenario order-spam \
  --num-workers 20 \
  --duration-secs 60
```

#### 2. Order Matching (Matching Simulation)

```bash
cargo run --release -p pranklin-loadtest -- \
  --operator-mode \
  --scenario order-matching \
  --num-workers 50 \
  --duration-secs 120
```

#### 3. Aggressive Matching (High Intensity)

```bash
cargo run --release -p pranklin-loadtest -- \
  --operator-mode \
  --scenario aggressive \
  --num-workers 100 \
  --duration-secs 60
```

### Basic Load Test (Sustained Load)

```bash
cargo run --release -p pranklin-loadtest -- \
  --rpc-url http://localhost:3000 \
  --scenario standard \
  --num-workers 10 \
  --target-tps 100 \
  --duration-secs 30
```

### Load Test Modes

#### 1. Sustained (Constant Load)

Maintains a constant TPS while applying load.

```bash
cargo run --release -p pranklin-loadtest -- \
  --mode sustained \
  --target-tps 500 \
  --duration-secs 60
```

#### 2. Ramp (Gradual Increase)

Gradually increases load.

```bash
cargo run --release -p pranklin-loadtest -- \
  --mode ramp \
  --target-tps 1000 \
  --ramp-up-secs 30 \
  --duration-secs 120
```

#### 3. Burst (Periodic Bursts)

Periodically applies high load.

```bash
cargo run --release -p pranklin-loadtest -- \
  --mode burst \
  --burst-duration-secs 5 \
  --burst-interval-secs 15 \
  --duration-secs 120
```

#### 4. Stress (Maximum Throughput)

Tests system limits by sending requests as fast as possible.

```bash
cargo run --release -p pranklin-loadtest -- \
  --mode stress \
  --num-workers 50 \
  --duration-secs 30
```

### Transaction Types

#### PlaceOrder (Create Order)

```bash
cargo run --release -p pranklin-loadtest -- \
  --tx-type place-order \
  --market-id 0 \
  --target-tps 200
```

#### Mixed

```bash
cargo run --release -p pranklin-loadtest -- \
  --tx-type mixed \
  --target-tps 500
```

## Options

| Option                  | Description                                                     | Default                 |
| ----------------------- | --------------------------------------------------------------- | ----------------------- |
| `--rpc-url`             | RPC endpoint URL                                                | `http://localhost:3000` |
| `--operator-mode`       | Enable account initialization with bridge operator              | `false`                 |
| `--initial-balance`     | Initial balance per wallet (base units)                         | `10000000000`           |
| `--scenario`            | Scenario: standard, order-spam, order-matching, aggressive      | `standard`              |
| `--num-workers`         | Number of concurrent workers                                    | `10`                    |
| `--target-tps`          | Target TPS (transactions per second, standard mode)             | `100`                   |
| `--duration-secs`       | Test duration (seconds)                                         | `30`                    |
| `--mode`                | Test mode (standard scenario): sustained, ramp, burst, stress   | `sustained`             |
| `--tx-type`             | Transaction type (standard scenario): place-order, cancel-order | `mixed`                 |
| `--num-wallets`         | Number of unique wallets to use                                 | `100`                   |
| `--market-id`           | Market ID                                                       | `0`                     |
| `--asset-id`            | Asset ID                                                        | `0`                     |
| `--ramp-up-secs`        | Ramp-up duration in ramp mode                                   | `10`                    |
| `--burst-duration-secs` | Burst duration in burst mode                                    | `5`                     |
| `--burst-interval-secs` | Burst interval in burst mode                                    | `15`                    |

## Example Output

```text
🚀 Starting Pranklin Load Test
  Target: http://localhost:3000
  Mode: OrderMatching (20 workers)
  Duration: 60s, TPS Target: 0

🔍 Checking server health...
  ✓ Server is healthy: OK

💰 Generating 100 wallets...
  ✓ Wallets generated

🔧 PHASE 1: Account Initialization
⚠️  Bridge operator address: 0x1234...5678
   Make sure this address is authorized as a bridge operator on the server!
   Waiting 3 seconds before proceeding...

💰 Initializing 100 wallets with 10000000000 units of asset 0
  Initialized 10/100 wallets
  Initialized 20/100 wallets
  ...
  ✓ Wallet initialization complete: 100 success, 0 failed

🔍 Verifying wallet balances...
  ✓ Verified 10/10 sampled wallets have correct balance

🎯 PHASE 2: Load Testing
🔄 Running order matching scenario
📈 1250 requests (1240 success, 10 failed) | TPS: 248.5 | Latency p50/p95/p99: 8.3/35.2/79.1ms
📈 2580 requests (2565 success, 15 failed) | TPS: 255.2 | Latency p50/p95/p99: 7.8/33.5/75.3ms
...

📊 Load Test Results:
  Total Requests: 15420
  Successful: 15385
  Failed: 35
  Success Rate: 99.77%
  Duration: 60.08s
  Actual TPS: 256.73

⏱️  Latency Statistics (ms):
  Min: 3.12
  Max: 186.45
  Mean: 12.34
  P50: 8.23
  P95: 34.56
  P99: 78.90
  P99.9: 142.33
```

## Features

- ✅ **Various Load Patterns**: Supports sustained, ramp, burst, stress modes
- ✅ **Transaction Diversity**: Various transaction types including orders, deposits, withdrawals, transfers
- ✅ **Accurate Metrics**: Precise latency measurement based on HDR Histogram
- ✅ **Automatic Wallet Management**: Collision-free transaction creation with automatic nonce management
- ✅ **Real-time Monitoring**: Progress output every 5 seconds
- ✅ **Error Tracking**: Aggregates error messages from failed requests

## Advanced Usage Examples

### 1. High Load Stress Test

```bash
cargo run --release -p pranklin-loadtest -- \
  --mode stress \
  --num-workers 100 \
  --num-wallets 1000 \
  --duration-secs 60
```

### 2. Order Creation Focus Test

```bash
cargo run --release -p pranklin-loadtest -- \
  --tx-type place-order \
  --target-tps 500 \
  --num-workers 20 \
  --duration-secs 300
```

### 3. Find Critical Point with Gradual Load Increase

```bash
cargo run --release -p pranklin-loadtest -- \
  --mode ramp \
  --target-tps 2000 \
  --ramp-up-secs 120 \
  --duration-secs 180 \
  --num-workers 50
```

## Tips

1. **Worker Count**: 2-4 times the number of CPU cores is recommended.
2. **Wallet Count**: Set `num-wallets` higher than worker count to avoid nonce collisions.
3. **Network Optimization**: Running on the same network as the server provides more accurate results.
4. **Gradual Testing**: Start with low TPS and gradually increase to find system limits.
