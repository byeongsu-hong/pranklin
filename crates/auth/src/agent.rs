use alloy_primitives::{Address, Signature, keccak256};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// EIP-712 Agent Nomination
// ============================================================================

/// EIP-712 domain separator for agent nomination messages
///
/// This follows the EIP-712 standard for structured data signing.
/// The domain separator prevents signature replay attacks across different
/// chains or contract deployments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentNominationDomain {
    /// Protocol name (e.g., "PranklinPerp")
    pub name: String,
    /// Protocol version (e.g., "1")
    pub version: String,
    /// Chain ID to prevent cross-chain replay
    pub chain_id: u64,
    /// Contract address to prevent cross-contract replay
    pub verifying_contract: alloy_primitives::Address,
}

impl Default for AgentNominationDomain {
    fn default() -> Self {
        Self {
            name: "PranklinPerp".to_string(),
            version: "1".to_string(),
            chain_id: 1,                                         // Mainnet for EIP-712
            verifying_contract: alloy_primitives::Address::ZERO, // Can be set to actual contract address
        }
    }
}

/// Agent nomination message for EIP-712 signing
///
/// This message is signed by the account owner to authorize an agent
/// to act on their behalf with specific permissions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentNomination {
    /// Account nominating the agent (must be the signer)
    pub account: Address,
    /// Agent address being nominated
    pub agent: Address,
    /// Permissions bitmap (see `pranklin_tx::permissions`)
    pub permissions: u64,
    /// Nonce for replay protection (should increment with each nomination)
    pub nonce: u64,
}

impl AgentNomination {
    /// Create EIP-712 hash for signing
    ///
    /// Generates the EIP-712 typed data hash according to the specification:
    /// `keccak256("\x19\x01" ‖ domainSeparator ‖ hashStruct(message))`
    ///
    /// # Arguments
    /// - `domain`: The EIP-712 domain separator
    ///
    /// # Returns
    /// The 32-byte hash ready for signing
    pub fn eip712_hash(&self, domain: &AgentNominationDomain) -> alloy_primitives::B256 {
        let domain_separator = Self::compute_domain_separator(domain);
        let struct_hash = self.compute_struct_hash();

        // Final EIP-712 hash: keccak256("\x19\x01" ‖ domainSeparator ‖ structHash)
        keccak256(
            [
                &[0x19, 0x01], // EIP-712 magic prefix
                domain_separator.as_slice(),
                struct_hash.as_slice(),
            ]
            .concat(),
        )
    }

    /// Compute the EIP-712 domain separator
    fn compute_domain_separator(domain: &AgentNominationDomain) -> alloy_primitives::B256 {
        const DOMAIN_TYPEHASH: &[u8] =
            b"EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)";

        keccak256(
            [
                keccak256(DOMAIN_TYPEHASH).as_slice(),
                keccak256(domain.name.as_bytes()).as_slice(),
                keccak256(domain.version.as_bytes()).as_slice(),
                &domain.chain_id.to_be_bytes(),
                domain.verifying_contract.as_slice(),
            ]
            .concat(),
        )
    }

    /// Compute the struct hash for this nomination
    fn compute_struct_hash(&self) -> alloy_primitives::B256 {
        const TYPE_HASH: &[u8] =
            b"AgentNomination(address account,address agent,uint64 permissions,uint64 nonce)";

        keccak256(
            [
                keccak256(TYPE_HASH).as_slice(),
                self.account.as_slice(),
                self.agent.as_slice(),
                &self.permissions.to_be_bytes(),
                &self.nonce.to_be_bytes(),
            ]
            .concat(),
        )
    }

    /// Verify the signature on this nomination and recover the signer
    ///
    /// # Arguments
    /// - `signature`: The signature to verify
    /// - `domain`: The EIP-712 domain separator
    ///
    /// # Returns
    /// The recovered signer address
    ///
    /// # Errors
    /// Returns `SignatureRecoveryFailed` if the signature is invalid
    pub fn verify(
        &self,
        signature: &Signature,
        domain: &AgentNominationDomain,
    ) -> Result<Address, crate::AuthError> {
        let hash = self.eip712_hash(domain);
        signature
            .recover_address_from_prehash(&hash)
            .map_err(|e| crate::AuthError::SignatureRecoveryFailed(e.to_string()))
    }
}

// ============================================================================
// Agent Registry
// ============================================================================

/// Agent information with permissions
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentInfo {
    /// Agent address
    pub agent: Address,
    /// Permissions bitmap
    pub permissions: u64,
}

