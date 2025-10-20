mod error;
mod funding;
mod liquidation;
mod orderbook;
mod position;
mod risk;

pub use error::*;
pub use funding::*;
pub use liquidation::*;
pub use orderbook::*;
pub use position::*;
pub use risk::{MarginMode, RiskEngine};

use pranklin_state::{Order, StateManager};
use pranklin_tx::*;

/// Main engine for perp DEX operations
pub struct Engine {
    /// State manager
    state: StateManager,
    /// Orderbook manager (in-memory, fast)
    orderbook: orderbook::OrderbookManager,
    /// Position manager
    position_mgr: position::PositionManager,
    /// Liquidation engine
    liquidation: liquidation::LiquidationEngine,
    /// Funding rate calculator
    funding: funding::FundingRateCalculator,
    /// Risk engine
    risk: risk::RiskEngine,
}

impl Engine {
    /// Create a new engine
    pub fn new(state: StateManager) -> Self {
        Self {
            state,
            orderbook: orderbook::OrderbookManager::new(),
            position_mgr: position::PositionManager::new(),
            liquidation: liquidation::LiquidationEngine::new(),
            funding: funding::FundingRateCalculator::new(),
            risk: risk::RiskEngine::new(),
        }
    }

    /// Rebuild orderbook from state (for recovery after restart)
    ///
    /// This is called when the node restarts to reconstruct the in-memory
    /// orderbook from active orders stored in state.
    ///
    /// # Architecture
    /// - **State (JMT)**: Source of truth for active orders
    /// - **Orderbook (Memory)**: Fast matching engine, rebuilt from state
    /// - **Recovery**: O(n) where n = total active orders across all markets
    pub fn rebuild_orderbook_from_state(&mut self) -> Result<(), EngineError> {
        // Clear existing orderbook (in case of re-initialization)
        self.orderbook = orderbook::OrderbookManager::new();

        // CRITICAL FIX: Use list_all_markets instead of hard-coded range
        let markets_to_rebuild = self.state.list_all_markets()?;

        let mut _total_orders_recovered = 0;

        // Rebuild orderbook for each market
        for market_id in markets_to_rebuild {
            let active_order_ids = self.state.get_active_orders_by_market(market_id)?;

            for order_id in active_order_ids {
                // Load order from state
                if let Some(order) = self.state.get_order(order_id)? {
                    // Only add active orders to orderbook
                    if order.status == pranklin_state::OrderStatus::Active
                        && order.remaining_size > 0
                    {
                        // Add to orderbook
                        self.orderbook.add_order(order_id, &order);
                        _total_orders_recovered += 1;
                    } else {
                        // Clean up: remove non-active orders from active list
                        // This handles edge cases where state got out of sync
                        self.state.remove_active_order(market_id, order_id)?;
                    }
                }
            }
        }

        // Log recovery completion
        // Note: Logging would be added here in production
        // log::info!("🔄 Orderbook rebuilt from state: {} active orders recovered", total_orders_recovered);

        Ok(())
    }

    /// Get current orderbook state (for monitoring/debugging)
    #[allow(clippy::type_complexity)]
    pub fn get_orderbook_depth(
        &self,
        market_id: u32,
        depth: usize,
    ) -> (Vec<(u64, u64)>, Vec<(u64, u64)>) {
        self.orderbook.get_depth(market_id, depth)
    }

    /// Process a deposit transaction
    pub fn process_deposit(
        &mut self,
        tx: &Transaction,
        deposit: &DepositTx,
    ) -> Result<(), EngineError> {
        // Get current balance
        let current_balance = self.state.get_balance(tx.from, deposit.asset_id)?;

        // Add deposit amount
        let new_balance = current_balance
            .checked_add(deposit.amount)
            .ok_or(EngineError::Overflow)?;

        // Update balance
        self.state
            .set_balance(tx.from, deposit.asset_id, new_balance)?;

        Ok(())
    }

    /// Process a withdraw transaction
    pub fn process_withdraw(
        &mut self,
        tx: &Transaction,
        withdraw: &WithdrawTx,
    ) -> Result<(), EngineError> {
        // Check risk (available balance after considering positions)
        self.risk.check_withdraw_allowed(
            &self.state,
            tx.from,
            withdraw.asset_id,
            withdraw.amount,
        )?;

        // Get current balance
        let current_balance = self.state.get_balance(tx.from, withdraw.asset_id)?;

        // Check sufficient balance
        if current_balance < withdraw.amount {
            return Err(EngineError::InsufficientBalance);
        }

        // Subtract withdrawal amount
        let new_balance = current_balance - withdraw.amount;

        // Update balance
        self.state
            .set_balance(tx.from, withdraw.asset_id, new_balance)?;

        Ok(())
    }

