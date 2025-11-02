use alloy_primitives::{Address, keccak256};
use k256::ecdsa::SigningKey;
use pranklin_tx::Transaction;
use std::sync::atomic::{AtomicU64, Ordering};

/// A wallet for signing transactions with automatic nonce management
pub struct Wallet {
    signing_key: SigningKey,
    address: Address,
    nonce: AtomicU64,
}

impl Wallet {
    pub fn new_random() -> Self {
        let mut secret_key_bytes = [0u8; 32];
        fastrand::fill(&mut secret_key_bytes);
        let signing_key = SigningKey::from_bytes(&secret_key_bytes.into())
            .expect("Failed to create signing key");

        let public_key_bytes = signing_key.verifying_key().to_encoded_point(false);
        let hash = keccak256(&public_key_bytes.as_bytes()[1..]);
        let address = Address::from_slice(&hash[12..]);

        Self {
            signing_key,
            address,
            nonce: AtomicU64::new(0),
        }
    }

    pub fn address(&self) -> Address {
        self.address
    }

    fn next_nonce(&self) -> u64 {
        self.nonce.fetch_add(1, Ordering::SeqCst)
    }

    fn sign_transaction(&self, tx: &mut Transaction) -> anyhow::Result<()> {
        let (signature, recovery_id) = self
            .signing_key
            .sign_prehash_recoverable(tx.signing_hash().as_slice())
            .map_err(|e| anyhow::anyhow!("Failed to sign: {}", e))?;

        let mut sig_65 = [0u8; 65];
        sig_65[0..64].copy_from_slice(&signature.to_bytes());
        sig_65[64] = recovery_id.to_byte();
        tx.set_signature(sig_65);
        Ok(())
    }

    pub fn create_signed_transaction(
        &self,
        payload: pranklin_tx::TxPayload,
    ) -> anyhow::Result<Transaction> {
        let mut tx = Transaction::new_raw(self.next_nonce(), self.address, payload);
        self.sign_transaction(&mut tx)?;
        Ok(tx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pranklin_tx::{DepositTx, TxPayload};

    #[test]
    fn test_wallet_creation() {
        let wallet = Wallet::new_random();
        assert_ne!(wallet.address(), Address::ZERO);
    }

    #[test]
    fn test_transaction_signing() {
        let wallet = Wallet::new_random();
        let payload = TxPayload::Deposit(DepositTx {
            amount: 1000,
            asset_id: 0,
        });

        let tx = wallet.create_signed_transaction(payload).unwrap();
        assert_ne!(tx.signature_bytes(), &[0u8; 65]);
    }
}
