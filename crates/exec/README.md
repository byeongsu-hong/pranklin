# Execution - EV-Node Protobuf Definitions

이 크레이트는 [EV-Node](https://github.com/evstack/ev-node) 리포지토리의 protobuf definitions를 Rust에서 사용할 수 있도록 prost + tonic을 사용해 구현한 것입니다.

## 특징

- ✅ **prost 0.14** - 최신 버전의 protobuf 직렬화
- ✅ **tonic 0.14** - 최신 버전의 gRPC 런타임 (서비스 구현 가능)
- ✅ **EV-Node 호환** - 공식 EV-Node proto 정의 사용

## 포함된 Protobuf 정의

### execution.proto

- `ExecutorService` - Execution layer 인터페이스
  - `InitChain` - 블록체인 초기화
  - `GetTxs` - 트랜잭션 조회
  - `ExecuteTxs` - 트랜잭션 실행
  - `SetFinal` - 블록 finalization

### 메시지 타입

- `InitChainRequest`, `InitChainResponse`
- `GetTxsRequest`, `GetTxsResponse`
- `ExecuteTxsRequest`, `ExecuteTxsResponse`
- `SetFinalRequest`, `SetFinalResponse`
- `Batch` - 트랜잭션 배치
- `State` - 블록체인 상태
- `Header`, `SignedHeader` - 블록 헤더
- `Vote` - 합의 투표

## 사용 예제

```rust
use execution::*;

fn main() {
    // InitChainRequest 생성
    let init_request = InitChainRequest {
        genesis_time: Some(prost_types::Timestamp {
            seconds: 1609459200,
            nanos: 0,
        }),
        initial_height: 1,
        chain_id: "my-chain".to_string(),
    };

    // ExecuteTxsRequest 생성
    let execute_request = ExecuteTxsRequest {
        txs: vec![b"tx1".to_vec(), b"tx2".to_vec()],
        block_height: 100,
        timestamp: Some(prost_types::Timestamp::default()),
        prev_state_root: vec![0u8; 32],
    };
}
```

더 많은 예제는 `examples/basic_usage.rs`를 참조하세요.

## 빌드

```bash
cargo build
```

## 예제 실행

```bash
cargo run --example basic_usage
```

## 라이선스

Apache-2.0 (EV-Node와 동일)
