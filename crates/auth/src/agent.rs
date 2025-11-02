use alloy_primitives::{Address, Signature, keccak256};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// EIP-712 Constants
// ============================================================================

const EIP712_DOMAIN_TYPEHASH: &[u8] =
    b"EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)";

const AGENT_NOMINATION_TYPEHASH: &[u8] =
    b"AgentNomination(address account,address agent,uint64 permissions,uint64 nonce)";

const EIP712_PREFIX: [u8; 2] = [0x19, 0x01];

// ============================================================================
// EIP-712 Agent Nomination
// ============================================================================

/// EIP-712 domain separator for agent nomination messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentNominationDomain {
    pub name: String,
    pub version: String,
    pub chain_id: u64,
    pub verifying_contract: Address,
}

impl Default for AgentNominationDomain {
    fn default() -> Self {
        Self {
            name: "PranklinPerp".into(),
            version: "1".into(),
            chain_id: 1,
            verifying_contract: Address::ZERO,
        }
    }
}

/// Agent nomination message for EIP-712 signing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentNomination {
    pub account: Address,
    pub agent: Address,
    pub permissions: u64,
    pub nonce: u64,
}

impl AgentNomination {
    /// Create EIP-712 hash for signing
    pub fn eip712_hash(&self, domain: &AgentNominationDomain) -> alloy_primitives::B256 {
        keccak256(
            [
                EIP712_PREFIX.as_slice(),
                Self::domain_separator(domain).as_slice(),
                self.struct_hash().as_slice(),
            ]
            .concat(),
        )
    }

    fn domain_separator(domain: &AgentNominationDomain) -> alloy_primitives::B256 {
        keccak256(
            [
                keccak256(EIP712_DOMAIN_TYPEHASH).as_slice(),
                keccak256(domain.name.as_bytes()).as_slice(),
                keccak256(domain.version.as_bytes()).as_slice(),
                &domain.chain_id.to_be_bytes(),
                domain.verifying_contract.as_slice(),
            ]
            .concat(),
        )
    }

    fn struct_hash(&self) -> alloy_primitives::B256 {
        keccak256(
            [
                keccak256(AGENT_NOMINATION_TYPEHASH).as_slice(),
                self.account.as_slice(),
                self.agent.as_slice(),
                &self.permissions.to_be_bytes(),
                &self.nonce.to_be_bytes(),
            ]
            .concat(),
        )
    }

    /// Verify the signature on this nomination and recover the signer
    pub fn verify(
        &self,
        signature: &Signature,
        domain: &AgentNominationDomain,
    ) -> Result<Address, crate::AuthError> {
        signature
            .recover_address_from_prehash(&self.eip712_hash(domain))
            .map_err(Into::into)
    }
}

// ============================================================================
// Agent Registry
// ============================================================================

/// Agent registry for managing delegated permissions
#[derive(Debug, Clone, Default)]
pub struct AgentRegistry {
    agents: HashMap<Address, HashMap<Address, u64>>,
}

impl AgentRegistry {
    pub fn set_agent(&mut self, account: Address, agent: Address, permissions: u64) {
        self.agents
            .entry(account)
            .or_default()
            .insert(agent, permissions);
    }

    pub fn remove_agent(&mut self, account: Address, agent: Address) {
        if let Some(agents) = self.agents.get_mut(&account) {
            agents.remove(&agent);
            if agents.is_empty() {
                self.agents.remove(&account);
            }
        }
    }

    pub fn is_authorized(&self, account: Address, agent: Address, permission: u64) -> bool {
        self.agents
            .get(&account)
            .and_then(|agents| agents.get(&agent))
            .is_some_and(|&perms| (perms & permission) == permission)
    }

    pub fn get_permissions(&self, account: Address, agent: Address) -> Option<u64> {
        self.agents.get(&account)?.get(&agent).copied()
    }

    pub fn get_agents(&self, account: Address) -> Vec<(Address, u64)> {
        self.agents
            .get(&account)
            .map(|agents| agents.iter().map(|(&a, &p)| (a, p)).collect())
            .unwrap_or_default()
    }

    pub fn clear_agents(&mut self, account: Address) {
        self.agents.remove(&account);
    }

    pub fn account_count(&self) -> usize {
        self.agents.len()
    }