/// Agent registry for managing Hyperliquid-style delegated permissions
///
/// This registry maintains a mapping of accounts to their authorized agents.
/// Each agent can have specific permissions (e.g., place orders, withdraw funds)
/// represented as a bitmap.
///
/// # Example
/// ```ignore
/// use pranklin_auth::AgentRegistry;
/// use alloy_primitives::Address;
///
/// let mut registry = AgentRegistry::new();
/// let account = Address::with_last_byte(1);
/// let agent = Address::with_last_byte(2);
///
/// // Grant trading permissions
/// registry.set_agent(account, agent, 0b0011);
///
/// // Check authorization
/// assert!(registry.is_authorized(account, agent, 0b0001));
/// ```
#[derive(Debug, Clone, Default)]
pub struct AgentRegistry {
    /// Nested map: account -> (agent -> permissions)
    agents: HashMap<Address, HashMap<Address, u64>>,
}

impl AgentRegistry {
    /// Create a new empty agent registry
    pub fn new() -> Self {
        Self {
            agents: HashMap::new(),
        }
    }

    /// Set or update an agent's permissions for an account
    ///
    /// If the agent already exists, their permissions are replaced.
    ///
    /// # Arguments
    /// - `account`: The account granting permissions
    /// - `agent`: The agent receiving permissions
    /// - `permissions`: Permission bitmap
    pub fn set_agent(&mut self, account: Address, agent: Address, permissions: u64) {
        self.agents
            .entry(account)
            .or_default()
            .insert(agent, permissions);
    }

    /// Remove an agent from an account
    ///
    /// After removal, the agent will no longer have any permissions for this account.
    ///
    /// # Arguments
    /// - `account`: The account to remove the agent from
    /// - `agent`: The agent to remove
    pub fn remove_agent(&mut self, account: Address, agent: Address) {
        if let Some(agents) = self.agents.get_mut(&account) {
            agents.remove(&agent);
            // Clean up empty entries
            if agents.is_empty() {
                self.agents.remove(&account);
            }
        }
    }

    /// Check if an agent has specific permission(s) for an account
    ///
    /// # Arguments
    /// - `account`: The account to check
    /// - `agent`: The agent to check
    /// - `permission`: Permission bitmap to check (all bits must be set)
    ///
    /// # Returns
    /// `true` if the agent has all the specified permissions
    pub fn is_authorized(&self, account: Address, agent: Address, permission: u64) -> bool {
        if let Some(agents) = self.agents.get(&account)
            && let Some(&agent_permissions) = agents.get(&agent)
        {
            // Check if all required permission bits are set
            return (agent_permissions & permission) == permission;
        }
        false
    }

    /// Get an agent's full permission bitmap
    ///
    /// # Returns
    /// `Some(permissions)` if the agent exists, `None` otherwise
    pub fn get_permissions(&self, account: Address, agent: Address) -> Option<u64> {
        self.agents.get(&account)?.get(&agent).copied()
    }

    /// Get all agents for an account
    ///
    /// # Returns
    /// A vector of `AgentInfo` containing all agents and their permissions
    pub fn get_agents(&self, account: Address) -> Vec<AgentInfo> {
        self.agents
            .get(&account)
            .map(|agents| {
                agents
                    .iter()
                    .map(|(&agent, &permissions)| AgentInfo { agent, permissions })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Clear all agents for an account
    ///
    /// Removes all delegated permissions for the specified account.
    pub fn clear_agents(&mut self, account: Address) {
        self.agents.remove(&account);
    }

    /// Get the total number of accounts with agents
    pub fn account_count(&self) -> usize {
        self.agents.len()
    }

    /// Get the total number of agents across all accounts
    pub fn total_agent_count(&self) -> usize {
        self.agents.values().map(|agents| agents.len()).sum()
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
        let mut registry = AgentRegistry::new();
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
        let mut registry = AgentRegistry::new();
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
        let mut registry = AgentRegistry::new();
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
        let mut registry = AgentRegistry::new();
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
        let mut registry = AgentRegistry::new();
        let account = Address::with_last_byte(1);
        let agent = Address::with_last_byte(2);

        registry.set_agent(account, agent, permissions::ALL);
        assert_eq!(registry.account_count(), 1);

        // Removing last agent should clean up the account entry
        registry.remove_agent(account, agent);
        assert_eq!(registry.account_count(), 0);
        assert_eq!(registry.total_agent_count(), 0);
    }

    #[test]
    fn test_agent_info_equality() {
        let info1 = AgentInfo {
            agent: Address::with_last_byte(1),
            permissions: 0xFF,
        };
        let info2 = AgentInfo {
            agent: Address::with_last_byte(1),
            permissions: 0xFF,
        };
        let info3 = AgentInfo {
            agent: Address::with_last_byte(2),
            permissions: 0xFF,
        };

        assert_eq!(info1, info2);
        assert_ne!(info1, info3);
    }
}
