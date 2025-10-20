# Authentication & Transaction Flow

## Overview

Pranklin uses a hybrid authentication approach that combines EVM compatibility for agent nomination with optimized binary encoding for regular transactions.

## Agent Nomination (EIP-712)

### Purpose

- **EVM Compatibility**: Agent nomination uses EIP-712 signatures with mainnet chainID (1)
- **Hyperliquid-style Agents**: Supports delegated trading permissions
- **One-time Setup**: Only needed when nominating a new agent

### Process

1. **Create Agent Nomination Message**

```rust
use pranklin_auth::AgentNomination;

let nomination = AgentNomination {
    account: owner_address,
    agent: agent_address,
    permissions: permissions::PLACE_ORDER | permissions::CANCEL_ORDER,
    nonce: 1,
};
```

2. **Sign with EIP-712** (mainnet chainID = 1)

```rust
let domain = AgentNominationDomain::default();  // chainID = 1
let hash = nomination.eip712_hash(&domain);
let signature = signer.sign_hash(&hash).await?;
```

3. **Submit to Auth Service**

```rust
auth_service.nominate_agent(nomination, signature, &domain)?;
```

### EIP-712 Domain

```typescript
{
  name: "PranklinPerp",
  version: "1",
  chainId: 1,  // Mainnet for EVM compatibility
  verifyingContract: "0x..."
}
```

### EIP-712 Message Types

```typescript
AgentNomination(
  address account,
  address agent,
  uint64 permissions,
  uint64 nonce
)
```

## Regular Transactions (Optimized Binary)

### Purpose

- **High Performance**: Optimized for perp DEX throughput
- **Compact Encoding**: Uses bincode (binary encoding) instead of RLP or JSON
- **No EVM Overhead**: After agent setup, no need for EVM-style encoding

### Transaction Structure

```rust
pub struct Transaction {
    pub nonce: u64,
    pub from: Address,           // EVM-compatible address
    pub payload: TxPayload,
    pub signature: [u8; 65],     // secp256k1 signature (r, s, v)
}
```

### Signing Process

1. **Create Transaction**

```rust
let tx = Transaction::new(
    nonce,
    sender_address,
    TxPayload::PlaceOrder(order_details),
);
```

2. **Get Signing Hash**

```rust
let signing_hash = tx.signing_hash();  // SHA256 hash of tx data
```

3. **Sign with secp256k1**

```rust
let signature = signer.sign_hash(&signing_hash).await?;
tx.set_signature(signature);
```

4. **Encode for Transmission**

```rust
let tx_bytes = tx.encode();  // Binary encoding via bincode
```

### Verification

```rust
// Simple verification
auth_service.verify_transaction(&tx)?;

// Verification with agent support
let account = auth_service.verify_transaction_with_agent(
    &tx,
    required_permission,
)?;
```

## Agent Permissions

Bitm

ap-based permission system:

```rust
pub mod permissions {
    pub const PLACE_ORDER: u64      = 1 << 0;  // 0x01
    pub const CANCEL_ORDER: u64     = 1 << 1;  // 0x02
    pub const MODIFY_ORDER: u64     = 1 << 2;  // 0x04
    pub const CLOSE_POSITION: u64   = 1 << 3;  // 0x08
    pub const WITHDRAW: u64         = 1 << 4;  // 0x10
    pub const ALL: u64              = 0x1F;     // All permissions
}
```

### Usage Examples

```rust
// Grant trading permissions only
let perms = permissions::PLACE_ORDER
          | permissions::CANCEL_ORDER
          | permissions::CLOSE_POSITION;

// Grant all permissions
let perms = permissions::ALL;

// Check specific permission
if auth.is_agent(account, agent, permissions::PLACE_ORDER) {
    // Agent can place orders
}
```

## Transaction Types

All transaction payloads use optimized binary encoding:

- **DepositTx**: Deposit collateral
- **WithdrawTx**: Withdraw collateral (subject to margin requirements)
- **PlaceOrderTx**: Place limit/market/stop orders
- **CancelOrderTx**: Cancel existing orders
- **ModifyOrderTx**: Modify order price/size
- **ClosePositionTx**: Close open positions
- **SetAgentTx**: Set agent permissions (requires owner signature)
- **RemoveAgentTx**: Remove agent (requires owner signature)

## Flow Diagrams

### Agent Nomination Flow

```
1. User creates EIP-712 AgentNomination message (chainID = 1)
2. User signs with wallet (MetaMask, etc.) - EVM compatible
3. Submit signature to Pranklin
4. Pranklin verifies EIP-712 signature
5. Agent permissions stored on-chain
6. Agent can now trade on behalf of user
```

### Trading Flow (with Agent)

```
1. Agent creates Transaction with binary encoding
2. Agent signs with secp256k1
3. Submit binary-encoded transaction to Pranklin RPC
4. Pranklin verifies:
   - Signature is valid
   - Signer is authorized agent OR account owner
   - Agent has required permissions
5. Execute transaction
```

## Security Considerations

1. **Agent Nomination**: Uses EIP-712 for maximum compatibility and security
2. **Nonce Protection**: Each transaction has a nonce to prevent replays
3. **Permission Granularity**: Fine-grained control over agent capabilities
4. **Revocable**: Agents can be removed at any time by the account owner
5. **Audit Trail**: All agent actions are signed and traceable

## Performance Benefits

- **Compact Encoding**: Bincode is ~50% smaller than JSON, ~30% smaller than RLP
- **Fast Parsing**: Binary deserialization is 10x faster than JSON
- **High Throughput**: Optimized for perp DEX order flow
- **EVM Compatible**: Addresses and signatures remain EVM-compatible

## Integration Example

```rust
// 1. Setup (one-time, EIP-712)
let nomination = AgentNomination { /* ... */ };
let signature = wallet.sign_eip712(&nomination).await?;
auth.nominate_agent(nomination, signature, &domain)?;

// 2. Trading (optimized binary)
let tx = Transaction::new(nonce, account, payload);
let sig = agent_wallet.sign(&tx.signing_hash()).await?;
tx.set_signature(sig);

// 3. Submit
let tx_bytes = tx.encode();  // Binary encoding
rpc_client.submit_tx(tx_bytes).await?;
```
