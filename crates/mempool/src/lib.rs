mod config;
mod error;
mod transaction_index;

pub use config::*;
pub use error::*;

use alloy_primitives::{Address, B256};
use transaction_index::TransactionIndex;
use pranklin_tx::Transaction;

/// Mempool for storing pending transactions
///
/// The mempool provides efficient transaction storage and retrieval with:
/// - Duplicate transaction detection
/// - Per-sender rate limiting
/// - Nonce ordering and validation
/// - Capacity management
#[derive(Debug, Clone)]
pub struct Mempool {
    /// Transaction index for efficient lookups
    index: TransactionIndex,
    /// Mempool configuration
    config: MempoolConfig,
}

impl Mempool {
    /// Create a new mempool with custom configuration
    pub fn new(config: MempoolConfig) -> Self {
        Self {
            index: TransactionIndex::new(),
            config,
        }
    }

    /// Create a new mempool with size limits
    pub fn with_limits(max_size: usize, max_txs_per_sender: usize) -> Self {
        Self::new(MempoolConfig::new(max_size, max_txs_per_sender))
    }

    /// Add a transaction to the mempool
    pub fn add(&mut self, tx: Transaction) -> Result<B256, MempoolError> {
        // Check if mempool is full
        if self.index.len() >= self.config.max_size {
            return Err(MempoolError::MempoolFull(self.config.max_size));
        }

        let tx_hash = tx.hash();

        // Check if transaction already exists
        if self.index.contains(&tx_hash) {
            return Err(MempoolError::DuplicateTransaction(tx_hash));
        }

        // Check rate limit per sender
        let sender_count = self.index.sender_tx_count(&tx.from);
        if sender_count >= self.config.max_txs_per_sender {
            return Err(MempoolError::RateLimitExceeded {
                sender: tx.from,
                current: sender_count,
                max: self.config.max_txs_per_sender,
            });
        }

        // Insert into index
        self.index.insert(tx);

        Ok(tx_hash)
    }

    /// Remove a transaction from the mempool
    pub fn remove(&mut self, tx_hash: &B256) -> Option<Transaction> {
        self.index.remove(tx_hash)
    }

    /// Get a transaction by hash
    pub fn get(&self, tx_hash: &B256) -> Option<&Transaction> {
        self.index.get(tx_hash)
    }

    /// Get pending transactions for a sender
    pub fn get_by_sender(&self, sender: Address) -> Vec<Transaction> {
        self.index.get_by_sender(&sender)
    }

    /// Get next transactions ready for execution
    /// Returns transactions sorted by nonce for each sender
    pub fn get_ready_txs(&self, max_count: usize) -> Vec<Transaction> {
        let mut ready = Vec::new();
        let mut count = 0;

        for sender in self.index.get_all_senders() {
            if count >= max_count {
                break;
            }

            // Get transactions for this sender in nonce order
            let sender_txs = self.index.get_by_sender(&sender);
            for tx in sender_txs {
                if count >= max_count {
                    break;
                }
                ready.push(tx);
                count += 1;
            }
        }

        ready
    }

    /// Remove transactions for a sender up to a nonce
    pub fn remove_up_to_nonce(&mut self, sender: Address, nonce: u64) -> Vec<B256> {
        self.index
            .remove_up_to_nonce(&sender, nonce)
            .into_iter()
            .map(|(hash, _)| hash)
            .collect()
    }

    /// Get the number of transactions in the mempool
    pub fn len(&self) -> usize {
        self.index.len()
    }

    /// Check if the mempool is empty
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// Clear all transactions
    pub fn clear(&mut self) {
        self.index.clear();
    }

    /// Get next nonce for a sender
    pub fn get_next_nonce(&self, sender: Address) -> Option<u64> {
        self.index.get_highest_nonce(&sender).map(|n| n + 1)
    }

    /// Check if a transaction with specific nonce exists for a sender
    pub fn has_nonce(&self, sender: Address, nonce: u64) -> bool {
        self.index.has_nonce(&sender, nonce)
    }
}

impl Default for Mempool {
    fn default() -> Self {
        Self::new(MempoolConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pranklin_tx::{DepositTx, TxPayload};

    fn create_test_tx(from: Address, nonce: u64) -> Transaction {
        Transaction {
            nonce,
            from,
            payload: TxPayload::Deposit(DepositTx {
                amount: 1000,
                asset_id: 0,
            }),
            signature: [0u8; 65],
        }
    }

    #[test]
    fn test_mempool_add_remove() {
        let mut mempool = Mempool::with_limits(100, 100);
        let sender = Address::with_last_byte(1);
        let tx = create_test_tx(sender, 1);
        let hash = tx.hash();

        // Add transaction
        mempool.add(tx.clone()).unwrap();
        assert_eq!(mempool.len(), 1);
        assert!(mempool.get(&hash).is_some());

        // Remove transaction
        let removed = mempool.remove(&hash);
        assert!(removed.is_some());
        assert_eq!(mempool.len(), 0);
    }

    #[test]
    fn test_mempool_by_sender() {
        let mut mempool = Mempool::with_limits(100, 100);
        let sender = Address::with_last_byte(1);

        // Add multiple transactions from same sender
        for nonce in 1..=5 {
            let tx = create_test_tx(sender, nonce);
            mempool.add(tx).unwrap();
        }

        let txs = mempool.get_by_sender(sender);
        assert_eq!(txs.len(), 5);
    }

    #[test]
    fn test_remove_up_to_nonce() {
        let mut mempool = Mempool::with_limits(100, 100);
        let sender = Address::with_last_byte(1);

        for nonce in 1..=5 {
            let tx = create_test_tx(sender, nonce);
            mempool.add(tx).unwrap();
        }

        mempool.remove_up_to_nonce(sender, 3);
        assert_eq!(mempool.len(), 2);

        let remaining = mempool.get_by_sender(sender);
        assert_eq!(remaining.len(), 2);
        assert_eq!(remaining[0].nonce, 4);
        assert_eq!(remaining[1].nonce, 5);
    }

    #[test]
    fn test_duplicate_transaction() {
        let mut mempool = Mempool::with_limits(100, 100);
        let tx = create_test_tx(Address::with_last_byte(1), 1);

        mempool.add(tx.clone()).unwrap();
        let result = mempool.add(tx);
        assert!(matches!(result, Err(MempoolError::DuplicateTransaction(_))));
    }

    #[test]
    fn test_mempool_full() {
        let mut mempool = Mempool::with_limits(2, 100);

        mempool
            .add(create_test_tx(Address::with_last_byte(1), 1))
            .unwrap();
        mempool
            .add(create_test_tx(Address::with_last_byte(2), 2))
            .unwrap();

        let result = mempool.add(create_test_tx(Address::with_last_byte(3), 3));
        assert!(matches!(result, Err(MempoolError::MempoolFull(_))));
    }
}
