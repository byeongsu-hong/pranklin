use alloy_primitives::Address;
use pranklin_engine::*;
use pranklin_state::{Market, PruningConfig, StateManager};
use pranklin_tx::*;

/// Helper to create a test engine
fn create_test_engine() -> (Engine, tempfile::TempDir) {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let state = StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();
    let engine = Engine::new(state);
    (engine, temp_dir)
}

/// Helper to create a test market
fn create_test_market(engine: &mut Engine, market_id: u32) {
    let market = Market {
        id: market_id,
        symbol: "BTC-PERP".to_string(),
        base_asset_id: 1,
        quote_asset_id: 0,
        tick_size: 1000, // Price moves in increments of 1000
        price_decimals: 2,
        size_decimals: 6,
        min_order_size: 1,
        max_order_size: 1_000_000_000, // 1 billion units max
        max_leverage: 20,
        initial_margin_bps: 1000,    // 10%
        maintenance_margin_bps: 500, // 5%
        liquidation_fee_bps: 100,    // 1%
        funding_interval: 3600,      // 1 hour
        max_funding_rate_bps: 100,   // 1%
    };
    engine.state_mut().set_market(market_id, market).unwrap();
}

#[test]
fn test_full_trade_flow() {
    let (mut engine, _dir) = create_test_engine();
    create_test_market(&mut engine, 0);

    let trader1 = Address::from([1u8; 20]);
    let trader2 = Address::from([2u8; 20]);
    let market_id = 0;
    let asset_id = 0;

    // Give both traders initial balance
    engine
        .state_mut()
        .set_balance(trader1, asset_id, 100000)
        .unwrap();
    engine
        .state_mut()
        .set_balance(trader2, asset_id, 100000)
        .unwrap();

    // Trader 1 places a bid
    let tx1 = Transaction::new(
        1,
        trader1,
        TxPayload::PlaceOrder(PlaceOrderTx {
            market_id,
            is_buy: true,
            order_type: OrderType::Limit,
            price: 50000,
            size: 10,
            time_in_force: TimeInForce::GTC,
            reduce_only: false,
            post_only: false,
        }),
    );

    let order_id1 = if let TxPayload::PlaceOrder(ref order) = tx1.payload {
        engine.process_place_order(&tx1, order).unwrap()
    } else {
        panic!("Wrong payload");
    };

    // Verify order is in state
    let order = engine.state().get_order(order_id1).unwrap();
    assert!(order.is_some());

    // Trader 2 places an ask that matches
    let tx2 = Transaction::new(
        1,
        trader2,
        TxPayload::PlaceOrder(PlaceOrderTx {
            market_id,
            is_buy: false,
            order_type: OrderType::Limit,
            price: 50000,
            size: 10,
            time_in_force: TimeInForce::GTC,
            reduce_only: false,
            post_only: false,
        }),
    );

    if let TxPayload::PlaceOrder(ref order) = tx2.payload {
        engine.process_place_order(&tx2, order).unwrap();
    }

    // Verify order is filled (exists in state with Filled status)
    let order = engine.state().get_order(order_id1).unwrap();
    assert!(order.is_some(), "Order should exist in state for history");
    assert_eq!(
        order.unwrap().status,
        pranklin_state::OrderStatus::Filled,
        "Order should be marked as Filled"
    );

    // Verify positions
    let pos1 = engine
        .state()
        .get_position(trader1, market_id)
        .unwrap()
        .unwrap();
    assert_eq!(pos1.size, 10);
    assert!(pos1.is_long);

    let pos2 = engine
        .state()
        .get_position(trader2, market_id)
        .unwrap()
        .unwrap();
    assert_eq!(pos2.size, 10);
    assert!(!pos2.is_long);
}

