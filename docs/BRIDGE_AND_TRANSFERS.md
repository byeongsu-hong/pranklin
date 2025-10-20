# Bridge and Transfer Features

This document describes the newly implemented bridge, transfer, and multi-asset features in Pranklin.

## Overview

Three major features have been added to Pranklin:

1. **Bridge Functionality** - Allows authorized operators to deposit/withdraw assets for users
2. **Account-to-Account Transfers** - Direct token transfers between accounts
3. **Multi-Asset Support** - Support for multiple collateral assets (USDC, USDT, DAI, etc.)

---

## 1. Multi-Asset Support

### Assets

The system now supports multiple assets as collateral. Each asset has:

- **Asset ID**: Unique identifier (u32)
- **Symbol**: Asset ticker (e.g., "USDC")
- **Name**: Full asset name (e.g., "USD Coin")
- **Decimals**: Decimal precision (e.g., 6 for USDC)
- **Collateral Status**: Whether the asset can be used as collateral
- **Collateral Weight**: Weight in basis points (e.g., 10000 = 100%)

### Default Assets

The system initializes with three default assets:

| Asset ID | Symbol | Name           | Decimals | Collateral Weight |
| -------- | ------ | -------------- | -------- | ----------------- |
| 0        | USDC   | USD Coin       | 6        | 100% (10000 bps)  |
| 1        | USDT   | Tether USD     | 6        | 98% (9800 bps)    |
| 2        | DAI    | Dai Stablecoin | 18       | 95% (9500 bps)    |

### RPC Endpoints

#### Get Asset Information

```bash
POST /asset/info
{
  "asset_id": 0
}

Response:
{
  "id": 0,
  "symbol": "USDC",
  "name": "USD Coin",
  "decimals": 6,
  "is_collateral": true,
  "collateral_weight_bps": 10000
}
```

#### List All Assets

```bash
GET /asset/list

Response:
{
  "assets": [
    {
      "id": 0,
      "symbol": "USDC",
      "name": "USD Coin",
      "decimals": 6,
      "is_collateral": true,
      "collateral_weight_bps": 10000
    },
    ...
  ]
}
```

---

## 2. Account-to-Account Transfer

### Overview

Users can transfer tokens directly between accounts. Transfers require:

- Sufficient balance in the sender's account
- Valid asset that is marked as collateral
- Non-zero amount
- Different sender and recipient addresses

### Transaction Type

```rust
TransferTx {
    to: Address,           // Recipient address
    amount: u128,          // Amount to transfer
    asset_id: u32,         // Asset ID (0 = USDC, 1 = USDT, etc.)
}
```

### Creating a Transfer Transaction

```javascript
// Example using ethers.js
const transfer = {
  to: "0x1234...",
  amount: "1000000", // 1 USDC (6 decimals)
  asset_id: 0,
};

const tx = {
  nonce: await getNonce(senderAddress),
  from: senderAddress,
  payload: {
    Transfer: transfer,
  },
};

// Sign and submit
const signedTx = await signTransaction(tx, privateKey);
await submitTransaction(signedTx);
```

### Security Features

- ✅ Self-transfer prevention
- ✅ Asset validation (must be registered)
- ✅ Collateral check (only collateral assets can be transferred)
- ✅ Balance verification
- ✅ Overflow protection

---

## 3. Bridge Functionality

### Overview

The bridge allows authorized operators to:

- **Bridge Deposits**: Credit user accounts with assets from external chains
- **Bridge Withdrawals**: Debit user accounts when withdrawing to external chains

### Authorization

Only addresses registered as **bridge operators** can execute bridge transactions.

#### Setting Bridge Operators

Bridge operators are configured at startup:

```bash
./pranklin-app start \
  --bridge.operators=0x1111...,0x2222...,0x3333...
```

Multiple operators can be configured using comma-separated addresses.

#### Checking Bridge Operator Status

```bash
POST /bridge/check_operator
{
  "address": "0x1111..."
}

Response:
{
  "is_operator": true
}
```

### Bridge Deposit Transaction

Allows bridge operators to credit user accounts.

```rust
BridgeDepositTx {
    user: Address,              // User to credit
    amount: u128,               // Amount to deposit
    asset_id: u32,              // Asset ID
    external_tx_hash: B256,     // External chain tx hash (for tracking)
}
```

**Example:**

```javascript
const bridgeDeposit = {
  user: "0x5678...",
  amount: "5000000000", // 5000 USDC
  asset_id: 0,
  external_tx_hash: "0xabcd...", // Ethereum tx hash
};

const tx = {
  nonce: await getNonce(operatorAddress),
  from: operatorAddress, // Must be a bridge operator!
  payload: {
    BridgeDeposit: bridgeDeposit,
  },
};

// Sign with operator key and submit
const signedTx = await signTransaction(tx, operatorPrivateKey);
await submitTransaction(signedTx);
```

### Bridge Withdrawal Transaction

Allows bridge operators to debit user accounts when processing withdrawals.

```rust
BridgeWithdrawTx {
    user: Address,              // User to debit
    amount: u128,               // Amount to withdraw
    asset_id: u32,              // Asset ID
    destination: Address,       // Destination on external chain
    external_tx_hash: B256,     // External chain tx hash (for tracking)
}
```

**Example:**

