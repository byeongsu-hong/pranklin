mod error;
mod types;

pub use error::*;
pub use types::*;

// Re-export Alloy primitives as the main types
pub use alloy_primitives::{Address, B256, U256};

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Main transaction structure for the perp DEX
/// Uses Borsh encoding for deterministic serialization and hash safety
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Transaction {
    /// Transaction nonce to prevent replay attacks
    pub nonce: u64,
    /// Sender address (derived from signature)
    pub from: Address,
    /// Transaction payload
    pub payload: TxPayload,
    /// Signature bytes (65 bytes: r[32] + s[32] + v[1])
    #[serde(with = "signature_serde")]
    pub signature: [u8; 65],
}

mod signature_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(bytes: &[u8; 65], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        bytes.as_slice().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 65], D::Error>
    where
        D: Deserializer<'de>,
    {
        let vec = Vec::<u8>::deserialize(deserializer)?;
        if vec.len() != 65 {
            return Err(serde::de::Error::custom(format!(
                "Invalid signature length: expected 65, got {}",
                vec.len()
            )));
        }
        let mut arr = [0u8; 65];
        arr.copy_from_slice(&vec);
        Ok(arr)
    }
}

impl Transaction {
    /// Create a new unsigned transaction
    pub fn new(nonce: u64, from: Address, payload: TxPayload) -> Self {
        Self {
            nonce,
            from,
            payload,
            signature: [0u8; 65],
        }
    }

    /// Get the transaction hash (used as transaction ID)
    pub fn hash(&self) -> B256 {
        let encoded = self.encode();
        let mut hasher = Sha256::new();
        hasher.update(&encoded);
        B256::from_slice(&hasher.finalize())
    }

    /// Get the signing hash (hash of transaction data without signature)
    pub fn signing_hash(&self) -> B256 {
        let mut data = Vec::new();
        data.extend_from_slice(&self.nonce.to_le_bytes());
        data.extend_from_slice(self.from.as_slice());

        // Encode payload with Borsh (deterministic serialization)
        let payload_bytes = borsh::to_vec(&self.payload).expect("Failed to serialize payload");
        data.extend_from_slice(&payload_bytes);

        let mut hasher = Sha256::new();
        hasher.update(&data);
        B256::from_slice(&hasher.finalize())
    }

    /// Set signature bytes
    pub fn set_signature(&mut self, sig: [u8; 65]) {
        self.signature = sig;
    }

    /// Get signature bytes
    pub fn signature(&self) -> &[u8; 65] {
        &self.signature
    }

    /// Encode the transaction to bytes (using Borsh for deterministic serialization)
    pub fn encode(&self) -> Vec<u8> {
        borsh::to_vec(self).expect("Failed to encode transaction")
    }

    /// Decode the transaction from bytes
    pub fn decode(data: &[u8]) -> Result<Self, TxError> {
        // SECURITY: Prevent DoS by limiting transaction size
        const MAX_TX_SIZE: usize = 100_000; // 100KB
        if data.len() > MAX_TX_SIZE {
            return Err(TxError::Other(format!(
                "Transaction too large: {} bytes (max: {} bytes)",
                data.len(),
                MAX_TX_SIZE
            )));
        }

        borsh::from_slice(data)
            .map_err(|e| TxError::DecodeError(format!("Borsh decode error: {}", e)))
    }
}

/// Transaction payload containing the actual operation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum TxPayload {
    /// Deposit collateral
    Deposit(DepositTx),
    /// Withdraw collateral
    Withdraw(WithdrawTx),
    /// Place a new order
    PlaceOrder(PlaceOrderTx),
    /// Cancel an existing order
    CancelOrder(CancelOrderTx),
    /// Modify an existing order
    ModifyOrder(ModifyOrderTx),
    /// Close a position
    ClosePosition(ClosePositionTx),
    /// Set trading agent (Hyperliquid-style agent)
    SetAgent(SetAgentTx),
    /// Remove trading agent
    RemoveAgent(RemoveAgentTx),
    /// Transfer tokens between accounts
    Transfer(TransferTx),
    /// Bridge deposit (only authorized operators)
    BridgeDeposit(BridgeDepositTx),
    /// Bridge withdrawal (only authorized operators)
    BridgeWithdraw(BridgeWithdrawTx),
}

