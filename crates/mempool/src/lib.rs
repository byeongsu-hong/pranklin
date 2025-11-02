mod config;
mod error;
mod transaction_index;

pub use config::MempoolConfig;
pub use error::MempoolError;

pub use transaction_index::TransactionIndex;

use alloy_primitives::{Address, B256};
use pranklin_tx::Transaction;
use std::{
    borrow::Borrow,
    ops::{Deref, DerefMut, Index},
};

/// Mempool for storing pending transactions
///
/// Provides efficient transaction storage and retrieval with:
/// - Duplicate detection
/// - Per-sender rate limiting  
/// - Nonce ordering and validation
/// - Capacity management
#[derive(Debug, Clone)]
pub struct Mempool {
    index: TransactionIndex,
    config: MempoolConfig,
}

/// Result type for mempool operations
pub type MempoolResult<T> = Result<T, MempoolError>;

impl Mempool {
    /// Create a new mempool with custom configuration
    #[must_use]
    pub fn new(config: MempoolConfig) -> Self {
        Self {
            index: TransactionIndex::new(),
            config,
        }
    }

    /// Create a new mempool with size limits
    #[must_use]
    pub fn with_limits(max_size: usize, max_txs_per_sender: usize) -> Self {
        Self::new((max_size, max_txs_per_sender).into())
    }

    /// Get configuration
    #[must_use]
    pub const fn config(&self) -> &MempoolConfig {
        &self.config
    }

    /// Check if mempool is at capacity
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.index.len() >= self.config.max_size
    }

    /// Get available capacity
    #[must_use]
    pub fn available_capacity(&self) -> usize {
        self.config.max_size.saturating_sub(self.index.len())
    }

    /// Add a transaction to the mempool
    pub fn add(&mut self, tx: Transaction) -> MempoolResult<B256> {
        self.validate_capacity()?;

        let tx_hash = tx.hash();
        self.validate_not_duplicate(&tx_hash)?;
        self.validate_sender_rate_limit(&tx.from)?;

        self.index.insert(tx);
        Ok(tx_hash)
    }

    fn validate_capacity(&self) -> MempoolResult<()> {
        if self.is_full() {
            Err(MempoolError::MempoolFull(self.config.max_size))
        } else {
            Ok(())
        }
    }

    fn validate_not_duplicate(&self, tx_hash: &B256) -> MempoolResult<()> {
        if self.index.contains(tx_hash) {
            Err(MempoolError::DuplicateTransaction(*tx_hash))
        } else {
            Ok(())
        }
    }

    fn validate_sender_rate_limit(&self, sender: &Address) -> MempoolResult<()> {
        let count = self.index.sender_tx_count(sender);
        if count >= self.config.max_txs_per_sender {
            Err(MempoolError::RateLimitExceeded {
                sender: *sender,
                current: count,
                max: self.config.max_txs_per_sender,
            })
        } else {
            Ok(())
        }
    }

    /// Remove a transaction from the mempool
    pub fn remove(&mut self, tx_hash: &B256) -> Option<Transaction> {
        self.index.remove(tx_hash)
    }

    /// Get a transaction by hash
    #[must_use]
    pub fn get(&self, tx_hash: &B256) -> Option<&Transaction> {
        self.index.get(tx_hash)
    }

    /// Check if transaction exists
    #[must_use]
    pub fn contains(&self, tx_hash: &B256) -> bool {
        self.index.contains(tx_hash)
    }

    /// Get all pending transactions for a sender
    #[must_use]
    pub fn get_sender_txs(&self, sender: &Address) -> Vec<Transaction> {
        self.index.get_by_sender(sender)
    }

    /// Get transaction count for a sender
    #[must_use]
    pub fn sender_tx_count(&self, sender: &Address) -> usize {
        self.index.sender_tx_count(sender)
    }

    /// Get next transactions ready for execution
    #[must_use]
    pub fn ready_txs(&self, limit: usize) -> Vec<Transaction> {
        self.index
            .get_all_senders()
            .into_iter()
            .flat_map(|sender| self.index.get_by_sender(&sender))
            .take(limit)
            .collect()
    }

    /// Get all senders with pending transactions
    #[must_use]
    pub fn senders(&self) -> Vec<Address> {
        self.index.get_all_senders()
    }

    /// Iterate over all senders
    pub fn senders_iter(&self) -> impl Iterator<Item = &Address> {
        self.index.senders_iter()
    }

    /// Iterate over transactions for a sender
    pub fn sender_txs_iter(&self, sender: &Address) -> impl Iterator<Item = &Transaction> + '_ {
        self.index.sender_txs_iter(sender)
    }

    /// Iterate over all transactions
    pub fn iter(&self) -> impl Iterator<Item = (&B256, &Transaction)> {
        self.index.into_iter()
    }

    /// Remove transactions for a sender up to (and including) a nonce
    pub fn prune_sender_nonces(&mut self, sender: &Address, up_to_nonce: u64) -> Vec<B256> {
        self.index
            .remove_up_to_nonce(sender, up_to_nonce)
            .into_iter()
            .map(|(hash, _)| hash)
            .collect()
    }

    /// Get the number of transactions in the mempool
    #[must_use]
    pub fn len(&self) -> usize {
        self.index.len()
    }

    /// Check if the mempool is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// Clear all transactions
    pub fn clear(&mut self) {
        self.index.clear();
    }

    /// Get expected next nonce for a sender (highest nonce + 1)
    #[must_use]
    pub fn next_nonce(&self, sender: &Address) -> Option<u64> {
        self.index.get_highest_nonce(sender).map(|n| n + 1)
    }

    /// Get expected next nonce for a sender, defaults to 0 if no transactions exist
    #[must_use]
    pub fn next_nonce_or_default(&self, sender: &Address) -> u64 {
        self.next_nonce(sender).unwrap_or(0)
    }

    /// Check if a transaction with specific nonce exists for a sender
    #[must_use]
    pub fn has_nonce(&self, sender: &Address, nonce: u64) -> bool {
        self.index.has_nonce(sender, nonce)
    }

    /// Get the highest nonce for a sender
    #[must_use]
    pub fn highest_nonce(&self, sender: &Address) -> Option<u64> {
        self.index.get_highest_nonce(sender)
    }

    /// Batch add multiple transactions
    pub fn add_batch(&mut self, txs: Vec<Transaction>) -> Vec<MempoolResult<B256>> {
        txs.into_iter().map(|tx| self.add(tx)).collect()
    }

    /// Get stats about the mempool
    #[must_use]
    pub fn stats(&self) -> MempoolStats {
        MempoolStats {
            total_txs: self.len(),
            unique_senders: self.index.sender_count(),
            capacity: self.config.max_size,
            available: self.available_capacity(),
            utilization_pct: (self.len() as f64 / self.config.max_size as f64 * 100.0) as u8,
        }
    }
}

