/// Restart recovery integration tests
use alloy_primitives::Address;
use pranklin_engine::Engine;
use pranklin_state::{Market, PruningConfig, StateManager};
use pranklin_tx::*;

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
fn test_orderbook_rebuild_after_restart() {
    let temp_dir = tempfile::TempDir::new().unwrap();

    // Phase 1: Create engine, place orders, then "crash"
    {
        let state = StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();
        let mut engine = Engine::new(state);

        // Create market
        let market = create_test_market();
        engine.state_mut().set_market(0, market).unwrap();

        let trader1 = Address::from([1u8; 20]);
        let trader2 = Address::from([2u8; 20]);

        // Fund traders
        engine
            .state_mut()
            .set_balance(trader1, 0, 1_000_000)
            .unwrap();
        engine
            .state_mut()
            .set_balance(trader2, 0, 1_000_000)
            .unwrap();

        // Place bid
        let tx1 = Transaction::new_raw(
            0,
            trader1,
            TxPayload::PlaceOrder(PlaceOrderTx {
                market_id: 0,
                is_buy: true,
                order_type: OrderType::Limit,
                price: 50_000,
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

        // Place ask
        let tx2 = Transaction::new_raw(
            0,
            trader2,
            TxPayload::PlaceOrder(PlaceOrderTx {
                market_id: 0,
                is_buy: false,
                order_type: OrderType::Limit,
                price: 51_000,
                size: 10,
                time_in_force: TimeInForce::GTC,
                reduce_only: false,
                post_only: false,
            }),
        );

        let order_id2 = if let TxPayload::PlaceOrder(ref order) = tx2.payload {
            engine.process_place_order(tx2.from, order).unwrap()
        } else {
            panic!("Wrong payload");
        };

        // Commit state
        engine.state_mut().begin_block(1);
        engine.state_mut().commit().unwrap();

        // Verify orders are in active list
        let active_orders = engine.state().get_active_orders_by_market(0).unwrap();
        assert_eq!(active_orders.len(), 2);
        assert!(active_orders.contains(&order_id1));
        assert!(active_orders.contains(&order_id2));

        // Verify best bid/ask
        let best_bid = engine.get_orderbook_depth(0, 1).0;
        let best_ask = engine.get_orderbook_depth(0, 1).1;
        assert_eq!(best_bid.len(), 1);
        assert_eq!(best_ask.len(), 1);
        assert_eq!(best_bid[0].0, 50_000);
        assert_eq!(best_ask[0].0, 51_000);

        println!("✅ Phase 1: Orders placed and committed");
        // Engine drops here (simulates crash)
    }

    // Phase 2: Restart - create new engine from same database
    {
        let state = StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();
        let mut engine = Engine::new(state);

        // Rebuild position index
        engine.state_mut().rebuild_position_index().unwrap();

        // Rebuild orderbook from state
        engine.rebuild_orderbook_from_state().unwrap();

        // Verify orders were recovered
        let active_orders = engine.state().get_active_orders_by_market(0).unwrap();
        assert_eq!(
            active_orders.len(),
            2,
            "Should recover 2 active orders after restart"
        );

        // Verify orderbook depth
        let (bids, asks) = engine.get_orderbook_depth(0, 10);
        assert_eq!(bids.len(), 1, "Should recover 1 bid");
        assert_eq!(asks.len(), 1, "Should recover 1 ask");
        assert_eq!(bids[0].0, 50_000, "Bid price should be recovered");
        assert_eq!(asks[0].0, 51_000, "Ask price should be recovered");

        println!("✅ Phase 2: Orderbook successfully rebuilt after restart");
    }
}

#[test]
fn test_position_index_rebuild() {
    let temp_dir = tempfile::TempDir::new().unwrap();

    // Phase 1: Create positions
    {
        let state = StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();
        let mut engine = Engine::new(state);

        let market = create_test_market();
        engine.state_mut().set_market(0, market).unwrap();

        // Create 3 positions
        for i in 0..3 {
            let trader = Address::from([i; 20]);
            engine
                .state_mut()
                .set_position(
                    trader,
                    0,
                    pranklin_state::Position {
                        size: 100 * (i as u64 + 1),
                        entry_price: 50_000,
                        is_long: true,
                        margin: 10_000 * (i as u128 + 1),
                        funding_index: 0,
                    },
                )
                .unwrap();
        }

        // Commit
        engine.state_mut().begin_block(1);
        engine.state_mut().commit().unwrap();

        // Verify positions
        let positions = engine.state().get_all_positions_in_market(0).unwrap();
        assert_eq!(positions.len(), 3);

        println!("✅ Phase 1: 3 positions created");
    }

    // Phase 2: Restart and rebuild
    {
        let mut state = StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();

        // Position index should be empty after restart
        let positions_before = state.get_all_positions_in_market(0).unwrap();
        // Should still work (fallback to state)
        assert_eq!(
            positions_before.len(),
            3,
            "Should load from state even without index"
        );

        // Rebuild index
        state.rebuild_position_index().unwrap();

        // Verify index is rebuilt
        let positions_after = state.get_all_positions_in_market(0).unwrap();
        assert_eq!(positions_after.len(), 3);

        println!("✅ Phase 2: Position index rebuilt successfully");
    }
}

#[test]
fn test_market_list_persistence() {
    let temp_dir = tempfile::TempDir::new().unwrap();

    // Phase 1: Create multiple markets
    {
        let state = StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();
        let mut engine = Engine::new(state);

        // Create markets with IDs: 0, 5, 100 (non-sequential!)
        for market_id in [0, 5, 100] {
            let mut market = create_test_market();
            market.id = market_id;
            market.symbol = format!("MARKET-{}", market_id);
            engine.state_mut().set_market(market_id, market).unwrap();
        }

        // Commit
        engine.state_mut().begin_block(1);
        engine.state_mut().commit().unwrap();

        // Verify market list
        let markets = engine.state().list_all_markets().unwrap();
        assert_eq!(markets.len(), 3);
        assert!(markets.contains(&0));
        assert!(markets.contains(&5));
        assert!(markets.contains(&100));

        println!("✅ Phase 1: 3 markets created with IDs [0, 5, 100]");
    }

    // Phase 2: Restart and verify
    {
        let state = StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();
        let mut engine = Engine::new(state);

        // Market list should be persisted
        let markets = engine.state().list_all_markets().unwrap();
        assert_eq!(markets.len(), 3, "Market list should persist after restart");
        assert!(markets.contains(&0));
        assert!(markets.contains(&5));
        assert!(markets.contains(&100));

        // Orderbook rebuild should find all markets
        engine.rebuild_orderbook_from_state().unwrap();

        println!("✅ Phase 2: Market list persisted and orderbook rebuilt");
    }
}

#[test]
fn test_full_crash_recovery_scenario() {
    let temp_dir = tempfile::TempDir::new().unwrap();

    let trader1 = Address::from([1u8; 20]);
    let trader2 = Address::from([2u8; 20]);
    let trader3 = Address::from([3u8; 20]);

    // Phase 1: Normal operation
    let order_id1: u64;
    let order_id2: u64;
    {
        let state = StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();
        let mut engine = Engine::new(state);

        let market = create_test_market();
        engine.state_mut().set_market(0, market).unwrap();

        // Fund all traders
        for trader in [trader1, trader2, trader3] {
            engine
                .state_mut()
                .set_balance(trader, 0, 10_000_000)
                .unwrap();
        }

        // Place 2 orders
        let tx1 = Transaction::new_raw(
            0,
            trader1,
            TxPayload::PlaceOrder(PlaceOrderTx {
                market_id: 0,
                is_buy: true,
                order_type: OrderType::Limit,
                price: 49_000,
                size: 100,
                time_in_force: TimeInForce::GTC,
                reduce_only: false,
                post_only: false,
            }),
        );

        order_id1 = if let TxPayload::PlaceOrder(ref order) = tx1.payload {
            engine.process_place_order(tx1.from, order).unwrap()
        } else {
            panic!();
        };

        let tx2 = Transaction::new_raw(
            0,
            trader2,
            TxPayload::PlaceOrder(PlaceOrderTx {
                market_id: 0,
                is_buy: false,
                order_type: OrderType::Limit,
                price: 51_000,
                size: 200,
                time_in_force: TimeInForce::GTC,
                reduce_only: false,
                post_only: false,
            }),
        );

        order_id2 = if let TxPayload::PlaceOrder(ref order) = tx2.payload {
            engine.process_place_order(tx2.from, order).unwrap()
        } else {
            panic!();
        };

        // Create position for trader3
        engine
            .state_mut()
            .set_position(
                trader3,
                0,
                pranklin_state::Position {
                    size: 50,
                    entry_price: 50_000,
                    is_long: true,
                    margin: 50_000,
                    funding_index: 0,
                },
            )
            .unwrap();

        // Commit
        engine.state_mut().begin_block(1);
        engine.state_mut().commit().unwrap();

        println!("✅ Phase 1: Created 2 orders + 1 position, then 'crashed'");
    }

    // Phase 2: Full recovery
    {
        let state = StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();
        let mut engine = Engine::new(state);

        // Rebuild all indices
        engine.state_mut().rebuild_position_index().unwrap();
        engine.rebuild_orderbook_from_state().unwrap();

        // Verify orderbook
        let active_orders = engine.state().get_active_orders_by_market(0).unwrap();
        assert_eq!(active_orders.len(), 2, "Should recover 2 active orders");

        let (bids, asks) = engine.get_orderbook_depth(0, 10);
        assert_eq!(bids.len(), 1);
        assert_eq!(asks.len(), 1);
        assert_eq!(bids[0], (49_000, 100));
        assert_eq!(asks[0], (51_000, 200));

        // Verify orders are retrievable
        let order1 = engine.state().get_order(order_id1).unwrap();
        assert!(order1.is_some());
        assert_eq!(order1.unwrap().remaining_size, 100);

        let order2 = engine.state().get_order(order_id2).unwrap();
        assert!(order2.is_some());
        assert_eq!(order2.unwrap().remaining_size, 200);

        // Verify position
        let positions = engine.state().get_all_positions_in_market(0).unwrap();
        assert_eq!(positions.len(), 1, "Should recover 1 position");
        assert_eq!(positions[0].0, trader3);
        assert_eq!(positions[0].1.size, 50);

        // Verify market list
        let markets = engine.state().list_all_markets().unwrap();
        assert_eq!(markets.len(), 1);
        assert_eq!(markets[0], 0);

        println!("✅ Phase 2: Full recovery successful!");
        println!("  - Orderbook: 2 orders recovered");
        println!("  - Positions: 1 position recovered");
        println!("  - Markets: 1 market discovered");

        // Phase 3: Continue trading after recovery
        let tx3 = Transaction::new_raw(
            1, // nonce = 1 (trader1's second tx)
            trader1,
            TxPayload::PlaceOrder(PlaceOrderTx {
                market_id: 0,
                is_buy: true,
                order_type: OrderType::Limit,
                price: 50_000,
                size: 50,
                time_in_force: TimeInForce::GTC,
                reduce_only: false,
                post_only: false,
            }),
        );

        if let TxPayload::PlaceOrder(ref order) = tx3.payload {
            let order_id3 = engine.process_place_order(tx3.from, order).unwrap();
            println!("✅ Phase 3: Placed new order #{} after recovery", order_id3);
        }

        // Verify orderbook now has 3 orders
        let active_orders = engine.state().get_active_orders_by_market(0).unwrap();
        assert_eq!(
            active_orders.len(),
            3,
            "Should have 3 orders after placing new one"
        );
    }
}

#[test]
fn test_recovery_with_multiple_markets() {
    let temp_dir = tempfile::TempDir::new().unwrap();

    // Create and populate
    {
        let state = StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();
        let mut engine = Engine::new(state);

        // Create 3 markets
        for market_id in [0, 1, 2] {
            let mut market = create_test_market();
            market.id = market_id;
            market.symbol = format!("MARKET-{}", market_id);
            engine.state_mut().set_market(market_id, market).unwrap();
        }

        // Place orders in each market
        let trader = Address::from([1u8; 20]);
        engine
            .state_mut()
            .set_balance(trader, 0, 100_000_000)
            .unwrap();

        for market_id in [0, 1, 2] {
            let tx = Transaction::new_raw(
                market_id as u64,
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

            if let TxPayload::PlaceOrder(ref order) = tx.payload {
                engine.process_place_order(tx.from, order).unwrap();
            }
        }

        engine.state_mut().begin_block(1);
        engine.state_mut().commit().unwrap();

        println!("✅ Created 3 markets with 1 order each");
    }

    // Restart and verify
    {
        let state = StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();
        let mut engine = Engine::new(state);

        // Rebuild
        engine.state_mut().rebuild_position_index().unwrap();
        engine.rebuild_orderbook_from_state().unwrap();

        // Verify all markets are discovered
        let markets = engine.state().list_all_markets().unwrap();
        assert_eq!(markets.len(), 3);

        // Verify each market has 1 active order
        for market_id in [0, 1, 2] {
            let active_orders = engine
                .state()
                .get_active_orders_by_market(market_id)
                .unwrap();
            assert_eq!(
                active_orders.len(),
                1,
                "Market {} should have 1 order",
                market_id
            );
        }

        println!("✅ All markets and orders recovered successfully");
    }
}
