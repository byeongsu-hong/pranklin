use super::ServiceContext;
use crate::EngineError;
use pranklin_state::StateManager;
use pranklin_tx::{Address, BridgeDepositTx, BridgeWithdrawTx, DepositTx, TransferTx, WithdrawTx};
use pranklin_types::{BalanceChangeReason, Event};

/// Account service handles balance operations
#[derive(Default)]
pub struct AccountService;

impl AccountService {
    /// Helper: Update balance and emit event
    fn update_balance(
        state: &mut StateManager,
        ctx: &mut ServiceContext,
        address: Address,
        asset_id: u32,
        amount: u128,
        is_credit: bool,
        reason: BalanceChangeReason,
    ) -> Result<(), EngineError> {
        let old_balance = state.get_balance(address, asset_id)?;

        let new_balance = if is_credit {
            old_balance
                .checked_add(amount)
                .ok_or(EngineError::Overflow)?
        } else {
            if old_balance < amount {
                return Err(EngineError::InsufficientBalance);
            }
            old_balance - amount
        };

        state.set_balance(address, asset_id, new_balance)?;

        ctx.emit(Event::BalanceChanged {
            address,
            asset_id,
            old_balance,
            new_balance,
            reason,
        });

        Ok(())
    }

    /// Process deposit
    pub fn deposit(
        &self,
        state: &mut StateManager,
        ctx: &mut ServiceContext,
        from: Address,
        deposit: &DepositTx,
    ) -> Result<(), EngineError> {
        Self::update_balance(
            state,
            ctx,
            from,
            deposit.asset_id,
            deposit.amount,
            true,
            BalanceChangeReason::Deposit,
        )
    }

    /// Process withdrawal
    pub fn withdraw(
        &self,
        state: &mut StateManager,
        ctx: &mut ServiceContext,
        from: Address,
        withdraw: &WithdrawTx,
    ) -> Result<(), EngineError> {
        Self::update_balance(
            state,
            ctx,
            from,
            withdraw.asset_id,
            withdraw.amount,
            false,
            BalanceChangeReason::Withdraw,
        )
    }

    /// Process transfer
    pub fn transfer(
        &self,
        state: &mut StateManager,
        ctx: &mut ServiceContext,
        from: Address,
        transfer: &TransferTx,
    ) -> Result<(), EngineError> {
        if from == transfer.to {
            return Err(EngineError::Other("Cannot transfer to self".into()));
        }

        let asset = state
            .get_asset(transfer.asset_id)?
            .ok_or_else(|| EngineError::Other("Asset not found".into()))?;

        if !asset.is_collateral {
            return Err(EngineError::Other("Asset not transferrable".into()));
        }

        // Debit sender
        let sender_balance = state.get_balance(from, transfer.asset_id)?;
        if sender_balance < transfer.amount {
            return Err(EngineError::InsufficientBalance);
        }

        let new_sender_balance = sender_balance - transfer.amount;
        state.set_balance(from, transfer.asset_id, new_sender_balance)?;

        // Credit recipient
        let recipient_balance = state.get_balance(transfer.to, transfer.asset_id)?;
        let new_recipient_balance = recipient_balance
            .checked_add(transfer.amount)
            .ok_or(EngineError::Overflow)?;
        state.set_balance(transfer.to, transfer.asset_id, new_recipient_balance)?;

        ctx.emit(Event::Transfer {
            from,
            to: transfer.to,
            asset_id: transfer.asset_id,
            amount: transfer.amount,
        });

        Ok(())
    }

    /// Process bridge deposit (operator only)
    pub fn bridge_deposit(
        &self,
        state: &mut StateManager,
        ctx: &mut ServiceContext,
        operator: Address,
        deposit: &BridgeDepositTx,
    ) -> Result<(), EngineError> {
        if !state.is_bridge_operator(operator)? {
            return Err(EngineError::Unauthorized);
        }

        state
            .get_asset(deposit.asset_id)?
            .ok_or_else(|| EngineError::Other("Asset not found".into()))?;

        let old_balance = state.get_balance(deposit.user, deposit.asset_id)?;
        let new_balance = old_balance
            .checked_add(deposit.amount)
            .ok_or(EngineError::Overflow)?;

        state.set_balance(deposit.user, deposit.asset_id, new_balance)?;

        ctx.emit(Event::BridgeDeposit {
            operator,
            user: deposit.user,
            asset_id: deposit.asset_id,
            amount: deposit.amount,
            external_tx_hash: deposit.external_tx_hash,
        });

        Ok(())
    }

    /// Process bridge withdrawal (operator only)
    pub fn bridge_withdraw(
        &self,
        state: &mut StateManager,
        ctx: &mut ServiceContext,
        operator: Address,
        withdraw: &BridgeWithdrawTx,
    ) -> Result<(), EngineError> {
        if !state.is_bridge_operator(operator)? {
            return Err(EngineError::Unauthorized);
        }

        state
            .get_asset(withdraw.asset_id)?
            .ok_or_else(|| EngineError::Other("Asset not found".into()))?;

        let old_balance = state.get_balance(withdraw.user, withdraw.asset_id)?;
        if old_balance < withdraw.amount {
            return Err(EngineError::InsufficientBalance);
        }

        let new_balance = old_balance - withdraw.amount;
        state.set_balance(withdraw.user, withdraw.asset_id, new_balance)?;

        ctx.emit(Event::BridgeWithdraw {
            operator,
            user: withdraw.user,
            asset_id: withdraw.asset_id,
            amount: withdraw.amount,
            destination: withdraw.destination,
            external_tx_hash: withdraw.external_tx_hash,
        });

        Ok(())
    }
}
