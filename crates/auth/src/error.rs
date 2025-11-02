use alloy_primitives::Address;
use thiserror::Error;

/// Authentication and authorization errors
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum AuthError {
    /// Invalid or malformed signature
    #[error("Invalid signature: {0}")]
    InvalidSignature(String),

    /// Unauthorized access attempt
    #[error("Unauthorized: signer {signer} not authorized for account {account}")]
    Unauthorized { signer: Address, account: Address },
}

impl AuthError {
    pub fn signature_error(msg: impl Into<String>) -> Self {
        Self::InvalidSignature(msg.into())
    }

    pub fn unauthorized(signer: Address, account: Address) -> Self {
        Self::Unauthorized { signer, account }
    }
}

impl From<alloy_primitives::SignatureError> for AuthError {
    fn from(err: alloy_primitives::SignatureError) -> Self {
        Self::signature_error(err.to_string())
    }
}
