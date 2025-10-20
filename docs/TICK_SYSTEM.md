# Tick-Based Order Management

## Overview

Pranklin implements a **tick-based order management system** for price discretization. This is an industry-standard approach used by major exchanges to ensure price precision, market structure consistency, and efficient order matching.

## What is a Tick?

A **tick** is the minimum price increment allowed for orders in a market. All order prices must be exact multiples of the tick size.

### Example

If `tick_size = 1000` (representing $0.10):

- ✅ Valid prices: `50_000`, `51_000`, `52_000` (multiples of 1000)
- ❌ Invalid prices: `50_123`, `50_500`, `50_999` (not multiples of 1000)

## Architecture

### 1. Market Configuration

Each market has a `tick_size` field that defines the minimum price increment:

```rust
pub struct Market {
    pub id: u32,
    pub symbol: String,
    pub tick_size: u64,  // Minimum price increment
    // ... other fields
}
```

### 2. Price Validation

When placing a limit order, the system validates that the price is on a valid tick boundary:

```rust
// In process_place_order()
if place_order.price > 0 && !market.validate_price(place_order.price) {
    return Err(EngineError::Other(format!(
        "Price {} is not on tick boundary (tick_size={})",
        place_order.price, market.tick_size
    )));
}
```

Market orders (price = 0) bypass tick validation.

### 3. Liquidation Price Normalization

During liquidation, mark prices are normalized to the nearest tick boundary:

```rust
// In liquidate_with_incentive()
let liquidation_price = market.normalize_price(mark_price);
```

This ensures that liquidations occur at valid tick boundaries, maintaining market integrity.

## Utility Functions

The `Market` struct provides several utility functions for tick management:

### `normalize_price(price: u64) -> u64`

Rounds a price to the nearest valid tick boundary.

```rust
market.normalize_price(50_123) // => 50_000 (rounds down)
market.normalize_price(50_500) // => 51_000 (rounds up)
```

### `validate_price(price: u64) -> bool`

Checks if a price is on a valid tick boundary.

```rust
market.validate_price(50_000)  // => true
market.validate_price(50_123)  // => false
```

### `price_to_tick(price: u64) -> u64`

Converts a price to a tick ID.

```rust
market.price_to_tick(50_000)  // => 50 (if tick_size=1000)
```

### `tick_to_price(tick: u64) -> u64`

Converts a tick ID back to a price.

```rust
market.tick_to_price(50)  // => 50_000 (if tick_size=1000)
```

## Performance Impact

### Validation Overhead

- **O(1)** single modulo operation (`price.is_multiple_of(tick_size)`)
- **~1-2 CPU cycles** on modern processors
- **Negligible** compared to order matching logic

### Memory Impact

- No additional memory required
- Uses existing `Market` struct field

## Why Tick-Based Ordering?

### 1. Price Precision

Prevents arbitrary price increments that can cause:

- Rounding errors in calculations
- Inconsistent pricing across different systems
- Unfair advantages for traders with higher precision

### 2. Market Structure

Provides clear price levels for:

- Aggregated orderbook display
- Market depth visualization
- Price level clustering

### 3. Fair Trading

Ensures:

- All traders operate under the same price constraints
- No front-running with micro-price advantages
- Predictable price movements

### 4. Liquidity Aggregation

Orders at the same tick can be:

- Easily aggregated for display
- Efficiently matched
- Consistently reported

## Example: BTC-PERP Market

```rust
let market = Market {
    id: 0,
    symbol: "BTC-PERP".to_string(),
    tick_size: 1000,  // $0.10 increments (if base unit is $0.0001)
    // ... other fields
};

// Valid orders
place_order(50_000);  // $50.00
place_order(51_000);  // $51.00

// Invalid order (will be rejected)
place_order(50_123);  // Not a multiple of 1000
```

## Integration Points

### Order Placement

- `Engine::process_place_order()` validates tick boundaries
- Rejects orders with invalid prices

### Liquidation

- `LiquidationEngine::liquidate_with_incentive()` normalizes liquidation prices
- Ensures liquidations occur at valid tick boundaries

### Risk Management

- Mark prices used in margin calculations are expected to be on tick boundaries
- Risk checks are performed with normalized prices

## Testing

Comprehensive tests ensure tick validation works correctly:

### `test_tick_validation`

- Tests valid tick prices are accepted
- Tests invalid tick prices are rejected
- Tests error messages contain "tick boundary"

### `test_market_tick_utilities`

- Tests `normalize_price()` rounding behavior
- Tests `validate_price()` correctness
- Tests `price_to_tick()` and `tick_to_price()` conversions

## Best Practices

### 1. Choose Appropriate Tick Sizes

- **High-value assets** (e.g., BTC): Larger tick sizes ($0.10 - $1.00)
- **Low-value assets** (e.g., altcoins): Smaller tick sizes ($0.001 - $0.01)
- **Stablecoins**: Very small tick sizes ($0.0001)

### 2. Normalize External Prices

Always normalize prices from external sources (oracles, APIs):

```rust
let mark_price = oracle.get_price();
let normalized_price = market.normalize_price(mark_price);
```

### 3. Display Tick Size to Users

Frontend applications should:

- Display the market's tick size
- Prevent users from entering invalid prices
- Auto-round prices to nearest tick

### 4. Handle Tick Changes

If tick size needs to be changed:

1. Cancel all active orders
2. Update market configuration
3. Notify traders
4. Allow order resubmission

## Future Enhancements

### 1. Dynamic Tick Sizes

Adjust tick sizes based on:

- Market volatility
- Asset price changes
- Trading volume

### 2. Tick-Level Indexing

For ultra-low-latency matching:

- Index orders by tick ID
- O(1) lookup for best bid/ask at each tick
- Faster price-level aggregation

### 3. Tick-Based Market Data

Provide tick-level market data:

- Volume by tick
- Order count by tick
- Time at each tick

## Comparison with Other Systems

| Exchange    | Tick System | Example (BTC)           |
| ----------- | ----------- | ----------------------- |
| **Binance** | ✅ Yes      | $0.10                   |
| **BitMEX**  | ✅ Yes      | $0.50                   |
| **dYdX**    | ✅ Yes      | $1.00                   |
| **Pranklin**   | ✅ Yes      | Configurable per market |

## Conclusion

The tick-based order management system provides:

- ✅ **Industry-standard** pricing structure
- ✅ **Negligible performance** overhead
- ✅ **Essential for fairness** and market integrity
- ✅ **Future-ready** for advanced features

This system is a foundational component of Pranklin's order management and is critical for operating a professional-grade perpetual DEX.
