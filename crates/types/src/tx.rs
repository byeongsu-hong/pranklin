use pranklin_macros::standard_enum;

/// Order type
#[standard_enum(u8)]
pub enum OrderType {
    Market = 0,
    Limit = 1,
    StopMarket = 2,
    StopLimit = 3,
    TakeProfitMarket = 4,
    TakeProfitLimit = 5,
}

impl OrderType {
    pub const fn is_market(&self) -> bool {
        matches!(
            self,
            Self::Market | Self::StopMarket | Self::TakeProfitMarket
        )
    }

    pub const fn is_limit(&self) -> bool {
        matches!(self, Self::Limit | Self::StopLimit | Self::TakeProfitLimit)
    }

    pub const fn has_trigger(&self) -> bool {
        matches!(
            self,
            Self::StopMarket | Self::StopLimit | Self::TakeProfitMarket | Self::TakeProfitLimit
        )
    }
}

/// Time in force
#[standard_enum(u8)]
pub enum TimeInForce {
    GTC = 0,
    IOC = 1,
    FOK = 2,
    PostOnly = 3,
}

impl TimeInForce {
    pub const fn requires_immediate_execution(&self) -> bool {
        matches!(self, Self::IOC | Self::FOK)
    }
}
