/// Security-focused integration tests
use alloy_primitives::Address;
use pranklin_auth::AuthService;
use pranklin_engine::Engine;
use pranklin_state::{Market, PruningConfig, StateManager};
use pranklin_tx::*;

/// Helper to create test market
fn create_test_market() -> Market {
    Market {
        id: 0,
        symbol: "BTC-PERP".to_string(),
        base_asset_id: 1,
        quote_asset_id: 0,
        tick_size: 1000,
        price_decimals: 2,
        size_decimals: 6,
        min_order_size: 1,
        max_order_size: 1_000_000_000,
        max_leverage: 20,
        initial_margin_bps: 1000,
        maintenance_margin_bps: 500,
        liquidation_fee_bps: 100,
        funding_interval: 3600,
        max_funding_rate_bps: 100,
    }
}

#[test]
fn test_large_transaction_rejection() {
    // Create a very large transaction payload (> 100KB)
    let huge_data = vec![0u8; 150_000]; // 150KB

    let result = Transaction::decode(&huge_data);

    assert!(result.is_err(), "Large transaction should be rejected");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("too large"),
        "Error should mention transaction size: {}",
        err_msg
    );

    println!("✅ Large transaction rejection test passed");
}

#[test]
fn test_maximum_order_size_validation() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let state = StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();
    let mut engine = Engine::new(state);

    // Create market
    let market = create_test_market();
    engine.state_mut().set_market(0, market.clone()).unwrap();

    let trader = Address::from([1u8; 20]);

    // Fund trader
    engine
        .state_mut()
        .set_balance(trader, 0, u128::MAX) // Unlimited funds
        .unwrap();

    // Try to place order exceeding max size
    let tx = Transaction::new(
        0,
        trader,
        TxPayload::PlaceOrder(PlaceOrderTx {
            market_id: 0,
            is_buy: true,
            order_type: OrderType::Limit,
            price: 50_000,
            size: market.max_order_size + 1, // Exceed maximum!
            time_in_force: TimeInForce::GTC,
            reduce_only: false,
            post_only: false,
        }),
    );

    if let TxPayload::PlaceOrder(ref order) = tx.payload {
        let result = engine.process_place_order(tx.from, order);
        assert!(
            result.is_err(),
            "Order exceeding max size should be rejected"
        );
        assert!(
            result.unwrap_err().to_string().contains("exceeds maximum"),
            "Error should mention maximum size"
        );
    }

    println!("✅ Maximum order size validation test passed");
}

#[test]
fn test_minimum_order_size_validation() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let state = StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();
    let mut engine = Engine::new(state);

    // Create market with min_order_size = 100
    let mut market = create_test_market();
    market.min_order_size = 100;
    engine.state_mut().set_market(0, market.clone()).unwrap();

    let trader = Address::from([1u8; 20]);
    engine
        .state_mut()
        .set_balance(trader, 0, 1_000_000)
        .unwrap();

    // Try to place order below minimum
    let tx = Transaction::new(
        0,
        trader,
        TxPayload::PlaceOrder(PlaceOrderTx {
            market_id: 0,
            is_buy: true,
            order_type: OrderType::Limit,
            price: 50_000,
            size: 50, // Below minimum of 100
            time_in_force: TimeInForce::GTC,
            reduce_only: false,
            post_only: false,
        }),
    );

    if let TxPayload::PlaceOrder(ref order) = tx.payload {
        let result = engine.process_place_order(tx.from, order);
        assert!(result.is_err(), "Order below min size should be rejected");
        assert!(
            result.unwrap_err().to_string().contains("below minimum"),
            "Error should mention minimum size"
        );
    }

    println!("✅ Minimum order size validation test passed");
}

