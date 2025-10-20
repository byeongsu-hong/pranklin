use crate::EngineError;
use alloy_primitives::Address;
use pranklin_state::{Market, Order, StateManager};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

/// Trade fill information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fill {
    /// Maker address
    pub maker: Address,
    /// Taker address
    pub taker: Address,
    /// Market ID
    pub market_id: u32,
    /// Fill price
    pub price: u64,
    /// Fill size
    pub size: u64,
    /// Taker side (buy/sell)
    pub taker_is_buy: bool,
    /// Maker order ID
    pub maker_order_id: u64,
    /// Taker order ID
    pub taker_order_id: u64,
}

/// Order book depth: (bids, asks) where each is Vec<(price, size)>
type OrderBookDepth = (Vec<(u64, u64)>, Vec<(u64, u64)>);

/// Price level in the orderbook
#[derive(Debug, Clone)]
struct PriceLevel {
    /// Orders at this price (FIFO queue for price-time priority)
    orders: VecDeque<u64>,
    /// Total size at this price level
    total_size: u64,
}

impl PriceLevel {
    fn new() -> Self {
        Self {
            orders: VecDeque::new(),
            total_size: 0,
        }
    }

    fn add_order(&mut self, order_id: u64, size: u64) {
        self.orders.push_back(order_id);
        self.total_size += size;
    }

    fn remove_order(&mut self, order_id: u64, size: u64) -> bool {
        if let Some(pos) = self.orders.iter().position(|&id| id == order_id) {
            self.orders.remove(pos);
            self.total_size = self.total_size.saturating_sub(size);
            return true;
        }
        false
    }

    fn is_empty(&self) -> bool {
        self.orders.is_empty()
    }
}

/// Orderbook manager with price-time priority
#[derive(Debug, Clone, Default)]
pub struct OrderbookManager {
    /// All orders indexed by order ID (for fast lookup)
    pub(crate) orders: HashMap<u64, Order>,
    /// Active order IDs by market
    orders_by_market: HashMap<u32, HashSet<u64>>,
    /// Bid side (price -> orders) - higher prices first
    bids: HashMap<u32, BTreeMap<u64, PriceLevel>>,
    /// Ask side (price -> orders) - lower prices first
    asks: HashMap<u32, BTreeMap<u64, PriceLevel>>,
}

impl OrderbookManager {
    /// Create a new orderbook manager
    pub fn new() -> Self {
        Self {
            orders: HashMap::new(),
            orders_by_market: HashMap::new(),
            bids: HashMap::new(),
            asks: HashMap::new(),
        }
    }

    /// Add an order to the orderbook
    pub fn add_order(&mut self, order_id: u64, order: &Order) {
        // Store the order for fast lookup
        self.orders.insert(order_id, order.clone());

        // Track order by market
        self.orders_by_market
            .entry(order.market_id)
            .or_default()
            .insert(order_id);

        // Add to appropriate side
        if order.is_buy {
            let bids = self.bids.entry(order.market_id).or_default();
            let level = bids.entry(order.price).or_insert_with(PriceLevel::new);
            level.add_order(order_id, order.remaining_size);
        } else {
            let asks = self.asks.entry(order.market_id).or_default();
            let level = asks.entry(order.price).or_insert_with(PriceLevel::new);
            level.add_order(order_id, order.remaining_size);
        }
    }

    /// Remove an order from the orderbook
    pub fn remove_order(&mut self, order_id: u64, order: &Order) {
        // Remove from orders map
        self.orders.remove(&order_id);

        // Remove from market tracking
        if let Some(orders) = self.orders_by_market.get_mut(&order.market_id) {
            orders.remove(&order_id);
        }

        // Remove from appropriate side
        let book = if order.is_buy {
            self.bids.get_mut(&order.market_id)
        } else {
            self.asks.get_mut(&order.market_id)
        };

        if let Some(book) = book
            && let Some(level) = book.get_mut(&order.price)
        {
            level.remove_order(order_id, order.remaining_size);
            if level.is_empty() {
                book.remove(&order.price);
            }
        }
    }

