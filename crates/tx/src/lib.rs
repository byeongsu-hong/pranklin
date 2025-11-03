mod error;
mod types;

pub use error::*;
pub use types::*;

// Re-export Alloy primitives as the main types
pub use alloy_primitives::{Address, B256, Signature, U256};

use pranklin_macros::standard;
use sha2::{Digest, Sha256};

/// Transaction signature type (Ethereum-compatible)
///
/// Supports two signing modes:
/// - EIP-712: For regular users (MetaMask, etc.) - human-readable structured data
/// - RawBorsh: For agents (hot wallets) - efficient raw transaction signing
#[standard]
#[repr(u8)]
#[borsh(use_discriminant = true)]
pub enum TxSignature {
    /// EIP-712 typed structured data signature
    /// Used by regular users for human-readable signing
    EIP712 {
        /// EIP-712 domain separator
        domain: TypedDataDomain,
        /// Signature (65 bytes: r[32] + s[32] + v[1])
        #[serde(with = "signature_serde")]
        signature: [u8; 65],
    } = 0,

    /// Raw Borsh transaction hash signature
    /// Used by agents for efficient signing
    RawBorsh {
        /// Signature (65 bytes: r[32] + s[32] + v[1])
        #[serde(with = "signature_serde")]
        signature: [u8; 65],
    } = 1,
}

/// EIP-712 Domain separator
#[standard]
pub struct TypedDataDomain {
    /// Domain name (e.g., "Pranklin")
    pub name: String,
    /// Domain version (e.g., "1")
    pub version: String,
    /// Chain ID (e.g., 1337 for Pranklin)
    pub chain_id: U256,
    /// Verifying contract (optional)
    pub verifying_contract: Option<Address>,
}

impl Default for TypedDataDomain {
    fn default() -> Self {
        Self {
            name: "Pranklin".to_string(),
            version: "1".to_string(),
            chain_id: U256::from(1337), // Custom chain ID
            verifying_contract: None,
        }
    }
}

impl TypedDataDomain {
    /// Compute EIP-712 domain separator hash
    pub fn hash(&self) -> B256 {
        const TYPE_HASH_STR: &[u8] = b"EIP712Domain(string name,string version,uint256 chainId)";

        let mut data = Vec::with_capacity(128);
        data.extend_from_slice(alloy_primitives::keccak256(TYPE_HASH_STR).as_slice());
        data.extend_from_slice(alloy_primitives::keccak256(self.name.as_bytes()).as_slice());
        data.extend_from_slice(alloy_primitives::keccak256(self.version.as_bytes()).as_slice());
        data.extend_from_slice(&self.chain_id.to_be_bytes::<32>());

        alloy_primitives::keccak256(&data)
    }

    /// Create a domain with custom values
    pub fn new(name: impl Into<String>, version: impl Into<String>, chain_id: u64) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            chain_id: U256::from(chain_id),
            verifying_contract: None,
        }
    }
}

/// Main transaction structure for the perp DEX
/// Supports both EIP-712 (user-friendly) and RawBorsh (efficient) signatures
#[standard]
pub struct Transaction {
    /// Transaction nonce to prevent replay attacks
    pub nonce: u64,
    /// Sender address (derived from signature)
    pub from: Address,
    /// Transaction payload
    pub payload: TxPayload,
    /// Signature with type information
    pub signature: TxSignature,
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
        Vec::<u8>::deserialize(deserializer)?
            .try_into()
            .map_err(|v: Vec<u8>| {
                serde::de::Error::custom(format!(
                    "Invalid signature length: expected 65, got {}",
                    v.len()
                ))
            })
    }
}

// Trait implementations for cleaner conversions
impl AsRef<[u8; 65]> for TxSignature {
    fn as_ref(&self) -> &[u8; 65] {
        match self {
            TxSignature::RawBorsh { signature } | TxSignature::EIP712 { signature, .. } => {
                signature
            }
        }
    }
}

impl AsMut<[u8; 65]> for TxSignature {
    fn as_mut(&mut self) -> &mut [u8; 65] {
        match self {
            TxSignature::RawBorsh { signature } | TxSignature::EIP712 { signature, .. } => {
                signature
            }
        }
    }
}

// Helper trait for EIP712 encoding
trait Eip712Encode {
    fn encode_as_bytes32(&self) -> [u8; 32];
}

impl Eip712Encode for Address {
    fn encode_as_bytes32(&self) -> [u8; 32] {
        let mut bytes = [0u8; 32];
        bytes[12..].copy_from_slice(self.as_slice());
        bytes
    }
}

