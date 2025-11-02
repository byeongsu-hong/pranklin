use crate::Address;
use pranklin_macros::{standard, standard_enum};

/// Position information
#[standard]
pub struct Position {
    pub size: u64,
    pub entry_price: u64,
    pub is_long: bool,
    pub margin: u128,
    pub funding_index: u128,
}

impl Position {
    pub const fn is_empty(&self) -> bool {
        self.size == 0
    }

    pub const fn notional_value(&self) -> u128 {
        self.size as u128 * self.entry_price as u128
    }

    pub const fn leverage(&self) -> u64 {
        if self.margin == 0 {
            return 0;
        }
        (self.notional_value() / self.margin) as u64
    }
}

/// Order status
#[standard_enum(u8)]
pub enum OrderStatus {
    Active,
    Filled,
    Cancelled,
}

impl OrderStatus {
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Filled | Self::Cancelled)
    }

    pub const fn can_match(&self) -> bool {
        matches!(self, Self::Active)
    }
}

/// Order information
#[standard]
pub struct Order {
    pub id: u64,
    pub market_id: u32,
    pub owner: Address,
    pub is_buy: bool,
    pub price: u64,
    pub original_size: u64,
    pub remaining_size: u64,
    pub status: OrderStatus,
    pub created_at: u64,
    pub reduce_only: bool,
    pub post_only: bool,
}

impl Order {
    pub const fn filled_size(&self) -> u64 {
        self.original_size.saturating_sub(self.remaining_size)
    }

    pub const fn is_fully_filled(&self) -> bool {
        self.remaining_size == 0
    }

    pub const fn fill_percentage(&self) -> u64 {
        if self.original_size == 0 {
            return 0;
        }
        (self.filled_size() as u128 * 100 / self.original_size as u128) as u64
    }
}

/// Market information
#[standard]
pub struct Market {
    pub id: u32,
    pub symbol: String,
    pub base_asset_id: u32,
    pub quote_asset_id: u32,
    pub tick_size: u64,
    pub price_decimals: u8,
    pub size_decimals: u8,
    pub min_order_size: u64,
    pub max_order_size: u64,
    pub max_leverage: u32,
    pub maintenance_margin_bps: u32,
    pub initial_margin_bps: u32,
    pub liquidation_fee_bps: u32,
    pub funding_interval: u64,
    pub max_funding_rate_bps: u32,
}

impl Market {
    pub const fn normalize_price(&self, price: u64) -> u64 {
        match self.tick_size {
            0 => price,
            tick_size => ((price + tick_size / 2) / tick_size) * tick_size,
        }
    }

    pub const fn validate_price(&self, price: u64) -> bool {
        match self.tick_size {
            0 => true,
            tick_size => price.is_multiple_of(tick_size),
        }
    }

    pub const fn price_to_tick(&self, price: u64) -> u64 {
        match self.tick_size {
            0 => price,
            tick_size => price / tick_size,
        }
    }

    pub const fn tick_to_price(&self, tick: u64) -> u64 {
        tick * self.tick_size
    }

    pub const fn validate_size(&self, size: u64) -> bool {
        size >= self.min_order_size && size <= self.max_order_size
    }

    pub const fn calculate_margin(&self, size: u64, price: u64, leverage: u32) -> u128 {
        if leverage == 0 {
            return 0;
        }
        let notional = size as u128 * price as u128;
        notional / leverage as u128
    }
}

/// Funding rate information
#[standard]
#[derive(Default)]
pub struct FundingRate {
    pub rate: i64,
    pub last_update: u64,
    pub index: i128,
    pub mark_price: u64,
    pub oracle_price: u64,
}

impl FundingRate {
    pub const fn is_positive(&self) -> bool {
        self.rate > 0
    }

    pub const fn is_negative(&self) -> bool {
        self.rate < 0
    }

    pub const fn premium(&self) -> i128 {
        self.mark_price as i128 - self.oracle_price as i128
    }
}

/// Asset information
#[standard]
pub struct Asset {
    pub id: u32,
    pub symbol: String,
    pub name: String,
    pub decimals: u8,
    pub is_collateral: bool,
    pub collateral_weight_bps: u32,
}

impl Asset {
    pub const fn collateral_value(&self, amount: u128) -> u128 {
        if !self.is_collateral {
            return 0;
        }
        amount * self.collateral_weight_bps as u128 / 10000
    }

    pub const fn display_amount(&self, raw_amount: u128) -> u128 {
        raw_amount / 10u128.pow(self.decimals as u32)
    }
}