    /// Match an incoming order against the orderbook
    pub fn match_order(
        &mut self,
        state: &mut StateManager,
        order: &Order,
        _market: &Market,
    ) -> Result<Vec<Fill>, EngineError> {
        let mut fills = Vec::new();
        let mut remaining_size = order.remaining_size;

        // Check for post-only violation
        if order.post_only && self.would_match(state, order)? {
            return Err(EngineError::PostOnlyWouldTake);
        }

        // Get the opposing book
        let opposing_book = if order.is_buy {
            self.asks.get_mut(&order.market_id)
        } else {
            self.bids.get_mut(&order.market_id)
        };

        if let Some(book) = opposing_book {
            // For buy orders, match against asks (lowest first)
            // For sell orders, match against bids (highest first)
            let prices: Vec<u64> = if order.is_buy {
                book.keys().copied().collect()
            } else {
                book.keys().rev().copied().collect()
            };

            for price in prices {
                // Check if price is acceptable
                if order.is_buy && order.price > 0 && price > order.price {
                    break; // Limit order, stop if ask is too high
                }
                if !order.is_buy && order.price > 0 && price < order.price {
                    break; // Limit order, stop if bid is too low
                }

                // Match against this price level
                if let Some(level) = book.get_mut(&price) {
                    let orders_to_match: Vec<u64> = level.orders.iter().copied().collect();

                    for maker_order_id in orders_to_match {
                        if remaining_size == 0 {
                            break;
                        }

                        // Get maker order from state
                        let maker_order = match state.get_order(maker_order_id)? {
                            Some(order) => order,
                            None => continue,
                        };

                        // Prevent self-trade: skip if maker and taker are the same
                        if maker_order.owner == order.owner {
                            continue;
                        }

                        // Calculate fill size
                        let fill_size = remaining_size.min(maker_order.remaining_size);

                        // Create fill
                        let fill = Fill {
                            maker: maker_order.owner,
                            taker: order.owner,
                            market_id: order.market_id,
                            price,
                            size: fill_size,
                            taker_is_buy: order.is_buy,
                            maker_order_id,
                            taker_order_id: order.id,
                        };
                        fills.push(fill);

                        // Update remaining sizes
                        remaining_size -= fill_size;

                        // Update maker order
                        let mut updated_maker = maker_order;
                        updated_maker.remaining_size -= fill_size;

                        if updated_maker.remaining_size == 0 {
                            // Fully filled: update status and remove from active orders
                            updated_maker.status = pranklin_state::OrderStatus::Filled;
                            state.set_order(maker_order_id, updated_maker.clone())?;
                            state.remove_active_order(updated_maker.market_id, maker_order_id)?;
                            level.remove_order(maker_order_id, fill_size);
                        } else {
                            // Partially filled: update remaining size, keep status as Active
                            state.set_order(maker_order_id, updated_maker)?;
                            level.total_size -= fill_size;
                        }
                    }

                    // Clean up empty price level
                    if level.is_empty() {
                        book.remove(&price);
                    }
                }

                if remaining_size == 0 {
                    break;
                }
            }
        }

        Ok(fills)
    }

