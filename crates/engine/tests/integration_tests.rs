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
        engine.process_place_order(tx1.from, order).unwrap()
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
        engine.process_place_order(tx2.from, order).unwrap();
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
        engine.process_place_order(tx1.from, order).unwrap()
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
        engine.process_place_order(tx2.from, order).unwrap();
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
        engine.process_place_order(tx1.from, order).unwrap();
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
        engine.process_place_order(tx2.from, order)
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
        engine.process_place_order(tx1.from, order).unwrap();
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
        engine.process_place_order(tx2.from, order).unwrap()
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
        engine.process_place_order(tx1.from, order).unwrap()
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
        engine.process_cancel_order(tx2.from, cancel).unwrap();
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
        engine.process_place_order(tx1.from, order)
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
        engine.process_place_order(tx2.from, order).unwrap();
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
        let result = engine.process_place_order(tx_valid.from, order);
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
        let result = engine.process_place_order(tx_invalid.from, order);
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
        let result = engine.process_place_order(tx_valid2.from, order);
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
        let result = engine.process_place_order(tx_invalid2.from, order);
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

#[test]
fn test_complete_trading_flow_with_deposit_withdraw() {
    let (mut engine, _dir) = create_test_engine();
    create_test_market(&mut engine, 0);

    let trader1 = Address::from([1u8; 20]);
    let trader2 = Address::from([2u8; 20]);
    let market_id = 0;
    let asset_id = 0;

    // Initial deposit simulation
    engine
        .state_mut()
        .set_balance(trader1, asset_id, 100_000)
        .unwrap();
    engine
        .state_mut()
        .set_balance(trader2, asset_id, 100_000)
        .unwrap();

    // Trader 1 places multiple orders
    for i in 0..3 {
        let tx = Transaction::new(
            i + 1,
            trader1,
            TxPayload::PlaceOrder(PlaceOrderTx {
                market_id,
                is_buy: true,
                order_type: OrderType::Limit,
                price: 50_000 + (i * 1_000),
                size: 5,
                time_in_force: TimeInForce::GTC,
                reduce_only: false,
                post_only: false,
            }),
        );

        if let TxPayload::PlaceOrder(ref order) = tx.payload {
            engine.process_place_order(tx.from, order).unwrap();
        }
    }

    // Trader 2 places matching sell order (larger size for liquidity)
    let tx = Transaction::new(
        1,
        trader2,
        TxPayload::PlaceOrder(PlaceOrderTx {
            market_id,
            is_buy: false,
            order_type: OrderType::Limit,
            price: 50_000,
            size: 20, // Increased to provide enough liquidity for closing
            time_in_force: TimeInForce::GTC,
            reduce_only: false,
            post_only: false,
        }),
    );

    if let TxPayload::PlaceOrder(ref order) = tx.payload {
        engine.process_place_order(tx.from, order).unwrap();
    }

    // Verify positions were created
    let pos1 = engine
        .state()
        .get_position(trader1, market_id)
        .unwrap()
        .unwrap();
    assert!(pos1.size > 0);
    assert!(pos1.is_long);

    let pos2 = engine
        .state()
        .get_position(trader2, market_id)
        .unwrap()
        .unwrap();
    assert!(pos2.size > 0);
    assert!(!pos2.is_long);

    // Trader 2 needs to provide buy liquidity for trader 1 to close their long position
    // Place buy limit order on the opposite side
    let tx_buy = Transaction::new(
        2,
        trader2,
        TxPayload::PlaceOrder(PlaceOrderTx {
            market_id,
            is_buy: true,
            order_type: OrderType::Limit,
            price: 50_000,
            size: 20, // Enough to close trader1's position
            time_in_force: TimeInForce::GTC,
            reduce_only: false,
            post_only: false,
        }),
    );

    if let TxPayload::PlaceOrder(ref order) = tx_buy.payload {
        engine.process_place_order(tx_buy.from, order).unwrap();
    }

    // Close position for trader 1
    let tx = Transaction::new(
        10,
        trader1,
        TxPayload::ClosePosition(ClosePositionTx {
            market_id,
            size: pos1.size, // Close entire position
        }),
    );

    if let TxPayload::ClosePosition(ref close) = tx.payload {
        engine.process_close_position(tx.from, close).unwrap();
    }

    // Verify position is closed
    let pos1_closed = engine.state().get_position(trader1, market_id).unwrap();
    assert!(
        pos1_closed.is_none() || pos1_closed.as_ref().unwrap().size == 0,
        "Position should be fully closed, but has size: {:?}",
        pos1_closed.map(|p| p.size)
    );
}