#[test]
fn test_partial_fill() {
    let (mut engine, _dir) = create_test_engine();
    create_test_market(&mut engine, 0);

    let trader1 = Address::from([1u8; 20]);
    let trader2 = Address::from([2u8; 20]);
    let market_id = 0;
    let asset_id = 0;

    engine
        .state_mut()
        .set_balance(trader1, asset_id, 100000)
        .unwrap();
    engine
        .state_mut()
        .set_balance(trader2, asset_id, 100000)
        .unwrap();

    // Trader 1 places a bid for 10
    let tx1 = Transaction::new(
        1,
        trader1,
        TxPayload::PlaceOrder(PlaceOrderTx {
            market_id,
            is_buy: true,
            order_type: OrderType::Limit,
            price: 50000,
            size: 10,
            time_in_force: TimeInForce::GTC,
            reduce_only: false,
            post_only: false,
        }),
    );

    let order_id1 = if let TxPayload::PlaceOrder(ref order) = tx1.payload {
        engine.process_place_order(&tx1, order).unwrap()
    } else {
        panic!("Wrong payload");
    };

    // Trader 2 places ask for 5 (partial fill)
    let tx2 = Transaction::new(
        1,
        trader2,
        TxPayload::PlaceOrder(PlaceOrderTx {
            market_id,
            is_buy: false,
            order_type: OrderType::Limit,
            price: 50000,
            size: 5,
            time_in_force: TimeInForce::GTC,
            reduce_only: false,
            post_only: false,
        }),
    );

    if let TxPayload::PlaceOrder(ref order) = tx2.payload {
        engine.process_place_order(&tx2, order).unwrap();
    }

    // Verify original order still exists with reduced size
    let order = engine.state().get_order(order_id1).unwrap().unwrap();
    assert_eq!(order.remaining_size, 5);

    // Verify position sizes
    let pos1 = engine
        .state()
        .get_position(trader1, market_id)
        .unwrap()
        .unwrap();
    assert_eq!(pos1.size, 5);
}

#[test]
fn test_post_only_rejection() {
    let (mut engine, _dir) = create_test_engine();
    create_test_market(&mut engine, 0);

    let trader1 = Address::from([1u8; 20]);
    let trader2 = Address::from([2u8; 20]);
    let market_id = 0;
    let asset_id = 0;

    engine
        .state_mut()
        .set_balance(trader1, asset_id, 100000)
        .unwrap();
    engine
        .state_mut()
        .set_balance(trader2, asset_id, 100000)
        .unwrap();

    // Trader 1 places a bid
    let tx1 = Transaction::new(
        1,
        trader1,
        TxPayload::PlaceOrder(PlaceOrderTx {
            market_id,
            is_buy: true,
            order_type: OrderType::Limit,
            price: 50000,
            size: 10,
            time_in_force: TimeInForce::GTC,
            reduce_only: false,
            post_only: false,
        }),
    );

    if let TxPayload::PlaceOrder(ref order) = tx1.payload {
        engine.process_place_order(&tx1, order).unwrap();
    }

    // Trader 2 tries to place post-only order that would match
    let tx2 = Transaction::new(
        1,
        trader2,
        TxPayload::PlaceOrder(PlaceOrderTx {
            market_id,
            is_buy: false,
            order_type: OrderType::Limit,
            price: 50000,
            size: 5,
            time_in_force: TimeInForce::PostOnly,
            reduce_only: false,
            post_only: true,
        }),
    );

    let result = if let TxPayload::PlaceOrder(ref order) = tx2.payload {
        engine.process_place_order(&tx2, order)
    } else {
        panic!("Wrong payload");
    };

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        EngineError::PostOnlyWouldTake
    ));
}

#[test]
fn test_ioc_order() {
    let (mut engine, _dir) = create_test_engine();
    create_test_market(&mut engine, 0);

    let trader1 = Address::from([1u8; 20]);
    let trader2 = Address::from([2u8; 20]);
    let market_id = 0;
    let asset_id = 0;

    engine
        .state_mut()
        .set_balance(trader1, asset_id, 100000)
        .unwrap();
    engine
        .state_mut()
        .set_balance(trader2, asset_id, 100000)
        .unwrap();

    // Trader 1 places a bid for 5
    let tx1 = Transaction::new(
        1,
        trader1,
        TxPayload::PlaceOrder(PlaceOrderTx {
            market_id,
            is_buy: true,
            order_type: OrderType::Limit,
            price: 50000,
            size: 5,
            time_in_force: TimeInForce::GTC,
            reduce_only: false,
            post_only: false,
        }),
    );

    if let TxPayload::PlaceOrder(ref order) = tx1.payload {
        engine.process_place_order(&tx1, order).unwrap();
    }

    // Trader 2 places IOC order for 10 (should only fill 5)
    let tx2 = Transaction::new(
        1,
        trader2,
        TxPayload::PlaceOrder(PlaceOrderTx {
            market_id,
            is_buy: false,
            order_type: OrderType::Limit,
            price: 50000,
            size: 10,
            time_in_force: TimeInForce::IOC,
            reduce_only: false,
            post_only: false,
        }),
    );

    let order_id2 = if let TxPayload::PlaceOrder(ref order) = tx2.payload {
        engine.process_place_order(&tx2, order).unwrap()
    } else {
        panic!("Wrong payload");
    };

    // IOC order should be cancelled (partial fill, remaining cancelled)
    let order = engine.state().get_order(order_id2).unwrap();
    assert!(order.is_some(), "Order should exist in state for history");
    assert_eq!(
        order.unwrap().status,
        pranklin_state::OrderStatus::Cancelled,
        "IOC order should be cancelled if not fully filled"
    );

    // But should have filled 5
    let pos2 = engine
        .state()
        .get_position(trader2, market_id)
        .unwrap()
        .unwrap();
    assert_eq!(pos2.size, 5);
}

