use crate::B256;
use pranklin_macros::standard;

/// Transaction receipt
#[standard]
pub struct TxReceipt {
    pub tx_hash: B256,
    pub block_height: u64,
    pub tx_index: u64,
    pub success: bool,
    pub gas_used: u64,
    pub error: Option<String>,
}

impl TxReceipt {
    pub const fn is_success(&self) -> bool {
        self.success
    }

    pub const fn is_failure(&self) -> bool {
        !self.success
    }
}

/// Agent permissions bitmap
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Permissions(pub u64);

impl Permissions {
    pub const PLACE_ORDER: u64 = 1 << 0;
    pub const CANCEL_ORDER: u64 = 1 << 1;
    pub const MODIFY_ORDER: u64 = 1 << 2;
    pub const CLOSE_POSITION: u64 = 1 << 3;
    pub const WITHDRAW: u64 = 1 << 4;
    pub const ALL: u64 = (1 << 5) - 1;

    pub const fn new(bits: u64) -> Self {
        Self(bits)
    }

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn all() -> Self {
        Self(Self::ALL)
    }

    pub const fn has(&self, permission: u64) -> bool {
        self.0 & permission != 0
    }

    pub const fn can_place_order(&self) -> bool {
        self.has(Self::PLACE_ORDER)
    }

    pub const fn can_cancel_order(&self) -> bool {
        self.has(Self::CANCEL_ORDER)
    }

    pub const fn can_modify_order(&self) -> bool {
        self.has(Self::MODIFY_ORDER)
    }

    pub const fn can_close_position(&self) -> bool {
        self.has(Self::CLOSE_POSITION)
    }

    pub const fn can_withdraw(&self) -> bool {
        self.has(Self::WITHDRAW)
    }

    pub const fn with(mut self, permission: u64) -> Self {
        self.0 |= permission;
        self
    }

    pub const fn without(mut self, permission: u64) -> Self {
        self.0 &= !permission;
        self
    }
}

impl Default for Permissions {
    fn default() -> Self {
        Self::empty()
    }
}

impl From<u64> for Permissions {
    fn from(bits: u64) -> Self {
        Self(bits)
    }
}

impl From<Permissions> for u64 {
    fn from(perms: Permissions) -> Self {
        perms.0
    }
}

impl core::ops::BitOr for Permissions {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl core::ops::BitOrAssign for Permissions {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl core::ops::BitAnd for Permissions {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        Self(self.0 & rhs.0)
    }
}

impl core::ops::BitAndAssign for Permissions {
    fn bitand_assign(&mut self, rhs: Self) {
        self.0 &= rhs.0;
    }
}

impl core::ops::Not for Permissions {
    type Output = Self;

    fn not(self) -> Self::Output {
        Self(!self.0 & Self::ALL)
    }
}

/// Legacy permissions module for backward compatibility
pub mod permissions {
    pub use super::Permissions;
    pub const PLACE_ORDER: u64 = Permissions::PLACE_ORDER;
    pub const CANCEL_ORDER: u64 = Permissions::CANCEL_ORDER;
    pub const MODIFY_ORDER: u64 = Permissions::MODIFY_ORDER;
    pub const CLOSE_POSITION: u64 = Permissions::CLOSE_POSITION;
    pub const WITHDRAW: u64 = Permissions::WITHDRAW;
    pub const ALL: u64 = Permissions::ALL;
}