    /// Process a transfer transaction
    pub fn process_transfer(
        &mut self,
        tx: &Transaction,
        transfer: &pranklin_tx::TransferTx,
    ) -> Result<(), EngineError> {
        // Prevent self-transfer
        if tx.from == transfer.to {
            return Err(EngineError::Other("Cannot transfer to self".to_string()));
        }

        // Validate asset exists
        let asset = self
            .state
            .get_asset(transfer.asset_id)?
            .ok_or_else(|| EngineError::Other("Asset not found".to_string()))?;

        // Check if asset can be transferred (must be collateral asset)
        if !asset.is_collateral {
            return Err(EngineError::Other(
                "Asset cannot be transferred".to_string(),
            ));
        }

        // Get sender balance
        let sender_balance = self.state.get_balance(tx.from, transfer.asset_id)?;

        // Check sufficient balance
        if sender_balance < transfer.amount {
            return Err(EngineError::InsufficientBalance);
        }

        // Deduct from sender
        let new_sender_balance = sender_balance - transfer.amount;
        self.state
            .set_balance(tx.from, transfer.asset_id, new_sender_balance)?;

        // Add to recipient
        let recipient_balance = self.state.get_balance(transfer.to, transfer.asset_id)?;
        let new_recipient_balance = recipient_balance
            .checked_add(transfer.amount)
            .ok_or(EngineError::Overflow)?;
        self.state
            .set_balance(transfer.to, transfer.asset_id, new_recipient_balance)?;

        Ok(())
    }

    /// Process a bridge deposit transaction (only bridge operators)
    pub fn process_bridge_deposit(
        &mut self,
        tx: &Transaction,
        deposit: &pranklin_tx::BridgeDepositTx,
    ) -> Result<(), EngineError> {
        // Verify caller is a bridge operator
        if !self.state.is_bridge_operator(tx.from)? {
            return Err(EngineError::Unauthorized);
        }

        // Validate asset exists
        self.state
            .get_asset(deposit.asset_id)?
            .ok_or_else(|| EngineError::Other("Asset not found".to_string()))?;

        // Get current balance
        let current_balance = self.state.get_balance(deposit.user, deposit.asset_id)?;

        // Add deposit amount
        let new_balance = current_balance
            .checked_add(deposit.amount)
            .ok_or(EngineError::Overflow)?;

        // Update balance
        self.state
            .set_balance(deposit.user, deposit.asset_id, new_balance)?;

        Ok(())
    }

    /// Process a bridge withdrawal transaction (only bridge operators)
    pub fn process_bridge_withdraw(
        &mut self,
        tx: &Transaction,
        withdraw: &pranklin_tx::BridgeWithdrawTx,
    ) -> Result<(), EngineError> {
        // Verify caller is a bridge operator
        if !self.state.is_bridge_operator(tx.from)? {
            return Err(EngineError::Unauthorized);
        }

        // Validate asset exists
        self.state
            .get_asset(withdraw.asset_id)?
            .ok_or_else(|| EngineError::Other("Asset not found".to_string()))?;

        // Get current balance
        let current_balance = self.state.get_balance(withdraw.user, withdraw.asset_id)?;

        // Check sufficient balance
        if current_balance < withdraw.amount {
            return Err(EngineError::InsufficientBalance);
        }

        // Subtract withdrawal amount
        let new_balance = current_balance - withdraw.amount;

        // Update balance
        self.state
            .set_balance(withdraw.user, withdraw.asset_id, new_balance)?;

        Ok(())
    }

