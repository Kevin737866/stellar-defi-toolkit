//! Shared test helpers for deterministic oracle-driven testing.
//!
//! Every integration/stress/benchmark test that needs a price feed should
//! build on [`stellar_defi_toolkit::MockOracle`] rather than hand-rolling
//! price bookkeeping — it gives every test file the same programmable price
//! source (set any asset's price, script trend/spike/crash scenarios, and
//! simulate staleness) without depending on wall-clock time.

use stellar_defi_toolkit::{InterestRateModel, LendingProtocol, MockOracle, ReserveConfig};

/// Build a `ReserveConfig` with sensible defaults for a given
/// `collateral_factor_bps`, matching the fixtures used across the test suite.
pub fn reserve(asset: &str, collateral_factor_bps: u32) -> ReserveConfig {
    ReserveConfig {
        asset: asset.to_string(),
        decimals: 7,
        collateral_factor_bps,
        liquidation_threshold_bps: collateral_factor_bps + 500,
        liquidation_bonus_bps: 1_000,
        reserve_factor_bps: 1_000,
        flash_loan_fee_bps: 9,
        borrow_enabled: true,
        deposit_enabled: true,
        flash_loan_enabled: true,
        supply_cap: 0,
        borrow_cap: 0,
        interest_rate_model: None,
    }
}

/// A `LendingProtocol` with XLM and USDC registered, paired with a
/// `MockOracle` primed at $1.00 for both assets — the standard starting point
/// for oracle-driven scenario tests.
pub fn setup_protocol_with_mock_oracle() -> (LendingProtocol, MockOracle) {
    let mut protocol = LendingProtocol::new(
        vec!["admin".to_string()],
        1,
        "treasury",
        InterestRateModel::default(),
    );
    protocol
        .register_asset("admin", reserve("XLM", 8_000), 0)
        .unwrap();
    protocol
        .register_asset("admin", reserve("USDC", 9_000), 0)
        .unwrap();

    let mut oracle = MockOracle::new("oracle");
    oracle
        .set_price("oracle", "XLM", stellar_defi_toolkit::WAD)
        .unwrap();
    oracle
        .set_price("oracle", "USDC", stellar_defi_toolkit::WAD)
        .unwrap();

    (protocol, oracle)
}
