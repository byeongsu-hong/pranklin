use crate::{EngineError, constants::BASIS_POINTS};
use alloy_primitives::Address;
use pranklin_state::{Position, StateManager};

/// Position manager for handling position updates and calculations
#[derive(Debug, Clone, Default)]
pub struct PositionManager;

impl PositionManager {
    /// Increase an existing position (add to same side)
    fn increase_position(
        &self,
        position: &mut Position,
        size: u64,
        price: u64,
        initial_margin_bps: u32,
    ) -> Result<(), EngineError> {
        let old_value = (position.size as u128)
            .checked_mul(position.entry_price as u128)
            .ok_or(EngineError::Overflow)?;
        let new_value = (size as u128)
            .checked_mul(price as u128)
            .ok_or(EngineError::Overflow)?;
        let total_value = old_value
            .checked_add(new_value)
            .ok_or(EngineError::Overflow)?;
        let total_size = (position.size as u128)
            .checked_add(size as u128)
            .ok_or(EngineError::Overflow)?;

        if total_size > 0 {
            position.entry_price = (total_value / total_size) as u64;
            position.size = total_size as u64;

            // Calculate additional margin needed
            let additional_margin = new_value
                .checked_mul(initial_margin_bps as u128)
                .ok_or(EngineError::Overflow)?
                / BASIS_POINTS;
            position.margin = position
                .margin
                .checked_add(additional_margin)
                .ok_or(EngineError::Overflow)?;
        }
        Ok(())
    }

    /// Calculate margin for a new position
    fn calculate_initial_margin(
        size: u64,
        price: u64,
        initial_margin_bps: u32,
    ) -> Result<u128, EngineError> {
        let position_value = (size as u128)
            .checked_mul(price as u128)
            .ok_or(EngineError::Overflow)?;
        position_value
            .checked_mul(initial_margin_bps as u128)
            .ok_or(EngineError::Overflow)?
            .checked_div(BASIS_POINTS)
            .ok_or(EngineError::DivisionByZero)
    }

    /// Update or create a position after a trade
    #[allow(clippy::too_many_arguments)]
    pub fn update_position(
        &mut self,
        state: &mut StateManager,
        trader: Address,
        market_id: u32,
        size: u64,
        price: u64,
        is_buy: bool,
        initial_margin_bps: u32,
    ) -> Result<(), EngineError> {
        // Get existing position
        let mut position = state.get_position(trader, market_id)?;

        match position {
            Some(ref mut pos) => {
                // Update existing position
                let is_same_side = pos.is_long == is_buy;

                if is_same_side {
                    // Increase position - add margin
                    self.increase_position(pos, size, price, initial_margin_bps)?;
                } else {
                    // Reduce or flip position
                    if size >= pos.size {
                        // Close and potentially flip
                        let remaining = size - pos.size;
                        if remaining > 0 {
                            // Position flipped - calculate new margin
                            let new_margin = Self::calculate_initial_margin(
                                remaining,
                                price,
                                initial_margin_bps,
                            )?;

                            pos.size = remaining;
                            pos.entry_price = price;
                            pos.is_long = is_buy;
                            pos.margin = new_margin;
                        } else {
                            // Position fully closed
                            state.delete_position(trader, market_id)?;
                            return Ok(());
                        }
                    } else {
                        // Partially reduce - reduce margin proportionally
                        if pos.size == 0 {
                            // Safety: This should never happen, but handle gracefully
                            return Err(EngineError::Other(
                                "Cannot reduce zero-sized position".to_string(),
                            ));
                        }

                        let reduction_ratio = (size as u128)
                            .checked_mul(10000)
                            .ok_or(EngineError::Overflow)?
                            / pos.size as u128;
                        let margin_reduction = pos
                            .margin
                            .checked_mul(reduction_ratio)
                            .ok_or(EngineError::Overflow)?
                            / 10000;

                        pos.size -= size;
                        pos.margin = pos.margin.saturating_sub(margin_reduction);
                    }
                }

                state.set_position(trader, market_id, pos.clone())?;
            }
            None => {
                // Create new position with initial margin
                let margin = Self::calculate_initial_margin(size, price, initial_margin_bps)?;

                let position = Position {
                    size,
                    entry_price: price,
                    is_long: is_buy,
                    margin,
                    funding_index: 0,
                };
                state.set_position(trader, market_id, position)?;
            }
        }

        Ok(())
    }

    /// Calculate unrealized PnL for a position
    pub fn calculate_pnl(
        &self,
        position: &Position,
        mark_price: u64,
    ) -> Result<(u128, bool), EngineError> {
        Self::calculate_pnl_static(position, mark_price)
    }

    /// Calculate PnL (static version for reuse)
    pub(crate) fn calculate_pnl_static(
        position: &Position,
        mark_price: u64,
    ) -> Result<(u128, bool), EngineError> {
        if position.size == 0 {
            return Ok((0, true));
        }

        let entry_value = (position.size as u128)
            .checked_mul(position.entry_price as u128)
            .ok_or(EngineError::Overflow)?;
        let mark_value = (position.size as u128)
            .checked_mul(mark_price as u128)
            .ok_or(EngineError::Overflow)?;

        let (pnl, is_profit) = if position.is_long {
            if mark_value >= entry_value {
                (mark_value - entry_value, true)
            } else {
                (entry_value - mark_value, false)
            }
        } else if entry_value >= mark_value {
            (entry_value - mark_value, true)
        } else {
            (mark_value - entry_value, false)
        };

        Ok((pnl, is_profit))
    }

    /// Calculate liquidation price for a position
    /// For longs: liquidation_price = entry_price * (1 - maintenance_margin_ratio)
    /// For shorts: liquidation_price = entry_price * (1 + maintenance_margin_ratio)
    pub fn calculate_liquidation_price(
        &self,
        position: &Position,
        maintenance_margin_bps: u32,
    ) -> Result<u64, EngineError> {
        if position.size == 0 {
            return Ok(0);
        }

        let margin_factor = (position.entry_price as u128)
            .checked_mul(maintenance_margin_bps as u128)
            .ok_or(EngineError::Overflow)?
            / BASIS_POINTS;

        let liq_price = if position.is_long {
            (position.entry_price as u128)
                .checked_sub(margin_factor)
                .ok_or(EngineError::Overflow)?
        } else {
            (position.entry_price as u128)
                .checked_add(margin_factor)
                .ok_or(EngineError::Overflow)?
        };

        Ok(liq_price as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_pnl() {
        let mgr = PositionManager;

        let position = Position {
            size: 100,
            entry_price: 50000,
            is_long: true,
            margin: 1000,
            funding_index: 0,
        };

        // Profit scenario
        let mark_price = 51000;
        let (pnl, is_profit) = mgr.calculate_pnl(&position, mark_price).unwrap();
        assert!(is_profit);
        assert_eq!(pnl, 100000); // 100 * (51000 - 50000)

        // Loss scenario
        let mark_price = 49000;
        let (pnl, is_profit) = mgr.calculate_pnl(&position, mark_price).unwrap();
        assert!(!is_profit);
        assert_eq!(pnl, 100000); // 100 * (50000 - 49000)
    }
}
