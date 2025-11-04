use crate::{
    EngineError, OrderbookManager, PositionManager,
    constants::{
        BASIS_POINTS, DEFAULT_INSURANCE_FEE_BPS, DEFAULT_LIQUIDATOR_FEE_BPS,
        DEFAULT_MIN_INSURANCE_RATIO_BPS, MARGIN_BUFFER_BPS, MIN_LIQUIDATION_PCT,
    },
};
use alloy_primitives::Address;
use pranklin_state::{Market, OrderStatus, Position, StateManager};
use std::collections::{BinaryHeap, HashMap};

/// Position risk information for liquidation priority
#[derive(Debug, Clone, PartialEq)]
struct PositionRisk {
    trader: Address,
    market_id: u32,
    margin_ratio: u32, // basis points
    position_value: u128,
    equity: u128,
}

impl Eq for PositionRisk {}

impl PartialOrd for PositionRisk {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PositionRisk {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Lower margin ratio = higher priority (more risky)
        other.margin_ratio.cmp(&self.margin_ratio)
    }
}

/// Liquidation result details
#[derive(Debug, Clone, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct LiquidationResult {
    pub trader: Address,
    pub market_id: u32,
    pub liquidated_size: u64,
    pub liquidation_price: u64,
    pub liquidation_fee: u128,
    pub remaining_equity: u128,
    pub liquidator: Option<Address>,
    pub liquidator_reward: u128,
    pub insurance_fund_contribution: u128,
    pub insurance_fund_usage: u128,
}

/// Insurance fund state per market
#[derive(Debug, Clone, Default, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct InsuranceFund {
    pub balance: u128,
    pub total_contributions: u128,
    pub total_payouts: u128,
}

/// Auto-Deleveraging candidate
#[derive(Debug, Clone)]
struct AdlCandidate {
    trader: Address,
    #[allow(dead_code)]
    market_id: u32,
    position_size: u64,
    #[allow(dead_code)]
    unrealized_pnl: u128,
    #[allow(dead_code)]
    is_profit: bool,
    profit_score: i128, // PnL * leverage
}

/// Advanced liquidation engine with partial liquidations, insurance fund, and ADL
#[derive(Debug, Clone)]
pub struct LiquidationEngine {
    /// Position manager for calculations
    position_mgr: PositionManager,
    /// At-risk positions index (market_id -> priority queue)
    at_risk_positions: HashMap<u32, BinaryHeap<PositionRisk>>,
    /// Insurance fund per market
    insurance_funds: HashMap<u32, InsuranceFund>,
    /// Liquidation fee split: (liquidator_bps, insurance_fund_bps)
    fee_split: (u32, u32),
    /// Minimum insurance fund ratio (basis points)
    min_insurance_ratio: u32,
    /// Enable partial liquidations
    enable_partial_liquidations: bool,
    /// ADL enabled
    adl_enabled: bool,
}

impl Default for LiquidationEngine {
    fn default() -> Self {
        Self {
            position_mgr: PositionManager,
            at_risk_positions: HashMap::new(),
            insurance_funds: HashMap::new(),
            fee_split: (DEFAULT_LIQUIDATOR_FEE_BPS, DEFAULT_INSURANCE_FEE_BPS),
            min_insurance_ratio: DEFAULT_MIN_INSURANCE_RATIO_BPS,
            enable_partial_liquidations: true,
            adl_enabled: true,
        }
    }
}

impl LiquidationEngine {
    /// Configure fee split between liquidator and insurance fund
    pub fn set_fee_split(&mut self, liquidator_bps: u32, insurance_fund_bps: u32) {
        assert_eq!(
            liquidator_bps + insurance_fund_bps,
            BASIS_POINTS as u32,
            "Fee split must sum to {} bps",
            BASIS_POINTS
        );
        self.fee_split = (liquidator_bps, insurance_fund_bps);
    }

    /// Calculate position equity (margin +/- PnL)
    fn calculate_position_equity(
        &self,
        position: &Position,
        mark_price: u64,
    ) -> Result<u128, EngineError> {
        let (pnl, is_profit) = self.position_mgr.calculate_pnl(position, mark_price)?;
        let equity = if is_profit {
            position
                .margin
                .checked_add(pnl)
                .ok_or(EngineError::Overflow)?
        } else {
            position.margin.saturating_sub(pnl)
        };
        Ok(equity)
    }

