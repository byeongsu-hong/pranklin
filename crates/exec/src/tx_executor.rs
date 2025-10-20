use pranklin_auth::AuthService;
use pranklin_engine::Engine;
use pranklin_mempool::Mempool;
use pranklin_tx::{Transaction, TxPayload};

/// Result of transaction execution
pub struct TxExecutionResult {
    pub successful: usize,
    pub failed: usize,
}

/// Execute a single transaction
pub fn execute_single_tx(
    tx: &Transaction,
    engine: &mut Engine,
    auth: &mut AuthService,
    mempool: &mut Mempool,
    block_height: u64,
) -> Result<(), String> {
    let tx_hash = tx.hash();
    tracing::debug!("Processing tx {:?}", tx_hash);

    // Verify signature
    auth.verify_transaction(tx)
        .map_err(|e| format!("Signature verification failed: {}", e))?;

    // CRITICAL FIX: Check nonce BEFORE execution, increment AFTER
    let current_nonce = engine
        .state()
        .get_nonce(tx.from)
        .map_err(|e| format!("Failed to get nonce: {}", e))?;

    // Nonce must equal current nonce (0-indexed, sequential)
    if tx.nonce < current_nonce {
        return Err(format!(
            "Nonce too low: current {}, got {}",
            current_nonce, tx.nonce
        ));
    }
    if tx.nonce > current_nonce {
        return Err(format!(
            "Nonce gap detected: current {}, got {}",
            current_nonce, tx.nonce
        ));
    }

    // Execute based on payload
    execute_tx_payload(tx, engine, auth, block_height)?;

    // Increment nonce AFTER successful execution
    engine
        .state_mut()
        .increment_nonce(tx.from)
        .map_err(|e| format!("Failed to increment nonce: {}", e))?;

    // Remove from mempool on success
    mempool.remove(&tx_hash);
    tracing::debug!("Tx {:?} executed successfully", tx_hash);

    Ok(())
}

/// Execute transaction payload
fn execute_tx_payload(
    tx: &Transaction,
    engine: &mut Engine,
    auth: &mut AuthService,
    _block_height: u64,
) -> Result<(), String> {
    match &tx.payload {
        TxPayload::Deposit(deposit) => {
            tracing::debug!("Processing deposit: {:?}", deposit);
            engine
                .process_deposit(tx, deposit)
                .map_err(|e| e.to_string())
        }
        TxPayload::Withdraw(withdraw) => {
            tracing::debug!("Processing withdraw: {:?}", withdraw);
            engine
                .process_withdraw(tx, withdraw)
                .map_err(|e| e.to_string())
        }
        TxPayload::PlaceOrder(place_order) => {
            tracing::debug!(
                "Processing place order: market_id={}",
                place_order.market_id
            );
            engine
                .process_place_order(tx, place_order)
                .map(|order_id| {
                    tracing::info!("Order placed with ID: {}", order_id);
                })
                .map_err(|e| e.to_string())
        }
        TxPayload::CancelOrder(cancel) => {
            tracing::debug!("Processing cancel order: order_id={}", cancel.order_id);
            engine
                .process_cancel_order(tx, cancel)
                .map_err(|e| e.to_string())
        }
        TxPayload::ModifyOrder(_) => Err("ModifyOrder not yet implemented".to_string()),
        TxPayload::ClosePosition(close) => {
            tracing::debug!("Processing close position: market_id={}", close.market_id);
            engine
                .process_close_position(tx, close)
                .map_err(|e| e.to_string())
        }
        TxPayload::SetAgent(set_agent) => {
            tracing::debug!("Setting agent {:?} for {:?}", set_agent.agent, tx.from);

            // SECURITY FIX: Only account owner can modify agents
            // Recover the actual signer to prevent agent privilege escalation
            let signer = auth
                .recover_signer(tx)
                .map_err(|e| format!("Failed to recover signer: {}", e))?;

            if signer != tx.from {
                return Err(format!(
                    "Only account owner can set agents. Signer: {:?}, Account: {:?}",
                    signer, tx.from
                ));
            }

            auth.set_agent(tx.from, set_agent.agent, set_agent.permissions);
            Ok(())
        }
        TxPayload::RemoveAgent(remove_agent) => {
            tracing::debug!("Removing agent {:?} for {:?}", remove_agent.agent, tx.from);

            // SECURITY FIX: Only account owner can remove agents
            let signer = auth
                .recover_signer(tx)
                .map_err(|e| format!("Failed to recover signer: {}", e))?;

            if signer != tx.from {
                return Err(format!(
                    "Only account owner can remove agents. Signer: {:?}, Account: {:?}",
                    signer, tx.from
                ));
            }

            auth.remove_agent(tx.from, remove_agent.agent);
            Ok(())
        }
        TxPayload::Transfer(transfer) => {
            tracing::debug!(
                "Processing transfer: from={:?}, to={:?}, amount={}, asset_id={}",
                tx.from,
                transfer.to,
                transfer.amount,
                transfer.asset_id
            );
            engine
                .process_transfer(tx, transfer)
                .map_err(|e| e.to_string())
        }
        TxPayload::BridgeDeposit(bridge_deposit) => {
            tracing::debug!(
                "Processing bridge deposit: operator={:?}, user={:?}, amount={}, asset_id={}",
                tx.from,
                bridge_deposit.user,
                bridge_deposit.amount,
                bridge_deposit.asset_id
            );
            engine
                .process_bridge_deposit(tx, bridge_deposit)
                .map_err(|e| e.to_string())
        }
        TxPayload::BridgeWithdraw(bridge_withdraw) => {
            tracing::debug!(
                "Processing bridge withdrawal: operator={:?}, user={:?}, amount={}, asset_id={}",
                tx.from,
                bridge_withdraw.user,
                bridge_withdraw.amount,
                bridge_withdraw.asset_id
            );
            engine
                .process_bridge_withdraw(tx, bridge_withdraw)
                .map_err(|e| e.to_string())
        }
    }
}

/// Execute a batch of transactions
pub fn execute_tx_batch(
    tx_bytes_list: &[Vec<u8>],
    engine: &mut Engine,
    auth: &mut AuthService,
    mempool: &mut Mempool,
    block_height: u64,
) -> TxExecutionResult {
    let mut successful_txs = 0;
    let mut failed_txs = 0;

    for (tx_idx, tx_bytes) in tx_bytes_list.iter().enumerate() {
        // Decode transaction
        let tx = match Transaction::decode(tx_bytes) {
            Ok(tx) => tx,
            Err(e) => {
                tracing::warn!(
                    "Block {}, tx {}: Failed to decode transaction: {}",
                    block_height,
                    tx_idx,
                    e
                );
                failed_txs += 1;
                continue;
            }
        };

        let tx_hash = tx.hash();

        // Execute transaction
        match execute_single_tx(&tx, engine, auth, mempool, block_height) {
            Ok(_) => {
                successful_txs += 1;
            }
            Err(e) => {
                failed_txs += 1;
                tracing::warn!("Block {}, tx {:?}: {}", block_height, tx_hash, e);
            }
        }
    }

    TxExecutionResult {
        successful: successful_txs,
        failed: failed_txs,
    }
}
