mod agent;
mod error;
mod signer;

pub use agent::*;
pub use error::*;
pub use signer::*;

use alloy_primitives::Address;
use pranklin_tx::Transaction;

/// Authentication service for validating transactions and managing agents
///
/// This service provides:
/// - Transaction signature verification
/// - Agent-based authorization (Hyperliquid-style)
/// - EIP-712 agent nomination support
#[derive(Debug, Clone, Default)]
pub struct AuthService {
    /// Agent registry for managing delegated permissions
    agents: agent::AgentRegistry,
}

impl AuthService {
    /// Create a new authentication service
    pub fn new() -> Self {
        Self {
            agents: agent::AgentRegistry::new(),
        }
    }

    /// Recover the signer address from a transaction signature
    ///
    /// This is a low-level utility that extracts and verifies the signature,
    /// returning the address that signed the transaction.
    pub fn recover_signer(&self, tx: &Transaction) -> Result<Address, AuthError> {
        let signing_hash = tx.signing_hash();
        let sig_bytes = tx.signature();

        // Convert to Alloy Signature format
        let signature = alloy_primitives::Signature::from_bytes_and_parity(
            &sig_bytes[..64],
            sig_bytes[64] != 0,
        );

        // Recover address from signature
        let signing_hash_alloy = alloy_primitives::B256::from_slice(signing_hash.as_slice());
        signature
            .recover_address_from_prehash(&signing_hash_alloy)
            .map_err(|e| AuthError::SignatureRecoveryFailed(e.to_string()))
    }

    /// Verify a transaction's signature
    ///
    /// Ensures that the transaction was signed by the account specified in `tx.from`.
    /// This does NOT check agent permissions - use `verify_transaction_with_agent` for that.
    ///
    /// # Errors
    /// - `InvalidSignature` if the signature doesn't match the `from` address
    /// - `SignatureRecoveryFailed` if signature recovery fails
    pub fn verify_transaction(&self, tx: &Transaction) -> Result<(), AuthError> {
        let recovered_address = self.recover_signer(tx)?;

        if recovered_address != tx.from {
            return Err(AuthError::InvalidSignature);
        }

        Ok(())
    }

    /// Verify transaction with agent support
    ///
    /// This method supports both direct signing and agent-based signing:
    /// 1. If the signer is the account owner (`tx.from`), authorization succeeds
    /// 2. If the signer is an authorized agent with the required permission, authorization succeeds
    /// 3. Otherwise, authorization fails
    ///
    /// # Arguments
    /// - `tx`: The transaction to verify
    /// - `required_permission`: The permission bitmap required for this operation
    ///
    /// # Returns
    /// The account address (`tx.from`) if authorized
    ///
    /// # Errors
    /// - `Unauthorized` if neither the owner nor an authorized agent signed the transaction
    /// - `SignatureRecoveryFailed` if signature recovery fails
    pub fn verify_transaction_with_agent(
        &self,
        tx: &Transaction,
        required_permission: u64,
    ) -> Result<Address, AuthError> {
        let signer = self.recover_signer(tx)?;

        // Check if signer is the account owner
        if signer == tx.from {
            return Ok(tx.from);
        }

        // Check if signer is an authorized agent
        if self
            .agents
            .is_authorized(tx.from, signer, required_permission)
        {
            return Ok(tx.from);
        }

        Err(AuthError::Unauthorized {
            signer,
            account: tx.from,
        })
    }

    /// Nominate an agent using EIP-712 signature
    pub fn nominate_agent(
        &mut self,
        nomination: AgentNomination,
        signature: alloy_primitives::Signature,
        domain: &AgentNominationDomain,
    ) -> Result<(), AuthError> {
        // Verify EIP-712 signature
        let recovered = nomination.verify(&signature, domain)?;

        // Check that the signer is the account owner
        if recovered != nomination.account {
            return Err(AuthError::Unauthorized {
                signer: recovered,
                account: nomination.account,
            });
        }

        // Set the agent
        self.set_agent(nomination.account, nomination.agent, nomination.permissions);

        Ok(())
    }

    /// Set an agent for an account
    pub fn set_agent(
        &mut self,
        account: pranklin_tx::Address,
        agent: pranklin_tx::Address,
        permissions: u64,
    ) {
        self.agents.set_agent(account, agent, permissions);
    }

    /// Remove an agent
    pub fn remove_agent(&mut self, account: pranklin_tx::Address, agent: pranklin_tx::Address) {
        self.agents.remove_agent(account, agent);
    }

