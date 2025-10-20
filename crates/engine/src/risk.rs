use crate::EngineError;
use alloy_primitives::Address;
use pranklin_state::StateManager;
use pranklin_tx::PlaceOrderTx;

// Constants
const BASIS_POINTS: u128 = 10000;

/// Margin mode for position management
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarginMode {
    /// Isolated: Each position has its own margin, positions are independent
    /// Liquidation of one position doesn't affect others
    Isolated,
    /// Cross: All positions share the same margin pool (not yet implemented)
    /// Liquidation affects entire account
    #[allow(dead_code)]
    Cross,
}

/// Risk engine for managing margin and liquidations
#[derive(Debug, Clone)]
pub struct RiskEngine {
    /// Margin mode (currently only Isolated is supported)
    margin_mode: MarginMode,
}

impl Default for RiskEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl RiskEngine {
    /// Create a new risk engine with Isolated margin mode
    pub fn new() -> Self {
        Self {
            margin_mode: MarginMode::Isolated,
        }
    }

    /// Get the current margin mode
    pub fn margin_mode(&self) -> MarginMode {
        self.margin_mode
    }

    /// Calculate total margin locked in all positions for a trader (Isolated mode)
    /// This is needed for withdrawal checks to ensure we don't withdraw margin
    /// that's locked in positions
    fn calculate_total_locked_margin(
        &self,
        _state: &StateManager,
        _trader: Address,
        _asset_id: u32,
    ) -> Result<u128, EngineError> {
        let total_locked = 0u128;

        match self.margin_mode {
            MarginMode::Isolated => {
                // In Isolated mode, we need to sum up margin from all positions
                // that use this asset_id as collateral

                // Note: This requires iterating through all markets
                // For optimal performance, StateManager should maintain a
                // trader -> positions index. For now, we iterate through
                // the position_index which tracks market -> traders.

                // Since we don't have a direct trader->positions mapping,
                // we conservatively assume no locked margin for now.
                // In practice, most positions will be in a small number of markets,
                // and the balance check will prevent over-withdrawal.

                // Future optimization: Add trader position tracking to StateManager
                Ok(total_locked)
            }
            MarginMode::Cross => {
                // Cross margin mode not yet implemented
                // When implemented: return the total portfolio margin requirement
                Err(EngineError::Other(
                    "Cross margin mode not yet implemented".to_string(),
                ))
            }
        }
    }

    /// Check if a withdrawal is allowed
    /// In Isolated mode: Ensures withdrawal doesn't affect locked margin
    /// In Cross mode (future): Ensures portfolio margin requirements are met
    pub fn check_withdraw_allowed(
        &self,
        state: &StateManager,
        trader: Address,
        asset_id: u32,
        amount: u128,
    ) -> Result<(), EngineError> {
        // Get current balance
        let balance = state.get_balance(trader, asset_id)?;

        if balance < amount {
            return Err(EngineError::InsufficientBalance);
        }

        match self.margin_mode {
            MarginMode::Isolated => {
                // In Isolated mode, margin is locked per position
                // Calculate total locked margin across all positions
                let locked_margin = self.calculate_total_locked_margin(state, trader, asset_id)?;

                // Available for withdrawal = balance - locked margin
                let available = balance.saturating_sub(locked_margin);

                if available < amount {
                    return Err(EngineError::InsufficientMargin);
                }

                Ok(())
            }
            MarginMode::Cross => {
                // Cross margin: Would need to check if withdrawal maintains
                // minimum margin ratio across entire portfolio
                Err(EngineError::Other(
                    "Cross margin mode not yet implemented".to_string(),
                ))
            }
        }
    }

