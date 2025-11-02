use crate::{Address, B256, OrderType};
use pranklin_macros::{standard, standard_enum};

/// Trade fill information
#[standard]
pub struct Fill {
    pub maker: Address,
    pub taker: Address,
    pub market_id: u32,
    pub price: u64,
    pub size: u64,
    pub taker_is_buy: bool,
    pub maker_order_id: u64,
    pub taker_order_id: u64,
}

impl Fill {
    pub const fn notional_value(&self) -> u128 {
        self.price as u128 * self.size as u128
    }

    pub const fn maker_is_buy(&self) -> bool {
        !self.taker_is_buy
    }
}

/// Comprehensive event system for audit trail, analytics, and state reconstruction
///
/// Events are emitted during transaction execution and provide a complete history
/// of state changes. They enable:
/// - Audit trails (compliance, debugging)
/// - Analytics (volume, fees, PnL)
/// - State reconstruction (replay from events)
/// - Time-travel queries (state at specific block)
#[standard]
pub struct DomainEvent {
    /// Block height when event was emitted
    pub block_height: u64,
    /// Transaction hash that triggered this event
    pub tx_hash: B256,
    /// Event index within the transaction (for ordering)
    pub event_index: u32,
    /// Timestamp (Unix seconds)
    pub timestamp: u64,
    /// The actual event data
    pub event: Event,
}

/// All possible events in the system
#[standard]
#[repr(u8)]
#[borsh(use_discriminant = true)]
pub enum Event {
    // Account events
    /// Balance changed (deposit, withdraw, transfer, settlement)
    BalanceChanged {
        address: Address,
        asset_id: u32,
        old_balance: u128,
        new_balance: u128,
        reason: BalanceChangeReason,
    } = 0,

    /// Transfer between accounts
    Transfer {
        from: Address,
        to: Address,
        asset_id: u32,
        amount: u128,
    } = 1,

    /// Nonce incremented
    NonceUpdated {
        address: Address,
        old_nonce: u64,
        new_nonce: u64,
    } = 2,

    // Order events
    /// Order placed in orderbook
    OrderPlaced {
        order_id: u64,
        owner: Address,
        market_id: u32,
        is_buy: bool,
        price: u64,
        size: u64,
        order_type: OrderType,
    } = 10,

    /// Order cancelled
    OrderCancelled {
        order_id: u64,
        owner: Address,
        market_id: u32,
        remaining_size: u64,
    } = 11,

    /// Order filled (partial or full)
    OrderFilled {
        maker_order_id: u64,
        taker_order_id: u64,
        maker: Address,
        taker: Address,
        market_id: u32,
        price: u64,
        size: u64,
        taker_is_buy: bool,
        maker_fee: u128,
        taker_fee: u128,
    } = 12,

    // Position events
    /// Position opened or increased
    PositionOpened {
        trader: Address,
        market_id: u32,
        is_long: bool,
        size: u64,
        entry_price: u64,
        margin: u128,
    } = 20,

    /// Position closed or reduced
    PositionClosed {
        trader: Address,
        market_id: u32,
        size: u64,
        exit_price: u64,
        pnl: i128,
        is_profit: bool,
    } = 21,

    /// Position modified (margin added/removed)
    PositionModified {
        trader: Address,
        market_id: u32,
        old_margin: u128,
        new_margin: u128,
    } = 22,

    /// Position liquidated
    PositionLiquidated {
        trader: Address,
        market_id: u32,
        liquidator: Option<Address>,
        liquidated_size: u64,
        liquidation_price: u64,
        liquidation_fee: u128,
        insurance_fund_contribution: u128,
        insurance_fund_usage: u128,
    } = 23,

    // Funding events
    /// Funding payment made
    FundingPaid {
        trader: Address,
        market_id: u32,
        amount: i128,
        is_payment: bool,
        funding_rate: i64,
        funding_index: i128,
    } = 30,

    /// Funding rate updated
    FundingRateUpdated {
        market_id: u32,
        rate: i64,
        mark_price: u64,
        oracle_price: u64,
        index: i128,
    } = 31,

    // Bridge events
    /// Bridge deposit processed
    BridgeDeposit {
        operator: Address,
        user: Address,
        asset_id: u32,
        amount: u128,
        external_tx_hash: B256,
    } = 40,

    /// Bridge withdrawal processed
    BridgeWithdraw {
        operator: Address,
        user: Address,
        asset_id: u32,
        amount: u128,
        destination: Address,
        external_tx_hash: B256,
    } = 41,

    // Agent events
    /// Agent set with permissions
    AgentSet {
        account: Address,
        agent: Address,
        permissions: u64,
    } = 50,

    /// Agent removed
    AgentRemoved { account: Address, agent: Address } = 51,