// Generic implementation for numeric types
macro_rules! impl_eip712_encode_for_uint {
    ($($t:ty),*) => {
        $(
            impl Eip712Encode for $t {
                fn encode_as_bytes32(&self) -> [u8; 32] {
                    U256::from(*self).to_be_bytes::<32>()
                }
            }
        )*
    };
}

impl_eip712_encode_for_uint!(u128, u64, u32, u16, u8);

impl Eip712Encode for bool {
    fn encode_as_bytes32(&self) -> [u8; 32] {
        (*self as u8).encode_as_bytes32()
    }
}

impl Transaction {
    /// Create a new unsigned transaction with RawBorsh signature type
    pub fn new_raw(nonce: u64, from: Address, payload: TxPayload) -> Self {
        Self {
            nonce,
            from,
            payload,
            signature: TxSignature::RawBorsh {
                signature: [0u8; 65],
            },
        }
    }

    /// Create a new transaction with EIP-712 signature type
    pub fn new_eip712(nonce: u64, from: Address, payload: TxPayload) -> Self {
        Self::new_eip712_with_domain(nonce, from, payload, TypedDataDomain::default())
    }

    /// Create a new transaction with custom EIP-712 domain
    pub fn new_eip712_with_domain(
        nonce: u64,
        from: Address,
        payload: TxPayload,
        domain: TypedDataDomain,
    ) -> Self {
        Self {
            nonce,
            from,
            payload,
            signature: TxSignature::EIP712 {
                domain,
                signature: [0u8; 65],
            },
        }
    }

    /// Get the transaction hash (used as transaction ID)
    pub fn hash(&self) -> B256 {
        B256::from_slice(Sha256::digest(self.encode()).as_slice())
    }

    /// Get the signing hash based on signature type
    pub fn signing_hash(&self) -> B256 {
        match &self.signature {
            TxSignature::RawBorsh { .. } => self.raw_signing_hash(),
            TxSignature::EIP712 { domain, .. } => self.eip712_signing_hash(domain),
        }
    }

    /// Get raw Borsh signing hash (for agents)
    fn raw_signing_hash(&self) -> B256 {
        let mut data = Vec::with_capacity(8 + 20 + 128);
        data.extend_from_slice(&self.nonce.to_le_bytes());
        data.extend_from_slice(self.from.as_slice());
        data.extend_from_slice(&borsh::to_vec(&self.payload).expect("Failed to serialize payload"));
        B256::from_slice(Sha256::digest(data).as_slice())
    }

    /// Get EIP-712 signing hash (for users)
    fn eip712_signing_hash(&self, domain: &TypedDataDomain) -> B256 {
        // EIP-712 structure hash: keccak256(typeHash || encodeData)
        let mut struct_hash_data = Vec::with_capacity(64);
        struct_hash_data.extend_from_slice(self.payload.eip712_type_hash().as_slice());
        struct_hash_data.extend_from_slice(
            self.payload
                .eip712_encode_data(self.nonce, self.from)
                .as_slice(),
        );
        let struct_hash = alloy_primitives::keccak256(&struct_hash_data);

        // Final EIP-712 hash: keccak256("\x19\x01" || domainSeparator || structHash)
        let mut final_data = Vec::with_capacity(66);
        final_data.extend_from_slice(&[0x19, 0x01]);
        final_data.extend_from_slice(domain.hash().as_slice());
        final_data.extend_from_slice(struct_hash.as_slice());

        alloy_primitives::keccak256(&final_data)
    }

    /// Set signature
    pub fn with_signature(mut self, sig: [u8; 65]) -> Self {
        *self.signature.as_mut() = sig;
        self
    }

    /// Set signature (mutable)
    pub fn set_signature(&mut self, sig: [u8; 65]) {
        *self.signature.as_mut() = sig;
    }

    /// Get signature bytes
    pub fn signature_bytes(&self) -> &[u8; 65] {
        self.signature.as_ref()
    }

    /// Encode the transaction to bytes (using Borsh for deterministic serialization)
    pub fn encode(&self) -> Vec<u8> {
        borsh::to_vec(self).expect("Failed to encode transaction")
    }

    /// Decode the transaction from bytes
    pub fn decode(data: &[u8]) -> Result<Self, TxError> {
        data.try_into()
    }
}

impl TryFrom<&[u8]> for Transaction {
    type Error = TxError;

    fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
        const MAX_TX_SIZE: usize = 100_000; // 100KB - Prevent DoS attacks

        if data.len() > MAX_TX_SIZE {
            return Err(TxError::Other(format!(
                "Transaction too large: {} bytes (max: {} bytes)",
                data.len(),
                MAX_TX_SIZE
            )));
        }

        borsh::from_slice(data).map_err(Into::into)
    }
}

impl TryFrom<Vec<u8>> for Transaction {
    type Error = TxError;

    fn try_from(data: Vec<u8>) -> Result<Self, Self::Error> {
        data.as_slice().try_into()
    }
}

impl From<Transaction> for Vec<u8> {
    fn from(tx: Transaction) -> Self {
        tx.encode()
    }
}

impl From<&Transaction> for Vec<u8> {
    fn from(tx: &Transaction) -> Self {
        tx.encode()
    }
}

/// Transaction payload containing the actual operation
#[standard]
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
#[standard]
pub struct DepositTx {
    /// Amount to deposit (in base units, u128 supports ~340 undecillion)
    pub amount: u128,
    /// Asset/token identifier (auto-incremental, supports 4B tokens)
    pub asset_id: u32,
}

/// Withdraw collateral transaction
#[standard]
pub struct WithdrawTx {
    /// Amount to withdraw (in base units)
    pub amount: u128,
    /// Asset/token identifier
    pub asset_id: u32,
    /// Destination address
    pub to: Address,
}

/// Place order transaction
#[standard]
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
#[standard]
pub struct CancelOrderTx {
    /// Order ID to cancel (u64 supports massive number of orders)
    pub order_id: u64,
}

/// Modify order transaction
#[standard]
pub struct ModifyOrderTx {
    /// Order ID to modify
    pub order_id: u64,
    /// New price (0 to keep existing)
    pub new_price: u64,
    /// New size (0 to keep existing)
    pub new_size: u64,
}

/// Close position transaction
#[standard]
pub struct ClosePositionTx {
    /// Market identifier
    pub market_id: u32,
    /// Size to close (0 for full position)
    pub size: u64,
}

/// Set agent transaction (Hyperliquid-style)
#[standard]
pub struct SetAgentTx {
    /// Agent address
    pub agent: Address,
    /// Permissions bitmap
    pub permissions: u64,
}

/// Remove agent transaction
#[standard]
pub struct RemoveAgentTx {
    /// Agent address to remove
    pub agent: Address,
}

/// Transfer tokens transaction
#[standard]
pub struct TransferTx {
    /// Recipient address
    pub to: Address,
    /// Amount to transfer
    pub amount: u128,
    /// Asset ID
    pub asset_id: u32,
}

/// Bridge deposit transaction (only authorized operators can execute)
#[standard]
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
#[standard]
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

// EIP-712 type hashes - using const functions for compile-time evaluation when possible
mod eip712_type_hashes {
    use alloy_primitives::B256;

    macro_rules! type_hash {
        ($name:ident, $type_str:expr) => {
            pub fn $name() -> B256 {
                alloy_primitives::keccak256($type_str)
            }
        };
    }

    type_hash!(
        deposit,
        b"Deposit(uint64 nonce,address from,uint128 amount,uint32 assetId)"
    );
    type_hash!(
        withdraw,
        b"Withdraw(uint64 nonce,address from,uint128 amount,uint32 assetId,address to)"
    );
    type_hash!(place_order, b"PlaceOrder(uint64 nonce,address from,uint32 marketId,bool isBuy,uint8 orderType,uint64 price,uint64 size,uint8 timeInForce,bool reduceOnly,bool postOnly)");
    type_hash!(
        cancel_order,
        b"CancelOrder(uint64 nonce,address from,uint64 orderId)"
    );
    type_hash!(
        transfer,
        b"Transfer(uint64 nonce,address from,address to,uint128 amount,uint32 assetId)"
    );
    type_hash!(
        generic,
        b"GenericTx(uint64 nonce,address from,bytes payload)"
    );
}

impl TxPayload {
    /// Get EIP-712 type hash for this payload
    pub fn eip712_type_hash(&self) -> B256 {
        match self {
            TxPayload::Deposit(_) => eip712_type_hashes::deposit(),
            TxPayload::Withdraw(_) => eip712_type_hashes::withdraw(),
            TxPayload::PlaceOrder(_) => eip712_type_hashes::place_order(),
            TxPayload::CancelOrder(_) => eip712_type_hashes::cancel_order(),
            TxPayload::Transfer(_) => eip712_type_hashes::transfer(),
            _ => eip712_type_hashes::generic(),
        }
    }

