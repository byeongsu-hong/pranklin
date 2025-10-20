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
    pub fn is_retryable(&self) -> bool {
        matches!(self, MempoolError::MempoolFull(_))
    }

    /// Check if error is due to rate limiting
    pub fn is_rate_limited(&self) -> bool {
        matches!(self, MempoolError::RateLimitExceeded { .. })
    }

    /// Check if error is due to nonce issues
    pub fn is_nonce_error(&self) -> bool {
        matches!(
            self,
            MempoolError::InvalidNonce { .. } | MempoolError::NonceGap { .. }
        )
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
}
