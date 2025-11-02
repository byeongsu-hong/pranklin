use alloy_primitives::Address;
use alloy_signer_local::PrivateKeySigner;
use std::ops::{Deref, DerefMut};

/// Wrapper for transaction signing operations
///
/// This is a convenience wrapper around `alloy_signer_local::PrivateKeySigner`
/// that provides direct access to all signer methods via `Deref`.
///
/// # Example
/// ```ignore
/// use pranklin_auth::TxSigner;
/// use alloy_primitives::B256;
/// use alloy_signer::SignerSync;
///
/// let signer = TxSigner::random();
/// let message = B256::ZERO;
/// let signature = signer.sign_hash_sync(&message).unwrap();
/// println!("Signed by: {:?}", signer.address());
/// ```
#[derive(Clone)]
pub struct TxSigner(PrivateKeySigner);

impl TxSigner {
    /// Create a random signer for testing
    ///
    /// # Security Warning
    /// Only use this for testing! Never use random keys for production.
    pub fn random() -> Self {
        Self(PrivateKeySigner::random())
    }

    /// Get the Ethereum address for this signer
    pub fn address(&self) -> Address {
        self.0.address()
    }
}

impl From<PrivateKeySigner> for TxSigner {
    fn from(signer: PrivateKeySigner) -> Self {
        Self(signer)
    }
}

impl From<TxSigner> for PrivateKeySigner {
    fn from(signer: TxSigner) -> Self {
        signer.0
    }
}

impl Deref for TxSigner {
    type Target = PrivateKeySigner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for TxSigner {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl std::fmt::Debug for TxSigner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TxSigner")
            .field("address", &self.address())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::B256;
    use alloy_signer::SignerSync;

    #[test]
    fn test_signer_sync() {
        let signer = TxSigner::random();
        let address = signer.address();

        let message = B256::ZERO;
        let signature = signer.sign_hash_sync(&message).unwrap();

        // Verify signature
        let recovered = signature.recover_address_from_prehash(&message).unwrap();
        assert_eq!(recovered, address);
    }

    #[test]
    fn test_from_trait() {
        let private_signer = PrivateKeySigner::random();
        let address = private_signer.address();

        let tx_signer: TxSigner = private_signer.into();
        assert_eq!(tx_signer.address(), address);
    }
}
