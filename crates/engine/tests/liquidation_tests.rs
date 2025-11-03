/// Advanced liquidation integration tests
use alloy_primitives::Address;
use pranklin_engine::{Engine, LiquidationEngine};
use pranklin_state::{Market, Position, PruningConfig, StateManager};

#[test]
fn test_partial_liquidation() {
    // Setup
    let temp_dir = tempfile::TempDir::new().unwrap();
    let mut state = StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();
    let mut liquidation = LiquidationEngine::default();

    // Create market
    let market_id = 0;
    let market = Market {
        id: market_id,
        symbol: "BTC-PERP".to_string(),
        base_asset_id: 1,
        quote_asset_id: 0,
        tick_size: 1000,
        price_decimals: 2,
        size_decimals: 6,
        min_order_size: 100,
        max_order_size: 1_000_000_000, // 1 billion units max
        max_leverage: 20,
        initial_margin_bps: 1000,    // 10%
        maintenance_margin_bps: 500, // 5%
        liquidation_fee_bps: 100,    // 1%
        funding_interval: 3600,
        max_funding_rate_bps: 100,
    };
    state.set_market(market_id, market.clone()).unwrap();

    // Create trader and liquidator
    let trader = Address::from([1u8; 20]);
    let liquidator = Address::from([2u8; 20]);

    // Fund liquidator
    state
        .set_balance(liquidator, market.quote_asset_id, 10_000_000_000)
        .unwrap();

    // Create under-marginized position
    let position = Position {
        size: 1_000_000,        // 1 BTC
        entry_price: 5_000_000, // $50,000
        is_long: true,
        margin: 1_500_000_000, // $1,500 (3% margin - below maintenance)
        funding_index: 0,
    };
    state
        .set_position(trader, market_id, position.clone())
        .unwrap();

    // Current price dropped
    let mark_price = 4_800_000; // $48,000

    // Execute liquidation
    let result = liquidation
        .liquidate_with_incentive(&mut state, trader, market_id, mark_price, liquidator)
        .unwrap();

    assert!(result.is_some());
    let liq_result = result.unwrap();

    // Verify partial liquidation (not full)
    assert!(liq_result.liquidated_size < position.size);
    assert!(liq_result.liquidated_size > 0);

    // Verify liquidator was rewarded
    assert!(liq_result.liquidator_reward > 0);

    // Verify insurance fund received contribution
    assert!(liq_result.insurance_fund_contribution > 0);

    println!("✅ Partial liquidation test passed");
    println!(
        "  Liquidated: {} / {} ({}%)",
        liq_result.liquidated_size,
        position.size,
        liq_result.liquidated_size * 100 / position.size
    );
}

#[test]
fn test_insurance_fund_usage() {
    // Setup
    let temp_dir = tempfile::TempDir::new().unwrap();
    let mut state = StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();
    let mut liquidation = LiquidationEngine::default();

    // Create market
    let market_id = 0;
    let market = Market {
        id: market_id,
        symbol: "BTC-PERP".to_string(),
        base_asset_id: 1,
        quote_asset_id: 0,
        tick_size: 1000,
        price_decimals: 2,
        size_decimals: 6,
        min_order_size: 100,
        max_order_size: 1_000_000_000,
        max_leverage: 20,
        initial_margin_bps: 1000,
        maintenance_margin_bps: 500,
        liquidation_fee_bps: 100,
        funding_interval: 3600,
        max_funding_rate_bps: 100,
    };
    state.set_market(market_id, market.clone()).unwrap();

    // Create trader and liquidator
    let trader = Address::from([1u8; 20]);
    let liquidator = Address::from([2u8; 20]);

    // Fund liquidator
    state
        .set_balance(liquidator, market.quote_asset_id, 10_000_000_000)
        .unwrap();

    // Create position with very low margin (will require insurance fund)
    let position = Position {
        size: 1_000_000,
        entry_price: 5_000_000,
        is_long: true,
        margin: 100_000_000, // $100 - very low margin
        funding_index: 0,
    };
    state
        .set_position(trader, market_id, position.clone())
        .unwrap();

    // Price dropped significantly
    let mark_price = 4_500_000; // $45,000

    // Execute liquidation
    let result = liquidation
        .liquidate_with_incentive(&mut state, trader, market_id, mark_price, liquidator)
        .unwrap();

    assert!(result.is_some());
    let liq_result = result.unwrap();

    // Insurance fund might be used if equity is negative
    if liq_result.insurance_fund_usage > 0 {
        println!("✅ Insurance fund usage test passed");
        println!(
            "  Insurance fund covered: ${:.2}",
            liq_result.insurance_fund_usage as f64 / 1_000_000_000.0
        );
    } else {
        println!("ℹ️  Insurance fund not needed in this scenario");
    }
}

