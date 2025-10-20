use alloy_primitives::{Address, B256, Signature};
use alloy_signer::{Signer, SignerSync};
use alloy_signer_local::PrivateKeySigner;

/// Wrapper for transaction signing operations
///
/// This is a convenience wrapper around `alloy_signer_local::PrivateKeySigner`
/// that provides a simpler interface for common signing operations.
///
/// # Example
/// ```ignore
/// use pranklin_auth::TxSigner;
/// use alloy_primitives::B256;
///
/// #[tokio::main]
/// async fn main() {
///     let signer = TxSigner::random();
///     let message = B256::ZERO;
///     let signature = signer.sign_hash(&message).await.unwrap();
///     println!("Signed by: {:?}", signer.address());
/// }
/// ```
#[derive(Clone)]
pub struct TxSigner {
    signer: PrivateKeySigner,
}

impl TxSigner {
    /// Create a new signer from a private key
    ///
    /// # Arguments
    /// - `signer`: The underlying private key signer
    pub fn new(signer: PrivateKeySigner) -> Self {
        Self { signer }
    }

    /// Create a random signer for testing
    ///
    /// # Security Warning
    /// Only use this for testing! Never use random keys for production.
    pub fn random() -> Self {
        Self {
            signer: PrivateKeySigner::random(),
        }
    }

    /// Get the Ethereum address for this signer
    pub fn address(&self) -> Address {
        self.signer.address()
    }

    /// Sign a message hash asynchronously
    ///
    /// # Arguments
    /// - `hash`: The 32-byte hash to sign
    ///
    /// # Returns
    /// The ECDSA signature
    pub async fn sign_hash(&self, hash: &B256) -> Result<Signature, alloy_signer::Error> {
        self.signer.sign_hash(hash).await
    }

    /// Sign a message hash synchronously
    ///
    /// This is useful when you don't need async signing, such as in tests
    /// or synchronous contexts.
    ///
    /// # Arguments
    /// - `hash`: The 32-byte hash to sign
    ///
    /// # Returns
    /// The ECDSA signature
    pub fn sign_hash_sync(&self, hash: &B256) -> Result<Signature, alloy_signer::Error> {
        self.signer.sign_hash_sync(hash)
    }

    /// Get a reference to the inner signer
    ///
    /// This provides access to the full `PrivateKeySigner` API when needed.
    pub fn inner(&self) -> &PrivateKeySigner {
        &self.signer
    }

    /// Consume self and return the inner signer
    pub fn into_inner(self) -> PrivateKeySigner {
        self.signer
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

    #[tokio::test]
    async fn test_signer() {
        let signer = TxSigner::random();
        let address = signer.address();

        let message = B256::ZERO;
        let signature = signer.sign_hash(&message).await.unwrap();

        // Verify signature
        let recovered = signature.recover_address_from_prehash(&message).unwrap();
        assert_eq!(recovered, address);
    }
}
