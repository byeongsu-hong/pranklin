use crate::error::{Result, TxExecutionError};
use pranklin_auth::AuthService;
use pranklin_engine::Engine;
use pranklin_mempool::Mempool;
use pranklin_tx::{Transaction, TxPayload};

/// Transaction execution result statistics
#[derive(Debug, Default, Clone, Copy)]
pub struct TxExecutionStats {
    pub successful: usize,
    pub failed: usize,
}

impl TxExecutionStats {
    pub const fn new() -> Self {
        Self {
            successful: 0,
            failed: 0,
        }
    }

    pub fn record_success(&mut self) {
        self.successful += 1;
    }

    pub fn record_failure(&mut self) {
        self.failed += 1;
    }
}

/// Transaction executor with clean architecture
///
/// Refactored to:
/// - Use proper error types instead of strings
/// - Delegate to domain services
/// - Clear separation of concerns
pub struct TransactionExecutor<'a> {
    engine: &'a mut Engine,
    auth: &'a mut AuthService,
    mempool: &'a mut Mempool,
    block_height: u64,
}

impl<'a> TransactionExecutor<'a> {
    pub fn new(
        engine: &'a mut Engine,
        auth: &'a mut AuthService,
        mempool: &'a mut Mempool,
        block_height: u64,
    ) -> Self {
        Self {
            engine,
            auth,
            mempool,
            block_height,
        }
    }

    /// Execute a single transaction
    pub fn execute(&mut self, tx: &Transaction) -> Result<()> {
        let tx_hash = tx.hash();
        tracing::debug!("Processing tx {:?}", tx_hash);

        self.auth.verify_transaction(tx)?;
        self.verify_nonce(tx)?;
        self.execute_payload(tx)?;
        self.engine.state_mut().increment_nonce(tx.from)?;
        self.mempool.remove(&tx_hash);

        tracing::debug!("Tx {:?} executed successfully", tx_hash);
        Ok(())
    }

    /// Execute transaction batch
    pub fn execute_batch(&mut self, tx_bytes_list: &[Vec<u8>]) -> TxExecutionStats {
        tx_bytes_list.iter().enumerate().fold(
            TxExecutionStats::default(),
            |mut stats, (idx, tx_bytes)| {
                match Transaction::decode(tx_bytes)
                    .map_err(Into::into)
                    .and_then(|tx| self.execute(&tx))
                {
                    Ok(()) => stats.record_success(),
                    Err(e) => {
                        stats.record_failure();
                        tracing::warn!("Block {}, tx {}: {}", self.block_height, idx, e);
                    }
                }
                stats
            },
        )
    }

    fn verify_nonce(&self, tx: &Transaction) -> Result<()> {
        let current_nonce = self.engine.state().get_nonce(tx.from)?;
        match tx.nonce.cmp(&current_nonce) {
            std::cmp::Ordering::Less => Err(TxExecutionError::InvalidNonce {
                expected: current_nonce,
                got: tx.nonce,
            }),
            std::cmp::Ordering::Greater => Err(TxExecutionError::NonceGap {
                expected: current_nonce,
                got: tx.nonce,
            }),
            std::cmp::Ordering::Equal => Ok(()),
        }
    }

    fn execute_payload(&mut self, tx: &Transaction) -> Result<()> {
        execute_tx_payload(self.engine, self.auth, tx)
    }
}

/// Execute transaction payload (shared logic with auth)
fn execute_tx_payload(engine: &mut Engine, auth: &mut AuthService, tx: &Transaction) -> Result<()> {
    match &tx.payload {
        TxPayload::SetAgent(a) => {
            verify_tx_owner(auth, tx)?;
            auth.set_agent(tx.from, a.agent, a.permissions);
            engine.process_set_agent(tx.from, a)?;
        }
        TxPayload::RemoveAgent(a) => {
            verify_tx_owner(auth, tx)?;
            auth.remove_agent(tx.from, a.agent);
            engine.process_remove_agent(tx.from, a)?;
        }
        _ => execute_tx_payload_readonly(engine, tx)?,
    }
    Ok(())
}

/// Execute transaction payload without auth (for readonly executors)
pub fn execute_tx_payload_readonly(engine: &mut Engine, tx: &Transaction) -> Result<()> {
    match &tx.payload {
        TxPayload::Deposit(d) => engine.process_deposit(tx.from, d)?,
        TxPayload::Withdraw(w) => engine.process_withdraw(tx.from, w)?,
        TxPayload::Transfer(t) => engine.process_transfer(tx.from, t)?,
        TxPayload::PlaceOrder(o) => {
            let order_id = engine.process_place_order(tx.from, o)?;
            tracing::info!("Order placed: {}", order_id);
        }
        TxPayload::CancelOrder(c) => engine.process_cancel_order(tx.from, c)?,
        TxPayload::ClosePosition(p) => engine.process_close_position(tx.from, p)?,
        TxPayload::SetAgent(a) => engine.process_set_agent(tx.from, a)?,
        TxPayload::RemoveAgent(a) => engine.process_remove_agent(tx.from, a)?,
        TxPayload::BridgeDeposit(d) => engine.process_bridge_deposit(tx.from, d)?,
        TxPayload::BridgeWithdraw(w) => engine.process_bridge_withdraw(tx.from, w)?,
        TxPayload::ModifyOrder(_) => {
            return Err(TxExecutionError::NotImplemented("ModifyOrder".into()));
        }
    }
    Ok(())
}

/// Verify that the transaction signer is the account owner
fn verify_tx_owner(auth: &AuthService, tx: &Transaction) -> Result<()> {
    let signer = auth.recover_signer(tx)?;
    (signer == tx.from)
        .then_some(())
        .ok_or(TxExecutionError::Unauthorized)
}

/// Execute a single transaction (convenience function)
pub fn execute_single_tx(
    tx: &Transaction,
    engine: &mut Engine,
    auth: &mut AuthService,
    mempool: &mut Mempool,
    block_height: u64,
) -> Result<()> {
    TransactionExecutor::new(engine, auth, mempool, block_height).execute(tx)
}

/// Execute a batch of transactions (convenience function)
pub fn execute_tx_batch(
    tx_bytes_list: &[Vec<u8>],
    engine: &mut Engine,
    auth: &mut AuthService,
    mempool: &mut Mempool,
    block_height: u64,
) -> TxExecutionStats {
    TransactionExecutor::new(engine, auth, mempool, block_height).execute_batch(tx_bytes_list)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pranklin_state::{PruningConfig, StateManager};
    // Removed unused imports
    use tempfile::TempDir;

    #[test]
    fn test_executor_nonce_validation() {
        let temp_dir = TempDir::new().unwrap();
        let state = StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();
        let engine = Engine::new(state);
        let _auth = AuthService::new();
        let _mempool = Mempool::new(pranklin_mempool::MempoolConfig::default());

        let address = alloy_primitives::Address::ZERO;

        let result = engine.state().get_nonce(address);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }
}