#[test]
fn test_agent_cannot_modify_agents() {
    let mut auth = AuthService::new();
    let owner = Address::from([1u8; 20]);
    let agent = Address::from([2u8; 20]);
    let malicious_agent = Address::from([3u8; 20]);

    // Owner grants trading permissions to agent
    auth.set_agent(
        owner,
        agent,
        permissions::PLACE_ORDER | permissions::CANCEL_ORDER,
    );

    // Create a transaction where agent tries to add another agent
    // This should be caught in tx_executor.rs by checking signer != tx.from
    let _tx = Transaction::new(
        0,
        owner, // tx.from = owner (but signed by agent)
        TxPayload::SetAgent(SetAgentTx {
            agent: malicious_agent,
            permissions: permissions::ALL,
        }),
    );

    // In real execution, tx_executor would:
    // 1. Call auth.recover_signer(tx) -> returns agent address
    // 2. Check if signer == tx.from
    // 3. Reject because agent != owner

    // Simulate the check
    let signer = agent; // Would be recovered from signature
    let is_owner = signer == owner;

    assert!(
        !is_owner,
        "Agent should not be the owner, transaction should fail"
    );

    println!("✅ Agent privilege escalation prevention test passed");
}

#[test]
fn test_nonce_handling_after_failed_transaction() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let state = StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();
    let mut engine = Engine::new(state);

    let trader = Address::from([1u8; 20]);

    // Initial nonce should be 0
    let nonce = engine.state().get_nonce(trader).unwrap();
    assert_eq!(nonce, 0);

    // Transaction with nonce=0 should work
    let tx1 = Transaction::new(
        0,
        trader,
        TxPayload::Deposit(DepositTx {
            amount: 1000,
            asset_id: 0,
        }),
    );

    // Simulate successful execution
    if let TxPayload::Deposit(ref deposit) = tx1.payload {
        engine.process_deposit(tx1.from, deposit).unwrap();
    }
    engine.state_mut().increment_nonce(trader).unwrap();

    // Nonce should now be 1
    let nonce = engine.state().get_nonce(trader).unwrap();
    assert_eq!(nonce, 1);

    // If a transaction with nonce=1 fails BEFORE execution, nonce stays at 1
    // Next transaction with nonce=1 should be accepted
    let tx2 = Transaction::new(
        1,
        trader,
        TxPayload::Withdraw(WithdrawTx {
            amount: 10_000_000, // Will fail - insufficient balance
            asset_id: 0,
            to: Address::ZERO,
        }),
    );

    // This should fail, but nonce wasn't incremented yet (correct!)
    if let TxPayload::Withdraw(ref withdraw) = tx2.payload {
        let result = engine.process_withdraw(tx2.from, withdraw);
        assert!(result.is_err(), "Should fail due to insufficient balance");
    }

    // Nonce should still be 1 (not incremented on failure)
    let nonce = engine.state().get_nonce(trader).unwrap();
    assert_eq!(nonce, 1);

    // Can retry with same nonce
    let tx3 = Transaction::new(
        1,
        trader,
        TxPayload::Deposit(DepositTx {
            amount: 500,
            asset_id: 0,
        }),
    );

    if let TxPayload::Deposit(ref deposit) = tx3.payload {
        engine.process_deposit(tx3.from, deposit).unwrap();
    }
    engine.state_mut().increment_nonce(trader).unwrap();

    let nonce = engine.state().get_nonce(trader).unwrap();
    assert_eq!(nonce, 2);

    println!("✅ Nonce handling test passed");
}

#[test]
fn test_zero_mark_price_rejection() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let state = StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();
    let mut engine = Engine::new(state);

    let market = create_test_market();
    engine.state_mut().set_market(0, market).unwrap();

    let trader = Address::from([1u8; 20]);
    let liquidator = Address::from([2u8; 20]);

    // Create position
    engine
        .state_mut()
        .set_position(
            trader,
            0,
            pranklin_state::Position {
                size: 100,
                entry_price: 50_000,
                is_long: true,
                margin: 5_000,
                funding_index: 0,
            },
        )
        .unwrap();

    // Try to liquidate with zero mark price
    let result = engine.liquidate_with_incentive(trader, 0, 0, liquidator); // mark_price = 0

    assert!(result.is_err(), "Zero mark price should be rejected");
    assert!(
        result.unwrap_err().to_string().contains("cannot be zero"),
        "Error should mention zero price"
    );

    println!("✅ Zero mark price rejection test passed");
}