/// Deposit collateral transaction
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct DepositTx {
    /// Amount to deposit (in base units, u128 supports ~340 undecillion)
    pub amount: u128,
    /// Asset/token identifier (auto-incremental, supports 4B tokens)
    pub asset_id: u32,
}

/// Withdraw collateral transaction
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct WithdrawTx {
    /// Amount to withdraw (in base units)
    pub amount: u128,
    /// Asset/token identifier
    pub asset_id: u32,
    /// Destination address
    pub to: Address,
}

/// Place order transaction
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct PlaceOrderTx {
    /// Market identifier (auto-incremental, supports 4B markets)
    pub market_id: u32,
    /// Order side (true = buy, false = sell)
    pub is_buy: bool,
    /// Order type
    pub order_type: OrderType,
    /// Order price (in base units with proper decimals, 0 for market orders)
    /// u64 supports up to ~18 quintillion, sufficient with 6-8 decimal places
    pub price: u64,
    /// Order size (in base units)
    pub size: u64,
    /// Time in force
    pub time_in_force: TimeInForce,
    /// Reduce only flag
    pub reduce_only: bool,
    /// Post only flag (for limit orders)
    pub post_only: bool,
}

/// Cancel order transaction
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct CancelOrderTx {
    /// Order ID to cancel (u64 supports massive number of orders)
    pub order_id: u64,
}

/// Modify order transaction
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ModifyOrderTx {
    /// Order ID to modify
    pub order_id: u64,
    /// New price (0 to keep existing)
    pub new_price: u64,
    /// New size (0 to keep existing)
    pub new_size: u64,
}

/// Close position transaction
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ClosePositionTx {
    /// Market identifier
    pub market_id: u32,
    /// Size to close (0 for full position)
    pub size: u64,
}

/// Set agent transaction (Hyperliquid-style)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct SetAgentTx {
    /// Agent address
    pub agent: Address,
    /// Permissions bitmap
    pub permissions: u64,
}

/// Remove agent transaction
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct RemoveAgentTx {
    /// Agent address to remove
    pub agent: Address,
}

/// Transfer tokens transaction
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct TransferTx {
    /// Recipient address
    pub to: Address,
    /// Amount to transfer
    pub amount: u128,
    /// Asset ID
    pub asset_id: u32,
}

/// Bridge deposit transaction (only authorized operators can execute)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct BridgeDepositTx {
    /// User address to credit
    pub user: Address,
    /// Amount to deposit
    pub amount: u128,
    /// Asset ID
    pub asset_id: u32,
    /// External transaction hash (for tracking)
    pub external_tx_hash: B256,
}

/// Bridge withdrawal transaction (only authorized operators can execute)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct BridgeWithdrawTx {
    /// User address to debit
    pub user: Address,
    /// Amount to withdraw
    pub amount: u128,
    /// Asset ID
    pub asset_id: u32,
    /// Destination address on external chain
    pub destination: Address,
    /// External transaction hash (for tracking)
    pub external_tx_hash: B256,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_encoding() {
        let tx = Transaction::new(
            1,
            Address::ZERO,
            TxPayload::Deposit(DepositTx {
                amount: 1000,
                asset_id: 0, // USDC or primary collateral token
            }),
        );

        let encoded = tx.encode();
        let decoded = Transaction::decode(&encoded).unwrap();
        assert_eq!(tx, decoded);
    }

    #[test]
    fn test_transaction_hash() {
        let tx = Transaction::new(
            1,
            Address::ZERO,
            TxPayload::PlaceOrder(PlaceOrderTx {
                market_id: 0, // BTC-PERP
                is_buy: true,
                order_type: OrderType::Limit,
                price: 50000_000000, // $50,000 with 6 decimals
                size: 100_000000,    // 100 contracts with 6 decimals
                time_in_force: TimeInForce::GTC,
                reduce_only: false,
                post_only: false,
            }),
        );

        let hash = tx.hash();
        assert_ne!(hash, B256::ZERO);
    }
}