/// Statistics about mempool state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MempoolStats {
    pub total_txs: usize,
    pub unique_senders: usize,
    pub capacity: usize,
    pub available: usize,
    pub utilization_pct: u8,
}

impl From<MempoolConfig> for Mempool {
    fn from(config: MempoolConfig) -> Self {
        Self::new(config)
    }
}

impl Default for Mempool {
    fn default() -> Self {
        Self::new(MempoolConfig::default())
    }
}

impl Deref for Mempool {
    type Target = TransactionIndex;

    fn deref(&self) -> &Self::Target {
        &self.index
    }
}

impl DerefMut for Mempool {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.index
    }
}

impl AsRef<TransactionIndex> for Mempool {
    fn as_ref(&self) -> &TransactionIndex {
        &self.index
    }
}

impl AsMut<TransactionIndex> for Mempool {
    fn as_mut(&mut self) -> &mut TransactionIndex {
        &mut self.index
    }
}

impl Borrow<TransactionIndex> for Mempool {
    fn borrow(&self) -> &TransactionIndex {
        &self.index
    }
}

impl Index<&B256> for Mempool {
    type Output = Transaction;

    fn index(&self, tx_hash: &B256) -> &Self::Output {
        self.get(tx_hash).expect("transaction not found")
    }
}

impl<'a> IntoIterator for &'a Mempool {
    type Item = (&'a B256, &'a Transaction);
    type IntoIter = std::collections::hash_map::Iter<'a, B256, Transaction>;