#[test]
fn test_liquidation() {
    let (mut engine, _dir) = create_test_engine();
    create_test_market(&mut engine, 0);

    let trader = Address::from([1u8; 20]);
    let market_id = 0;

    // Create position with low margin
    let position = pranklin_state::Position {
        size: 100,
        entry_price: 50000,
        is_long: true,
        margin: 250000, // 5% margin (at liquidation threshold)
        funding_index: 0,
    };
    engine
        .state_mut()
        .set_position(trader, market_id, position)
        .unwrap();

    // Check at entry price - should not liquidate
    let should_liq = engine.should_liquidate(trader, market_id, 50000).unwrap();
    assert!(!should_liq);

    // Price drops 3% - should trigger liquidation
    let mark_price = 48500;
    let should_liq = engine
        .should_liquidate(trader, market_id, mark_price)
        .unwrap();
    assert!(should_liq);

    // Perform liquidation with liquidator
    let liquidator = Address::from([2u8; 20]);
    engine
        .state_mut()
        .set_balance(liquidator, 0, 10_000_000)
        .unwrap();

    let result = engine
        .liquidate_with_incentive(trader, market_id, mark_price, liquidator)
        .unwrap();

    assert!(result.is_some());
    let liq_result = result.unwrap();
    assert_eq!(liq_result.trader, trader);
    assert!(liq_result.liquidated_size > 0);
}

#[test]
fn test_order_cancellation() {
    let (mut engine, _dir) = create_test_engine();
    create_test_market(&mut engine, 0);

    let trader = Address::from([1u8; 20]);
    let market_id = 0;
    let asset_id = 0;

    engine
        .state_mut()
        .set_balance(trader, asset_id, 100000)
        .unwrap();

    // Place order
    let tx1 = Transaction::new(
        1,
        trader,
        TxPayload::PlaceOrder(PlaceOrderTx {
            market_id,
            is_buy: true,
            order_type: OrderType::Limit,
            price: 50000,
            size: 10,
            time_in_force: TimeInForce::GTC,
            reduce_only: false,
            post_only: false,
        }),
    );

    let order_id = if let TxPayload::PlaceOrder(ref order) = tx1.payload {
        engine.process_place_order(&tx1, order).unwrap()
    } else {
        panic!("Wrong payload");
    };

    // Verify order exists
    assert!(engine.state().get_order(order_id).unwrap().is_some());

    // Cancel order
    let tx2 = Transaction::new(
        2,
        trader,
        TxPayload::CancelOrder(CancelOrderTx { order_id }),
    );

    if let TxPayload::CancelOrder(ref cancel) = tx2.payload {
        engine.process_cancel_order(&tx2, cancel).unwrap();
    }

    // Verify order is cancelled (exists in state with Cancelled status)
    let order = engine.state().get_order(order_id).unwrap();
    assert!(order.is_some(), "Order should exist in state for history");
    assert_eq!(
        order.unwrap().status,
        pranklin_state::OrderStatus::Cancelled,
        "Order should be marked as Cancelled"
    );
}

#[test]
fn test_reduce_only_order() {
    let (mut engine, _dir) = create_test_engine();
    create_test_market(&mut engine, 0);

    let trader = Address::from([1u8; 20]);
    let market_id = 0;
    let asset_id = 0;

    engine
        .state_mut()
        .set_balance(trader, asset_id, 100000)
        .unwrap();

    // Create a long position first
    let position = pranklin_state::Position {
        size: 10,
        entry_price: 50000,
        is_long: true,
        margin: 50000,
        funding_index: 0,
    };
    engine
        .state_mut()
        .set_position(trader, market_id, position)
        .unwrap();

    // Try to place reduce-only order on the wrong side (buy instead of sell)
    let tx1 = Transaction::new(
        1,
        trader,
        TxPayload::PlaceOrder(PlaceOrderTx {
            market_id,
            is_buy: true, // Wrong side
            order_type: OrderType::Limit,
            price: 51000,
            size: 5,
            time_in_force: TimeInForce::GTC,
            reduce_only: true,
            post_only: false,
        }),
    );

    let result = if let TxPayload::PlaceOrder(ref order) = tx1.payload {
        engine.process_place_order(&tx1, order)
    } else {
        panic!("Wrong payload");
    };

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        EngineError::ReduceOnlyWouldIncrease
    ));

    // Place valid reduce-only order (sell to reduce long)
    let tx2 = Transaction::new(
        2,
        trader,
        TxPayload::PlaceOrder(PlaceOrderTx {
            market_id,
            is_buy: false, // Correct side
            order_type: OrderType::Limit,
            price: 51000,
            size: 5,
            time_in_force: TimeInForce::GTC,
            reduce_only: true,
            post_only: false,
        }),
    );

    if let TxPayload::PlaceOrder(ref order) = tx2.payload {
        engine.process_place_order(&tx2, order).unwrap();
    }
}

