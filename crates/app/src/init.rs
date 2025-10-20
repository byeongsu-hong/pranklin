use alloy_primitives::Address;
use anyhow::Result;
use pranklin_state::{Asset, StateManager};

/// Initialize default assets in the system
#[allow(dead_code)]
pub fn initialize_default_assets(state: &mut StateManager) -> Result<()> {
    tracing::info!("📦 Initializing default assets...");

    // Asset ID 0: USDC (primary collateral)
    let usdc = Asset {
        id: 0,
        symbol: "USDC".to_string(),
        name: "USD Coin".to_string(),
        decimals: 6,
        is_collateral: true,
        collateral_weight_bps: 10000, // 100% - stable coin
    };
    state.set_asset(0, usdc)?;
    tracing::info!("  ✓ Asset 0: USDC (100% collateral weight)");

    // Asset ID 1: USDT (secondary collateral)
    let usdt = Asset {
        id: 1,
        symbol: "USDT".to_string(),
        name: "Tether USD".to_string(),
        decimals: 6,
        is_collateral: true,
        collateral_weight_bps: 9800, // 98% - slight haircut for risk
    };
    state.set_asset(1, usdt)?;
    tracing::info!("  ✓ Asset 1: USDT (98% collateral weight)");

    // Asset ID 2: DAI (tertiary collateral)
    let dai = Asset {
        id: 2,
        symbol: "DAI".to_string(),
        name: "Dai Stablecoin".to_string(),
        decimals: 18,
        is_collateral: true,
        collateral_weight_bps: 9500, // 95% - decentralized stablecoin
    };
    state.set_asset(2, dai)?;
    tracing::info!("  ✓ Asset 2: DAI (95% collateral weight)");

    tracing::info!("✅ Default assets initialized");
    Ok(())
}

/// Initialize bridge operators
///
/// In production, these addresses should be:
/// 1. Multi-sig wallets controlled by validators
/// 2. Hardware security modules (HSMs)
/// 3. Distributed key generation (DKG) systems
#[allow(dead_code)]
pub fn initialize_bridge_operators(state: &mut StateManager, operators: &[Address]) -> Result<()> {
    tracing::info!("🌉 Initializing bridge operators...");

    for (i, operator) in operators.iter().enumerate() {
        state.set_bridge_operator(*operator, true)?;
        tracing::info!("  ✓ Operator {}: {:?}", i + 1, operator);
    }

    tracing::info!("✅ {} bridge operator(s) authorized", operators.len());
    Ok(())
}

/// Initialize a test environment with default assets and a test bridge operator
#[cfg(feature = "testing")]
pub fn initialize_test_environment(state: &mut StateManager) -> Result<Address> {
    // Initialize default assets
    initialize_default_assets(state)?;

    // Create a test bridge operator
    let test_operator = Address::from([1u8; 20]);
    initialize_bridge_operators(state, &[test_operator])?;

    Ok(test_operator)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pranklin_state::PruningConfig;
    use tempfile::TempDir;

    #[test]
    fn test_initialize_assets() {
        let temp_dir = TempDir::new().unwrap();
        let mut state = StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();

        initialize_default_assets(&mut state).unwrap();

        // Check USDC
        let usdc = state.get_asset(0).unwrap().unwrap();
        assert_eq!(usdc.symbol, "USDC");
        assert_eq!(usdc.decimals, 6);
        assert!(usdc.is_collateral);

        // Check USDT
        let usdt = state.get_asset(1).unwrap().unwrap();
        assert_eq!(usdt.symbol, "USDT");

        // Check asset list
        let assets = state.list_all_assets().unwrap();
        assert_eq!(assets.len(), 3);
    }

    #[test]
    fn test_initialize_operators() {
        let temp_dir = TempDir::new().unwrap();
        let mut state = StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();

        let operators = vec![Address::from([1u8; 20]), Address::from([2u8; 20])];

        initialize_bridge_operators(&mut state, &operators).unwrap();

        // Check operators
        assert!(state.is_bridge_operator(operators[0]).unwrap());
        assert!(state.is_bridge_operator(operators[1]).unwrap());

        // Check non-operator
        let non_operator = Address::from([3u8; 20]);
        assert!(!state.is_bridge_operator(non_operator).unwrap());
    }
}