#[test]
fn test_multi_market_trading() {
    let (mut engine, _dir) = create_test_engine();

    // Create multiple markets
    create_test_market(&mut engine, 0); // BTC-PERP

    let eth_market = Market {
        id: 1,
        symbol: "ETH-PERP".to_string(),
        base_asset_id: 2,
        quote_asset_id: 0,
        tick_size: 100,
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
    };
    engine.state_mut().set_market(1, eth_market).unwrap();

    let trader = Address::from([1u8; 20]);
    let asset_id = 0;

    // Give trader initial balance
    engine
        .state_mut()
        .set_balance(trader, asset_id, 200_000)
        .unwrap();

    // Place order in BTC market
    let tx_btc = Transaction::new(
        1,
        trader,
        TxPayload::PlaceOrder(PlaceOrderTx {
            market_id: 0,
            is_buy: true,
            order_type: OrderType::Limit,
            price: 50_000,
            size: 5,
            time_in_force: TimeInForce::GTC,
            reduce_only: false,
            post_only: false,
        }),
    );

    if let TxPayload::PlaceOrder(ref order) = tx_btc.payload {
        engine.process_place_order(tx_btc.from, order).unwrap();
    }

    // Place order in ETH market
    let tx_eth = Transaction::new(
        2,
        trader,
        TxPayload::PlaceOrder(PlaceOrderTx {
            market_id: 1,
            is_buy: true,
            order_type: OrderType::Limit,
            price: 3_000,
            size: 10,
            time_in_force: TimeInForce::GTC,
            reduce_only: false,
            post_only: false,
        }),
    );

    if let TxPayload::PlaceOrder(ref order) = tx_eth.payload {
        engine.process_place_order(tx_eth.from, order).unwrap();
    }

    // Verify orders exist in different markets
    // TODO: Add get_orders_by_market method or verify through state
    // let btc_orders = engine.get_orders_by_market(0);
    // let eth_orders = engine.get_orders_by_market(1);
    // assert!(!btc_orders.is_empty());
    // assert!(!eth_orders.is_empty());
}

#[test]
fn test_transfer_between_accounts() {
    let (mut engine, _dir) = create_test_engine();

    // Create asset first
    let asset = pranklin_state::Asset {
        id: 0,
        name: "USD Coin".to_string(),
        symbol: "USDC".to_string(),
        decimals: 6,
        is_collateral: true,
        collateral_weight_bps: 10000, // 100%
    };
    engine.state_mut().set_asset(0, asset).unwrap();

    let sender = Address::from([1u8; 20]);
    let recipient = Address::from([2u8; 20]);
    let asset_id = 0;
    let transfer_amount = 50_000u64;

    // Setup initial balances
    engine
        .state_mut()
        .set_balance(sender, asset_id, 100_000)
        .unwrap();
    engine
        .state_mut()
        .set_balance(recipient, asset_id, 10_000)
        .unwrap();

    // Execute transfer
    let tx = Transaction::new(
        1,
        sender,
        TxPayload::Transfer(TransferTx {
            to: recipient,
            asset_id,
            amount: transfer_amount as u128,
        }),
    );

    if let TxPayload::Transfer(ref transfer) = tx.payload {
        engine.process_transfer(tx.from, transfer).unwrap();
    }

    // Verify balances updated correctly
    let sender_balance = engine.state().get_balance(sender, asset_id).unwrap();
    let recipient_balance = engine.state().get_balance(recipient, asset_id).unwrap();

    assert_eq!(sender_balance, 50_000);
    assert_eq!(recipient_balance, 60_000);
}

