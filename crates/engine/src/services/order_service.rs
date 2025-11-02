use super::ServiceContext;
use crate::{EngineError, OrderbookManager, PositionManager, RiskEngine};
use pranklin_state::{Order, OrderStatus, StateManager};
use pranklin_tx::{Address, CancelOrderTx, PlaceOrderTx, TimeInForce};
use pranklin_types::{Event, Fill};

/// Order service handles order lifecycle
#[derive(Default)]
pub struct OrderService;

impl OrderService {
    /// Place a new order
    #[allow(clippy::too_many_arguments)]
    pub fn place_order(
        &self,
        state: &mut StateManager,
        ctx: &mut ServiceContext,
        orderbook: &mut OrderbookManager,
        position_mgr: &mut PositionManager,
        risk: &RiskEngine,
        trader: Address,
        order_request: &PlaceOrderTx,
    ) -> Result<u64, EngineError> {
        // Validate market
        let market = state
            .get_market(order_request.market_id)?
            .ok_or(EngineError::MarketNotFound)?;

        // Validate price
        if order_request.price > 0 && !market.validate_price(order_request.price) {
            return Err(EngineError::Other(format!(
                "Invalid price {} does not align with tick boundary (tick size: {})",
                order_request.price, market.tick_size
            )));
        }

        // Validate size
        if order_request.size < market.min_order_size {
            return Err(EngineError::Other(format!(
                "Order size {} is below minimum size of {}",
                order_request.size, market.min_order_size
            )));
        }
        if order_request.size > market.max_order_size {
            return Err(EngineError::Other(format!(
                "Order size {} exceeds maximum size of {}",
                order_request.size, market.max_order_size
            )));
        }

        // Check risk
        risk.check_order_allowed(state, trader, order_request)?;

        // Create order
        let order_id = state.next_order_id()?;
        let order = Order {
            id: order_id,
            market_id: order_request.market_id,
            owner: trader,
            is_buy: order_request.is_buy,
            price: order_request.price,
            original_size: order_request.size,
            remaining_size: order_request.size,
            status: OrderStatus::Active,
            created_at: state.version(),
            reduce_only: order_request.reduce_only,
            post_only: order_request.post_only,
        };

        // Match order
        let fills = orderbook.match_order(state, &order, &market)?;

        // Process fills
        for fill in &fills {
            self.process_fill(state, ctx, position_mgr, fill, &market)?;
        }

        // Calculate remaining size
        let mut updated_order = order;
        let total_filled: u64 = fills.iter().map(|f| f.size).sum();
        updated_order.remaining_size = updated_order.remaining_size.saturating_sub(total_filled);

        let order_type = order_request.order_type;

        ctx.emit(pranklin_types::Event::OrderPlaced {
            order_id,
            owner: trader,
            market_id: order_request.market_id,
            is_buy: order_request.is_buy,
            price: order_request.price,
            size: order_request.size,
            order_type,
        });

        // Determine final status
        if updated_order.remaining_size == 0 {
            updated_order.status = OrderStatus::Filled;
            state.set_order(order_id, updated_order)?;
        } else {
            match order_request.time_in_force {
                TimeInForce::GTC | TimeInForce::PostOnly => {
                    updated_order.status = OrderStatus::Active;
                    state.set_order(order_id, updated_order.clone())?;
                    state.add_active_order(order_request.market_id, order_id)?;
                    orderbook.add_order(order_id, &updated_order);
                }
                TimeInForce::IOC => {
                    updated_order.status = OrderStatus::Cancelled;
                    state.set_order(order_id, updated_order)?;
                }
                TimeInForce::FOK => {
                    // FOK must be completely filled or fail
                    if updated_order.remaining_size > 0 {
                        return Err(EngineError::OrderNotFilled);
                    }
                }
            }
        }

        Ok(order_id)
    }

    /// Cancel an order
    pub fn cancel_order(
        &self,
        state: &mut StateManager,
        ctx: &mut ServiceContext,
        orderbook: &mut OrderbookManager,
        trader: Address,
        cancel: &CancelOrderTx,
    ) -> Result<(), EngineError> {
        let mut order = state
            .get_order(cancel.order_id)?
            .ok_or(EngineError::OrderNotFound)?;

        if order.owner != trader {
            return Err(EngineError::Unauthorized);
        }

        if order.status != OrderStatus::Active {
            return Err(EngineError::OrderNotFound);
        }

        orderbook.remove_order(cancel.order_id, &order);

        order.status = OrderStatus::Cancelled;
        state.set_order(cancel.order_id, order.clone())?;
        state.remove_active_order(order.market_id, cancel.order_id)?;

        ctx.emit(Event::OrderCancelled {
            order_id: cancel.order_id,
            owner: trader,
            market_id: order.market_id,
            remaining_size: order.remaining_size,
        });

        Ok(())
    }

    /// Process a fill
    fn process_fill(
        &self,
        state: &mut StateManager,
        ctx: &mut ServiceContext,
        position_mgr: &mut PositionManager,
        fill: &Fill,
        market: &pranklin_state::Market,
    ) -> Result<(), EngineError> {
        // Update positions
        position_mgr.update_position(
            state,
            fill.taker,
            fill.market_id,
            fill.size,
            fill.price,
            fill.taker_is_buy,
            market.initial_margin_bps,
        )?;

        position_mgr.update_position(
            state,
            fill.maker,
            fill.market_id,
            fill.size,
            fill.price,
            !fill.taker_is_buy,
            market.initial_margin_bps,
        )?;

        // Emit fill event
        // TODO: Calculate actual maker_fee and taker_fee based on market config
        let maker_fee = 0u128;
        let taker_fee = 0u128;

        ctx.emit(Event::OrderFilled {
            maker_order_id: fill.maker_order_id,
            taker_order_id: fill.taker_order_id,
            maker: fill.maker,
            taker: fill.taker,
            market_id: fill.market_id,
            price: fill.price,
            size: fill.size,
            taker_is_buy: fill.taker_is_buy,
            maker_fee,
            taker_fee,
        });

        Ok(())
    }
}