    /// Check if an order would match immediately
    fn would_match(&self, _state: &StateManager, order: &Order) -> Result<bool, EngineError> {
        let opposing_book = if order.is_buy {
            self.asks.get(&order.market_id)
        } else {
            self.bids.get(&order.market_id)
        };

        if let Some(book) = opposing_book
            && let Some((&best_price, _)) = if order.is_buy {
                book.iter().next() // Lowest ask
            } else {
                book.iter().next_back() // Highest bid
            }
        {
            // Market order always matches if there's liquidity
            if order.price == 0 {
                return Ok(true);
            }

            // Check if limit order would cross
            if order.is_buy && order.price >= best_price {
                return Ok(true);
            }
            if !order.is_buy && order.price <= best_price {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Get all orders for a market
    pub fn get_orders_for_market(&self, market_id: u32) -> Vec<u64> {
        self.orders_by_market
            .get(&market_id)
            .map(|orders| orders.iter().copied().collect())
            .unwrap_or_default()
    }

    /// Get best bid price for a market
    pub fn get_best_bid(&self, market_id: u32) -> Option<u64> {
        self.bids.get(&market_id)?.keys().next_back().copied()
    }

    /// Get best ask price for a market
    pub fn get_best_ask(&self, market_id: u32) -> Option<u64> {
        self.asks.get(&market_id)?.keys().next().copied()
    }

    /// Get orderbook depth for a market (top N levels)
    pub fn get_depth(&self, market_id: u32, levels: usize) -> OrderBookDepth {
        let bids = self
            .bids
            .get(&market_id)
            .map(|book| {
                book.iter()
                    .rev()
                    .take(levels)
                    .map(|(price, level)| (*price, level.total_size))
                    .collect()
            })
            .unwrap_or_default();

        let asks = self
            .asks
            .get(&market_id)
            .map(|book| {
                book.iter()
                    .take(levels)
                    .map(|(price, level)| (*price, level.total_size))
                    .collect()
            })
            .unwrap_or_default();

        (bids, asks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::Address;

    #[test]
    fn test_orderbook_price_time_priority() {
        let mut orderbook = OrderbookManager::new();

        // Create orders at same price
        let order1 = Order {
            id: 1,
            market_id: 0,
            owner: Address::ZERO,
            is_buy: false,
            price: 50000,
            original_size: 100,
            remaining_size: 100,
            status: pranklin_state::OrderStatus::Active,
            created_at: 0,
            reduce_only: false,
            post_only: false,
        };

        let order2 = Order {
            id: 2,
            market_id: 0,
            owner: Address::ZERO,
            is_buy: false,
            price: 50000,
            original_size: 50,
            remaining_size: 50,
            status: pranklin_state::OrderStatus::Active,
            created_at: 1,
            reduce_only: false,
            post_only: false,
        };

        orderbook.add_order(1, &order1);
        orderbook.add_order(2, &order2);

        // Check that orders are in FIFO order
        let asks = orderbook.asks.get(&0).unwrap();
        let level = asks.get(&50000).unwrap();
        assert_eq!(level.orders[0], 1);
        assert_eq!(level.orders[1], 2);
        assert_eq!(level.total_size, 150);
    }

    #[test]
    fn test_best_bid_ask() {
        let mut orderbook = OrderbookManager::new();

        let bid = Order {
            id: 1,
            market_id: 0,
            owner: Address::ZERO,
            is_buy: true,
            price: 49000,
            original_size: 100,
            remaining_size: 100,
            status: pranklin_state::OrderStatus::Active,
            created_at: 0,
            reduce_only: false,
            post_only: false,
        };

        let ask = Order {
            id: 2,
            market_id: 0,
            owner: Address::ZERO,
            is_buy: false,
            price: 51000,
            original_size: 100,
            remaining_size: 100,
            status: pranklin_state::OrderStatus::Active,
            created_at: 0,
            reduce_only: false,
            post_only: false,
        };

        orderbook.add_order(1, &bid);
        orderbook.add_order(2, &ask);

        assert_eq!(orderbook.get_best_bid(0), Some(49000));
        assert_eq!(orderbook.get_best_ask(0), Some(51000));
    }

    #[test]
    fn test_orderbook_depth() {
        let mut orderbook = OrderbookManager::new();

        // Add multiple bids
        for i in 0..5 {
            let order = Order {
                id: i,
                market_id: 0,
                owner: Address::ZERO,
                is_buy: true,
                price: 50000 - (i * 100),
                original_size: 100,
                remaining_size: 100,
                status: pranklin_state::OrderStatus::Active,
                created_at: 0,
                reduce_only: false,
                post_only: false,
            };
            orderbook.add_order(i, &order);
        }

        let (bids, _) = orderbook.get_depth(0, 3);
        assert_eq!(bids.len(), 3);
        assert_eq!(bids[0].0, 50000); // Highest bid first
        assert_eq!(bids[1].0, 49900);
        assert_eq!(bids[2].0, 49800);
    }
}