    /// Calculate position value at mark price
    fn calculate_position_value(position: &Position, mark_price: u64) -> Result<u128, EngineError> {
        (position.size as u128)
            .checked_mul(mark_price as u128)
            .ok_or(EngineError::Overflow)
    }

    /// Update at-risk positions index for a single position
    pub fn update_risk_index(
        &mut self,
        state: &StateManager,
        trader: Address,
        market_id: u32,
        mark_price: u64,
    ) -> Result<(), EngineError> {
        let position = match state.get_position(trader, market_id)? {
            Some(pos) => pos,
            None => return Ok(()),
        };

        if position.size == 0 {
            return Ok(());
        }

        let risk = self.create_position_risk(trader, market_id, &position, mark_price)?;

        self.at_risk_positions
            .entry(market_id)
            .or_default()
            .push(risk);

        Ok(())
    }

    /// Create a PositionRisk from a position
    fn create_position_risk(
        &self,
        trader: Address,
        market_id: u32,
        position: &Position,
        mark_price: u64,
    ) -> Result<PositionRisk, EngineError> {
        let margin_ratio = self.calculate_margin_ratio(position, mark_price)?;
        let position_value = Self::calculate_position_value(position, mark_price)?;
        let equity = self.calculate_position_equity(position, mark_price)?;

        Ok(PositionRisk {
            trader,
            market_id,
            margin_ratio,
            position_value,
            equity,
        })
    }

    /// Rebuild the entire risk index for a market
    /// This should be called periodically to clean up stale entries
    pub fn rebuild_risk_index(
        &mut self,
        state: &StateManager,
        market_id: u32,
        mark_price: u64,
    ) -> Result<(), EngineError> {
        // Clear existing index for this market
        self.at_risk_positions.insert(market_id, BinaryHeap::new());

        // Get all positions in the market
        let positions = state.get_all_positions_in_market(market_id)?;

        for (trader, position) in positions {
            if position.size == 0 {
                continue;
            }

            let risk = self.create_position_risk(trader, market_id, &position, mark_price)?;

            self.at_risk_positions
                .entry(market_id)
                .or_default()
                .push(risk);
        }

        Ok(())
    }

