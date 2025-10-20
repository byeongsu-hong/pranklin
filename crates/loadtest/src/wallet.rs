use alloy_primitives::{Address, keccak256};
use k256::ecdsa::{Signature, SigningKey, signature::Signer};
use pranklin_tx::Transaction;
use std::sync::atomic::{AtomicU64, Ordering};

/// A wallet for signing transactions with automatic nonce management
pub struct Wallet {
    signing_key: SigningKey,
    address: Address,
    nonce: AtomicU64,
}

impl Wallet {
    /// Create a new random wallet
    pub fn new_random() -> Self {
        let mut secret_key_bytes = [0u8; 32];
        fastrand::fill(&mut secret_key_bytes);
        let signing_key =
            SigningKey::from_bytes(&secret_key_bytes.into()).expect("Failed to create signing key");

        let verifying_key = signing_key.verifying_key();
        let public_key_bytes = verifying_key.to_encoded_point(false);
        let public_key_bytes = public_key_bytes.as_bytes();

        // Derive Ethereum-style address from public key
        let hash = keccak256(&public_key_bytes[1..]);
        let address = Address::from_slice(&hash[12..]);

        Self {
            signing_key,
            address,
            nonce: AtomicU64::new(0),
        }
    }

    /// Get the wallet address
    pub fn address(&self) -> Address {
        self.address
    }

    /// Get the next nonce
    pub fn next_nonce(&self) -> u64 {
        self.nonce.fetch_add(1, Ordering::SeqCst)
    }

    /// Sign a transaction
    pub fn sign_transaction(&self, tx: &mut Transaction) -> anyhow::Result<()> {
        let signing_hash = tx.signing_hash();

        // Sign the hash
        let signature: Signature = self.signing_key.sign(signing_hash.as_slice());
        let sig_bytes = signature.to_bytes();

        // Compute recovery ID
        let recovery_id = self.compute_recovery_id(signing_hash.as_slice(), &sig_bytes)?;

        // Create 65-byte signature (r + s + v)
        let mut sig_65 = [0u8; 65];
        sig_65[0..64].copy_from_slice(&sig_bytes[..]);
        sig_65[64] = recovery_id;

        tx.set_signature(sig_65);
        Ok(())
    }

    /// Compute the recovery ID for ECDSA signature
    fn compute_recovery_id(&self, message: &[u8], sig_bytes: &[u8]) -> anyhow::Result<u8> {
        use k256::ecdsa::{RecoveryId, VerifyingKey};

        let signature = Signature::from_slice(sig_bytes)
            .map_err(|e| anyhow::anyhow!("Invalid signature: {}", e))?;

        // Try recovery IDs 0 and 1
        for recovery_id in 0..2 {
            let rid = RecoveryId::try_from(recovery_id)
                .map_err(|e| anyhow::anyhow!("Invalid recovery ID: {}", e))?;

            if let Ok(recovered_key) = VerifyingKey::recover_from_prehash(message, &signature, rid)
                && recovered_key == *self.signing_key.verifying_key()
            {
                return Ok(recovery_id);
            }
        }

        Err(anyhow::anyhow!("Failed to compute recovery ID"))
    }

    /// Create and sign a transaction with the given payload
    pub fn create_signed_transaction(
        &self,
        payload: pranklin_tx::TxPayload,
    ) -> anyhow::Result<Transaction> {
        let nonce = self.next_nonce();
        let mut tx = Transaction::new(nonce, self.address, payload);
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
        assert_ne!(tx.signature(), &[0u8; 65]);
    }
}
