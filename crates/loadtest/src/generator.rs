use crate::config::TransactionType;
use crate::wallet::Wallet;
use pranklin_tx::*;

/// Generate a random transaction based on the specified type
pub fn generate_transaction(
    wallet: &Wallet,
    tx_type: TransactionType,
    market_id: u32,
    asset_id: u32,
) -> anyhow::Result<Transaction> {
    let payload = match tx_type {
        TransactionType::PlaceOrder => generate_place_order(market_id),
        TransactionType::CancelOrder => generate_cancel_order(),
        TransactionType::Deposit => generate_deposit(asset_id),
        TransactionType::Withdraw => generate_withdraw(wallet.address(), asset_id),
        TransactionType::Transfer => generate_transfer(asset_id),
        TransactionType::Mixed => generate_mixed(wallet.address(), market_id, asset_id),
    };

    wallet.create_signed_transaction(payload)
}

/// Generate a random place order transaction
fn generate_place_order(market_id: u32) -> TxPayload {
    // Random order parameters
    let is_buy = fastrand::bool();
    let price = fastrand::u64(45000_000000..55000_000000); // $45k-$55k with 6 decimals
    let size = fastrand::u64(1_000000..100_000000); // 1-100 contracts with 6 decimals

    let order_type = if fastrand::u8(0..10) < 8 {
        OrderType::Limit
    } else {
        OrderType::Market
    };

    let time_in_force = match fastrand::u8(0..4) {
        0 => TimeInForce::GTC,
        1 => TimeInForce::IOC,
        2 => TimeInForce::FOK,
        _ => TimeInForce::PostOnly,
    };

    TxPayload::PlaceOrder(PlaceOrderTx {
        market_id,
        is_buy,
        order_type,
        price: if order_type == OrderType::Market {
            0
        } else {
            price
        },
        size,
        time_in_force,
        reduce_only: false,
        post_only: time_in_force == TimeInForce::PostOnly,
    })
}

/// Generate a random cancel order transaction
fn generate_cancel_order() -> TxPayload {
    let order_id = fastrand::u64(1..1000000);

    TxPayload::CancelOrder(CancelOrderTx { order_id })
}

/// Generate a random deposit transaction
fn generate_deposit(asset_id: u32) -> TxPayload {
    let amount = fastrand::u128(100_000000..10000_000000); // $100-$10k with 6 decimals

    TxPayload::Deposit(DepositTx { amount, asset_id })
}

/// Generate a random withdraw transaction
fn generate_withdraw(from: alloy_primitives::Address, asset_id: u32) -> TxPayload {
    let amount = fastrand::u128(10_000000..1000_000000); // $10-$1k with 6 decimals

    TxPayload::Withdraw(WithdrawTx {
        amount,
        asset_id,
        to: from, // Withdraw to self for testing
    })
}

/// Generate a random transfer transaction
fn generate_transfer(asset_id: u32) -> TxPayload {
    let amount = fastrand::u128(1_000000..100_000000); // $1-$100 with 6 decimals

    // Generate random recipient
    let mut random_bytes = [0u8; 20];
    fastrand::fill(&mut random_bytes);
    let to = alloy_primitives::Address::from_slice(&random_bytes);

    TxPayload::Transfer(TransferTx {
        to,
        amount,
        asset_id,
    })
}

/// Generate a random mixed transaction
fn generate_mixed(
    wallet_address: alloy_primitives::Address,
    market_id: u32,
    asset_id: u32,
) -> TxPayload {
    match fastrand::u8(0..5) {
        0 => generate_place_order(market_id),
        1 => generate_cancel_order(),
        2 => generate_deposit(asset_id),
        3 => generate_withdraw(wallet_address, asset_id),
        _ => generate_transfer(asset_id),
    }
}
