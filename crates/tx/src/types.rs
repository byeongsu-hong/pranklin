use crate::B256;
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

/// Order type
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
#[repr(u8)]
#[borsh(use_discriminant = true)]
pub enum OrderType {
    /// Market order
    Market = 0,
    /// Limit order
    Limit = 1,
    /// Stop market order
    StopMarket = 2,
    /// Stop limit order
    StopLimit = 3,
    /// Take profit market order
    TakeProfitMarket = 4,
    /// Take profit limit order
    TakeProfitLimit = 5,
}

/// Time in force
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
#[repr(u8)]
#[borsh(use_discriminant = true)]
pub enum TimeInForce {
    /// Good till cancel
    GTC = 0,
    /// Immediate or cancel
    IOC = 1,
    /// Fill or kill
    FOK = 2,
    /// Post only
    PostOnly = 3,
}

/// Agent permissions bitmap constants
pub mod permissions {
    /// Permission to place orders
    pub const PLACE_ORDER: u64 = 1 << 0;
    /// Permission to cancel orders
    pub const CANCEL_ORDER: u64 = 1 << 1;
    /// Permission to modify orders
    pub const MODIFY_ORDER: u64 = 1 << 2;
    /// Permission to close positions
    pub const CLOSE_POSITION: u64 = 1 << 3;
    /// Permission to withdraw
    pub const WITHDRAW: u64 = 1 << 4;
    /// All permissions
    pub const ALL: u64 = (1 << 5) - 1;
}

/// Transaction receipt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxReceipt {
    /// Transaction hash
    pub tx_hash: B256,
    /// Block height
    pub block_height: u64,
    /// Transaction index in block
    pub tx_index: u64,
    /// Success flag
    pub success: bool,
    /// Gas used
    pub gas_used: u64,
    /// Error message if failed
    pub error: Option<String>,
}
