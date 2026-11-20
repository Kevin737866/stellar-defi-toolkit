//! Tests for the shared debt pool for synthetic assets (#156).

use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env, Symbol};

use stellar_defi_toolkit::contracts::synthetic_protocol::{
    SyntheticProtocolContract, SyntheticProtocolContractClient,
};
use stellar_defi_toolkit::types::synthetic::{AssetType, SyntheticAsset};

const BASE_TIME: u64 = 1_700_000_000;
/// Synthetic unit, matching the 6 decimals used by the mock oracle.
const UNIT: u64 = 1_000_000;
/// `get_asset_price` quotes every asset at $100.00 with 6 decimals.
const PRICE: u64 = 100;

/// Base-currency value of `units` synthetic tokens at the mock oracle price.
fn usd(units: u64) -> u64 {
    units * PRICE
}

struct Fixture<'a> {
    env: Env,
    admin: Address,
    oracle: Address,
    collateral: Address,
    client: SyntheticProtocolContractClient<'a>,
}

fn setup() -> Fixture<'static> {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(BASE_TIME);

    let contract_id = env.register_contract(None, SyntheticProtocolContract);
    let client = SyntheticProtocolContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let collateral = Address::generate(&env);
    client.initialize(&admin);

    Fixture {
        env,
        admin,
        oracle,
        collateral,
        client,
    }
}

fn asset(fixture: &Fixture, symbol: &str) -> SyntheticAsset {
    SyntheticAsset {
        asset_id: 0,
        symbol: Symbol::new(&fixture.env, symbol),
        name: Symbol::new(&fixture.env, "Synthetic"),
        description: Symbol::new(&fixture.env, "Test"),
        asset_type: AssetType::Stock,
        oracle_address: fixture.oracle.clone(),
        min_collateral_ratio: 15_000,
        max_collateral_ratio: 100_000,
        minting_fee_bps: 0,
        active: true,
        listed_at: BASE_TIME,
        total_supply: 0,
        total_collateral: 0,
    }
}

/// List one asset and return its id.
fn list(fixture: &Fixture, symbol: &str) -> u32 {
    fixture
        .client
        .list_asset(&fixture.admin, &asset(fixture, symbol))
}

/// Over-collateralised mint: 3x the synthetic amount in collateral.
fn mint(fixture: &Fixture, user: &Address, asset_id: u32, amount: u64) {
    fixture.client.mint_synthetic(
        user,
        &asset_id,
        &fixture.collateral,
        &(amount * 3),
        &amount,
    );
}

/// Repay `amount` of synthetic debt and release the matching collateral.
fn burn(fixture: &Fixture, user: &Address, asset_id: u32, amount: u64) {
    fixture.client.burn_synthetic(
        user,
        &asset_id,
        &amount,
        &fixture.collateral,
        &(amount * 3),
    );
}

// ─── Pool lifecycle ──────────────────────────────────────────────────────────

#[test]
fn pool_starts_empty_and_balanced() {
    let f = setup();

    let pool = f.client.get_debt_pool();
    assert_eq!(pool.total_debt, 0);
    assert_eq!(pool.settled_debt, 0);
    assert_eq!(pool.total_synth_supply, 0);
    assert_eq!(pool.active_positions, 0);
}

#[test]
fn minting_adds_debt_to_the_pool() {
    let f = setup();
    let asset_id = list(&f, "sAAPL");
    let user = Address::generate(&f.env);

    mint(&f, &user, asset_id, 1_000 * UNIT);

    let pool = f.client.get_debt_pool();
    assert_eq!(pool.total_debt, usd(1_000 * UNIT));
    assert_eq!(pool.total_synth_supply, 1_000 * UNIT);
    assert_eq!(pool.active_positions, 1);

    let share = f.client.get_debt_share(&user, &asset_id).unwrap();
    assert_eq!(share.base_debt, usd(1_000 * UNIT));
    assert_eq!(share.share_bps, 10_000, "sole position owns the pool");
}

#[test]
fn repeated_mints_accumulate_for_the_same_owner() {
    let f = setup();
    let asset_id = list(&f, "sAAPL");
    let user = Address::generate(&f.env);

    mint(&f, &user, asset_id, 400 * UNIT);
    mint(&f, &user, asset_id, 600 * UNIT);

    let pool = f.client.get_debt_pool();
    assert_eq!(pool.total_debt, usd(1_000 * UNIT));
    assert_eq!(pool.active_positions, 1, "one owner, one share");

    let share = f.client.get_debt_share(&user, &asset_id).unwrap();
    assert_eq!(share.base_debt, usd(1_000 * UNIT));
}

