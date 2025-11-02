/// Common constants used across the engine

/// Basis points constant (10000 = 100%)
pub(crate) const BASIS_POINTS: u128 = 10000;

/// Default liquidator fee in basis points (50%)
pub(crate) const DEFAULT_LIQUIDATOR_FEE_BPS: u32 = 5000;

/// Default insurance fund fee in basis points (50%)
pub(crate) const DEFAULT_INSURANCE_FEE_BPS: u32 = 5000;

/// Default minimum insurance fund ratio in basis points (1%)
pub(crate) const DEFAULT_MIN_INSURANCE_RATIO_BPS: u32 = 100;

/// Margin buffer for partial liquidations in basis points (2%)
pub(crate) const MARGIN_BUFFER_BPS: u32 = 200;

/// Minimum liquidation percentage (10%)
pub(crate) const MIN_LIQUIDATION_PCT: u64 = 10;

/// Default maximum funding rate in basis points per interval (10%)
pub(crate) const DEFAULT_MAX_FUNDING_RATE_BPS: u32 = 1000;

