use pranklin_macros::standard;
use pranklin_types::Address;

/// State access pattern for transaction execution
///
/// This enum declares which state keys a transaction will access.
#[standard]
#[derive(Hash, Copy)]
pub enum StateAccess {
    /// Account balance read/write
    Balance { address: Address, asset_id: u32 },

    /// Account nonce read/write
    Nonce { address: Address },

    /// Position read/write
    Position { address: Address, market_id: u32 },

    /// Order read/write
    Order { order_id: u64 },

    /// Orderbook (price levels for a market)
    OrderList { market_id: u32 },

    /// Market configuration (usually read-only)
    Market { market_id: u32 },

    /// Funding rate read/write
    FundingRate { market_id: u32 },

    /// Asset info (read-only)
    AssetInfo { asset_id: u32 },

    /// Bridge operator status (read-only)
    BridgeOperator { address: Address },
}

/// Access mode for state operations
#[standard]
#[repr(u8)]
#[borsh(use_discriminant = true)]
pub enum AccessMode {
    /// Read-only access
    Read = 0,
    /// Write access (implies read)
    Write = 1,
}

/// Trait for declaring state accesses before execution
///
/// Implementing this trait allows transactions to declare which state
/// keys they will access during execution.
pub trait DeclareStateAccess {
    /// Declare all state accesses this transaction will make
    ///
    /// This should be a conservative estimate - it's better to over-declare
    /// than under-declare accesses.
    ///
    /// # Returns
    /// A vector of (StateAccess, AccessMode) tuples
    fn declare_accesses(&self) -> Vec<(StateAccess, AccessMode)>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_access_equality() {
        let access1 = StateAccess::Balance {
            address: Address::ZERO,
            asset_id: 0,
        };
        let access2 = StateAccess::Balance {
            address: Address::ZERO,
            asset_id: 0,
        };
        let access3 = StateAccess::Balance {
            address: Address::ZERO,
            asset_id: 1,
        };

        assert_eq!(access1, access2);
        assert_ne!(access1, access3);
    }

    #[test]
    fn test_access_mode() {
        let read = AccessMode::Read;
        let write = AccessMode::Write;

        assert_ne!(read, write);
    }
}