    pub fn total_agent_count(&self) -> usize {
        self.agents.values().map(HashMap::len).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pranklin_tx::permissions;

    #[test]
    fn test_agent_nomination_eip712_deterministic() {
        let account = Address::with_last_byte(1);
        let agent = Address::with_last_byte(2);

        let nomination = AgentNomination {
            account,
            agent,
            permissions: pranklin_tx::permissions::ALL,
            nonce: 1,
        };

        let domain = AgentNominationDomain::default();
        let hash1 = nomination.eip712_hash(&domain);
        let hash2 = nomination.eip712_hash(&domain);

        // Hash must be deterministic
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_agent_nomination_eip712_different_domains() {
        let nomination = AgentNomination {
            account: Address::with_last_byte(1),
            agent: Address::with_last_byte(2),
            permissions: pranklin_tx::permissions::ALL,
            nonce: 1,
        };

        let domain1 = AgentNominationDomain::default();
        let domain2 = AgentNominationDomain {
            chain_id: 2, // Different chain
            ..Default::default()
        };

        let hash1 = nomination.eip712_hash(&domain1);
        let hash2 = nomination.eip712_hash(&domain2);

        // Different domains should produce different hashes
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_agent_registry_basic() {
        let mut registry = AgentRegistry::default();
        let account = Address::with_last_byte(1);
        let agent = Address::with_last_byte(2);

        // Initially no permissions
        assert!(!registry.is_authorized(account, agent, permissions::PLACE_ORDER));
        assert_eq!(registry.get_permissions(account, agent), None);

        // Set agent
        registry.set_agent(
            account,
            agent,
            permissions::PLACE_ORDER | permissions::CANCEL_ORDER,
        );

        // Check authorization
        assert!(registry.is_authorized(account, agent, permissions::PLACE_ORDER));
        assert!(registry.is_authorized(account, agent, permissions::CANCEL_ORDER));
        assert!(registry.is_authorized(
            account,
            agent,
            permissions::PLACE_ORDER | permissions::CANCEL_ORDER
        ));
        assert!(!registry.is_authorized(account, agent, permissions::WITHDRAW));

        // Get permissions
        let perms = registry.get_permissions(account, agent).unwrap();
        assert_eq!(perms, permissions::PLACE_ORDER | permissions::CANCEL_ORDER);

        // Remove agent
        registry.remove_agent(account, agent);
        assert!(!registry.is_authorized(account, agent, permissions::PLACE_ORDER));
        assert_eq!(registry.get_permissions(account, agent), None);
    }

    #[test]
    fn test_agent_registry_multiple_agents() {
        let mut registry = AgentRegistry::default();
        let account = Address::with_last_byte(1);
        let agent1 = Address::with_last_byte(2);
        let agent2 = Address::with_last_byte(3);

        registry.set_agent(account, agent1, permissions::PLACE_ORDER);
        registry.set_agent(account, agent2, permissions::CANCEL_ORDER);

        // Both agents should be present
        let agents = registry.get_agents(account);
        assert_eq!(agents.len(), 2);

        // Each agent has different permissions
        assert!(registry.is_authorized(account, agent1, permissions::PLACE_ORDER));
        assert!(!registry.is_authorized(account, agent1, permissions::CANCEL_ORDER));
        assert!(!registry.is_authorized(account, agent2, permissions::PLACE_ORDER));
        assert!(registry.is_authorized(account, agent2, permissions::CANCEL_ORDER));

        // Clear all agents
        registry.clear_agents(account);
        assert_eq!(registry.get_agents(account).len(), 0);
        assert_eq!(registry.account_count(), 0);
    }

    #[test]
    fn test_agent_registry_update_permissions() {
        let mut registry = AgentRegistry::default();
        let account = Address::with_last_byte(1);
        let agent = Address::with_last_byte(2);

        // Initial permissions
        registry.set_agent(account, agent, permissions::PLACE_ORDER);
        assert_eq!(
            registry.get_permissions(account, agent),
            Some(permissions::PLACE_ORDER)
        );

        // Update permissions
        registry.set_agent(account, agent, permissions::WITHDRAW);
        assert_eq!(
            registry.get_permissions(account, agent),
            Some(permissions::WITHDRAW)
        );

        // Old permissions should be gone
        assert!(!registry.is_authorized(account, agent, permissions::PLACE_ORDER));
    }

    #[test]
    fn test_agent_registry_multiple_accounts() {
        let mut registry = AgentRegistry::default();
        let account1 = Address::with_last_byte(1);
        let account2 = Address::with_last_byte(2);
        let agent = Address::with_last_byte(3);

        registry.set_agent(account1, agent, permissions::PLACE_ORDER);
        registry.set_agent(account2, agent, permissions::WITHDRAW);

        // Same agent, different permissions per account
        assert!(registry.is_authorized(account1, agent, permissions::PLACE_ORDER));
        assert!(!registry.is_authorized(account1, agent, permissions::WITHDRAW));
        assert!(!registry.is_authorized(account2, agent, permissions::PLACE_ORDER));
        assert!(registry.is_authorized(account2, agent, permissions::WITHDRAW));

        assert_eq!(registry.account_count(), 2);
        assert_eq!(registry.total_agent_count(), 2);
    }

    #[test]
    fn test_agent_registry_remove_cleanup() {
        let mut registry = AgentRegistry::default();
        let account = Address::with_last_byte(1);
        let agent = Address::with_last_byte(2);

        registry.set_agent(account, agent, permissions::ALL);
        assert_eq!(registry.account_count(), 1);

        // Removing last agent should clean up the account entry
        registry.remove_agent(account, agent);
        assert_eq!(registry.account_count(), 0);
        assert_eq!(registry.total_agent_count(), 0);
    }
}