    /// Encode EIP-712 data for this payload
    pub fn eip712_encode_data(&self, nonce: u64, from: Address) -> B256 {
        let encoder = match self {
            TxPayload::Deposit(d) => Eip712DataEncoder::new(nonce, from)
                .add(d.amount)
                .add(d.asset_id),
            TxPayload::Withdraw(w) => Eip712DataEncoder::new(nonce, from)
                .add(w.amount)
                .add(w.asset_id)
                .add(w.to),
            TxPayload::PlaceOrder(o) => Eip712DataEncoder::new(nonce, from)
                .add(o.market_id)
                .add(o.is_buy)
                .add(o.order_type as u8)
                .add(o.price)
                .add(o.size)
                .add(o.time_in_force as u8)
                .add(o.reduce_only)
                .add(o.post_only),
            TxPayload::CancelOrder(c) => Eip712DataEncoder::new(nonce, from).add(c.order_id),
            TxPayload::Transfer(t) => Eip712DataEncoder::new(nonce, from)
                .add(t.to)
                .add(t.amount)
                .add(t.asset_id),
            _ => Eip712DataEncoder::new(nonce, from).add_hash(alloy_primitives::keccak256(
                borsh::to_vec(self).expect("Failed to encode payload"),
            )),
        };

        encoder.finish()
    }
}

// Helper struct for building EIP-712 encoded data
struct Eip712DataEncoder {
    data: Vec<u8>,
}

impl Eip712DataEncoder {
    fn new(nonce: u64, from: Address) -> Self {
        let mut data = Vec::with_capacity(256);
        data.extend_from_slice(&nonce.encode_as_bytes32());
        data.extend_from_slice(&from.encode_as_bytes32());
        Self { data }
    }

    fn add<T: Eip712Encode>(mut self, value: T) -> Self {
        self.data.extend_from_slice(&value.encode_as_bytes32());
        self
    }

    fn add_hash(mut self, hash: B256) -> Self {
        self.data.extend_from_slice(hash.as_slice());
        self
    }

    fn finish(self) -> B256 {
        alloy_primitives::keccak256(&self.data)
    }
}

// ============================================================================
// StateAccess Implementation
// ============================================================================

// Helper functions for common access patterns
fn balance_access(
    address: Address,
    asset_id: u32,
    mode: pranklin_state::AccessMode,
) -> (pranklin_state::StateAccess, pranklin_state::AccessMode) {
    (
        pranklin_state::StateAccess::Balance { address, asset_id },
        mode,
    )
}

fn asset_info_access(asset_id: u32) -> (pranklin_state::StateAccess, pranklin_state::AccessMode) {
    (
        pranklin_state::StateAccess::AssetInfo { asset_id },
        pranklin_state::AccessMode::Read,
    )
}

fn balance_with_asset(
    accesses: &mut Vec<(pranklin_state::StateAccess, pranklin_state::AccessMode)>,
    address: Address,
    asset_id: u32,
) {
    accesses.push(balance_access(
        address,
        asset_id,
        pranklin_state::AccessMode::Write,
    ));
    accesses.push(asset_info_access(asset_id));
}

