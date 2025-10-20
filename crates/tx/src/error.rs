use thiserror::Error;

/// Transaction errors
#[derive(Error, Debug)]
pub enum TxError {
    #[error("Failed to decode transaction: {0}")]
    DecodeError(String),

    #[error("Failed to encode transaction: {0}")]
    EncodeError(String),

    #[error("Invalid signature")]
    InvalidSignature,

    #[error("Invalid nonce: expected {expected}, got {got}")]
    InvalidNonce { expected: u64, got: u64 },

    #[error("Insufficient balance: required {required}, available {available}")]
    InsufficientBalance { required: String, available: String },

    #[error("Order not found: {0}")]
    OrderNotFound(String),

    #[error("Position not found for market: {0}")]
    PositionNotFound(String),

    #[error("Invalid order parameters: {0}")]
    InvalidOrderParams(String),

    #[error("Agent not authorized")]
    AgentNotAuthorized,

    #[error("Self-trade not allowed")]
    SelfTrade,

    #[error("Market not found: {0}")]
    MarketNotFound(String),

    #[error("Post-only order would take")]
    PostOnlyWouldTake,

    #[error("Reduce-only order would increase position")]
    ReduceOnlyWouldIncrease,

    #[error("Other error: {0}")]
    Other(String),
}