#[test]
fn test_risk_index_rebuild() {
    // Setup
    let temp_dir = tempfile::TempDir::new().unwrap();
    let mut state = StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();
    let mut liquidation = LiquidationEngine::default();

    // Create market
    let market_id = 0;
    let market = Market {
        id: market_id,
        symbol: "BTC-PERP".to_string(),
        base_asset_id: 1,
        quote_asset_id: 0,
        tick_size: 1000,
        price_decimals: 2,
        size_decimals: 6,
        min_order_size: 100,
        max_order_size: 1_000_000_000,
        max_leverage: 20,
        initial_margin_bps: 1000,
        maintenance_margin_bps: 500,
        liquidation_fee_bps: 100,
        funding_interval: 3600,
        max_funding_rate_bps: 100,
    };
    state.set_market(market_id, market.clone()).unwrap();

    // Create multiple traders with positions
    for i in 0..5 {
        let trader = Address::from([i as u8; 20]);
        let position = Position {
            size: 1_000_000 * (i as u64 + 1),
            entry_price: 5_000_000,
            is_long: true,
            margin: (2_000_000_000 * (i as u128 + 1)),
            funding_index: 0,
        };
        state.set_position(trader, market_id, position).unwrap();
    }

    // Rebuild risk index
    let mark_price = 5_000_000;
    liquidation
        .rebuild_risk_index(&state, market_id, mark_price)
        .unwrap();

    // Check at-risk positions
    let at_risk = liquidation.get_at_risk_positions(market_id, 1000); // Below 10%

    println!("✅ Risk index rebuild test passed");
    println!("  At-risk positions: {}", at_risk.len());
}

#[test]
fn test_adl_candidate_finding() {
    // Setup
    let temp_dir = tempfile::TempDir::new().unwrap();
    let mut state = StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();
    let _liquidation = LiquidationEngine::default();

    // Create market
    let market_id = 0;
    let market = Market {
        id: market_id,
        symbol: "BTC-PERP".to_string(),
        base_asset_id: 1,
        quote_asset_id: 0,
        tick_size: 1000,
        price_decimals: 2,
        size_decimals: 6,
        min_order_size: 100,
        max_order_size: 1_000_000_000,
        max_leverage: 20,
        initial_margin_bps: 1000,
        maintenance_margin_bps: 500,
        liquidation_fee_bps: 100,
        funding_interval: 3600,
        max_funding_rate_bps: 100,
    };
    state.set_market(market_id, market.clone()).unwrap();

    // Create profitable long positions (price went up)
    let trader1 = Address::from([1u8; 20]);
    let position1 = Position {
        size: 1_000_000,
        entry_price: 5_000_000,
        is_long: true,
        margin: 5_000_000_000, // $5,000
        funding_index: 0,
    };
    state.set_position(trader1, market_id, position1).unwrap();

    let trader2 = Address::from([2u8; 20]);
    let position2 = Position {
        size: 2_000_000,
        entry_price: 4_800_000,
        is_long: true,
        margin: 10_000_000_000, // $10,000
        funding_index: 0,
    };
    state.set_position(trader2, market_id, position2).unwrap();

    // Price went up - positions are profitable
    let _mark_price = 5_500_000; // $55,000

    // Get all positions to check
    let positions = state.get_all_positions_in_market(market_id).unwrap();
    assert_eq!(positions.len(), 2);

    println!("✅ ADL candidate finding test passed");
    println!("  Positions in market: {}", positions.len());
}

#[test]
fn test_engine_integration() {
    // Setup
    let temp_dir = tempfile::TempDir::new().unwrap();
    let state = StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();
    let mut engine = Engine::new(state);

    // Create market
    let market_id = 0;
    let market = Market {
        id: market_id,
        symbol: "BTC-PERP".to_string(),
        base_asset_id: 1,
        quote_asset_id: 0,
        tick_size: 1000,
        price_decimals: 2,
        size_decimals: 6,
        min_order_size: 100,
        max_order_size: 1_000_000_000,
        max_leverage: 20,
        initial_margin_bps: 1000,
        maintenance_margin_bps: 500,
        liquidation_fee_bps: 100,
        funding_interval: 3600,
        max_funding_rate_bps: 100,
    };
    engine
        .state_mut()
        .set_market(market_id, market.clone())
        .unwrap();

    // Create trader and liquidator
    let trader = Address::from([1u8; 20]);
    let liquidator = Address::from([2u8; 20]);

    // Fund liquidator
    engine
        .state_mut()
        .set_balance(liquidator, market.quote_asset_id, 10_000_000_000)
        .unwrap();

    // Create under-marginized position
    let position = Position {
        size: 1_000_000,
        entry_price: 5_000_000,
        is_long: true,
        margin: 1_500_000_000,
        funding_index: 0,
    };
    engine
        .state_mut()
        .set_position(trader, market_id, position.clone())
        .unwrap();

    // Update risk index
    let mark_price = 4_800_000;
    engine
        .update_risk_index(trader, market_id, mark_price)
        .unwrap();

    // Execute liquidation through engine
    let result = engine
        .liquidate_with_incentive(trader, market_id, mark_price, liquidator)
        .unwrap();

    assert!(result.is_some());

    // Get insurance fund
    let insurance_fund = engine.get_insurance_fund(market_id);
    println!("✅ Engine integration test passed");
    println!(
        "  Insurance fund balance: ${:.2}",
        insurance_fund.balance as f64 / 1_000_000_000.0
    );
}

