use alloy_primitives::Address;
use anyhow::Result;
use pranklin_state::{Asset, StateManager};

/// Asset configuration for initialization
#[derive(Clone, Copy)]
struct AssetConfig {
    id: u32,
    symbol: &'static str,
    name: &'static str,
    decimals: u8,
    collateral_weight_bps: u32,
}

impl From<AssetConfig> for Asset {
    fn from(config: AssetConfig) -> Self {
        Asset {
            id: config.id,
            symbol: config.symbol.to_string(),
            name: config.name.to_string(),
            decimals: config.decimals,
            is_collateral: true,
            collateral_weight_bps: config.collateral_weight_bps,
        }
    }
}

/// Initialize default assets in the system
#[allow(dead_code)]
pub fn initialize_default_assets(state: &mut StateManager) -> Result<()> {
    tracing::info!("📦 Initializing default assets...");

    const DEFAULT_ASSETS: &[AssetConfig] = &[
        AssetConfig {
            id: 0,
            symbol: "USDC",
            name: "USD Coin",
            decimals: 6,
            collateral_weight_bps: 10000, // 100% - stable coin
        },
        AssetConfig {
            id: 1,
            symbol: "USDT",
            name: "Tether USD",
            decimals: 6,
            collateral_weight_bps: 9800, // 98% - slight haircut for risk
        },
        AssetConfig {
            id: 2,
            symbol: "DAI",
            name: "Dai Stablecoin",
            decimals: 18,
            collateral_weight_bps: 9500, // 95% - decentralized stablecoin
        },
    ];

    for config in DEFAULT_ASSETS {
        let asset = Asset::from(*config);
        state.set_asset(config.id, asset)?;
        tracing::info!(
            "  ✓ Asset {}: {} ({}% collateral weight)",
            config.id,
            config.symbol,
            config.collateral_weight_bps / 100
        );
    }

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

    operators
        .iter()
        .enumerate()
        .try_for_each(|(i, &operator)| -> Result<()> {
            state.set_bridge_operator(operator, true)?;
            tracing::info!("  ✓ Operator {}: {:?}", i + 1, operator);
            Ok(())
        })?;

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

    fn create_test_state() -> (TempDir, StateManager) {
        let temp_dir = TempDir::new().unwrap();
        let state = StateManager::new(temp_dir.path(), PruningConfig::default()).unwrap();
        (temp_dir, state)
    }

    #[test]
    fn test_initialize_assets() {
        let (_temp_dir, mut state) = create_test_state();
        initialize_default_assets(&mut state).unwrap();

        let expected_assets = [("USDC", 6), ("USDT", 6), ("DAI", 18)];

        for (id, (symbol, decimals)) in expected_assets.iter().enumerate() {
            let asset = state.get_asset(id as u32).unwrap().unwrap();
            assert_eq!(asset.symbol, *symbol);
            assert_eq!(asset.decimals, *decimals);
            assert!(asset.is_collateral);
        }

        assert_eq!(state.list_all_assets().unwrap().len(), 3);
    }

    #[test]
    fn test_initialize_operators() {
        let (_temp_dir, mut state) = create_test_state();
        let operators = vec![Address::from([1u8; 20]), Address::from([2u8; 20])];

        initialize_bridge_operators(&mut state, &operators).unwrap();

        operators
            .iter()
            .for_each(|&op| assert!(state.is_bridge_operator(op).unwrap()));

        let non_operator = Address::from([3u8; 20]);
        assert!(!state.is_bridge_operator(non_operator).unwrap());
    }
}