#[test]
fn test_tick_validation() {
    let (mut engine, _dir) = create_test_engine();
    create_test_market(&mut engine, 0);

    let trader = Address::from([1u8; 20]);

    // Fund trader
    engine
        .state_mut()
        .set_balance(trader, 0, 1_000_000)
        .unwrap();

    // Test 1: Valid price on tick boundary (multiple of 1000)
    let tx_valid = Transaction::new(
        1,
        trader,
        TxPayload::PlaceOrder(PlaceOrderTx {
            market_id: 0,
            is_buy: true,
            order_type: OrderType::Limit,
            price: 50_000, // Valid: 50_000 % 1000 = 0
            size: 1,
            time_in_force: TimeInForce::GTC,
            reduce_only: false,
            post_only: false,
        }),
    );

    if let TxPayload::PlaceOrder(ref order) = tx_valid.payload {
        let result = engine.process_place_order(&tx_valid, order);
        assert!(result.is_ok(), "Valid tick price should be accepted");
    }

    // Test 2: Invalid price NOT on tick boundary
    let tx_invalid = Transaction::new(
        2,
        trader,
        TxPayload::PlaceOrder(PlaceOrderTx {
            market_id: 0,
            is_buy: true,
            order_type: OrderType::Limit,
            price: 50_123, // Invalid: 50_123 % 1000 != 0
            size: 1,
            time_in_force: TimeInForce::GTC,
            reduce_only: false,
            post_only: false,
        }),
    );

    if let TxPayload::PlaceOrder(ref order) = tx_invalid.payload {
        let result = engine.process_place_order(&tx_invalid, order);
        assert!(result.is_err(), "Invalid tick price should be rejected");
        assert!(
            result.unwrap_err().to_string().contains("tick boundary"),
            "Error should mention tick boundary"
        );
    }

    // Test 3: Multiple valid tick prices
    let tx_valid2 = Transaction::new(
        3,
        trader,
        TxPayload::PlaceOrder(PlaceOrderTx {
            market_id: 0,
            is_buy: false,
            order_type: OrderType::Limit,
            price: 100_000, // Valid: 100_000 % 1000 = 0
            size: 1,
            time_in_force: TimeInForce::GTC,
            reduce_only: false,
            post_only: false,
        }),
    );

    if let TxPayload::PlaceOrder(ref order) = tx_valid2.payload {
        let result = engine.process_place_order(&tx_valid2, order);
        assert!(result.is_ok(), "Valid tick price (100k) should be accepted");
    }

    // Test 4: Another invalid price
    let tx_invalid2 = Transaction::new(
        4,
        trader,
        TxPayload::PlaceOrder(PlaceOrderTx {
            market_id: 0,
            is_buy: false,
            order_type: OrderType::Limit,
            price: 99_999, // Invalid: 99_999 % 1000 != 0
            size: 1,
            time_in_force: TimeInForce::GTC,
            reduce_only: false,
            post_only: false,
        }),
    );

    if let TxPayload::PlaceOrder(ref order) = tx_invalid2.payload {
        let result = engine.process_place_order(&tx_invalid2, order);
        assert!(
            result.is_err(),
            "Invalid tick price (99_999) should be rejected"
        );
    }
}

#[test]
fn test_market_tick_utilities() {
    let (mut engine, _dir) = create_test_engine();
    create_test_market(&mut engine, 0);

    let market = engine.state().get_market(0).unwrap().unwrap();

    // Test normalize_price
    assert_eq!(market.normalize_price(50_123), 50_000); // Rounds down
    assert_eq!(market.normalize_price(50_500), 51_000); // Rounds up
    assert_eq!(market.normalize_price(50_000), 50_000); // Already on tick

    // Test validate_price
    assert!(market.validate_price(50_000)); // Valid
    assert!(market.validate_price(51_000)); // Valid
    assert!(!market.validate_price(50_123)); // Invalid
    assert!(!market.validate_price(50_500)); // Invalid

    // Test price_to_tick and tick_to_price
    assert_eq!(market.price_to_tick(50_000), 50);
    assert_eq!(market.tick_to_price(50), 50_000);
    assert_eq!(market.price_to_tick(51_000), 51);
    assert_eq!(market.tick_to_price(51), 51_000);
}