#[test]
#[ignore = "ModifyPosition not yet implemented"]
fn test_position_modification() {
    let (mut engine, _dir) = create_test_engine();
    create_test_market(&mut engine, 0);

    let trader = Address::from([1u8; 20]);
    let market_id = 0;
    let asset_id = 0;

    engine
        .state_mut()
        .set_balance(trader, asset_id, 200_000)
        .unwrap();

    // Create initial position
    let position = pranklin_state::Position {
        size: 10,
        entry_price: 50_000,
        is_long: true,
        margin: 100_000,
        funding_index: 0,
    };
    engine
        .state_mut()
        .set_position(trader, market_id, position)
        .unwrap();

    // TODO: Modify position to add margin (ModifyPosition not yet implemented)
    // Skip this test until ModifyPosition is implemented
    /*
    let tx = Transaction::new(
        1,
        trader,
        TxPayload::ModifyPosition(ModifyPositionTx {
            market_id,
            margin_delta: 50_000i64,
        }),
    );

    if let TxPayload::ModifyPosition(ref modify) = tx.payload {
        engine.process_modify_position(tx.from, modify).unwrap();
    }
    */

    // Verify margin increased
    let updated_pos = engine
        .state()
        .get_position(trader, market_id)
        .unwrap()
        .unwrap();
    assert_eq!(updated_pos.margin, 150_000);
}

#[test]
fn test_fok_order_execution() {
    let (mut engine, _dir) = create_test_engine();
    create_test_market(&mut engine, 0);

    let trader1 = Address::from([1u8; 20]);
    let trader2 = Address::from([2u8; 20]);
    let market_id = 0;
    let asset_id = 0;

    engine
        .state_mut()
        .set_balance(trader1, asset_id, 100_000)
        .unwrap();
    engine
        .state_mut()
        .set_balance(trader2, asset_id, 100_000)
        .unwrap();

    // Trader 1 places partial liquidity
    let tx1 = Transaction::new(
        1,
        trader1,
        TxPayload::PlaceOrder(PlaceOrderTx {
            market_id,
            is_buy: true,
            order_type: OrderType::Limit,
            price: 50_000,
            size: 5,
            time_in_force: TimeInForce::GTC,
            reduce_only: false,
            post_only: false,
        }),
    );

    if let TxPayload::PlaceOrder(ref order) = tx1.payload {
        engine.process_place_order(tx1.from, order).unwrap();
    }

    // Trader 2 places FOK order for larger size (should fail)
    let tx2 = Transaction::new(
        1,
        trader2,
        TxPayload::PlaceOrder(PlaceOrderTx {
            market_id,
            is_buy: false,
            order_type: OrderType::Limit,
            price: 50_000,
            size: 10, // More than available
            time_in_force: TimeInForce::FOK,
            reduce_only: false,
            post_only: false,
        }),
    );

    let result = if let TxPayload::PlaceOrder(ref order) = tx2.payload {
        engine.process_place_order(tx2.from, order)
    } else {
        panic!("Wrong payload");
    };

    // FOK should fail if can't fill completely
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), EngineError::OrderNotFilled));
}

#[test]
fn test_event_emission() {
    let (mut engine, _dir) = create_test_engine();
    create_test_market(&mut engine, 0);

    let trader = Address::from([1u8; 20]);
    let market_id = 0;
    let asset_id = 0;

    engine
        .state_mut()
        .set_balance(trader, asset_id, 100_000)
        .unwrap();

    // Begin transaction BEFORE processing
    let tx_hash = B256::from([1u8; 32]);

    // Place order and collect events
    let tx = Transaction::new(
        1,
        trader,
        TxPayload::PlaceOrder(PlaceOrderTx {
            market_id,
            is_buy: true,
            order_type: OrderType::Limit,
            price: 50_000,
            size: 10,
            time_in_force: TimeInForce::GTC,
            reduce_only: false,
            post_only: false,
        }),
    );

    engine.begin_tx(tx_hash, 1, 1000);

    if let TxPayload::PlaceOrder(ref order) = tx.payload {
        engine.process_place_order(tx.from, order).unwrap();
    }

    // Take events
    let events = engine.take_events();

    // Should have emitted events (OrderPlaced, BalanceChanged, etc.)
    assert!(
        !events.is_empty(),
        "Events should be emitted after processing order"
    );

    // Verify event structure
    for event in events {
        assert_eq!(event.block_height, 1);
        assert_eq!(event.tx_hash, tx_hash);
        assert_eq!(event.timestamp, 1000);
    }
}
