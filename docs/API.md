# Pranklin Perp DEX - API Documentation

## Overview

The Pranklin Perp DEX provides a REST API for trading operations and a WebSocket API for real-time updates.

**Base URL**: `http://localhost:8545` (development)  
**WebSocket URL**: `ws://localhost:8545/ws`

## Authentication

Currently, transactions require valid signatures using EIP-712 (for agent nomination) or bincode encoding (for regular transactions).

## REST API Endpoints

### Health Check

```
GET /health
```

Returns the health status of the node.

**Response:**

```
OK
```

### Metrics

```
GET /metrics
```

Returns Prometheus-formatted metrics.

### Submit Transaction

```
POST /tx/submit
```

Submit a signed transaction to the mempool.

**Request Body:**

```json
{
  "tx": "0x..." // hex-encoded transaction
}
```

**Response:**

```json
{
  "tx_hash": "0x..."
}
```

**Example (curl):**

```bash
curl -X POST http://localhost:8545/tx/submit \
  -H "Content-Type: application/json" \
  -d '{"tx":"0x..."}'
```

### Get Transaction Status

```
POST /tx/status
```

Get the status of a transaction.

**Request Body:**

```json
{
  "tx_hash": "0x..."
}
```

**Response:**

```json
{
  "tx_hash": "0x...",
  "status": "pending|confirmed|not_found",
  "block_height": 12345,
  "error": null
}
```

### Get Balance

```
POST /account/balance
```

Get the balance for an address and asset.

**Request Body:**

```json
{
  "address": "0x...",
  "asset_id": 0
}
```

**Response:**

```json
{
  "balance": "100000000000"
}
```

### Get Nonce

```
POST /account/nonce
```

Get the current nonce for an address.

**Request Body:**

```json
{
  "address": "0x..."
}
```

**Response:**

```json
{
  "nonce": 42
}
```

### Get Position

```
POST /account/position
```

Get position information for a specific market.

**Request Body:**

```json
{
  "address": "0x...",
  "market_id": 0
}
```

**Response:**

```json
{
  "market_id": 0,
  "size": 1000000,
  "entry_price": 5000000,
  "is_long": true,
  "margin": 50000000,
  "unrealized_pnl": 0,
  "is_profit": true
}
```

### Get Positions

```
POST /account/positions
```

Get all positions for an address.

**Request Body:**

```json
{
  "address": "0x..."
}
```

**Response:**

```json
{
  "positions": [
    {
      "market_id": 0,
      "size": 1000000,
      "entry_price": 5000000,
      "is_long": true,
      "margin": 50000000,
      "unrealized_pnl": 0,
      "is_profit": true
    }
  ]
}
```

### Get Order

```
POST /order/get
```

Get information about a specific order.

**Request Body:**

```json
{
  "order_id": 123456789
}
```

**Response:**

```json
{
  "id": 123456789,
  "market_id": 0,
  "owner": "0x...",
  "is_buy": true,
  "price": 5000000,
  "original_size": 1000000,
  "remaining_size": 500000,
  "created_at": 12345
}
```

### List Orders

```
POST /order/list
```

List all orders for an address.

**Request Body:**

```json
{
  "address": "0x...",
  "market_id": 0
}
```

**Response:**

```json
{
  "orders": [
    {
      "id": 123456789,
      "market_id": 0,
      "owner": "0x...",
      "is_buy": true,
      "price": 5000000,
      "original_size": 1000000,
      "remaining_size": 500000,
      "created_at": 12345
    }
  ]
}
```

### Get Market Info

```
POST /market/info
```

Get information about a market.

**Request Body:**

```json
{
  "market_id": 0
}
```

**Response:**

```json
{
  "id": 0,
  "symbol": "BTC-PERP",
  "base_asset_id": 1,
  "quote_asset_id": 0,
  "price_decimals": 2,
  "size_decimals": 6,
  "min_order_size": 1,
  "max_leverage": 20
}
```

### Get Funding Rate

```
POST /market/funding
```

Get the current funding rate for a market.

**Request Body:**

```json
{
  "market_id": 0
}
```

**Response:**

```json
{
  "rate": 100,
  "last_update": 1234567890,
  "index": 1000,
  "mark_price": 5000000,
  "oracle_price": 5000000
}
```

### Set Agent

```
POST /agent/set
```

Set an agent with specific permissions (requires signed transaction).

**Request Body:**

```json
{
  "tx": "0x..." // hex-encoded SetAgent transaction
}
```

**Response:**

```json
{
  "success": true
}
```

### Remove Agent

```
POST /agent/remove
```

Remove an agent (requires signed transaction).

**Request Body:**

```json
{
  "tx": "0x..." // hex-encoded RemoveAgent transaction
}
```

**Response:**

```json
{
  "success": true
}
```

### List Agents

```
POST /agent/list
```

List all agents for an address.

**Request Body:**

```json
{
  "address": "0x..."
}
```

**Response:**

```json
{
  "agents": []
}
```

## WebSocket API

### Connection

Connect to the WebSocket endpoint:

```javascript
const ws = new WebSocket("ws://localhost:8545/ws");
```

### Message Format

All messages are JSON-encoded with a `type` field indicating the message type.

### Subscribe to Channel

```json
{
  "type": "Subscribe",
  "channel": "orderbook:0"
}
```

**Channels:**

- `orderbook:<market_id>`: Orderbook updates for a market
- `trades:<market_id>`: Trade updates for a market
- `positions:<address>`: Position updates for an address
- `funding:<market_id>`: Funding rate updates
- `liquidations`: Global liquidation events

