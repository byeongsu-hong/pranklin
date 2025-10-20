use alloy_primitives::Address;
use thiserror::Error;

/// Authentication and authorization errors
///
/// These errors cover signature verification, agent authorization,
/// and permission checks.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum AuthError {
    /// The signature does not match the claimed signer address
    #[error("Invalid signature: recovered address does not match")]
    InvalidSignature,

    /// Failed to recover the signer address from the signature
    ///
    /// This typically indicates a malformed signature or invalid parameters.
    #[error("Signature recovery failed: {0}")]
    SignatureRecoveryFailed(String),

    /// The signer is not authorized to act on behalf of the account
    ///
    /// This occurs when:
    /// - The signer is neither the account owner nor an authorized agent
    /// - The agent lacks the required permissions
    #[error("Unauthorized: signer {signer} is not authorized for account {account}")]
    Unauthorized {
        /// The address that signed the transaction
        signer: Address,
        /// The account the signer attempted to act on behalf of
        account: Address,
    },

    /// The specified agent does not exist
    #[error("Agent {agent} not found for account {account}")]
    AgentNotFound {
        /// The account that was queried
        account: Address,
        /// The agent that was not found
        agent: Address,
    },

    /// The agent lacks the required permissions for this operation
    #[error("Insufficient permissions: agent {agent} has {current:#x}, requires {required:#x}")]
    InsufficientPermissions {
        /// The agent address
        agent: Address,
        /// Current permissions bitmap
        current: u64,
        /// Required permissions bitmap
        required: u64,
    },

    /// Generic error for other authentication issues
    #[error("Authentication error: {0}")]
    Other(String),
}

impl AuthError {
    /// Check if this error is related to signature validation
    pub fn is_signature_error(&self) -> bool {
        matches!(
            self,
            AuthError::InvalidSignature | AuthError::SignatureRecoveryFailed(_)
        )
    }

    /// Check if this error is related to authorization
    pub fn is_authorization_error(&self) -> bool {
        matches!(
            self,
            AuthError::Unauthorized { .. }
                | AuthError::AgentNotFound { .. }
                | AuthError::InsufficientPermissions { .. }
        )
    }
}
