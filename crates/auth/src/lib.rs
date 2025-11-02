mod agent;
mod error;
mod signer;

pub use agent::{AgentNomination, AgentNominationDomain, AgentRegistry};
pub use error::AuthError;
pub use signer::TxSigner;

use alloy_primitives::Address;
use pranklin_tx::Transaction;

/// Authentication service for transaction verification and agent management
#[derive(Debug, Clone, Default)]
pub struct AuthService {
    agents: AgentRegistry,
}

impl AuthService {
    /// Create a new auth service
    pub fn new() -> Self {
        Self::default()
    }

    /// Recover the signer address from a transaction signature
    pub fn recover_signer(&self, tx: &Transaction) -> Result<Address, AuthError> {
        let sig_bytes = tx.signature_bytes();
        alloy_primitives::Signature::from_bytes_and_parity(&sig_bytes[..64], sig_bytes[64] != 0)
            .recover_address_from_prehash(&tx.signing_hash())
            .map_err(Into::into)
    }

    /// Verify a transaction's signature matches tx.from
    pub fn verify_transaction(&self, tx: &Transaction) -> Result<(), AuthError> {
        let signer = self.recover_signer(tx)?;
        (signer == tx.from)
            .then_some(())
            .ok_or_else(|| AuthError::unauthorized(signer, tx.from))
    }

    /// Verify transaction with agent support - returns account address if authorized
    pub fn verify_with_agent(&self, tx: &Transaction, permission: u64) -> Result<Address, AuthError> {
        let signer = self.recover_signer(tx)?;
        (signer == tx.from || self.agents.is_authorized(tx.from, signer, permission))
            .then_some(tx.from)
            .ok_or_else(|| AuthError::unauthorized(signer, tx.from))
    }

    /// Nominate an agent using EIP-712 signature
    pub fn nominate_agent(
        &mut self,
        nomination: AgentNomination,
        signature: alloy_primitives::Signature,
        domain: &AgentNominationDomain,
    ) -> Result<(), AuthError> {
        let recovered = nomination.verify(&signature, domain)?;
        (recovered == nomination.account)
            .then_some(())
            .ok_or_else(|| AuthError::unauthorized(recovered, nomination.account))?;

        self.set_agent(nomination.account, nomination.agent, nomination.permissions);
        Ok(())
    }

    pub fn set_agent(&mut self, account: Address, agent: Address, permissions: u64) {
        self.agents.set_agent(account, agent, permissions);
    }

    pub fn remove_agent(&mut self, account: Address, agent: Address) {
        self.agents.remove_agent(account, agent);
    }

    pub fn is_authorized(&self, account: Address, agent: Address, permission: u64) -> bool {
        self.agents.is_authorized(account, agent, permission)
    }

    pub fn agent_permissions(&self, account: Address, agent: Address) -> Option<u64> {
        self.agents.get_permissions(account, agent)
    }

    pub fn agents(&self, account: Address) -> Vec<(Address, u64)> {
        self.agents.get_agents(account)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_verification_invalid() {
        let auth = AuthService::default();
        let tx = Transaction::new_raw(
            1,
            Address::with_last_byte(1),
            pranklin_tx::TxPayload::Deposit(pranklin_tx::DepositTx {
                amount: 1000,
                asset_id: 0,
            }),
        );

        assert!(auth.verify_transaction(&tx).is_err());
    }

    #[test]
    fn test_agent_authorization_basic() {
        let (owner, agent) = (Address::with_last_byte(1), Address::with_last_byte(2));
        let mut auth = AuthService::default();

        assert!(!auth.is_authorized(owner, agent, pranklin_tx::permissions::PLACE_ORDER));

        auth.set_agent(owner, agent, pranklin_tx::permissions::PLACE_ORDER);

        assert!(auth.is_authorized(owner, agent, pranklin_tx::permissions::PLACE_ORDER));
        assert!(!auth.is_authorized(owner, agent, pranklin_tx::permissions::WITHDRAW));
        assert_eq!(auth.agent_permissions(owner, agent), Some(pranklin_tx::permissions::PLACE_ORDER));

        auth.remove_agent(owner, agent);
        assert!(!auth.is_authorized(owner, agent, pranklin_tx::permissions::PLACE_ORDER));
        assert_eq!(auth.agent_permissions(owner, agent), None);
    }

    #[test]
    fn test_agent_multiple_permissions() {
        let (owner, agent) = (Address::with_last_byte(1), Address::with_last_byte(2));
        let mut auth = AuthService::default();

        let perms = pranklin_tx::permissions::PLACE_ORDER
            | pranklin_tx::permissions::CANCEL_ORDER
            | pranklin_tx::permissions::MODIFY_ORDER;

        auth.set_agent(owner, agent, perms);

        assert!(auth.is_authorized(owner, agent, pranklin_tx::permissions::PLACE_ORDER));
        assert!(auth.is_authorized(owner, agent, pranklin_tx::permissions::CANCEL_ORDER));
        assert!(auth.is_authorized(owner, agent, pranklin_tx::permissions::MODIFY_ORDER));
        assert!(auth.is_authorized(owner, agent, pranklin_tx::permissions::PLACE_ORDER | pranklin_tx::permissions::CANCEL_ORDER));
        assert!(!auth.is_authorized(owner, agent, pranklin_tx::permissions::WITHDRAW));
    }

    #[test]
    fn test_auth_service_cloneable() {
        let (owner, agent) = (Address::with_last_byte(1), Address::with_last_byte(2));
        let mut auth1 = AuthService::default();
        auth1.set_agent(owner, agent, pranklin_tx::permissions::ALL);

        let auth2 = auth1.clone();
        assert!(auth2.is_authorized(owner, agent, pranklin_tx::permissions::ALL));
    }
}
