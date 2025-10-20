use crate::EngineError;
use pranklin_state::{FundingRate, StateManager};

// Constants
const BASIS_POINTS: u128 = 10000;
const DEFAULT_MAX_FUNDING_RATE_BPS: u32 = 1000; // 10% per interval

/// Funding rate calculator for perpetual futures
#[derive(Debug, Clone, Default)]
pub struct FundingRateCalculator {
    /// Maximum funding rate (in basis points per interval)
    max_funding_rate_bps: u32,
}

impl FundingRateCalculator {
    /// Create a new funding rate calculator with default settings
    pub fn new() -> Self {
        Self {
            max_funding_rate_bps: DEFAULT_MAX_FUNDING_RATE_BPS,
        }
    }

    /// Calculate funding rate based on mark price and oracle price
    /// Returns the funding rate in basis points (positive = longs pay shorts, negative = shorts pay longs)
    pub fn calculate_funding_rate(
        &self,
        mark_price: u64,
        oracle_price: u64,
        time_elapsed: u64,
        funding_interval: u64,
    ) -> Result<i64, EngineError> {
        if oracle_price == 0 {
            return Ok(0);
        }

        // Premium = (Mark Price - Oracle Price) / Oracle Price in basis points
        let (premium, is_positive) = if mark_price >= oracle_price {
            let diff = mark_price - oracle_price;
            (
                ((diff as u128) * BASIS_POINTS / (oracle_price as u128)) as u64,
                true,
            )
        } else {
            let diff = oracle_price - mark_price;
            (
                ((diff as u128) * BASIS_POINTS / (oracle_price as u128)) as u64,
                false,
            )
        };

        // Clamp to max funding rate
        let clamped_premium = premium.min(self.max_funding_rate_bps as u64);

        // Scale by time elapsed
        let funding_rate = if funding_interval > 0 {
            clamped_premium
                .checked_mul(time_elapsed)
                .ok_or(EngineError::Overflow)?
                .checked_div(funding_interval)
                .ok_or(EngineError::DivisionByZero)?
        } else {
            clamped_premium
        };

        // Apply sign (positive = longs pay shorts, negative = shorts pay longs)
        if is_positive {
            Ok(funding_rate as i64)
        } else {
            Ok(-(funding_rate as i64))
        }
    }

    /// Update funding rate for a market
    pub fn update_funding_rate(
        &self,
        state: &mut StateManager,
        market_id: u32,
        mark_price: u64,
        oracle_price: u64,
        timestamp: u64,
    ) -> Result<(), EngineError> {
        let current = state.get_funding_rate(market_id)?;

        let time_elapsed = if current.last_update > 0 {
            timestamp.saturating_sub(current.last_update)
        } else {
            0
        };

        // Get market info for funding interval
        let market_info = state
            .get_market(market_id)?
            .ok_or(EngineError::MarketNotFound)?;

        let funding_rate = self.calculate_funding_rate(
            mark_price,
            oracle_price,
            time_elapsed,
            market_info.funding_interval,
        )?;

        // Update cumulative funding index
        let new_index = current
            .index
            .checked_add(funding_rate as i128)
            .ok_or(EngineError::Overflow)?;

        let new_funding = FundingRate {
            rate: funding_rate,
            last_update: timestamp,
            index: new_index,
            mark_price,
            oracle_price,
        };

        state.set_funding_rate(market_id, new_funding)?;

        Ok(())
    }

    /// Calculate funding payment for a position
    /// Returns (payment_amount, is_paying)
    /// - is_paying: true if the trader pays, false if the trader receives
    pub fn calculate_funding_payment(
        &self,
        position_size: u64,
        is_long: bool,
        entry_funding_index: i128,
        current_funding_index: i128,
    ) -> Result<(u128, bool), EngineError> {
        let funding_diff = current_funding_index - entry_funding_index;

        if funding_diff == 0 {
            return Ok((0, true));
        }

        let payment_i128 = (position_size as i128)
            .checked_mul(funding_diff)
            .ok_or(EngineError::Overflow)?
            .checked_div(BASIS_POINTS as i128)
            .ok_or(EngineError::DivisionByZero)?;

        // Determine if payment is owed or received
        // Positive funding: longs pay shorts
        // Negative funding: shorts pay longs
        let (payment, is_paying) = if payment_i128 > 0 {
            (payment_i128 as u128, is_long)
        } else {
            ((-payment_i128) as u128, !is_long)
        };

        Ok((payment, is_paying))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_funding_rate() {
        let calc = FundingRateCalculator::new();

        // Mark price > Oracle price (longs pay shorts)
        let mark_price = 51000;
        let oracle_price = 50000;
        let rate = calc
            .calculate_funding_rate(mark_price, oracle_price, 3600, 28800)
            .unwrap();

        assert!(rate > 0);
    }

    #[test]
    fn test_funding_payment() {
        let calc = FundingRateCalculator::new();

        let position_size = 100;
        let entry_index = 1000;
        let current_index = 1100;

        let (payment, longs_pay) = calc
            .calculate_funding_payment(
                position_size,
                true, // long
                entry_index,
                current_index,
            )
            .unwrap();

        assert!(payment > 0);
        assert!(longs_pay); // Longs pay when funding index increases
    }
}