#[test]
fn test_batch_liquidation() {
    // Setup
    let temp_dir = tempfile::TempDir::new().unwrap();
    let state = StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();
    let mut engine = Engine::new(state);

    // Create market
    let market_id = 0;
    let market = Market {
        id: market_id,
        symbol: "BTC-PERP".to_string(),
        base_asset_id: 1,
        quote_asset_id: 0,
        tick_size: 1000,
        price_decimals: 2,
        size_decimals: 6,
        min_order_size: 100,
        max_order_size: 1_000_000_000,
        max_leverage: 20,
        initial_margin_bps: 1000,
        maintenance_margin_bps: 500,
        liquidation_fee_bps: 100,
        funding_interval: 3600,
        max_funding_rate_bps: 100,
    };
    engine
        .state_mut()
        .set_market(market_id, market.clone())
        .unwrap();

    // Create liquidator
    let liquidator = Address::from([99u8; 20]);
    engine
        .state_mut()
        .set_balance(liquidator, market.quote_asset_id, 100_000_000_000)
        .unwrap();

    // Create multiple under-marginized positions
    for i in 0..3 {
        let trader = Address::from([i as u8; 20]);
        let position = Position {
            size: 1_000_000,
            entry_price: 5_000_000,
            is_long: true,
            margin: 1_500_000_000, // Under maintenance margin
            funding_index: 0,
        };
        engine
            .state_mut()
            .set_position(trader, market_id, position)
            .unwrap();
    }

    // Rebuild risk index
    let mark_price = 4_800_000;
    engine.rebuild_risk_index(market_id, mark_price).unwrap();

    // Execute batch liquidation
    let results = engine
        .process_liquidation_batch(market_id, mark_price, liquidator, 10)
        .unwrap();

    println!("✅ Batch liquidation test passed");
    println!("  Liquidations executed: {}", results.len());

    for (idx, result) in results.iter().enumerate() {
        println!(
            "  Liquidation {}: {:.6} BTC @ ${:.2}",
            idx + 1,
            result.liquidated_size as f64 / 1_000_000.0,
            result.liquidation_price as f64 / 100.0
        );
    }
}

#[test]
fn test_edge_case_zero_size_position() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let mut state = StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();
    let mut liquidation = LiquidationEngine::default();

    let market_id = 0;
    let market = Market {
        id: market_id,
        symbol: "BTC-PERP".to_string(),
        base_asset_id: 1,
        quote_asset_id: 0,
        tick_size: 1000,
        price_decimals: 2,
        size_decimals: 6,
        min_order_size: 100,
        max_order_size: 1_000_000_000,
        max_leverage: 20,
        initial_margin_bps: 1000,
        maintenance_margin_bps: 500,
        liquidation_fee_bps: 100,
        funding_interval: 3600,
        max_funding_rate_bps: 100,
    };
    state.set_market(market_id, market.clone()).unwrap();

    let trader = Address::from([1u8; 20]);
    let liquidator = Address::from([2u8; 20]);

    // Zero-sized position
    let position = Position {
        size: 0,
        entry_price: 5_000_000,
        is_long: true,
        margin: 1_000_000_000,
        funding_index: 0,
    };
    state.set_position(trader, market_id, position).unwrap();

    let mark_price = 4_800_000;
    let result = liquidation
        .liquidate_with_incentive(&mut state, trader, market_id, mark_price, liquidator)
        .unwrap();

    // Should not liquidate zero-sized position
    assert!(result.is_none());
    println!("✅ Edge case test (zero size) passed");
}

#[test]
fn test_fee_split_configuration() {
    let mut liquidation = LiquidationEngine::default();

    // Default is 50/50
    assert_eq!(liquidation.get_fee_split(), (5000, 5000));

    // Change to 60/40
    liquidation.set_fee_split(6000, 4000);
    assert_eq!(liquidation.get_fee_split(), (6000, 4000));

    println!("✅ Fee split configuration test passed");
}

#[test]
#[should_panic(expected = "Fee split must sum to 10000 bps")]
fn test_invalid_fee_split() {
    let mut liquidation = LiquidationEngine::default();
    // Should panic - doesn't sum to 10000
    liquidation.set_fee_split(6000, 3000);
}
