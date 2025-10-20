use alloy_primitives::{Address, B256};
use std::collections::{BTreeMap, HashMap};
use pranklin_tx::Transaction;

/// Index for organizing transactions by sender and nonce
#[derive(Debug, Clone, Default)]
pub struct TransactionIndex {
    /// Transactions by hash
    txs: HashMap<B256, Transaction>,
    /// Transactions by sender (sorted by nonce)
    by_sender: HashMap<Address, BTreeMap<u64, B256>>,
}

impl TransactionIndex {
    /// Create a new transaction index
    pub fn new() -> Self {
        Self {
            txs: HashMap::new(),
            by_sender: HashMap::new(),
        }
    }

    /// Insert a transaction into the index
    pub fn insert(&mut self, tx: Transaction) -> B256 {
        let tx_hash = tx.hash();
        let sender = tx.from;
        let nonce = tx.nonce;

        self.txs.insert(tx_hash, tx);
        self.by_sender
            .entry(sender)
            .or_default()
            .insert(nonce, tx_hash);

        tx_hash
    }

    /// Remove a transaction by hash
    pub fn remove(&mut self, tx_hash: &B256) -> Option<Transaction> {
        if let Some(tx) = self.txs.remove(tx_hash) {
            // Remove from sender's nonce map
            if let Some(nonces) = self.by_sender.get_mut(&tx.from) {
                nonces.remove(&tx.nonce);
                if nonces.is_empty() {
                    self.by_sender.remove(&tx.from);
                }
            }
            Some(tx)
        } else {
            None
        }
    }

    /// Get a transaction by hash
    pub fn get(&self, tx_hash: &B256) -> Option<&Transaction> {
        self.txs.get(tx_hash)
    }

    /// Check if a transaction exists
    pub fn contains(&self, tx_hash: &B256) -> bool {
        self.txs.contains_key(tx_hash)
    }

    /// Get all transactions for a sender (sorted by nonce)
    pub fn get_by_sender(&self, sender: &Address) -> Vec<Transaction> {
        if let Some(nonces) = self.by_sender.get(sender) {
            nonces
                .values()
                .filter_map(|hash| self.txs.get(hash).cloned())
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Get transaction count for a sender
    pub fn sender_tx_count(&self, sender: &Address) -> usize {
        self.by_sender
            .get(sender)
            .map(|nonces| nonces.len())
            .unwrap_or(0)
    }

    /// Get the highest nonce for a sender
    pub fn get_highest_nonce(&self, sender: &Address) -> Option<u64> {
        self.by_sender
            .get(sender)
            .and_then(|nonces| nonces.keys().last().copied())
    }

    /// Get the lowest nonce for a sender
    #[allow(dead_code)]
    pub(crate) fn get_lowest_nonce(&self, sender: &Address) -> Option<u64> {
        self.by_sender
            .get(sender)
            .and_then(|nonces| nonces.keys().next().copied())
    }

    /// Check if a specific nonce exists for a sender
    pub fn has_nonce(&self, sender: &Address, nonce: u64) -> bool {
        self.by_sender
            .get(sender)
            .map(|nonces| nonces.contains_key(&nonce))
            .unwrap_or(false)
    }

    /// Remove all transactions for a sender up to (and including) a nonce
    pub fn remove_up_to_nonce(&mut self, sender: &Address, nonce: u64) -> Vec<(B256, Transaction)> {
        let mut removed = Vec::new();

        if let Some(nonces) = self.by_sender.get_mut(sender) {
            let to_remove: Vec<u64> = nonces.keys().filter(|&&n| n <= nonce).copied().collect();

            for n in to_remove {
                if let Some(hash) = nonces.remove(&n)
                    && let Some(tx) = self.txs.remove(&hash)
                {
                    removed.push((hash, tx));
                }
            }

            if nonces.is_empty() {
                self.by_sender.remove(sender);
            }
        }

        removed
    }

    /// Get all senders with pending transactions
    pub fn get_all_senders(&self) -> Vec<Address> {
        self.by_sender.keys().copied().collect()
    }

    /// Get total number of transactions
    pub fn len(&self) -> usize {
        self.txs.len()
    }

    /// Check if the index is empty
    pub fn is_empty(&self) -> bool {
        self.txs.is_empty()
    }

    /// Clear all transactions
    pub fn clear(&mut self) {
        self.txs.clear();
        self.by_sender.clear();
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
    fn test_insert_and_get() {
        let mut index = TransactionIndex::new();
        let sender = Address::with_last_byte(1);
        let tx = create_test_tx(sender, 1);
        let hash = tx.hash();

        index.insert(tx.clone());
        assert_eq!(index.len(), 1);
        assert!(index.contains(&hash));
        assert_eq!(index.get(&hash).unwrap().nonce, 1);
    }

    #[test]
    fn test_remove() {
        let mut index = TransactionIndex::new();
        let tx = create_test_tx(Address::with_last_byte(1), 1);
        let hash = tx.hash();

        index.insert(tx);
        assert_eq!(index.len(), 1);

        let removed = index.remove(&hash);
        assert!(removed.is_some());
        assert_eq!(index.len(), 0);
    }

    #[test]
    fn test_by_sender() {
        let mut index = TransactionIndex::new();
        let sender = Address::with_last_byte(1);

        for nonce in 1..=5 {
            index.insert(create_test_tx(sender, nonce));
        }

        assert_eq!(index.sender_tx_count(&sender), 5);
        assert_eq!(index.get_lowest_nonce(&sender), Some(1));
        assert_eq!(index.get_highest_nonce(&sender), Some(5));

        let txs = index.get_by_sender(&sender);
        assert_eq!(txs.len(), 5);
    }

    #[test]
    fn test_remove_up_to_nonce() {
        let mut index = TransactionIndex::new();
        let sender = Address::with_last_byte(1);

        for nonce in 1..=5 {
            index.insert(create_test_tx(sender, nonce));
        }

        let removed = index.remove_up_to_nonce(&sender, 3);
        assert_eq!(removed.len(), 3);
        assert_eq!(index.len(), 2);
        assert_eq!(index.get_lowest_nonce(&sender), Some(4));
    }
}