    /// Process a place order transaction
    pub fn process_place_order(
        &mut self,
        tx: &Transaction,
        place_order: &PlaceOrderTx,
    ) -> Result<u64, EngineError> {
        // Validate market exists
        let market = self
            .state
            .get_market(place_order.market_id)?
            .ok_or(EngineError::MarketNotFound)?;

        // Validate price is on tick boundary (for limit orders)
        if place_order.price > 0 && !market.validate_price(place_order.price) {
            return Err(EngineError::Other(format!(
                "Price {} is not on tick boundary (tick_size={})",
                place_order.price, market.tick_size
            )));
        }

        // Validate order size bounds
        if place_order.size < market.min_order_size {
            return Err(EngineError::Other(format!(
                "Order size {} below minimum {}",
                place_order.size, market.min_order_size
            )));
        }
        if place_order.size > market.max_order_size {
            return Err(EngineError::Other(format!(
                "Order size {} exceeds maximum {}",
                place_order.size, market.max_order_size
            )));
        }

        // Check risk (margin requirements)
        self.risk
            .check_order_allowed(&self.state, tx.from, place_order)?;

        // Create order
        let order_id = self.generate_order_id()?;
        let order = Order {
            id: order_id,
            market_id: place_order.market_id,
            owner: tx.from,
            is_buy: place_order.is_buy,
            price: place_order.price,
            original_size: place_order.size,
            remaining_size: place_order.size,
            status: pranklin_state::OrderStatus::Active,
            created_at: self.state.version(),
            reduce_only: place_order.reduce_only,
            post_only: place_order.post_only,
        };

        // Try to match the order
        let fills = self
            .orderbook
            .match_order(&mut self.state, &order, &market)?;

        // Process fills
        for fill in &fills {
            self.process_fill(fill.clone())?;
        }

        // Update order with remaining size after matching
        let mut updated_order = order.clone();
        let total_filled = fills.iter().map(|f| f.size).sum::<u64>();
        updated_order.remaining_size = updated_order.remaining_size.saturating_sub(total_filled);

        // Determine final order status
        if updated_order.remaining_size == 0 {
            // Fully filled
            updated_order.status = pranklin_state::OrderStatus::Filled;
            self.state.set_order(order_id, updated_order)?;
            // No need to add to active orders list
        } else if updated_order.remaining_size > 0 {
            // Partially filled or unfilled
            match place_order.time_in_force {
                TimeInForce::GTC | TimeInForce::PostOnly => {
                    // Add to orderbook and active orders list
                    updated_order.status = pranklin_state::OrderStatus::Active;
                    self.state.set_order(order_id, updated_order.clone())?;
                    self.state
                        .add_active_order(place_order.market_id, order_id)?;
                    self.orderbook.add_order(order_id, &updated_order);
                }
                TimeInForce::IOC => {
                    // Immediate or cancel - mark as cancelled if not fully filled
                    updated_order.status = pranklin_state::OrderStatus::Cancelled;
                    self.state.set_order(order_id, updated_order)?;
                }
                TimeInForce::FOK => {
                    // Fill or kill - if not fully filled, revert
                    if updated_order.remaining_size == updated_order.original_size {
                        return Err(EngineError::OrderNotFilled);
                    }
                }
            }
        }

        Ok(order_id)
    }

    /// Process a cancel order transaction
    pub fn process_cancel_order(
        &mut self,
        tx: &Transaction,
        cancel: &CancelOrderTx,
    ) -> Result<(), EngineError> {
        // Get order
        let mut order = self
            .state
            .get_order(cancel.order_id)?
            .ok_or(EngineError::OrderNotFound)?;

        // Check ownership
        if order.owner != tx.from {
            return Err(EngineError::Unauthorized);
        }

        // Check if order is active
        if order.status != pranklin_state::OrderStatus::Active {
            return Err(EngineError::OrderNotFound); // Already filled or cancelled
        }

        // Remove from orderbook (memory)
        self.orderbook.remove_order(cancel.order_id, &order);

        // Update order status to Cancelled and save to state (for history)
        order.status = pranklin_state::OrderStatus::Cancelled;
        self.state.set_order(cancel.order_id, order.clone())?;

        // Remove from active orders list
        self.state
            .remove_active_order(order.market_id, cancel.order_id)?;

        Ok(())
    }

    /// Process a close position transaction
    pub fn process_close_position(
        &mut self,
        tx: &Transaction,
        close: &ClosePositionTx,
    ) -> Result<(), EngineError> {
        // Get position
        let position = self
            .state
            .get_position(tx.from, close.market_id)?
            .ok_or(EngineError::PositionNotFound)?;

        // Calculate size to close
        let size_to_close = if close.size == 0 {
            position.size
        } else {
            close.size.min(position.size)
        };

        // Create market order to close position
        let order = PlaceOrderTx {
            market_id: close.market_id,
            is_buy: !position.is_long, // Opposite side
            order_type: OrderType::Market,
            price: 0,
            size: size_to_close,
            time_in_force: TimeInForce::IOC,
            reduce_only: true,
            post_only: false,
        };

        self.process_place_order(tx, &order)?;

        Ok(())
    }

    /// Process a trade fill
    fn process_fill(&mut self, fill: Fill) -> Result<(), EngineError> {
        // Get market info for margin requirements
        let market = self
            .state
            .get_market(fill.market_id)?
            .ok_or(EngineError::MarketNotFound)?;

        // Update positions for both maker and taker
        self.position_mgr.update_position(
            &mut self.state,
            fill.taker,
            fill.market_id,
            fill.size,
            fill.price,
            fill.taker_is_buy,
            market.initial_margin_bps,
        )?;

        self.position_mgr.update_position(
            &mut self.state,
            fill.maker,
            fill.market_id,
            fill.size,
            fill.price,
            !fill.taker_is_buy,
            market.initial_margin_bps,
        )?;

        Ok(())
    }

