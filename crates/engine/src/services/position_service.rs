use super::ServiceContext;
use crate::{EngineError, OrderService, OrderbookManager, PositionManager, RiskEngine};
use pranklin_state::StateManager;
use pranklin_tx::{Address, ClosePositionTx, OrderType, PlaceOrderTx, TimeInForce};

/// Position service handles position operations
#[derive(Default)]
pub struct PositionService;

impl PositionService {
    /// Close a position (fully or partially)
    #[allow(clippy::too_many_arguments)]
    pub fn close_position(
        &self,
        state: &mut StateManager,
        ctx: &mut ServiceContext,
        orderbook: &mut OrderbookManager,
        position_mgr: &mut PositionManager,
        risk: &RiskEngine,
        order_service: &OrderService,
        trader: Address,
        close: &ClosePositionTx,
    ) -> Result<(), EngineError> {
        let position = state
            .get_position(trader, close.market_id)?
            .ok_or(EngineError::PositionNotFound)?;

        let size_to_close = if close.size == 0 {
            position.size
        } else {
            close.size.min(position.size)
        };

        // Create opposite market order
        let close_order = PlaceOrderTx {
            market_id: close.market_id,
            is_buy: !position.is_long,
            order_type: OrderType::Market,
            price: 0,
            size: size_to_close,
            time_in_force: TimeInForce::IOC,
            reduce_only: true,
            post_only: false,
        };

        order_service.place_order(
            state,
            ctx,
            orderbook,
            position_mgr,
            risk,
            trader,
            &close_order,
        )?;

        Ok(())
    }
}
