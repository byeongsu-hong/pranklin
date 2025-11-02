use crate::config::TransactionType;
use crate::wallet::Wallet;
use pranklin_tx::*;

/// Trait for generating transaction payloads
pub trait TxGenerator {
    fn generate(&self) -> TxPayload;
}

/// Transaction generator based on type and parameters
pub struct TransactionGenerator {
    pub tx_type: TransactionType,
    pub market_id: u32,
    pub asset_id: u32,
    pub address: alloy_primitives::Address,
}

impl TxGenerator for TransactionGenerator {
    fn generate(&self) -> TxPayload {
        match self.tx_type {
            TransactionType::PlaceOrder => OrderGenerator::new(self.market_id).generate(),
            TransactionType::CancelOrder => CancelGenerator.generate(),
            TransactionType::Deposit => DepositGenerator::new(self.asset_id).generate(),
            TransactionType::Withdraw => {
                WithdrawGenerator::new(self.asset_id, self.address).generate()
            }
            TransactionType::Transfer => TransferGenerator::new(self.asset_id).generate(),
            TransactionType::Mixed => {
                MixedGenerator::new(self.market_id, self.asset_id, self.address).generate()
            }
        }
    }
}

/// Generate a random transaction based on the specified type
pub fn generate_transaction(
    wallet: &Wallet,
    tx_type: TransactionType,
    market_id: u32,
    asset_id: u32,
) -> anyhow::Result<Transaction> {
    let generator = TransactionGenerator {
        tx_type,
        market_id,
        asset_id,
        address: wallet.address(),
    };
    wallet.create_signed_transaction(generator.generate())
}

/// Order generator
pub struct OrderGenerator {
    market_id: u32,
}

impl OrderGenerator {
    pub fn new(market_id: u32) -> Self {
        Self { market_id }
    }

    pub fn with_params(
        market_id: u32,
        is_buy: bool,
        order_type: OrderType,
        price: u64,
        size: u64,
        time_in_force: TimeInForce,
    ) -> TxPayload {
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
}

impl TxGenerator for OrderGenerator {
    fn generate(&self) -> TxPayload {
        let is_buy = fastrand::bool();
        let price = fastrand::u64(45000_000000..55000_000000);
        let size = fastrand::u64(1_000000..100_000000);
        let order_type = if fastrand::u8(0..10) < 8 {
            OrderType::Limit
        } else {
            OrderType::Market
        };
        let time_in_force = [
            TimeInForce::GTC,
            TimeInForce::IOC,
            TimeInForce::FOK,
            TimeInForce::PostOnly,
        ][fastrand::usize(0..4)];

        Self::with_params(
            self.market_id,
            is_buy,
            order_type,
            price,
            size,
            time_in_force,
        )
    }
}

/// Cancel order generator
pub struct CancelGenerator;

impl TxGenerator for CancelGenerator {
    fn generate(&self) -> TxPayload {
        TxPayload::CancelOrder(CancelOrderTx {
            order_id: fastrand::u64(1..1000000),
        })
    }
}

/// Deposit generator
pub struct DepositGenerator {
    asset_id: u32,
}

impl DepositGenerator {
    pub fn new(asset_id: u32) -> Self {
        Self { asset_id }
    }
}

impl TxGenerator for DepositGenerator {
    fn generate(&self) -> TxPayload {
        TxPayload::Deposit(DepositTx {
            amount: fastrand::u128(100_000000..10000_000000),
            asset_id: self.asset_id,
        })
    }
}

/// Withdraw generator
pub struct WithdrawGenerator {
    asset_id: u32,
    address: alloy_primitives::Address,
}

impl WithdrawGenerator {
    pub fn new(asset_id: u32, address: alloy_primitives::Address) -> Self {
        Self { asset_id, address }
    }
}

impl TxGenerator for WithdrawGenerator {
    fn generate(&self) -> TxPayload {
        TxPayload::Withdraw(WithdrawTx {
            amount: fastrand::u128(10_000000..1000_000000),
            asset_id: self.asset_id,
            to: self.address,
        })
    }
}

/// Transfer generator
pub struct TransferGenerator {
    asset_id: u32,
}

impl TransferGenerator {
    pub fn new(asset_id: u32) -> Self {
        Self { asset_id }
    }
}

impl TxGenerator for TransferGenerator {
    fn generate(&self) -> TxPayload {
        let mut random_bytes = [0u8; 20];
        fastrand::fill(&mut random_bytes);

        TxPayload::Transfer(TransferTx {
            to: alloy_primitives::Address::from_slice(&random_bytes),
            amount: fastrand::u128(1_000000..100_000000),
            asset_id: self.asset_id,
        })
    }
}

/// Mixed transaction generator
pub struct MixedGenerator {
    market_id: u32,
    asset_id: u32,
    address: alloy_primitives::Address,
}

impl MixedGenerator {
    pub fn new(market_id: u32, asset_id: u32, address: alloy_primitives::Address) -> Self {
        Self {
            market_id,
            asset_id,
            address,
        }
    }
}

impl TxGenerator for MixedGenerator {
    fn generate(&self) -> TxPayload {
        match fastrand::u8(0..5) {
            0 => OrderGenerator::new(self.market_id).generate(),
            1 => CancelGenerator.generate(),
            2 => DepositGenerator::new(self.asset_id).generate(),
            3 => WithdrawGenerator::new(self.asset_id, self.address).generate(),
            _ => TransferGenerator::new(self.asset_id).generate(),
        }
    }
}