```javascript
const bridgeWithdraw = {
  user: "0x5678...",
  amount: "1000000000", // 1000 USDC
  asset_id: 0,
  destination: "0x9999...", // Ethereum destination
  external_tx_hash: "0xdef0...", // Planned Ethereum tx
};

const tx = {
  nonce: await getNonce(operatorAddress),
  from: operatorAddress, // Must be a bridge operator!
  payload: {
    BridgeWithdraw: bridgeWithdraw,
  },
};

// Sign with operator key and submit
const signedTx = await signTransaction(tx, operatorPrivateKey);
await submitTransaction(signedTx);
```

### Security Features

- ✅ **Operator-only access**: Only authorized operators can execute bridge transactions
- ✅ **Asset validation**: Only registered assets can be bridged
- ✅ **Balance checks**: Withdrawals verify sufficient balance
- ✅ **External tx tracking**: All bridge operations include external transaction hashes
- ✅ **Overflow protection**: Safe arithmetic for all balance operations

### Bridge Architecture

```
┌─────────────────┐
│  External Chain │
│   (Ethereum)    │
└────────┬────────┘
         │
    ┌────▼────┐
    │ Bridge  │  ← Monitors both chains
    │ Operator│  ← Signs transactions
    └────┬────┘
         │
┌────────▼─────────┐
│  Pranklin L2 Chain  │
│  (Rollkit)       │
└──────────────────┘
```

**Typical Flow:**

1. **Deposit:**

   - User locks assets in Ethereum bridge contract
   - Bridge operator detects the lock event
   - Bridge operator submits `BridgeDeposit` transaction to Pranklin
   - User's Pranklin balance increases

2. **Withdrawal:**
   - User requests withdrawal (via regular `Withdraw` tx or RPC call)
   - Bridge operator submits `BridgeWithdraw` transaction to Pranklin
   - User's Pranklin balance decreases
   - Bridge operator releases assets on Ethereum to user's address

---

## Production Recommendations

### Bridge Operator Security

In production, bridge operators should be:

1. **Multi-sig Wallets**: Require multiple signatures for bridge operations
2. **Hardware Security Modules (HSMs)**: Store operator keys in secure hardware
3. **Distributed Key Generation (DKG)**: Use threshold signatures for decentralization
4. **Monitoring & Alerting**: Real-time monitoring of bridge operations
5. **Rate Limiting**: Limit deposit/withdrawal amounts and frequency

### Example Multi-sig Setup

```bash
# Configure with 3-of-5 multi-sig addresses
./pranklin-app start \
  --bridge.operators=0xMultisig1...,0xMultisig2...,0xMultisig3...
```

### Asset Configuration

When adding new assets:

1. Register the asset with appropriate collateral weight
2. Update price feeds for the new asset
3. Configure risk parameters (margin requirements, etc.)
4. Test thoroughly on testnet before mainnet deployment

---

## Error Codes

| Error                         | Description                                |
| ----------------------------- | ------------------------------------------ |
| `Unauthorized`                | Caller is not a bridge operator            |
| `Asset not found`             | Asset ID does not exist                    |
| `Asset cannot be transferred` | Asset is not marked as collateral          |
| `InsufficientBalance`         | Account has insufficient funds             |
| `Overflow`                    | Arithmetic overflow in balance calculation |
| `Cannot transfer to self`     | Sender and recipient are the same          |

---

## Monitoring

### Key Metrics

- Bridge deposit volume (by asset)
- Bridge withdrawal volume (by asset)
- Number of transfer transactions
- Failed bridge operations (unauthorized attempts)
- Average bridge processing time

### Logs

Bridge operations are logged with:

- Transaction hash
- Operator address
- User address
- Amount and asset
- External transaction hash
- Success/failure status

Example:

```
INFO Processing bridge deposit: operator=0x1111..., user=0x5678..., amount=5000000000, asset_id=0
INFO Bridge deposit successful: tx_hash=0xabcd...
```

---

## Testing

### Local Testing

```bash
# Start with test bridge operator
./pranklin-app start \
  --bridge.operators=0x0000000000000000000000000000000000000001 \
  --log.debug

# Test transfer
curl -X POST http://localhost:3000/tx/submit \
  -H "Content-Type: application/json" \
  -d '{"tx": "<hex_encoded_transfer_tx>"}'

# Test bridge deposit (requires operator signature)
curl -X POST http://localhost:3000/tx/submit \
  -H "Content-Type: application/json" \
  -d '{"tx": "<hex_encoded_bridge_deposit_tx>"}'
```

### Integration Tests

See `crates/engine/tests/` for comprehensive test suites covering:

- Multi-asset transfers
- Bridge deposits and withdrawals
- Authorization checks
- Edge cases and error handling

---

## Migration Guide

### Existing Deployments

If you're upgrading from a version without these features:

1. **Backup your database** before upgrading
2. **Run migration** (assets will be auto-initialized)
3. **Configure bridge operators** via command line
4. **Update client libraries** to support new transaction types
5. **Test thoroughly** on staging before production

### Client Library Updates

Update your client libraries to include:

- `TransferTx` support
- `BridgeDepositTx` support
- `BridgeWithdrawTx` support
- Asset info queries

---

## Support

For questions or issues:

- GitHub Issues: https://github.com/your-org/pranklin
- Documentation: https://docs.pranklin.exchange
- Discord: https://discord.gg/pranklin
