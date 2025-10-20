use crate::client::RpcClient;
use crate::wallet::Wallet;
use alloy_primitives::Address;
use anyhow::Result;
use std::sync::Arc;
use pranklin_tx::{B256, BridgeDepositTx, TxPayload};

/// Setup phase: Initialize accounts with mock balances using bridge operator authority
pub struct AccountSetup {
    operator_wallet: Arc<Wallet>,
    client: Arc<RpcClient>,
}

impl AccountSetup {
    /// Create a new account setup manager
    pub fn new(client: Arc<RpcClient>) -> Self {
        // Create operator wallet (should be authorized as bridge operator on the server)
        let operator_wallet = Arc::new(Wallet::new_random());

        Self {
            operator_wallet,
            client,
        }
    }

    /// Get the operator address (this should be authorized on the server)
    pub fn operator_address(&self) -> Address {
        self.operator_wallet.address()
    }

    /// Initialize a batch of wallets with mock balances
    pub async fn initialize_wallets(
        &self,
        wallets: &[Arc<Wallet>],
        asset_id: u32,
        amount_per_wallet: u128,
    ) -> Result<()> {
        tracing::info!(
            "💰 Initializing {} wallets with {} units of asset {}",
            wallets.len(),
            amount_per_wallet,
            asset_id
        );

        let mut success_count = 0;
        let mut error_count = 0;

        for (idx, wallet) in wallets.iter().enumerate() {
            // Create bridge deposit transaction
            let mut random_bytes = [0u8; 32];
            fastrand::fill(&mut random_bytes);
            let external_tx_hash = B256::from(random_bytes);

            let payload = TxPayload::BridgeDeposit(BridgeDepositTx {
                user: wallet.address(),
                amount: amount_per_wallet,
                asset_id,
                external_tx_hash,
            });

            match self.operator_wallet.create_signed_transaction(payload) {
                Ok(tx) => match self.client.submit_transaction(&tx).await {
                    Ok(_) => {
                        success_count += 1;
                        if (idx + 1) % 10 == 0 {
                            tracing::debug!("  Initialized {}/{} wallets", idx + 1, wallets.len());
                        }
                    }
                    Err(e) => {
                        error_count += 1;
                        tracing::warn!("  Failed to initialize wallet {}: {}", idx, e);
                    }
                },
                Err(e) => {
                    error_count += 1;
                    tracing::warn!("  Failed to create tx for wallet {}: {}", idx, e);
                }
            }

            // Small delay to avoid overwhelming the server during setup
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }

        tracing::info!(
            "  ✓ Wallet initialization complete: {} success, {} failed",
            success_count,
            error_count
        );

        if success_count == 0 {
            anyhow::bail!("Failed to initialize any wallets");
        }

        Ok(())
    }

    /// Verify balances were credited
    pub async fn verify_balances(
        &self,
        wallets: &[Arc<Wallet>],
        asset_id: u32,
        expected_amount: u128,
    ) -> Result<usize> {
        tracing::info!("🔍 Verifying wallet balances...");

        let mut verified = 0;

        for wallet in wallets.iter().take(10) {
            // Sample first 10 wallets
            match self.client.get_balance(wallet.address(), asset_id).await {
                Ok(response) => {
                    if response.balance >= expected_amount {
                        verified += 1;
                    }
                }
                Err(e) => {
                    tracing::debug!("  Failed to get balance: {}", e);
                }
            }
        }

        tracing::info!(
            "  ✓ Verified {}/10 sampled wallets have correct balance",
            verified
        );

        Ok(verified)
    }
}