    /// Update funding rate for a market
    pub fn update_funding_rate(
        &mut self,
        market_id: u32,
        mark_price: u64,
        oracle_price: u64,
        timestamp: u64,
    ) -> Result<(), EngineError> {
        self.funding.update_funding_rate(
            &mut self.state,
            market_id,
            mark_price,
            oracle_price,
            timestamp,
        )
    }

    /// Check if a specific position should be liquidated
    pub fn should_liquidate(
        &self,
        trader: alloy_primitives::Address,
        market_id: u32,
        mark_price: u64,
    ) -> Result<bool, EngineError> {
        let market = self
            .state
            .get_market(market_id)?
            .ok_or(EngineError::MarketNotFound)?;
        self.liquidation
            .should_liquidate(&self.state, trader, market_id, mark_price, &market)
    }

    /// Liquidate position with incentives (partial liquidation, insurance fund)
    pub fn liquidate_with_incentive(
        &mut self,
        trader: alloy_primitives::Address,
        market_id: u32,
        mark_price: u64,
        liquidator: alloy_primitives::Address,
    ) -> Result<Option<LiquidationResult>, EngineError> {
        self.liquidation.liquidate_with_incentive(
            &mut self.state,
            trader,
            market_id,
            mark_price,
            liquidator,
        )
    }

    /// Update risk index for liquidation monitoring
    pub fn update_risk_index(
        &mut self,
        trader: alloy_primitives::Address,
        market_id: u32,
        mark_price: u64,
    ) -> Result<(), EngineError> {
        self.liquidation
            .update_risk_index(&self.state, trader, market_id, mark_price)
    }

    /// Rebuild entire risk index for a market
    pub fn rebuild_risk_index(
        &mut self,
        market_id: u32,
        mark_price: u64,
    ) -> Result<(), EngineError> {
        self.liquidation
            .rebuild_risk_index(&self.state, market_id, mark_price)
    }

    /// Get at-risk positions
    pub fn get_at_risk_positions(
        &self,
        market_id: u32,
        threshold_bps: u32,
    ) -> Vec<(alloy_primitives::Address, u32)> {
        self.liquidation
            .get_at_risk_positions(market_id, threshold_bps)
    }

    /// Get insurance fund balance
    pub fn get_insurance_fund(&self, market_id: u32) -> InsuranceFund {
        self.liquidation.get_insurance_fund(market_id)
    }

    /// Batch liquidation processing
    pub fn process_liquidation_batch(
        &mut self,
        market_id: u32,
        mark_price: u64,
        liquidator: alloy_primitives::Address,
        max_liquidations: usize,
    ) -> Result<Vec<LiquidationResult>, EngineError> {
        self.liquidation.process_liquidation_batch(
            &mut self.state,
            market_id,
            mark_price,
            liquidator,
            max_liquidations,
        )
    }

    /// Auto-deleverage positions when insurance fund is depleted
    pub fn auto_deleverage(
        &mut self,
        market_id: u32,
        required_amount: u128,
        mark_price: u64,
    ) -> Result<Vec<(alloy_primitives::Address, u64)>, EngineError> {
        self.liquidation
            .auto_deleverage(&mut self.state, market_id, required_amount, mark_price)
    }

    /// Generate a unique order ID using atomic counter
    fn generate_order_id(&mut self) -> Result<u64, EngineError> {
        Ok(self.state.next_order_id()?)
    }

    /// Get state reference
    pub fn state(&self) -> &StateManager {
        &self.state
    }

    /// Get mutable state reference
    pub fn state_mut(&mut self) -> &mut StateManager {
        &mut self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deposit_withdraw() {
        use alloy_primitives::Address;
        use pranklin_state::PruningConfig;

        let temp_dir = tempfile::TempDir::new().unwrap();
        let state = StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();
        let mut engine = Engine::new(state);

        let sender = Address::ZERO; // Use a fixed address for tests
        let asset_id = 0; // USDC

        // Create deposit transaction
        let tx = Transaction::new(
            1,
            sender,
            TxPayload::Deposit(DepositTx {
                amount: 1000,
                asset_id,
            }),
        );

        // Process deposit
        if let TxPayload::Deposit(ref deposit) = tx.payload {
            engine.process_deposit(&tx, deposit).unwrap();
        }

        // Check balance
        let balance = engine.state().get_balance(sender, asset_id).unwrap();
        assert_eq!(balance, 1000);
    }
}
