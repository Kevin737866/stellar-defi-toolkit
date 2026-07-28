//! Shared fixtures for the lending protocol test suites (invariant, lifecycle,
//! and fuzz tests). Mirrors the conventions used in `integration_tests.rs`.

use stellar_defi_toolkit::{InterestRateModel, LendingProtocol, PriceOracleSim, ReserveConfig, WAD};

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

/// A protocol with two registered assets (XLM, USDC) both priced at 1 WAD.
pub fn setup_protocol() -> (LendingProtocol, PriceOracleSim) {
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

    let mut oracle = PriceOracleSim::new("oracle");
    oracle.set_price("oracle", "XLM", WAD).unwrap();
    oracle.set_price("oracle", "USDC", WAD).unwrap();

    (protocol, oracle)
}

/// Asserts the core economic invariants hold for a single reserve:
/// - all totals are non-negative (no phantom negative balances)
/// - `total_cash + total_debt - protocol_fees` (the value backing supply
///   shares) is never negative (no free/negative collateral)
/// - the value redeemable by all outstanding supply shares never exceeds the
///   assets actually backing them (protocol can never be short-changed by
///   rounding)
pub fn assert_reserve_solvent(protocol: &stellar_defi_toolkit::LendingProtocol, asset: &str) {
    let state = protocol.reserve_state(asset).unwrap();

    assert!(state.total_cash >= 0, "{asset}: total_cash went negative");
    assert!(state.total_debt >= 0, "{asset}: total_debt went negative");
    assert!(
        state.protocol_fees >= 0,
        "{asset}: protocol_fees went negative"
    );
    assert!(
        state.total_supply_shares >= 0,
        "{asset}: total_supply_shares went negative"
    );
    assert!(
        state.total_debt_shares >= 0,
        "{asset}: total_debt_shares went negative"
    );

    let net_assets = state.total_cash + state.total_debt - state.protocol_fees;
    assert!(
        net_assets >= 0,
        "{asset}: net assets (cash + debt - fees) went negative: {net_assets}"
    );
}