#[test]
fn multiple_positions_split_the_pool() {
    let f = setup();
    let asset_id = list(&f, "sAAPL");
    let alice = Address::generate(&f.env);
    let bob = Address::generate(&f.env);

    mint(&f, &alice, asset_id, 750 * UNIT);
    mint(&f, &bob, asset_id, 250 * UNIT);

    let pool = f.client.get_debt_pool();
    assert_eq!(pool.total_debt, usd(1_000 * UNIT));
    assert_eq!(pool.active_positions, 2);

    assert_eq!(f.client.debt_share_bps(&alice, &asset_id), 7_500);
    assert_eq!(f.client.debt_share_bps(&bob, &asset_id), 2_500);
    assert_eq!(
        f.client.debt_share_bps(&alice, &asset_id) + f.client.debt_share_bps(&bob, &asset_id),
        10_000
    );
}

#[test]
fn burning_removes_debt_from_the_pool() {
    let f = setup();
    let asset_id = list(&f, "sAAPL");
    let alice = Address::generate(&f.env);
    let bob = Address::generate(&f.env);
    mint(&f, &alice, asset_id, 600 * UNIT);
    mint(&f, &bob, asset_id, 400 * UNIT);

    burn(&f, &bob, asset_id, 400 * UNIT);

    let pool = f.client.get_debt_pool();
    assert_eq!(pool.total_debt, usd(600 * UNIT));
    assert_eq!(pool.active_positions, 1);
    assert_eq!(f.client.get_debt_share(&bob, &asset_id), None);
    assert_eq!(f.client.debt_share_bps(&alice, &asset_id), 10_000);
}

#[test]
fn full_repayment_clears_the_position_share() {
    let f = setup();
    let asset_id = list(&f, "sAAPL");
    let user = Address::generate(&f.env);
    mint(&f, &user, asset_id, 100 * UNIT);

    burn(&f, &user, asset_id, 100 * UNIT);

    let pool = f.client.get_debt_pool();
    assert_eq!(pool.total_debt, 0);
    assert_eq!(pool.total_synth_supply, 0);
    assert_eq!(pool.active_positions, 0);
    assert_eq!(f.client.get_debt_share(&user, &asset_id), None);
    assert_eq!(f.client.debt_share_bps(&user, &asset_id), 0);
}

#[test]
fn debt_index_tracks_debt_per_synth_unit() {
    let f = setup();
    let asset_id = list(&f, "sAAPL");
    let user = Address::generate(&f.env);

    mint(&f, &user, asset_id, 100 * UNIT);

    let pool = f.client.get_debt_pool();
    // The index is debt per synth unit scaled by DEBT_INDEX_SCALE, so it tracks
    // the oracle price.
    assert_eq!(pool.debt_index, PRICE * 1_000_000);
}

#[test]
fn unknown_position_reports_no_share() {
    let f = setup();
    let asset_id = list(&f, "sAAPL");
    let stranger = Address::generate(&f.env);

    assert_eq!(f.client.get_debt_share(&stranger, &asset_id), None);
    assert_eq!(f.client.debt_share_bps(&stranger, &asset_id), 0);
}

// ─── Revaluation ─────────────────────────────────────────────────────────────

#[test]
fn price_update_revalues_outstanding_debt() {
    let f = setup();
    let asset_id = list(&f, "sAAPL");
    let user = Address::generate(&f.env);
    mint(&f, &user, asset_id, 1_000 * UNIT);

    let before = f.client.get_debt_pool();
    assert_eq!(before.total_debt, usd(1_000 * UNIT));

    // Quote the asset at $1.00 instead of $100.00.
    f.client
        .update_oracle_price(&f.oracle, &asset_id, &UNIT, &9_500);

    let after = f.client.get_debt_pool();
    assert_eq!(after.total_debt, 1_000 * UNIT, "debt follows the new price");
    assert_eq!(after.debt_index, 1_000_000);
    assert_eq!(
        f.client.get_debt_share(&user, &asset_id).unwrap().base_debt,
        1_000 * UNIT
    );
}

#[test]
fn revaluation_refreshes_position_shares() {
    let f = setup();
    let asset_id = list(&f, "sAAPL");
    let alice = Address::generate(&f.env);
    let bob = Address::generate(&f.env);
    mint(&f, &alice, asset_id, 800 * UNIT);
    mint(&f, &bob, asset_id, 200 * UNIT);

    f.client
        .update_oracle_price(&f.oracle, &asset_id, &UNIT, &9_500);

    assert_eq!(f.client.debt_share_bps(&alice, &asset_id), 8_000);
    assert_eq!(f.client.debt_share_bps(&bob, &asset_id), 2_000);
}

#[test]
fn price_update_requires_authorized_oracle() {
    let f = setup();
    let asset_id = list(&f, "sAAPL");
    let rogue = Address::generate(&f.env);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client
            .update_oracle_price(&rogue, &asset_id, &UNIT, &9_500);
    }));

    assert!(result.is_err());
}

// ─── Settlement snapshots ────────────────────────────────────────────────────