    /// Get positions at risk of liquidation
    pub fn get_at_risk_positions(&self, market_id: u32, threshold_bps: u32) -> Vec<(Address, u32)> {
        if let Some(heap) = self.at_risk_positions.get(&market_id) {
            heap.iter()
                .filter(|p| p.margin_ratio < threshold_bps)
                .map(|p| (p.trader, p.market_id))
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Cancel all active orders for a trader in a specific market
    /// This is called before liquidation to free up margin and prevent new positions
    fn cancel_trader_orders(
        &self,
        state: &mut StateManager,
        orderbook: &mut OrderbookManager,
        trader: Address,
        market_id: u32,
    ) -> Result<Vec<u64>, EngineError> {
        let mut cancelled_orders = Vec::new();

        // Get all active orders for this market
        let active_order_ids = state.get_active_orders_by_market(market_id)?;

        // Filter and cancel orders belonging to the trader
        for order_id in active_order_ids {
            if let Some(mut order) = state.get_order(order_id)?
                && order.owner == trader
                && order.status == OrderStatus::Active
            {
                // Remove from orderbook
                orderbook.remove_order(order_id, &order);

                // Update order status
                order.status = OrderStatus::Cancelled;
                state.set_order(order_id, order)?;

                // Remove from active orders
                state.remove_active_order(market_id, order_id)?;

                cancelled_orders.push(order_id);
            }
        }

        Ok(cancelled_orders)
    }

    /// Check and liquidate positions with liquidator incentives
    ///
    /// Liquidation process:
    /// 1. Verify position needs liquidation
    /// 2. Cancel all active orders for the trader in this market
    /// 3. Calculate liquidation size (partial or full)
    /// 4. Execute liquidation and distribute fees
    /// 5. Use insurance fund if needed for bad debt
    pub fn liquidate_with_incentive(
        &mut self,
        state: &mut StateManager,
        orderbook: &mut OrderbookManager,
        trader: Address,
        market_id: u32,
        mark_price: u64,
        liquidator: Address,
    ) -> Result<Option<LiquidationResult>, EngineError> {
        // CRITICAL: Validate mark_price early
        if mark_price == 0 {
            return Err(EngineError::Other(
                "Invalid mark price: cannot be zero".to_string(),
            ));
        }

        let market = state
            .get_market(market_id)?
            .ok_or(EngineError::MarketNotFound)?;

        // Normalize mark price to tick boundary for consistency
        let liquidation_price = market.normalize_price(mark_price);

        if !self.should_liquidate(state, trader, market_id, liquidation_price, &market)? {
            return Ok(None);
        }

        // Step 1: Cancel all active orders for the trader in this market
        // This frees up margin and prevents new positions from being opened
        let _cancelled_orders = self.cancel_trader_orders(state, orderbook, trader, market_id)?;

        let position = state
            .get_position(trader, market_id)?
            .ok_or(EngineError::PositionNotFound)?;

        // Determine liquidation size
        let liquidation_size = if self.enable_partial_liquidations {
            self.calculate_partial_liquidation_size(&position, liquidation_price, &market)?
        } else {
            position.size
        };

        // Calculate fees and rewards
        let liquidation_value = (liquidation_size as u128)
            .checked_mul(liquidation_price as u128)
            .ok_or(EngineError::Overflow)?;

        let liquidation_fee = liquidation_value
            .checked_mul(market.liquidation_fee_bps as u128)
            .ok_or(EngineError::Overflow)?
            / BASIS_POINTS;

        let liquidator_reward = liquidation_fee
            .checked_mul(self.fee_split.0 as u128)
            .ok_or(EngineError::Overflow)?
            / BASIS_POINTS;

        let insurance_contribution = liquidation_fee
            .checked_mul(self.fee_split.1 as u128)
            .ok_or(EngineError::Overflow)?
            / BASIS_POINTS;

        // Calculate equity
        let equity = self.calculate_position_equity(&position, liquidation_price)?;

        // Calculate remaining equity after fees
        let mut remaining_equity = equity.saturating_sub(liquidation_fee);

        // If remaining equity is negative (loss), use insurance fund to cover
        let insurance_fund_usage = if equity < liquidation_fee {
            let shortfall = liquidation_fee - equity;
            let insurance_fund = self.insurance_funds.entry(market_id).or_default();

            if insurance_fund.balance >= shortfall {
                // Insurance fund covers the loss
                insurance_fund.balance -= shortfall;
                insurance_fund.total_payouts += shortfall;
                remaining_equity = 0;
                shortfall
            } else {
                // Insurance fund depleted - need ADL
                let covered = insurance_fund.balance;
                insurance_fund.balance = 0;
                insurance_fund.total_payouts += covered;
                remaining_equity = 0;
                covered
            }
        } else {
            0
        };

        // Execute liquidation
        if liquidation_size >= position.size {
            // Full liquidation
            state.delete_position(trader, market_id)?;
        } else {
            // Partial liquidation
            if position.size == 0 {
                // Safety: Should not happen, but handle gracefully
                return Err(EngineError::Other(
                    "Cannot partially liquidate zero-sized position".to_string(),
                ));
            }

            let mut updated_position = position.clone();
            updated_position.size -= liquidation_size;

            // Adjust margin proportionally
            let margin_reduction = position
                .margin
                .checked_mul(liquidation_size as u128)
                .ok_or(EngineError::Overflow)?
                / position.size as u128;
            updated_position.margin = updated_position.margin.saturating_sub(margin_reduction);

            state.set_position(trader, market_id, updated_position)?;
        }

        // Reward liquidator
        let liquidator_balance = state.get_balance(liquidator, market.quote_asset_id)?;
        state.set_balance(
            liquidator,
            market.quote_asset_id,
            liquidator_balance
                .checked_add(liquidator_reward)
                .ok_or(EngineError::Overflow)?,
        )?;

        // Update insurance fund
        let insurance_fund = self.insurance_funds.entry(market_id).or_default();
        insurance_fund.balance = insurance_fund
            .balance
            .checked_add(insurance_contribution)
            .ok_or(EngineError::Overflow)?;
        insurance_fund.total_contributions = insurance_fund
            .total_contributions
            .checked_add(insurance_contribution)
            .ok_or(EngineError::Overflow)?;

        // Return remaining equity to trader if any
        if remaining_equity > 0 {
            let trader_balance = state.get_balance(trader, market.quote_asset_id)?;
            state.set_balance(
                trader,
                market.quote_asset_id,
                trader_balance
                    .checked_add(remaining_equity)
                    .ok_or(EngineError::Overflow)?,
            )?;
        }

        // Normalize liquidation price to tick boundary
        let liquidation_price = market.normalize_price(mark_price);

        Ok(Some(LiquidationResult {
            trader,
            market_id,
            liquidated_size: liquidation_size,
            liquidation_price,
            liquidation_fee,
            remaining_equity,
            liquidator: Some(liquidator),
            liquidator_reward,
            insurance_fund_contribution: insurance_contribution,
            insurance_fund_usage,
        }))
    }

    /// Calculate optimal partial liquidation size
    fn calculate_partial_liquidation_size(
        &self,
        position: &Position,
        mark_price: u64,
        market: &Market,
    ) -> Result<u64, EngineError> {
        // CRITICAL: Check mark_price early to prevent division by zero
        if mark_price == 0 {
            return Ok(position.size); // Full liquidation if price is invalid
        }

        // Edge case: zero-sized position
        if position.size == 0 {
            return Ok(0);
        }

        // Calculate how much to liquidate to bring margin ratio back to maintenance + buffer
        let target_margin_ratio = market.maintenance_margin_bps + MARGIN_BUFFER_BPS;

        let position_value = Self::calculate_position_value(position, mark_price)?;

        // Edge case: position value is zero
        if position_value == 0 {
            return Ok(0);
        }

        let equity = self.calculate_position_equity(position, mark_price)?;

        // Calculate required equity for target margin ratio
        let required_equity = position_value
            .checked_mul(target_margin_ratio as u128)
            .ok_or(EngineError::Overflow)?
            / BASIS_POINTS;

        // Edge case: already above target margin ratio
        if equity >= required_equity {
            return Ok(0);
        }

        // Calculate equity deficit (how much we need to restore)
        let equity_deficit = required_equity.saturating_sub(equity);

        // Calculate the size to liquidate to restore the required equity
        // Adjusted formula accounting for liquidation fee:
        // (liquidation_size * mark_price * (1 + liquidation_fee)) should cover equity_deficit
        let liquidation_fee_bps = market.liquidation_fee_bps as u128;

        let liquidation_size_raw = equity_deficit
            .checked_mul(BASIS_POINTS)
            .ok_or(EngineError::Overflow)?
            .checked_div(mark_price as u128 * (BASIS_POINTS + liquidation_fee_bps))
            .ok_or(EngineError::DivisionByZero)? as u64;

        // Apply bounds:
        // Minimum: 10% of position or min_order_size, whichever is larger
        // Maximum: 100% of position
        let min_liquidation_pct = position.size / MIN_LIQUIDATION_PCT;
        let min_liquidation = min_liquidation_pct.max(market.min_order_size);
        let max_liquidation = position.size;

        let liquidation_size = liquidation_size_raw
            .max(min_liquidation)
            .min(max_liquidation);

        // Edge case: if calculated size is less than min_order_size but position is large,
        // we might need full liquidation
        if liquidation_size < market.min_order_size && position.size > market.min_order_size {
            // If the deficit is very small, try minimum liquidation
            return Ok(market.min_order_size.min(position.size));
        }

        // Edge case: if calculated size would leave a position smaller than min_order_size,
        // do full liquidation instead
        let remaining_size = position.size.saturating_sub(liquidation_size);
        if remaining_size > 0 && remaining_size < market.min_order_size {
            return Ok(position.size); // Full liquidation
        }

        Ok(liquidation_size)
    }

    /// ADL (Auto-Deleveraging) - reduce profitable positions when insurance fund is depleted
    pub fn auto_deleverage(
        &mut self,
        state: &mut StateManager,
        market_id: u32,
        required_amount: u128,
        mark_price: u64,
    ) -> Result<Vec<(Address, u64)>, EngineError> {
        if !self.adl_enabled {
            return Ok(Vec::new());
        }

        let mut candidates = self.find_adl_candidates(state, market_id, mark_price)?;

        // Sort by profit score (highest first)
        candidates.sort_by(|a, b| b.profit_score.cmp(&a.profit_score));

        let mut deleveraged = Vec::new();
        let mut collected_amount = 0u128;

        for candidate in candidates {
            if collected_amount >= required_amount {
                break;
            }

            // Calculate how much to deleverage
            let position_value = (candidate.position_size as u128)
                .checked_mul(mark_price as u128)
                .ok_or(EngineError::Overflow)?;

            let deleverage_size = if collected_amount + position_value > required_amount {
                // Partial deleverage
                let remaining = required_amount - collected_amount;
                ((remaining
                    .checked_mul(candidate.position_size as u128)
                    .ok_or(EngineError::Overflow)?)
                    / position_value) as u64
            } else {
                // Full deleverage
                candidate.position_size
            };

            // Execute ADL
            self.execute_adl(state, candidate.trader, market_id, deleverage_size)?;

            collected_amount += (deleverage_size as u128)
                .checked_mul(mark_price as u128)
                .ok_or(EngineError::Overflow)?;

            deleveraged.push((candidate.trader, deleverage_size));
        }

        Ok(deleveraged)
    }

    /// Find candidates for auto-deleveraging
    fn find_adl_candidates(
        &self,
        state: &StateManager,
        market_id: u32,
        mark_price: u64,
    ) -> Result<Vec<AdlCandidate>, EngineError> {
        let mut candidates = Vec::new();

        // Get all positions in the market
        let positions = state.get_all_positions_in_market(market_id)?;

        for (trader, position) in positions {
            if position.size == 0 {
                continue;
            }

            // Calculate PnL
            let (pnl, is_profit) = self.position_mgr.calculate_pnl(&position, mark_price)?;

            // Only consider profitable positions for ADL
            if !is_profit {
                continue;
            }

            // Calculate position value and leverage
            let position_value = Self::calculate_position_value(&position, mark_price)?;
            let equity = self.calculate_position_equity(&position, mark_price)?;

            // Calculate leverage (position_value / equity) in basis points
            let leverage = if equity > 0 {
                position_value
                    .checked_mul(BASIS_POINTS)
                    .ok_or(EngineError::Overflow)?
                    / equity
            } else {
                u128::MAX
            };

            // Calculate profit score = PnL * leverage
            // Higher profit score means higher priority for ADL
            let profit_score = (pnl as i128)
                .checked_mul(leverage as i128)
                .ok_or(EngineError::Overflow)?;

            candidates.push(AdlCandidate {
                trader,
                market_id,
                position_size: position.size,
                unrealized_pnl: pnl,
                is_profit,
                profit_score,
            });
        }

        Ok(candidates)
    }

    /// Execute auto-deleveraging on a position
    fn execute_adl(
        &self,
        state: &mut StateManager,
        trader: Address,
        market_id: u32,
        size: u64,
    ) -> Result<(), EngineError> {
        let position = state
            .get_position(trader, market_id)?
            .ok_or(EngineError::PositionNotFound)?;

        if size >= position.size {
            // Full ADL
            state.delete_position(trader, market_id)?;
        } else {
            // Partial ADL
            if position.size == 0 {
                // Safety: Should not happen, but handle gracefully
                return Err(EngineError::Other(
                    "Cannot partially ADL zero-sized position".to_string(),
                ));
            }

            let mut updated_position = position.clone();
            updated_position.size -= size;

            // Adjust margin proportionally
            let margin_reduction = position
                .margin
                .checked_mul(size as u128)
                .ok_or(EngineError::Overflow)?
                / position.size as u128;
            updated_position.margin = updated_position.margin.saturating_sub(margin_reduction);

            state.set_position(trader, market_id, updated_position)?;
        }

        Ok(())
    }

    /// Get insurance fund balance
    pub fn get_insurance_fund(&self, market_id: u32) -> InsuranceFund {
        self.insurance_funds
            .get(&market_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Get current fee split configuration
    pub fn get_fee_split(&self) -> (u32, u32) {
        self.fee_split
    }

    /// Check if insurance fund is healthy
    pub fn is_insurance_fund_healthy(&self, market_id: u32, total_positions_value: u128) -> bool {
        let fund = self.get_insurance_fund(market_id);
        let required_minimum = total_positions_value
            .checked_mul(self.min_insurance_ratio as u128)
            .unwrap_or(0)
            / BASIS_POINTS;

        fund.balance >= required_minimum
    }

    /// Calculate margin ratio (equity / position_value) in basis points
    fn calculate_margin_ratio(
        &self,
        position: &Position,
        mark_price: u64,
    ) -> Result<u32, EngineError> {
        if position.size == 0 {
            return Ok(u32::MAX);
        }

        let equity = self.calculate_position_equity(position, mark_price)?;
        let position_value = Self::calculate_position_value(position, mark_price)?;

        if position_value == 0 {
            return Ok(u32::MAX);
        }

        let ratio = equity
            .checked_mul(BASIS_POINTS)
            .ok_or(EngineError::Overflow)?
            / position_value;

        Ok(ratio.try_into().unwrap_or(u32::MAX))
    }

    /// Check if position should be liquidated
    pub fn should_liquidate(
        &self,
        state: &StateManager,
        trader: Address,
        market_id: u32,
        mark_price: u64,
        market: &Market,
    ) -> Result<bool, EngineError> {
        let position = match state.get_position(trader, market_id)? {
            Some(pos) => pos,
            None => return Ok(false),
        };

        if position.size == 0 {
            return Ok(false);
        }

        let margin_ratio = self.calculate_margin_ratio(&position, mark_price)?;
        Ok(margin_ratio < market.maintenance_margin_bps)
    }

    /// Batch liquidation processing
    /// Processes multiple liquidations in a single call for efficiency
    pub fn process_liquidation_batch(
        &mut self,
        state: &mut StateManager,
        orderbook: &mut OrderbookManager,
        market_id: u32,
        mark_price: u64,
        liquidator: Address,
        max_liquidations: usize,
    ) -> Result<Vec<LiquidationResult>, EngineError> {
        let market = state
            .get_market(market_id)?
            .ok_or(EngineError::MarketNotFound)?;

        let at_risk = self.get_at_risk_positions(market_id, market.maintenance_margin_bps);
        let mut results = Vec::new();

        for (trader, mid) in at_risk.into_iter().take(max_liquidations) {
            if let Some(result) = self
                .liquidate_with_incentive(state, orderbook, trader, mid, mark_price, liquidator)?
            {
                results.push(result);
            }
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_partial_liquidation_calculation() {
        let liquidation = LiquidationEngine::default();

        let position = Position {
            size: 1000,
            entry_price: 50000,
            is_long: true,
            margin: 250000, // 5% margin
            funding_index: 0,
        };

        let market = Market {
            id: 0,
            symbol: "BTC-PERP".to_string(),
            base_asset_id: 1,
            quote_asset_id: 0,
            tick_size: 1000,
            price_decimals: 2,
            size_decimals: 6,
            min_order_size: 1,
            max_order_size: 1_000_000_000, // 1 billion units max
            max_leverage: 20,
            initial_margin_bps: 1000,
            maintenance_margin_bps: 500,
            liquidation_fee_bps: 100,
            funding_interval: 3600,
            max_funding_rate_bps: 100,
        };

        let size = liquidation
            .calculate_partial_liquidation_size(&position, 48000, &market)
            .unwrap();

        assert!(size > 0);
        assert!(size < position.size);
    }

    #[test]
    fn test_insurance_fund() {
        let mut liquidation = LiquidationEngine::default();

        // Initially empty
        let fund = liquidation.get_insurance_fund(0);
        assert_eq!(fund.balance, 0);

        // Add contribution
        liquidation.insurance_funds.entry(0).or_default().balance = 1000000;

        let fund = liquidation.get_insurance_fund(0);
        assert_eq!(fund.balance, 1000000);
    }

    #[test]
    fn test_fee_split_configuration() {
        let mut liquidation = LiquidationEngine::default();

        // Default is 50/50
        assert_eq!(liquidation.fee_split, (5000, 5000));

        // Change to 70/30
        liquidation.set_fee_split(7000, 3000);
        assert_eq!(liquidation.fee_split, (7000, 3000));
    }

    #[test]
    #[should_panic(expected = "Fee split must sum to 10000 bps")]
    fn test_invalid_fee_split() {
        let mut liquidation = LiquidationEngine::default();
        // Should panic - doesn't sum to 10000
        liquidation.set_fee_split(6000, 3000);
    }
}
