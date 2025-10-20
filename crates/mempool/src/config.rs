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
    pub fn new(max_size: usize, max_txs_per_sender: usize) -> Self {
        Self {
            max_size,
            max_txs_per_sender,
            strict_nonce_ordering: true,
        }
    }

    /// Create configuration with relaxed nonce ordering
    pub fn with_relaxed_nonce_ordering(mut self) -> Self {
        self.strict_nonce_ordering = false;
        self
    }

    /// Validate configuration parameters
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.max_size == 0 {
            return Err("max_size must be greater than 0");
        }
        if self.max_txs_per_sender == 0 {
            return Err("max_txs_per_sender must be greater than 0");
        }
        if self.max_txs_per_sender > self.max_size {
            return Err("max_txs_per_sender cannot exceed max_size");
        }
        Ok(())
    }
}

impl Default for MempoolConfig {
    fn default() -> Self {
        Self {
            max_size: 10_000,
            max_txs_per_sender: 100,
            strict_nonce_ordering: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = MempoolConfig::default();
        assert_eq!(config.max_size, 10_000);
        assert_eq!(config.max_txs_per_sender, 100);
        assert!(config.strict_nonce_ordering);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validation() {
        let invalid_config = MempoolConfig {
            max_size: 0,
            max_txs_per_sender: 100,
            strict_nonce_ordering: true,
        };
        assert!(invalid_config.validate().is_err());

        let invalid_config = MempoolConfig {
            max_size: 100,
            max_txs_per_sender: 200,
            strict_nonce_ordering: true,
        };
        assert!(invalid_config.validate().is_err());
    }

    #[test]
    fn test_relaxed_nonce_ordering() {
        let config = MempoolConfig::default().with_relaxed_nonce_ordering();
        assert!(!config.strict_nonce_ordering);
    }
}