    // Market events
    /// Market created or updated
    MarketUpdated { market_id: u32, symbol: String } = 60,

    // System events
    /// Insurance fund updated
    InsuranceFundUpdated {
        market_id: u32,
        old_balance: u128,
        new_balance: u128,
        reason: InsuranceFundChangeReason,
    } = 70,
}

/// Reason for balance change (for audit trail)
#[standard_enum(u8)]
pub enum BalanceChangeReason {
    Deposit = 0,
    Withdraw = 1,
    Transfer = 2,
    TradeFee = 3,
    FundingPayment = 4,
    FundingReceipt = 5,
    Liquidation = 6,
    PnLSettlement = 7,
    BridgeDeposit = 8,
    BridgeWithdraw = 9,
}

impl BalanceChangeReason {
    pub const fn is_inflow(&self) -> bool {
        matches!(
            self,
            Self::Deposit | Self::FundingReceipt | Self::BridgeDeposit
        )
    }

    pub const fn is_outflow(&self) -> bool {
        matches!(
            self,
            Self::Withdraw | Self::TradeFee | Self::FundingPayment | Self::BridgeWithdraw
        )
    }

    pub const fn is_trade_related(&self) -> bool {
        matches!(self, Self::TradeFee | Self::PnLSettlement)
    }

    pub const fn is_funding_related(&self) -> bool {
        matches!(self, Self::FundingPayment | Self::FundingReceipt)
    }

    pub const fn is_bridge_related(&self) -> bool {
        matches!(self, Self::BridgeDeposit | Self::BridgeWithdraw)
    }
}

/// Reason for insurance fund balance change
#[standard_enum(u8)]
pub enum InsuranceFundChangeReason {
    LiquidationFee = 0,
    LiquidationCoverage = 1,
    AdminDeposit = 2,
    AdminWithdraw = 3,
}

impl InsuranceFundChangeReason {
    pub const fn is_liquidation_related(&self) -> bool {
        matches!(self, Self::LiquidationFee | Self::LiquidationCoverage)
    }

    pub const fn is_admin_action(&self) -> bool {
        matches!(self, Self::AdminDeposit | Self::AdminWithdraw)
    }

    pub const fn is_inflow(&self) -> bool {
        matches!(self, Self::LiquidationFee | Self::AdminDeposit)
    }

    pub const fn is_outflow(&self) -> bool {
        matches!(self, Self::LiquidationCoverage | Self::AdminWithdraw)
    }
}

impl Event {
    pub const fn is_account_event(&self) -> bool {
        matches!(
            self,
            Self::BalanceChanged { .. } | Self::Transfer { .. } | Self::NonceUpdated { .. }
        )
    }

    pub const fn is_order_event(&self) -> bool {
        matches!(
            self,
            Self::OrderPlaced { .. } | Self::OrderCancelled { .. } | Self::OrderFilled { .. }
        )
    }

    pub const fn is_position_event(&self) -> bool {
        matches!(
            self,
            Self::PositionOpened { .. }
                | Self::PositionClosed { .. }
                | Self::PositionModified { .. }
                | Self::PositionLiquidated { .. }
        )
    }

    pub const fn is_funding_event(&self) -> bool {
        matches!(
            self,
            Self::FundingPaid { .. } | Self::FundingRateUpdated { .. }
        )
    }

    pub const fn is_bridge_event(&self) -> bool {
        matches!(
            self,
            Self::BridgeDeposit { .. } | Self::BridgeWithdraw { .. }
        )
    }

    pub const fn is_agent_event(&self) -> bool {
        matches!(self, Self::AgentSet { .. } | Self::AgentRemoved { .. })
    }
}

impl DomainEvent {
    pub const fn new(
        block_height: u64,
        tx_hash: B256,
        event_index: u32,
        timestamp: u64,
        event: Event,
    ) -> Self {
        Self {
            block_height,
            tx_hash,
            event_index,
            timestamp,
            event,
        }
    }

    pub const fn is_account_event(&self) -> bool {
        self.event.is_account_event()
    }

    pub const fn is_order_event(&self) -> bool {
        self.event.is_order_event()
    }

    pub const fn is_position_event(&self) -> bool {
        self.event.is_position_event()
    }

    pub const fn is_funding_event(&self) -> bool {
        self.event.is_funding_event()
    }

    pub const fn is_bridge_event(&self) -> bool {
        self.event.is_bridge_event()
    }

    pub const fn is_agent_event(&self) -> bool {
        self.event.is_agent_event()
    }
}

impl From<(u64, B256, u32, u64, Event)> for DomainEvent {
    fn from(
        (block_height, tx_hash, event_index, timestamp, event): (u64, B256, u32, u64, Event),
    ) -> Self {
        Self::new(block_height, tx_hash, event_index, timestamp, event)
    }
}
