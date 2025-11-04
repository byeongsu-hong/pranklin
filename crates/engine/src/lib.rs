mod constants;
mod error;
mod event_store;
mod event_store_key;
mod events;
mod funding;
mod liquidation;
mod orderbook;
mod position;
mod risk;
mod services;

pub use error::*;
pub use event_store::*;
pub use event_store_key::*;
pub use events::*;
pub use funding::*;
pub use liquidation::*;
pub use orderbook::*;
pub use position::*;
pub use risk::{MarginMode, RiskEngine};
pub use services::*;

/// Type alias for orderbook depth (bids, asks)
pub type OrderbookDepth = (Vec<(u64, u64)>, Vec<(u64, u64)>);

use pranklin_state::StateManager;
use pranklin_tx::Address;

/// Main engine for perp DEX operations
///
/// Refactored with clean architecture principles:
/// - Domain services handle business logic
/// - Engine orchestrates services and manages state
/// - Event-driven architecture with proper encapsulation
pub struct Engine {
    /// State manager
    state: StateManager,
    /// Domain services
    account_service: AccountService,
    order_service: OrderService,
    position_service: PositionService,
    agent_service: AgentService,
    /// Infrastructure components
    orderbook: OrderbookManager,
    position_mgr: PositionManager,
    liquidation: LiquidationEngine,
    funding: FundingRateCalculator,
    risk: RiskEngine,
    /// Service context for event collection
    context: ServiceContext,
    /// Current block metadata
    current_block: BlockContext,
}

/// Block execution context
#[derive(Debug, Clone, Default)]
struct BlockContext {
    height: u64,
    timestamp: u64,
    tx_hash: alloy_primitives::B256,
}

impl Engine {
    /// Create a new engine
    pub fn new(state: StateManager) -> Self {
        Self {
            state,
            account_service: AccountService,
            order_service: OrderService,
            position_service: PositionService,
            agent_service: AgentService,
            orderbook: OrderbookManager::default(),
            position_mgr: PositionManager,
            liquidation: LiquidationEngine::default(),
            funding: FundingRateCalculator::default(),
            risk: RiskEngine::default(),
            context: ServiceContext::default(),
            current_block: BlockContext::default(),
        }
    }

    /// Begin processing a transaction
    pub fn begin_tx(&mut self, tx_hash: alloy_primitives::B256, block_height: u64, timestamp: u64) {
        self.current_block = BlockContext {
            height: block_height,
            timestamp,
            tx_hash,
        };
        self.context = ServiceContext::new();
    }

    /// Get and clear events for current transaction
    pub fn take_events(&mut self) -> Vec<pranklin_types::DomainEvent> {
        self.context
            .take_events()
            .into_iter()
            .enumerate()
            .map(|(idx, event)| {
                pranklin_types::DomainEvent::new(
                    self.current_block.height,
                    self.current_block.tx_hash,
                    idx as u32,
                    self.current_block.timestamp,
                    event,
                )
            })
            .collect()
    }

    // ============================================================================
    // Account Operations (Delegated to AccountService)
    // ============================================================================

    pub fn process_deposit(
        &mut self,
        from: Address,
        deposit: &pranklin_tx::DepositTx,
    ) -> Result<(), EngineError> {
        self.account_service
            .deposit(&mut self.state, &mut self.context, from, deposit)
    }

    pub fn process_withdraw(
        &mut self,
        from: Address,
        withdraw: &pranklin_tx::WithdrawTx,
    ) -> Result<(), EngineError> {
        self.account_service
            .withdraw(&mut self.state, &mut self.context, from, withdraw)
    }

    pub fn process_transfer(
        &mut self,
        from: Address,
        transfer: &pranklin_tx::TransferTx,
    ) -> Result<(), EngineError> {
        self.account_service
            .transfer(&mut self.state, &mut self.context, from, transfer)
    }

    pub fn process_bridge_deposit(
        &mut self,
        operator: Address,
        deposit: &pranklin_tx::BridgeDepositTx,
    ) -> Result<(), EngineError> {
        self.account_service
            .bridge_deposit(&mut self.state, &mut self.context, operator, deposit)
    }

    pub fn process_bridge_withdraw(
        &mut self,
        operator: Address,
        withdraw: &pranklin_tx::BridgeWithdrawTx,
    ) -> Result<(), EngineError> {
        self.account_service
            .bridge_withdraw(&mut self.state, &mut self.context, operator, withdraw)
    }

    // ============================================================================
    // Order Operations (Delegated to OrderService)
    // ============================================================================

    pub fn process_place_order(
        &mut self,
        trader: Address,
        order: &pranklin_tx::PlaceOrderTx,
    ) -> Result<u64, EngineError> {
        self.order_service.place_order(
            &mut self.state,
            &mut self.context,
            &mut self.orderbook,
            &mut self.position_mgr,
            &self.risk,
            trader,
            order,
        )
    }

    pub fn process_cancel_order(
        &mut self,
        trader: Address,
        cancel: &pranklin_tx::CancelOrderTx,
    ) -> Result<(), EngineError> {
        self.order_service.cancel_order(
            &mut self.state,
            &mut self.context,
            &mut self.orderbook,
            trader,
            cancel,
        )
    }