    fn into_iter(self) -> Self::IntoIter {
        self.index.into_iter()
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
            signature: pranklin_tx::TxSignature::RawBorsh {
                signature: [0u8; 65],
            },
        }
    }

    #[test]
    fn test_add_and_remove() {
        let mut mempool = Mempool::with_limits(100, 100);
        let sender = Address::with_last_byte(1);
        let tx = create_test_tx(sender, 1);
        let hash = tx.hash();

        assert_eq!(mempool.add(tx).unwrap(), hash);
        assert_eq!(mempool.len(), 1);
        assert!(mempool.contains(&hash));

        assert!(mempool.remove(&hash).is_some());
        assert_eq!(mempool.len(), 0);
    }

    #[test]
    fn test_sender_operations() {
        let mut mempool = Mempool::with_limits(100, 100);
        let sender = Address::with_last_byte(1);

        for nonce in 1..=5 {
            mempool.add(create_test_tx(sender, nonce)).unwrap();
        }

        assert_eq!(mempool.sender_tx_count(&sender), 5);
        assert_eq!(mempool.get_sender_txs(&sender).len(), 5);
        assert_eq!(mempool.highest_nonce(&sender), Some(5));
        assert_eq!(mempool.next_nonce(&sender), Some(6));
    }

    #[test]
    fn test_prune_nonces() {
        let mut mempool = Mempool::with_limits(100, 100);
        let sender = Address::with_last_byte(1);

        for nonce in 1..=5 {
            mempool.add(create_test_tx(sender, nonce)).unwrap();
        }

        let removed = mempool.prune_sender_nonces(&sender, 3);
        assert_eq!(removed.len(), 3);
        assert_eq!(mempool.len(), 2);

        let remaining = mempool.get_sender_txs(&sender);
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
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Duplicate"));
    }

    #[test]
    fn test_capacity_limits() {
        let mut mempool = Mempool::with_limits(2, 100);

        assert!(!mempool.is_full());
        assert_eq!(mempool.available_capacity(), 2);

        mempool
            .add(create_test_tx(Address::with_last_byte(1), 1))
            .unwrap();
        assert_eq!(mempool.available_capacity(), 1);

        mempool
            .add(create_test_tx(Address::with_last_byte(2), 2))
            .unwrap();
        assert!(mempool.is_full());
        assert_eq!(mempool.available_capacity(), 0);

        let result = mempool.add(create_test_tx(Address::with_last_byte(3), 3));
        assert!(result.is_err());
    }

    #[test]
    fn test_ready_txs() {
        let mut mempool = Mempool::with_limits(100, 100);

        for i in 1..=3 {
            for nonce in 1..=3 {
                mempool
                    .add(create_test_tx(Address::with_last_byte(i), nonce))
                    .unwrap();
            }
        }

        let ready = mempool.ready_txs(5);
        assert_eq!(ready.len(), 5);
    }

    #[test]
    fn test_iterators() {
        let mut mempool = Mempool::with_limits(100, 100);
        let sender = Address::with_last_byte(1);

        for nonce in 1..=3 {
            mempool.add(create_test_tx(sender, nonce)).unwrap();
        }

        let count = mempool.iter().count();
        assert_eq!(count, 3);

        let sender_count = mempool.sender_txs_iter(&sender).count();
        assert_eq!(sender_count, 3);

        let senders_count = mempool.senders_iter().count();
        assert_eq!(senders_count, 1);
    }

    #[test]
    fn test_indexing() {
        let mut mempool = Mempool::with_limits(100, 100);
        let tx = create_test_tx(Address::with_last_byte(1), 1);
        let hash = tx.hash();

        mempool.add(tx).unwrap();
        assert_eq!(mempool[&hash].nonce, 1);
    }

    #[test]
    fn test_trait_conversions() {
        let config: MempoolConfig = (1000, 50).into();
        let mempool: Mempool = config.into();
        assert_eq!(mempool.config().max_size, 1000);

        let config_with_ordering: MempoolConfig = (500, 25, false).into();
        assert!(!config_with_ordering.strict_nonce_ordering);
    }

    #[test]
    fn test_next_nonce_helpers() {
        let mut mempool = Mempool::with_limits(100, 100);
        let sender = Address::with_last_byte(1);

        assert_eq!(mempool.next_nonce_or_default(&sender), 0);

        mempool.add(create_test_tx(sender, 0)).unwrap();
        assert_eq!(mempool.next_nonce(&sender), Some(1));
        assert_eq!(mempool.next_nonce_or_default(&sender), 1);
    }

    #[test]
    fn test_batch_add() {
        let mut mempool = Mempool::with_limits(100, 100);
        let sender = Address::with_last_byte(1);

        let txs = (1..=5).map(|n| create_test_tx(sender, n)).collect();
        let results = mempool.add_batch(txs);

        assert_eq!(results.len(), 5);
        assert!(results.iter().all(|r| r.is_ok()));
        assert_eq!(mempool.len(), 5);
    }

    #[test]
    fn test_stats() {
        let mut mempool = Mempool::with_limits(100, 100);

        for i in 1..=10 {
            mempool
                .add(create_test_tx(Address::with_last_byte(i as u8), 1))
                .unwrap();
        }

        let stats = mempool.stats();
        assert_eq!(stats.total_txs, 10);
        assert_eq!(stats.unique_senders, 10);
        assert_eq!(stats.capacity, 100);
        assert_eq!(stats.available, 90);
        assert_eq!(stats.utilization_pct, 10);
    }
}
