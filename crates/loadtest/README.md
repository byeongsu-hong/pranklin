# Pranklin Load Testing Framework

REST API 엔드포인트에 대한 고성능 부하 테스트 도구입니다.

## 빌드

```bash
cargo build --release -p pranklin-loadtest
```

## 테스트 플로우

1. **계정 초기화** (`--operator-mode`) - Bridge operator 권한으로 지갑에 잔액 주입
2. **주문 스팸** (`--scenario order-spam`) - 주문 생성 & 취소 반복
3. **주문 매칭** (`--scenario order-matching`) - Buy/Sell 주문 매칭 시뮬레이션
4. **공격적 매칭** (`--scenario aggressive`) - Orderbook 구축 후 시장가 주문 폭격

## 사용법

### 전체 플로우 실행 (권장)

```bash
# 1단계: 서버에 bridge operator 등록
# 먼저 loadtest를 실행하면 operator 주소가 출력됨
cargo run --release -p pranklin-loadtest -- \
  --operator-mode \
  --scenario order-matching \
  --num-workers 20 \
  --num-wallets 100 \
  --initial-balance 10000000000 \
  --duration-secs 60

# 서버 콘솔에서 출력된 operator 주소를 서버 시작 시 --bridge.operators 플래그에 추가
```

### 시나리오별 실행

#### 1. Order Spam (주문 생성 & 취소)

```bash
cargo run --release -p pranklin-loadtest -- \
  --operator-mode \
  --scenario order-spam \
  --num-workers 20 \
  --duration-secs 60
```

#### 2. Order Matching (매칭 시뮬레이션)

```bash
cargo run --release -p pranklin-loadtest -- \
  --operator-mode \
  --scenario order-matching \
  --num-workers 50 \
  --duration-secs 120
```

#### 3. Aggressive Matching (고강도)

```bash
cargo run --release -p pranklin-loadtest -- \
  --operator-mode \
  --scenario aggressive \
  --num-workers 100 \
  --duration-secs 60
```

### 기본 부하 테스트 (Sustained Load)

```bash
cargo run --release -p pranklin-loadtest -- \
  --rpc-url http://localhost:3000 \
  --scenario standard \
  --num-workers 10 \
  --target-tps 100 \
  --duration-secs 30
```

### 부하 테스트 모드

#### 1. Sustained (지속적인 일정 부하)

일정한 TPS를 유지하면서 부하를 가합니다.

```bash
cargo run --release -p pranklin-loadtest -- \
  --mode sustained \
  --target-tps 500 \
  --duration-secs 60
```

#### 2. Ramp (점진적 증가)

부하를 점진적으로 증가시킵니다.

```bash
cargo run --release -p pranklin-loadtest -- \
  --mode ramp \
  --target-tps 1000 \
  --ramp-up-secs 30 \
  --duration-secs 120
```

#### 3. Burst (주기적 폭발)

주기적으로 높은 부하를 가합니다.

```bash
cargo run --release -p pranklin-loadtest -- \
  --mode burst \
  --burst-duration-secs 5 \
  --burst-interval-secs 15 \
  --duration-secs 120
```

#### 4. Stress (최대 처리량)

최대한 빠르게 요청을 전송하여 시스템 한계를 테스트합니다.

```bash
cargo run --release -p pranklin-loadtest -- \
  --mode stress \
  --num-workers 50 \
  --duration-secs 30
```

### 트랜잭션 타입

#### PlaceOrder (주문 생성)

```bash
cargo run --release -p pranklin-loadtest -- \
  --tx-type place-order \
  --market-id 0 \
  --target-tps 200
```

#### Mixed (혼합)

```bash
cargo run --release -p pranklin-loadtest -- \
  --tx-type mixed \
  --target-tps 500
```

## 옵션

| 옵션                    | 설명                                                              | 기본값                  |
| ----------------------- | ----------------------------------------------------------------- | ----------------------- |
| `--rpc-url`             | RPC 엔드포인트 URL                                                | `http://localhost:3000` |
| `--operator-mode`       | Bridge operator로 계정 초기화 활성화                              | `false`                 |
| `--initial-balance`     | 지갑당 초기 잔액 (base units)                                     | `10000000000`           |
| `--scenario`            | 시나리오: standard, order-spam, order-matching, aggressive        | `standard`              |
| `--num-workers`         | 동시 워커 수                                                      | `10`                    |
| `--target-tps`          | 목표 TPS (초당 트랜잭션, standard 모드)                           | `100`                   |
| `--duration-secs`       | 테스트 지속 시간 (초)                                             | `30`                    |
| `--mode`                | 테스트 모드 (standard 시나리오): sustained, ramp, burst, stress   | `sustained`             |
| `--tx-type`             | 트랜잭션 타입 (standard 시나리오): place-order, cancel-order, etc | `mixed`                 |
| `--num-wallets`         | 사용할 고유 지갑 수                                               | `100`                   |
| `--market-id`           | 마켓 ID                                                           | `0`                     |
| `--asset-id`            | 자산 ID                                                           | `0`                     |
| `--ramp-up-secs`        | Ramp 모드에서 증가 시간                                           | `10`                    |
| `--burst-duration-secs` | Burst 모드에서 버스트 지속 시간                                   | `5`                     |
| `--burst-interval-secs` | Burst 모드에서 버스트 간격                                        | `15`                    |

## 출력 예시

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

## 특징

- ✅ **다양한 부하 패턴**: Sustained, Ramp, Burst, Stress 모드 지원
- ✅ **트랜잭션 다양성**: 주문, 입금, 출금, 전송 등 다양한 트랜잭션 타입
- ✅ **정확한 메트릭**: HDR Histogram 기반 정밀한 레이턴시 측정
- ✅ **자동 지갑 관리**: Nonce 자동 관리로 충돌 없는 트랜잭션 생성
- ✅ **실시간 모니터링**: 5초마다 진행 상황 출력
- ✅ **에러 추적**: 실패한 요청의 에러 메시지 집계

## 고급 사용 예시

### 1. 고부하 스트레스 테스트

```bash
cargo run --release -p pranklin-loadtest -- \
  --mode stress \
  --num-workers 100 \
  --num-wallets 1000 \
  --duration-secs 60
```

### 2. 주문 생성 집중 테스트

```bash
cargo run --release -p pranklin-loadtest -- \
  --tx-type place-order \
  --target-tps 500 \
  --num-workers 20 \
  --duration-secs 300
```

### 3. 점진적 부하 증가로 임계점 찾기

```bash
cargo run --release -p pranklin-loadtest -- \
  --mode ramp \
  --target-tps 2000 \
  --ramp-up-secs 120 \
  --duration-secs 180 \
  --num-workers 50
```

## 팁

1. **워커 수 조정**: CPU 코어 수의 2-4배 정도가 적당합니다.
2. **지갑 수 조정**: Nonce 충돌을 피하려면 `num-wallets`를 워커 수보다 많게 설정하세요.
3. **네트워크 최적화**: 서버와 같은 네트워크에서 실행하면 더 정확한 결과를 얻을 수 있습니다.
4. **점진적 테스트**: 낮은 TPS부터 시작해서 점차 늘려가며 시스템 한계를 찾으세요.