### Unsubscribe from Channel

```json
{
  "type": "Unsubscribe",
  "channel": "orderbook:0"
}
```

### Message Types

#### OrderBook Update

```json
{
  "type": "OrderBookUpdate",
  "market_id": 0,
  "bids": [
    [5000000, 1000000],
    [4999000, 500000]
  ],
  "asks": [
    [5001000, 800000],
    [5002000, 600000]
  ]
}
```

#### Trade Update

```json
{
  "type": "TradeUpdate",
  "market_id": 0,
  "price": 5000000,
  "size": 1000000,
  "is_buy": true,
  "timestamp": 1234567890
}
```

#### Position Update

```json
{
  "type": "PositionUpdate",
  "market_id": 0,
  "trader": "0x...",
  "size": 1000000,
  "entry_price": 5000000,
  "is_long": true
}
```

#### Funding Update

```json
{
  "type": "FundingUpdate",
  "market_id": 0,
  "rate": 100
}
```

#### Liquidation Event

```json
{
  "type": "LiquidationEvent",
  "market_id": 0,
  "trader": "0x...",
  "size": 1000000,
  "price": 4800000
}
```

#### Ping/Pong

Keep-alive messages:

```json
{ "type": "Ping" }
{ "type": "Pong" }
```

#### Error

```json
{
  "type": "Error",
  "message": "Error description"
}
```

## Transaction Types

### Deposit

Deposit assets into the exchange.

```rust
TxPayload::Deposit(DepositTx {
    amount: 100_000_000_000, // 100,000 USDC (6 decimals)
    asset_id: 0,
})
```

### Withdraw

Withdraw assets from the exchange.

```rust
TxPayload::Withdraw(WithdrawTx {
    amount: 50_000_000_000, // 50,000 USDC
    asset_id: 0,
})
```

### Place Order

Place a new order.

```rust
TxPayload::PlaceOrder(PlaceOrderTx {
    market_id: 0,
    is_buy: true,
    order_type: OrderType::Limit,
    price: 50_000_00,     // $50,000 with 2 decimals
    size: 1_000_000,      // 1 BTC with 6 decimals
    time_in_force: TimeInForce::GTC,
    reduce_only: false,
    post_only: false,
})
```

**Order Types:**

- `Market`: Execute immediately at best available price
- `Limit`: Execute only at specified price or better

**Time in Force:**

- `GTC` (Good Till Cancel): Remains active until filled or cancelled
- `IOC` (Immediate or Cancel): Fill immediately, cancel remainder
- `FOK` (Fill or Kill): Fill entirely or cancel
- `PostOnly`: Only add liquidity, never take

### Cancel Order

Cancel an existing order.

```rust
TxPayload::CancelOrder(CancelOrderTx {
    order_id: 123456789,
})
```

### Close Position

Close an open position (creates market order on opposite side).

```rust
TxPayload::ClosePosition(ClosePositionTx {
    market_id: 0,
    size: 0, // 0 = close entire position
})
```

### Set Agent

Grant trading permissions to an agent address.

```rust
TxPayload::SetAgent(SetAgentTx {
    agent: agent_address,
    permissions: AgentPermissions {
        can_trade: true,
        can_deposit: false,
        can_withdraw: false,
    },
})
```

### Remove Agent

Revoke agent permissions.

```rust
TxPayload::RemoveAgent(RemoveAgentTx {
    agent: agent_address,
})
```

## Error Codes

| Code | Message               | Description                  |
| ---- | --------------------- | ---------------------------- |
| 400  | Invalid Request       | Malformed request body       |
| 401  | Unauthorized          | Invalid or missing signature |
| 429  | Too Many Requests     | Rate limit exceeded          |
| 500  | Internal Server Error | Server error occurred        |
| 503  | Service Unavailable   | Circuit breaker open         |

## Rate Limits

Default rate limits (configurable):

- REST API: 100 requests per second per IP
- WebSocket: 50 messages per second per connection

Circuit breaker thresholds:

- Failure threshold: 3 failures
- Success threshold: 2 successes
- Timeout: 100ms

## Examples

### JavaScript/TypeScript Client

```typescript
// Connect to WebSocket
const ws = new WebSocket("ws://localhost:8545/ws");

ws.onopen = () => {
  // Subscribe to BTC-PERP orderbook
  ws.send(
    JSON.stringify({
      type: "Subscribe",
      channel: "orderbook:0",
    })
  );
};

ws.onmessage = (event) => {
  const message = JSON.parse(event.data);

  if (message.type === "OrderBookUpdate") {
    console.log("Orderbook updated:", message);
  }
};

// Place an order via REST
async function placeOrder() {
  const response = await fetch("http://localhost:8545/tx/submit", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      tx: signedTxHex,
    }),
  });

  const result = await response.json();
  console.log("Order placed:", result.tx_hash);
}
```

### Python Client

```python
import requests
import json

# Get account balance
def get_balance(address, asset_id):
    response = requests.post(
        'http://localhost:8545/account/balance',
        json={'address': address, 'asset_id': asset_id}
    )
    return response.json()['balance']

# Submit transaction
def submit_tx(signed_tx_hex):
    response = requests.post(
        'http://localhost:8545/tx/submit',
        json={'tx': signed_tx_hex}
    )
    return response.json()['tx_hash']
```

## Support

- Documentation: https://docs.pranklin-dex.example.com
- API Status: https://status.pranklin-dex.example.com
- GitHub: https://github.com/pranklin-dex/pranklin-core
