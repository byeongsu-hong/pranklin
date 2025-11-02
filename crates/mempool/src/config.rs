/// Configuration for the mempool
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MempoolConfig {
    /// Maximum number of transactions in the pool
    pub max_size: usize,
    /// Maximum transactions per sender
    pub max_txs_per_sender: usize,
    /// Whether to enforce strict nonce ordering
    pub strict_nonce_ordering: bool,
}

impl MempoolConfig {
    /// Create a new mempool configuration
    #[must_use]
    pub const fn new(max_size: usize, max_txs_per_sender: usize) -> Self {
        Self {
            max_size,
            max_txs_per_sender,
            strict_nonce_ordering: true,
        }
    }

    /// Create configuration with relaxed nonce ordering
    #[must_use]
    pub const fn with_relaxed_nonce_ordering(mut self) -> Self {
        self.strict_nonce_ordering = false;
        self
    }

    /// Validate configuration parameters
    pub const fn validate(&self) -> Result<(), &'static str> {
        if self.max_size == 0 {
            Err("max_size must be greater than 0")
        } else if self.max_txs_per_sender == 0 {
            Err("max_txs_per_sender must be greater than 0")
        } else if self.max_txs_per_sender > self.max_size {
            Err("max_txs_per_sender cannot exceed max_size")
        } else {
            Ok(())
        }
    }
}

impl From<(usize, usize)> for MempoolConfig {
    fn from((max_size, max_txs_per_sender): (usize, usize)) -> Self {
        Self::new(max_size, max_txs_per_sender)
    }
}

impl From<(usize, usize, bool)> for MempoolConfig {
    fn from((max_size, max_txs_per_sender, strict_nonce_ordering): (usize, usize, bool)) -> Self {
        Self {
            max_size,
            max_txs_per_sender,
            strict_nonce_ordering,
        }
    }
}

impl Default for MempoolConfig {
    fn default() -> Self {
        Self::new(10_000, 100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_and_validation() {
        let config = MempoolConfig::default();
        assert_eq!(config.max_size, 10_000);
        assert_eq!(config.max_txs_per_sender, 100);
        assert!(config.strict_nonce_ordering);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_from_tuple() {
        let config: MempoolConfig = (1000, 50).into();
        assert_eq!(config.max_size, 1000);
        assert_eq!(config.max_txs_per_sender, 50);
    }

    #[test]
    fn test_validation_errors() {
        assert!(MempoolConfig::new(0, 100).validate().is_err());
        assert!(MempoolConfig::new(100, 0).validate().is_err());
        assert!(MempoolConfig::new(100, 200).validate().is_err());
        assert!(MempoolConfig::new(100, 100).validate().is_ok());
    }

    #[test]
    fn test_relaxed_nonce_ordering() {
        let config = MempoolConfig::default().with_relaxed_nonce_ordering();
        assert!(!config.strict_nonce_ordering);
    }
}