    /// Check if an address is an authorized agent
    pub fn is_agent(
        &self,
        account: pranklin_tx::Address,
        agent: pranklin_tx::Address,
        permission: u64,
    ) -> bool {
        self.agents.is_authorized(account, agent, permission)
    }

    /// Get agent permissions
    pub fn get_agent_permissions(
        &self,
        account: pranklin_tx::Address,
        agent: pranklin_tx::Address,
    ) -> Option<u64> {
        self.agents.get_permissions(account, agent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_verification_invalid() {
        // Transaction without valid signature should fail
        let auth = AuthService::new();

        let address = Address::with_last_byte(1);
        let tx = Transaction::new(
            1,
            address,
            pranklin_tx::TxPayload::Deposit(pranklin_tx::DepositTx {
                amount: 1000,
                asset_id: 0,
            }),
        );

        let result = auth.verify_transaction(&tx);
        assert!(result.is_err());

        if let Err(e) = result {
            assert!(e.is_signature_error());
        }
    }

    #[test]
    fn test_agent_authorization_basic() {
        let owner_address = Address::with_last_byte(1);
        let agent_address = Address::with_last_byte(2);

        let mut auth = AuthService::new();

        // Initially no agent
        assert!(!auth.is_agent(
            owner_address,
            agent_address,
            pranklin_tx::permissions::PLACE_ORDER
        ));

        // Set agent with specific permission
        auth.set_agent(
            owner_address,
            agent_address,
            pranklin_tx::permissions::PLACE_ORDER,
        );

        // Check if agent is authorized
        assert!(auth.is_agent(
            owner_address,
            agent_address,
            pranklin_tx::permissions::PLACE_ORDER
        ));

        // Agent should not have other permissions
        assert!(!auth.is_agent(
            owner_address,
            agent_address,
            pranklin_tx::permissions::WITHDRAW
        ));

        // Check permissions
        let perms = auth.get_agent_permissions(owner_address, agent_address);
        assert_eq!(perms, Some(pranklin_tx::permissions::PLACE_ORDER));

        // Remove agent
        auth.remove_agent(owner_address, agent_address);
        assert!(!auth.is_agent(
            owner_address,
            agent_address,
            pranklin_tx::permissions::PLACE_ORDER
        ));
        assert_eq!(
            auth.get_agent_permissions(owner_address, agent_address),
            None
        );
    }

    #[test]
    fn test_agent_multiple_permissions() {
        let owner = Address::with_last_byte(1);
        let agent = Address::with_last_byte(2);
        let mut auth = AuthService::new();

        // Grant multiple permissions
        let perms = pranklin_tx::permissions::PLACE_ORDER
            | pranklin_tx::permissions::CANCEL_ORDER
            | pranklin_tx::permissions::MODIFY_ORDER;

        auth.set_agent(owner, agent, perms);

        // Check each permission individually
        assert!(auth.is_agent(owner, agent, pranklin_tx::permissions::PLACE_ORDER));
        assert!(auth.is_agent(owner, agent, pranklin_tx::permissions::CANCEL_ORDER));
        assert!(auth.is_agent(owner, agent, pranklin_tx::permissions::MODIFY_ORDER));

        // Check combined permissions
        assert!(auth.is_agent(
            owner,
            agent,
            pranklin_tx::permissions::PLACE_ORDER | pranklin_tx::permissions::CANCEL_ORDER
        ));

        // Should not have ungranted permissions
        assert!(!auth.is_agent(owner, agent, pranklin_tx::permissions::WITHDRAW));
    }

    #[test]
    fn test_auth_service_cloneable() {
        let mut auth1 = AuthService::new();
        let owner = Address::with_last_byte(1);
        let agent = Address::with_last_byte(2);

        auth1.set_agent(owner, agent, pranklin_tx::permissions::ALL);

        // Clone the service
        let auth2 = auth1.clone();

        // Both should have the same agent
        assert!(auth2.is_agent(owner, agent, pranklin_tx::permissions::ALL));
    }

    #[test]
    fn test_error_types() {
        let err1 = AuthError::InvalidSignature;
        assert!(err1.is_signature_error());
        assert!(!err1.is_authorization_error());

        let err2 = AuthError::Unauthorized {
            signer: Address::ZERO,
            account: Address::ZERO,
        };
        assert!(!err2.is_signature_error());
        assert!(err2.is_authorization_error());

        let err3 = AuthError::SignatureRecoveryFailed("test".to_string());
        assert!(err3.is_signature_error());
    }
}