#[test]
fn snapshot_freezes_settled_debt() {
    let f = setup();
    let asset_id = list(&f, "sAAPL");
    let user = Address::generate(&f.env);
    mint(&f, &user, asset_id, 1_000 * UNIT);

    let snapshot = f.client.take_debt_snapshot(&f.admin);
    assert_eq!(snapshot.id, 1);
    assert_eq!(snapshot.total_debt, usd(1_000 * UNIT));
    assert_eq!(snapshot.total_synth_supply, 1_000 * UNIT);
    assert_eq!(snapshot.timestamp, BASE_TIME);

    let pool = f.client.get_debt_pool();
    assert_eq!(pool.settled_debt, usd(1_000 * UNIT));
}

#[test]
fn snapshot_ids_increment() {
    let f = setup();
    let asset_id = list(&f, "sAAPL");
    let user = Address::generate(&f.env);
    mint(&f, &user, asset_id, 100 * UNIT);

    assert_eq!(f.client.take_debt_snapshot(&f.admin).id, 1);
    assert_eq!(f.client.take_debt_snapshot(&f.admin).id, 2);
}

#[test]
fn settled_debt_lags_new_debt() {
    let f = setup();
    let asset_id = list(&f, "sAAPL");
    let user = Address::generate(&f.env);
    mint(&f, &user, asset_id, 100 * UNIT);
    f.client.take_debt_snapshot(&f.admin);

    mint(&f, &user, asset_id, 50 * UNIT);

    let pool = f.client.get_debt_pool();
    assert_eq!(pool.total_debt, usd(150 * UNIT));
    assert_eq!(pool.settled_debt, usd(100 * UNIT), "snapshot is a fixed point");
}

#[test]
fn snapshot_requires_admin() {
    let f = setup();
    let impostor = Address::generate(&f.env);
    f.env.set_auths(&[]);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client.take_debt_snapshot(&impostor);
    }));

    assert!(result.is_err());
}

// ─── Liquidation interaction ─────────────────────────────────────────────────

#[test]
fn liquidation_removes_position_debt() {
    let f = setup();
    let asset_id = list(&f, "sAAPL");
    let user = Address::generate(&f.env);
    let liquidator = Address::generate(&f.env);

    // 1_000 units of debt against 1_100 units of collateral: a 110% ratio,
    // below the 120% liquidation threshold.
    f.client.mint_synthetic(
        &user,
        &asset_id,
        &f.collateral,
        &(1_100 * UNIT),
        &(1_000 * UNIT),
    );
    assert_eq!(f.client.debt_share_bps(&user, &asset_id), 10_000);

    f.client.liquidate_position(&liquidator, &user, &asset_id);

    let pool = f.client.get_debt_pool();
    assert_eq!(pool.total_debt, 0);
    assert_eq!(pool.active_positions, 0);
    assert_eq!(f.client.get_debt_share(&user, &asset_id), None);
}

#[test]
fn healthy_position_cannot_be_liquidated() {
    let f = setup();
    let asset_id = list(&f, "sAAPL");
    let user = Address::generate(&f.env);
    let liquidator = Address::generate(&f.env);
    mint(&f, &user, asset_id, 1_000 * UNIT);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client.liquidate_position(&liquidator, &user, &asset_id);
    }));

    assert!(result.is_err());
    assert_eq!(f.client.get_debt_pool().total_debt, usd(1_000 * UNIT));
}

// ─── Cross-asset accounting ──────────────────────────────────────────────────

#[test]
fn debt_pool_spans_multiple_assets() {
    let f = setup();
    let first = list(&f, "sAAPL");
    let second = list(&f, "sTSLA");
    let alice = Address::generate(&f.env);
    let bob = Address::generate(&f.env);

    mint(&f, &alice, first, 300 * UNIT);
    mint(&f, &bob, second, 200 * UNIT);

    let pool = f.client.get_debt_pool();
    assert_eq!(pool.total_debt, usd(500 * UNIT));
    assert_eq!(pool.active_positions, 2);

    assert_eq!(f.client.debt_share_bps(&alice, &first), 6_000);
    assert_eq!(f.client.debt_share_bps(&bob, &second), 4_000);
    assert_eq!(f.client.debt_share_bps(&alice, &second), 0);
}

#[test]
fn one_asset_price_update_preserves_other_asset_debt() {
    let f = setup();
    let first = list(&f, "sAAPL");
    let second = list(&f, "sTSLA");
    let alice = Address::generate(&f.env);
    let bob = Address::generate(&f.env);
    mint(&f, &alice, first, 300 * UNIT);
    mint(&f, &bob, second, 200 * UNIT);

    // `first` is re-quoted at $1.00; `second` keeps its $100.00 quote.
    f.client
        .update_oracle_price(&f.oracle, &first, &UNIT, &9_500);

    let pool = f.client.get_debt_pool();
    assert_eq!(
        pool.total_debt,
        300 * UNIT + usd(200 * UNIT),
        "revaluing one asset must not drop the other"
    );
    // 300 / 20_300 and 20_000 / 20_300 in basis points.
    assert_eq!(f.client.debt_share_bps(&alice, &first), 147);
    assert_eq!(f.client.debt_share_bps(&bob, &second), 9_852);
}
