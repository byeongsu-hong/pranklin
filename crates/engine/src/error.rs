use thiserror::Error;

/// Engine errors
#[derive(Error, Debug)]
pub enum EngineError {
    #[error("Market not found")]
    MarketNotFound,

    #[error("Order not found")]
    OrderNotFound,

    #[error("Position not found")]
    PositionNotFound,

    #[error("Insufficient balance")]
    InsufficientBalance,

    #[error("Insufficient margin")]
    InsufficientMargin,

    #[error("Unauthorized")]
    Unauthorized,

    #[error("Invalid order parameters")]
    InvalidOrderParams,

    #[error("Order not filled")]
    OrderNotFilled,

    #[error("Self-trade not allowed")]
    SelfTrade,

    #[error("Post-only order would take")]
    PostOnlyWouldTake,

    #[error("Reduce-only order would increase position")]
    ReduceOnlyWouldIncrease,

    #[error("Leverage too high")]
    LeverageTooHigh,

    #[error("Position would be liquidated")]
    WouldBeLiquidated,

    #[error("Overflow in calculation")]
    Overflow,

    #[error("Division by zero")]
    DivisionByZero,

    #[error("State error: {0}")]
    StateError(#[from] pranklin_state::StateError),

    #[error("Transaction error: {0}")]
    TxError(#[from] pranklin_tx::TxError),

    #[error("Other error: {0}")]
    Other(String),
}