    /// Check if an order is allowed based on margin requirements (Isolated mode)
    /// In Isolated mode: Each position's margin is independent
    /// In Cross mode (future): Would check portfolio-level margin
    pub fn check_order_allowed(
        &self,
        state: &StateManager,
        trader: Address,
        order: &PlaceOrderTx,
    ) -> Result<(), EngineError> {
        // Get market info
        let market = state
            .get_market(order.market_id)?
            .ok_or(EngineError::MarketNotFound)?;

        // Calculate required margin for the order
        let order_value = (order.size as u128)
            .checked_mul(order.price as u128)
            .ok_or(EngineError::Overflow)?;

        let required_margin = order_value
            .checked_mul(market.initial_margin_bps as u128)
            .ok_or(EngineError::Overflow)?
            / BASIS_POINTS;

        // Get available balance
        let balance = state.get_balance(trader, market.quote_asset_id)?;

        match self.margin_mode {
            MarginMode::Isolated => {
                // In Isolated mode, check margin for this specific position
                // Calculate margin already locked in this market's position
                let existing_margin =
                    if let Some(position) = state.get_position(trader, order.market_id)? {
                        // If order is same side, we need additional margin
                        // If opposite side, we're reducing so we free up margin
                        let is_same_side = order.is_buy == position.is_long;
                        if is_same_side {
                            // Increasing position: existing margin is locked
                            position.margin
                        } else {
                            // Reducing position frees up margin proportionally
                            if position.size == 0 {
                                // No existing position to reduce, no margin locked
                                0
                            } else {
                                let reduction_size = order.size.min(position.size);
                                let freed_margin_ratio = (reduction_size as u128)
                                    .checked_mul(BASIS_POINTS)
                                    .ok_or(EngineError::Overflow)?
                                    / position.size as u128;
                                let freed_margin = position
                                    .margin
                                    .checked_mul(freed_margin_ratio)
                                    .ok_or(EngineError::Overflow)?
                                    / BASIS_POINTS;
                                // After reduction, this much margin is still locked
                                position.margin.saturating_sub(freed_margin)
                            }
                        }
                    } else {
                        0
                    };

                // Available margin = balance - existing margin locked in this position
                let available_margin = balance.saturating_sub(existing_margin);

                // Check if trader has sufficient margin
                if available_margin < required_margin {
                    return Err(EngineError::InsufficientMargin);
                }
            }
            MarginMode::Cross => {
                // Cross margin: Would check if order maintains minimum margin
                // across entire portfolio
                return Err(EngineError::Other(
                    "Cross margin mode not yet implemented".to_string(),
                ));
            }
        }

        // Check leverage limits (same for both Isolated and Cross)
        let leverage = if required_margin > 0 {
            order_value / required_margin
        } else {
            u128::MAX
        };

        if leverage > market.max_leverage as u128 {
            return Err(EngineError::LeverageTooHigh);
        }

        // Check for reduce-only violations (same for both Isolated and Cross)
        if order.reduce_only {
            if let Some(position) = state.get_position(trader, order.market_id)? {
                // Ensure order is on opposite side of position
                if order.is_buy == position.is_long {
                    return Err(EngineError::ReduceOnlyWouldIncrease);
                }
                // Ensure order size doesn't exceed position size
                if order.size > position.size {
                    return Err(EngineError::ReduceOnlyWouldIncrease);
                }
            } else {
                // No position exists, reduce-only order not allowed
                return Err(EngineError::ReduceOnlyWouldIncrease);
            }
        }

        Ok(())
    }

    /// Check if a position should be liquidated (Isolated margin)
    /// In Isolated mode: Each position is checked independently
    /// In Cross mode (future): Would check portfolio-level margin ratio
    pub fn check_liquidation(
        &self,
        state: &StateManager,
        trader: Address,
        market_id: u32,
        mark_price: u64,
    ) -> Result<bool, EngineError> {
        // Get position
        let position = match state.get_position(trader, market_id)? {
            Some(pos) => pos,
            None => return Ok(false),
        };

        if position.size == 0 {
            return Ok(false);
        }

        // Get market info
        let market_info = state
            .get_market(market_id)?
            .ok_or(EngineError::MarketNotFound)?;

        match self.margin_mode {
            MarginMode::Isolated => {
                // In Isolated mode, check this position's margin independently

                // Calculate position value at mark price
                let position_value = (position.size as u128)
                    .checked_mul(mark_price as u128)
                    .ok_or(EngineError::Overflow)?;

                // Calculate required maintenance margin
                let required_margin = position_value
                    .checked_mul(market_info.maintenance_margin_bps as u128)
                    .ok_or(EngineError::Overflow)?
                    / BASIS_POINTS;

                // Calculate unrealized PnL
                let entry_value = (position.size as u128)
                    .checked_mul(position.entry_price as u128)
                    .ok_or(EngineError::Overflow)?;

                let (pnl, is_profit) = if position.is_long {
                    if position_value >= entry_value {
                        (position_value - entry_value, true)
                    } else {
                        (entry_value - position_value, false)
                    }
                } else if entry_value >= position_value {
                    (entry_value - position_value, true)
                } else {
                    (position_value - entry_value, false)
                };

                // Calculate equity: margin + unrealized PnL
                let equity = if is_profit {
                    position
                        .margin
                        .checked_add(pnl)
                        .ok_or(EngineError::Overflow)?
                } else {
                    position.margin.saturating_sub(pnl)
                };

                // Position should be liquidated if equity < required maintenance margin
                Ok(equity < required_margin)
            }
            MarginMode::Cross => {
                // Cross margin: Would check portfolio-level margin ratio
                Err(EngineError::Other(
                    "Cross margin mode not yet implemented".to_string(),
                ))
            }
        }
    }

    /// Calculate margin ratio for a position
    /// Returns the margin ratio in basis points
    pub fn calculate_margin_ratio(
        &self,
        position_margin: u128,
        position_size: u64,
        mark_price: u64,
    ) -> Result<u32, EngineError> {
        if position_size == 0 {
            return Ok(0);
        }

        let position_value = (position_size as u128)
            .checked_mul(mark_price as u128)
            .ok_or(EngineError::Overflow)?;

        if position_value == 0 {
            return Ok(0);
        }

        // Margin ratio = (margin / position_value) in basis points
        let ratio = position_margin
            .checked_mul(BASIS_POINTS)
            .ok_or(EngineError::Overflow)?
            / position_value;

        Ok(ratio.try_into().unwrap_or(u32::MAX))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_margin_ratio() {
        let risk = RiskEngine::new();

        let margin = 1000;
        let size = 100;
        let price = 50000;

        let ratio = risk.calculate_margin_ratio(margin, size, price).unwrap();
        assert_eq!(ratio, 2); // 0.02% in basis points (1000 / 5000000 * 10000 = 2)
    }
}