impl pranklin_state::DeclareStateAccess for Transaction {
    fn declare_accesses(&self) -> Vec<(pranklin_state::StateAccess, pranklin_state::AccessMode)> {
        let mut accesses = vec![(
            pranklin_state::StateAccess::Nonce { address: self.from },
            pranklin_state::AccessMode::Write,
        )];

        match &self.payload {
            TxPayload::Deposit(d) => {
                balance_with_asset(&mut accesses, self.from, d.asset_id);
            }
            TxPayload::Withdraw(w) => {
                balance_with_asset(&mut accesses, self.from, w.asset_id);
            }
            TxPayload::PlaceOrder(o) => {
                accesses.push(balance_access(
                    self.from,
                    0,
                    pranklin_state::AccessMode::Write,
                ));
                accesses.extend([
                    (
                        pranklin_state::StateAccess::Position {
                            address: self.from,
                            market_id: o.market_id,
                        },
                        pranklin_state::AccessMode::Write,
                    ),
                    (
                        pranklin_state::StateAccess::OrderList {
                            market_id: o.market_id,
                        },
                        pranklin_state::AccessMode::Write,
                    ),
                    (
                        pranklin_state::StateAccess::Market {
                            market_id: o.market_id,
                        },
                        pranklin_state::AccessMode::Read,
                    ),
                    (
                        pranklin_state::StateAccess::FundingRate {
                            market_id: o.market_id,
                        },
                        pranklin_state::AccessMode::Read,
                    ),
                ]);
            }
            TxPayload::CancelOrder(c) => {
                accesses.push((
                    pranklin_state::StateAccess::Order {
                        order_id: c.order_id,
                    },
                    pranklin_state::AccessMode::Write,
                ));
                // Note: We don't know the market_id without reading the order first
            }
            TxPayload::ModifyOrder(m) => {
                accesses.push((
                    pranklin_state::StateAccess::Order {
                        order_id: m.order_id,
                    },
                    pranklin_state::AccessMode::Write,
                ));
            }
            TxPayload::ClosePosition(c) => {
                accesses.extend([
                    (
                        pranklin_state::StateAccess::Position {
                            address: self.from,
                            market_id: c.market_id,
                        },
                        pranklin_state::AccessMode::Write,
                    ),
                    (
                        pranklin_state::StateAccess::OrderList {
                            market_id: c.market_id,
                        },
                        pranklin_state::AccessMode::Write,
                    ),
                    (
                        pranklin_state::StateAccess::Market {
                            market_id: c.market_id,
                        },
                        pranklin_state::AccessMode::Read,
                    ),
                ]);
            }
            TxPayload::SetAgent(_) | TxPayload::RemoveAgent(_) => {
                // Agent operations don't access on-chain state (only AuthService in-memory)
            }
            TxPayload::Transfer(t) => {
                accesses.push(balance_access(
                    self.from,
                    t.asset_id,
                    pranklin_state::AccessMode::Write,
                ));
                accesses.push(balance_access(
                    t.to,
                    t.asset_id,
                    pranklin_state::AccessMode::Write,
                ));
                accesses.push(asset_info_access(t.asset_id));
            }
            TxPayload::BridgeDeposit(bd) => {
                accesses.push((
                    pranklin_state::StateAccess::BridgeOperator { address: self.from },
                    pranklin_state::AccessMode::Read,
                ));
                balance_with_asset(&mut accesses, bd.user, bd.asset_id);
            }
            TxPayload::BridgeWithdraw(bw) => {
                accesses.push((
                    pranklin_state::StateAccess::BridgeOperator { address: self.from },
                    pranklin_state::AccessMode::Read,
                ));
                balance_with_asset(&mut accesses, bw.user, bw.asset_id);
            }
        }

        accesses
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_encoding_decoding() {
        let tx = Transaction::new_raw(
            1,
            Address::ZERO,
            TxPayload::Deposit(DepositTx {
                amount: 1000,
                asset_id: 0,
            }),
        );

        let encoded: Vec<u8> = (&tx).into();
        let decoded: Transaction = encoded.as_slice().try_into().unwrap();
        assert_eq!(tx, decoded);
    }

    #[test]
    fn test_transaction_hash() {
        let tx = Transaction::new_raw(
            1,
            Address::ZERO,
            TxPayload::PlaceOrder(PlaceOrderTx {
                market_id: 0,
                is_buy: true,
                order_type: OrderType::Limit,
                price: 50000_000000,
                size: 100_000000,
                time_in_force: TimeInForce::GTC,
                reduce_only: false,
                post_only: false,
            }),
        );

        let hash = tx.hash();
        assert_ne!(hash, B256::ZERO);
    }

    #[test]
    fn test_signature_trait() {
        let mut tx = Transaction::new_raw(
            1,
            Address::ZERO,
            TxPayload::Deposit(DepositTx {
                amount: 1000,
                asset_id: 0,
            }),
        );

        let sig = [1u8; 65];
        tx.set_signature(sig);
        assert_eq!(tx.signature_bytes(), &sig);
    }

    #[test]
    fn test_builder_pattern() {
        let sig = [1u8; 65];
        let tx = Transaction::new_eip712(
            1,
            Address::ZERO,
            TxPayload::Deposit(DepositTx {
                amount: 1000,
                asset_id: 0,
            }),
        )
        .with_signature(sig);

        assert_eq!(tx.signature_bytes(), &sig);
    }

    #[test]
    fn test_typed_data_domain() {
        let domain = TypedDataDomain::new("Pranklin", "1", 1337);
        assert_eq!(domain.name, "Pranklin");
        assert_eq!(domain.version, "1");
        assert_eq!(domain.chain_id, U256::from(1337));
    }
}
