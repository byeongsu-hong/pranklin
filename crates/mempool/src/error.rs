use alloy_primitives::{Address, B256};
use thiserror::Error;

/// Mempool errors
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum MempoolError {
    /// Mempool has reached its capacity
    #[error("Mempool is full (capacity: {0})")]
    MempoolFull(usize),

    /// Duplicate transaction detected
    #[error("Duplicate transaction: {0}")]
    DuplicateTransaction(B256),

    /// Transaction not found in mempool
    #[error("Transaction not found: {0}")]
    TransactionNotFound(B256),

    /// Invalid nonce for sender
    #[error("Invalid nonce {nonce} for sender {sender} (expected: {expected})")]
    InvalidNonce {
        sender: Address,
        nonce: u64,
        expected: u64,
    },

    /// Nonce gap detected
    #[error("Nonce gap detected for sender {sender}: got {nonce}, expected {expected}")]
    NonceGap {
        sender: Address,
        nonce: u64,
        expected: u64,
    },

    /// Rate limit exceeded
    #[error("Rate limit exceeded for sender {sender}: {current}/{max} transactions")]
    RateLimitExceeded {
        sender: Address,
        current: usize,
        max: usize,
    },

    /// Invalid configuration
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    /// Transaction validation failed
    #[error("Transaction validation failed: {0}")]
    ValidationFailed(String),

    /// Other error
    #[error("Other error: {0}")]
    Other(String),
}

impl MempoolError {
    /// Check if error is retryable
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        matches!(self, Self::MempoolFull(_))
    }

    /// Check if error is due to rate limiting
    #[must_use]
    pub const fn is_rate_limited(&self) -> bool {
        matches!(self, Self::RateLimitExceeded { .. })
    }

    /// Check if error is due to nonce issues
    #[must_use]
    pub const fn is_nonce_error(&self) -> bool {
        matches!(self, Self::InvalidNonce { .. } | Self::NonceGap { .. })
    }
}

impl From<&str> for MempoolError {
    fn from(s: &str) -> Self {
        Self::Other(s.to_string())
    }
}

impl From<String> for MempoolError {
    fn from(s: String) -> Self {
        Self::Other(s)
    }
}

impl AsRef<str> for MempoolError {
    fn as_ref(&self) -> &str {
        match self {
            Self::MempoolFull(_) => "mempool full",
            Self::DuplicateTransaction(_) => "duplicate transaction",
            Self::TransactionNotFound(_) => "transaction not found",
            Self::InvalidNonce { .. } => "invalid nonce",
            Self::NonceGap { .. } => "nonce gap",
            Self::RateLimitExceeded { .. } => "rate limit exceeded",
            Self::InvalidConfig(_) => "invalid config",
            Self::ValidationFailed(_) => "validation failed",
            Self::Other(_) => "other error",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_classification() {
        let full_error = MempoolError::MempoolFull(1000);
        assert!(full_error.is_retryable());
        assert!(!full_error.is_rate_limited());
        assert!(!full_error.is_nonce_error());

        let rate_limit_error = MempoolError::RateLimitExceeded {
            sender: Address::ZERO,
            current: 100,
            max: 100,
        };
        assert!(rate_limit_error.is_rate_limited());
        assert!(!rate_limit_error.is_retryable());

        let nonce_error = MempoolError::InvalidNonce {
            sender: Address::ZERO,
            nonce: 5,
            expected: 3,
        };
        assert!(nonce_error.is_nonce_error());
    }

    #[test]
    fn test_error_conversions() {
        let err: MempoolError = "test error".into();
        assert!(matches!(err, MempoolError::Other(_)));

        let err: MempoolError = String::from("another error").into();
        assert!(matches!(err, MempoolError::Other(_)));
    }

    #[test]
    fn test_error_as_ref() {
        let err = MempoolError::MempoolFull(100);
        assert_eq!(err.as_ref(), "mempool full");

        let err = MempoolError::DuplicateTransaction(Default::default());
        assert_eq!(err.as_ref(), "duplicate transaction");
    }
}