    // ============================================================================
    // Position Operations (Delegated to PositionService)
    // ============================================================================

    pub fn process_close_position(
        &mut self,
        trader: Address,
        close: &pranklin_tx::ClosePositionTx,
    ) -> Result<(), EngineError> {
        self.position_service.close_position(
            &mut self.state,
            &mut self.context,
            &mut self.orderbook,
            &mut self.position_mgr,
            &self.risk,
            &self.order_service,
            trader,
            close,
        )
    }

    // ============================================================================
    // Agent Operations (Delegated to AgentService)
    // ============================================================================

    pub fn process_set_agent(
        &mut self,
        account: Address,
        set_agent: &pranklin_tx::SetAgentTx,
    ) -> Result<(), EngineError> {
        self.agent_service
            .set_agent(&mut self.context, account, set_agent)
    }

    pub fn process_remove_agent(
        &mut self,
        account: Address,
        remove_agent: &pranklin_tx::RemoveAgentTx,
    ) -> Result<(), EngineError> {
        self.agent_service
            .remove_agent(&mut self.context, account, remove_agent)
    }

    // ============================================================================
    // Infrastructure Operations (Direct access)
    // ============================================================================

    /// Rebuild orderbook from state (for recovery)
    pub fn rebuild_orderbook_from_state(&mut self) -> Result<(), EngineError> {
        self.orderbook = OrderbookManager::new();

        let markets = self.state.list_all_markets()?;

        for market_id in markets {
            let active_orders = self.state.get_active_orders_by_market(market_id)?;

            for order_id in active_orders {
                if let Some(order) = self.state.get_order(order_id)? {
                    if order.status == pranklin_state::OrderStatus::Active
                        && order.remaining_size > 0
                    {
                        self.orderbook.add_order(order_id, &order);
                    } else {
                        self.state.remove_active_order(market_id, order_id)?;
                    }
                }
            }
        }

        Ok(())
    }

    /// Get orderbook depth
    pub fn get_orderbook_depth(&self, market_id: u32, depth: usize) -> OrderbookDepth {
        self.orderbook.get_depth(market_id, depth)
    }

    /// Update funding rate
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

    /// Check if position should be liquidated
    pub fn should_liquidate(
        &self,
        trader: Address,
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

    /// Liquidate position with incentives
    /// This will:
    /// 1. Cancel all active orders for the trader
    /// 2. Liquidate the position (partial or full)
    /// 3. Distribute liquidation fees to liquidator and insurance fund
    /// 4. Use insurance fund for bad debt if needed
    pub fn liquidate_with_incentive(
        &mut self,
        trader: Address,
        market_id: u32,
        mark_price: u64,
        liquidator: Address,
    ) -> Result<Option<LiquidationResult>, EngineError> {
        self.liquidation.liquidate_with_incentive(
            &mut self.state,
            &mut self.orderbook,
            trader,
            market_id,
            mark_price,
            liquidator,
        )
    }

    /// Update risk index
    pub fn update_risk_index(
        &mut self,
        trader: Address,
        market_id: u32,
        mark_price: u64,
    ) -> Result<(), EngineError> {
        self.liquidation
            .update_risk_index(&self.state, trader, market_id, mark_price)
    }

    /// Rebuild risk index
    pub fn rebuild_risk_index(
        &mut self,
        market_id: u32,
        mark_price: u64,
    ) -> Result<(), EngineError> {
        self.liquidation
            .rebuild_risk_index(&self.state, market_id, mark_price)
    }

    /// Get at-risk positions
    pub fn get_at_risk_positions(&self, market_id: u32, threshold_bps: u32) -> Vec<(Address, u32)> {
        self.liquidation
            .get_at_risk_positions(market_id, threshold_bps)
    }

    /// Get insurance fund
    pub fn get_insurance_fund(&self, market_id: u32) -> InsuranceFund {
        self.liquidation.get_insurance_fund(market_id)
    }

    /// Process liquidation batch
    /// Efficiently liquidates multiple positions in a single call
    pub fn process_liquidation_batch(
        &mut self,
        market_id: u32,
        mark_price: u64,
        liquidator: Address,
        max_liquidations: usize,
    ) -> Result<Vec<LiquidationResult>, EngineError> {
        self.liquidation.process_liquidation_batch(
            &mut self.state,
            &mut self.orderbook,
            market_id,
            mark_price,
            liquidator,
            max_liquidations,
        )
    }

    /// Auto-deleverage
    pub fn auto_deleverage(
        &mut self,
        market_id: u32,
        required_amount: u128,
        mark_price: u64,
    ) -> Result<Vec<(Address, u64)>, EngineError> {
        self.liquidation
            .auto_deleverage(&mut self.state, market_id, required_amount, mark_price)
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
    use pranklin_state::PruningConfig;
    use pranklin_tx::DepositTx;

    #[test]
    fn test_deposit_withdraw_refactored() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let state = StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();
        let mut engine = Engine::new(state);

        let sender = Address::ZERO;
        let asset_id = 0;

        let deposit = DepositTx {
            amount: 1000,
            asset_id,
        };

        engine.process_deposit(sender, &deposit).unwrap();

        let balance = engine.state().get_balance(sender, asset_id).unwrap();
        assert_eq!(balance, 1000);
    }
}
