use thiserror::Error;

/// Transaction execution errors
#[derive(Debug, Error)]
pub enum TxExecutionError {
    #[error("Authentication error: {0}")]
    Auth(#[from] pranklin_auth::AuthError),

    #[error("Engine error: {0}")]
    Engine(#[from] pranklin_engine::EngineError),

    #[error("State error: {0}")]
    State(#[from] pranklin_state::StateError),

    #[error("Transaction decode error: {0}")]
    Decode(#[from] pranklin_tx::TxError),

    #[error("Invalid nonce: expected {expected}, got {got}")]
    InvalidNonce { expected: u64, got: u64 },

    #[error("Nonce gap detected: expected {expected}, got {got}")]
    NonceGap { expected: u64, got: u64 },

    #[error("Operation not implemented: {0}")]
    NotImplemented(String),

    #[error("Unauthorized operation")]
    Unauthorized,
}

impl From<String> for TxExecutionError {
    fn from(s: String) -> Self {
        Self::NotImplemented(s)
    }
}

impl From<&str> for TxExecutionError {
    fn from(s: &str) -> Self {
        Self::NotImplemented(s.to_string())
    }
}

pub type Result<T> = std::result::Result<T, TxExecutionError>;
