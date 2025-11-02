use alloy_primitives::{Address, B256};
use pranklin_tx::Transaction;
use std::{
    collections::{BTreeMap, HashMap},
    ops::Index,
};

/// Index for organizing transactions by sender and nonce
#[derive(Debug, Clone, Default)]
pub struct TransactionIndex {
    txs: HashMap<B256, Transaction>,
    by_sender: HashMap<Address, BTreeMap<u64, B256>>,
}

impl TransactionIndex {
    /// Create a new transaction index
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a transaction into the index
    pub fn insert(&mut self, tx: Transaction) -> B256 {
        let (tx_hash, sender, nonce) = (tx.hash(), tx.from, tx.nonce);
        self.txs.insert(tx_hash, tx);
        self.by_sender
            .entry(sender)
            .or_default()
            .insert(nonce, tx_hash);
        tx_hash
    }

    /// Remove a transaction by hash
    pub fn remove(&mut self, tx_hash: &B256) -> Option<Transaction> {
        let tx = self.txs.remove(tx_hash)?;

        if let Some(nonces) = self.by_sender.get_mut(&tx.from) {
            nonces.remove(&tx.nonce);
            if nonces.is_empty() {
                self.by_sender.remove(&tx.from);
            }
        }

        Some(tx)
    }

    /// Get a transaction by hash
    #[must_use]
    pub fn get(&self, tx_hash: &B256) -> Option<&Transaction> {
        self.txs.get(tx_hash)
    }

    /// Get number of unique senders
    #[must_use]
    pub fn sender_count(&self) -> usize {
        self.by_sender.len()
    }

    /// Check if a transaction exists
    #[must_use]
    pub fn contains(&self, tx_hash: &B256) -> bool {
        self.txs.contains_key(tx_hash)
    }

    /// Get all transactions for a sender (sorted by nonce)
    #[must_use]
    pub fn get_by_sender(&self, sender: &Address) -> Vec<Transaction> {
        self.by_sender
            .get(sender)
            .map(|nonces| {
                nonces
                    .values()
                    .filter_map(|hash| self.txs.get(hash).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get transaction count for a sender
    #[must_use]
    pub fn sender_tx_count(&self, sender: &Address) -> usize {
        self.by_sender.get(sender).map_or(0, BTreeMap::len)
    }

    /// Get the highest nonce for a sender
    #[must_use]
    pub fn get_highest_nonce(&self, sender: &Address) -> Option<u64> {
        self.by_sender.get(sender)?.keys().last().copied()
    }

    /// Get the lowest nonce for a sender
    #[allow(dead_code)]
    #[must_use]
    pub(crate) fn get_lowest_nonce(&self, sender: &Address) -> Option<u64> {
        self.by_sender.get(sender)?.keys().next().copied()
    }

    /// Check if a specific nonce exists for a sender
    #[must_use]
    pub fn has_nonce(&self, sender: &Address, nonce: u64) -> bool {
        self.by_sender
            .get(sender)
            .is_some_and(|nonces| nonces.contains_key(&nonce))
    }

    /// Remove all transactions for a sender up to (and including) a nonce
    pub fn remove_up_to_nonce(&mut self, sender: &Address, nonce: u64) -> Vec<(B256, Transaction)> {
        let Some(nonces) = self.by_sender.get_mut(sender) else {
            return Vec::new();
        };

        let to_remove: Vec<u64> = nonces.keys().filter(|&&n| n <= nonce).copied().collect();
        let removed = to_remove
            .into_iter()
            .filter_map(|n| {
                let hash = nonces.remove(&n)?;
                let tx = self.txs.remove(&hash)?;
                Some((hash, tx))
            })
            .collect();

        if nonces.is_empty() {
            self.by_sender.remove(sender);
        }

        removed
    }

    /// Get all senders with pending transactions
    #[must_use]
    pub fn get_all_senders(&self) -> Vec<Address> {
        self.by_sender.keys().copied().collect()
    }

    /// Iterate over all senders
    pub fn senders_iter(&self) -> impl Iterator<Item = &Address> {
        self.by_sender.keys()
    }

    /// Iterate over transactions for a sender
    pub fn sender_txs_iter(&self, sender: &Address) -> impl Iterator<Item = &Transaction> + '_ {
        self.by_sender
            .get(sender)
            .into_iter()
            .flat_map(|nonces| nonces.values())
            .filter_map(|hash| self.txs.get(hash))
    }

    /// Get total number of transactions
    #[must_use]
    pub fn len(&self) -> usize {
        self.txs.len()
    }

    /// Check if the index is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.txs.is_empty()
    }

    /// Clear all transactions
    pub fn clear(&mut self) {
        self.txs.clear();
        self.by_sender.clear();
    }
}

impl Index<&B256> for TransactionIndex {
    type Output = Transaction;

    fn index(&self, tx_hash: &B256) -> &Self::Output {
        self.get(tx_hash).expect("transaction not found")
    }
}

impl<'a> IntoIterator for &'a TransactionIndex {
    type Item = (&'a B256, &'a Transaction);
    type IntoIter = std::collections::hash_map::Iter<'a, B256, Transaction>;

    fn into_iter(self) -> Self::IntoIter {
        self.txs.iter()
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
