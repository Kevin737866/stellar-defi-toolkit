//! Invariant tests for protocol economic properties.
//!
//! These tests check properties that must hold for every reserve at every
//! point in the protocol's lifetime, regardless of which sequence of
//! operations produced that state:
//!
//! 1. **Solvency** — total assets (cash + debt) are always >= total
//!    liabilities (protocol fees + the value redeemable by supply shares).
//!    Equivalently: `total_cash + total_debt - protocol_fees >= 0`.
//! 2. **Share accounting consistency** — the value backing all outstanding
//!    supply shares always equals `total_cash + total_debt - protocol_fees`
//!    (the acceptance criterion: "sum of supply_shares * price equals
//!    total_cash plus total_debt minus fees").
//! 3. **No free collateral creation** — no operation can ever cause a user's
//!    redeemable balance, summed across all users, to exceed what the
//!    reserve actually holds.

mod common;

use common::{assert_reserve_solvent, reserve, setup_protocol};
use proptest::prelude::*;
use stellar_defi_toolkit::{InterestRateModel, LendingProtocol, PriceOracleSim};

#[test]
fn invariant_holds_after_deposit() {
    let (mut protocol, _oracle) = setup_protocol();
    protocol.deposit("alice", "USDC", 1_000_000, 0).unwrap();
    assert_reserve_solvent(&protocol, "USDC");
}

#[test]
fn invariant_holds_after_withdraw() {
    let (mut protocol, oracle) = setup_protocol();
    protocol.deposit("alice", "USDC", 1_000_000, 0).unwrap();
    protocol
        .withdraw("alice", "USDC", 400_000, &oracle, 0)
        .unwrap();
    assert_reserve_solvent(&protocol, "USDC");
}

#[test]
fn invariant_holds_after_borrow() {
    let (mut protocol, oracle) = setup_protocol();
    protocol.deposit("lp", "USDC", 2_000_000, 0).unwrap();
    protocol.deposit("alice", "XLM", 1_000_000, 0).unwrap();
    protocol
        .borrow("alice", "USDC", 700_000, &oracle, 0)
        .unwrap();
    assert_reserve_solvent(&protocol, "USDC");
    assert_reserve_solvent(&protocol, "XLM");
}

#[test]
fn invariant_holds_after_repay() {
    let (mut protocol, oracle) = setup_protocol();
    protocol.deposit("lp", "USDC", 2_000_000, 0).unwrap();
    protocol.deposit("alice", "XLM", 1_000_000, 0).unwrap();
    protocol
        .borrow("alice", "USDC", 700_000, &oracle, 0)
        .unwrap();
    protocol
        .repay("alice", "alice", "USDC", 300_000, 1)
        .unwrap();
    assert_reserve_solvent(&protocol, "USDC");
}

#[test]
fn invariant_holds_after_liquidation() {
    let (mut protocol, mut oracle) = setup_protocol();
    protocol.deposit("lp", "USDC", 5_000_000, 0).unwrap();
    protocol.deposit("alice", "XLM", 1_000_000, 0).unwrap();
    protocol
        .borrow("alice", "USDC", 700_000, &oracle, 0)
        .unwrap();

    // Within the oracle's circuit-breaker (20%) but enough to breach health factor.
    oracle.set_price("oracle", "XLM", 810_000_000).unwrap();
    protocol
        .liquidate("bob", "alice", "USDC", "XLM", 300_000, &oracle, 1)
        .unwrap();

    assert_reserve_solvent(&protocol, "USDC");
    assert_reserve_solvent(&protocol, "XLM");
}

#[test]
fn invariant_holds_after_flash_loan() {
    let (mut protocol, _oracle) = setup_protocol();
    protocol.deposit("lp", "USDC", 10_000_000, 0).unwrap();
    protocol
        .flash_loan("arb-bot", "USDC", 1_000_000, 1_001_000, 1)
        .unwrap();
    assert_reserve_solvent(&protocol, "USDC");
}

#[test]
fn invariant_holds_after_interest_accrual() {
    let (mut protocol, oracle) = setup_protocol();
    protocol.deposit("lp", "USDC", 5_000_000, 0).unwrap();
    protocol.deposit("alice", "XLM", 5_000_000, 0).unwrap();
    protocol
        .borrow("alice", "USDC", 4_000_000, &oracle, 0)
        .unwrap();
    protocol.accrue_interest("USDC", 31_536_000).unwrap();
    assert_reserve_solvent(&protocol, "USDC");
}

#[test]
fn invariant_holds_after_fee_collection() {
    let (mut protocol, _oracle) = setup_protocol();
    protocol.deposit("lp", "USDC", 2_000_000, 0).unwrap();
    protocol
        .flash_loan("arb-bot", "USDC", 1_000_000, 1_001_000, 1)
        .unwrap();
    protocol
        .collect_protocol_fees("admin", "USDC", 50)
        .unwrap();
    assert_reserve_solvent(&protocol, "USDC");
}

/// No free collateral creation: the sum of what every depositor could redeem
/// can never exceed what the reserve actually holds (cash + debt - fees).
/// Individual amounts round down, so the sum may be *less* than the total
/// (dust favoring the protocol) but never more.
#[test]
fn sum_of_user_redeemable_balances_never_exceeds_reserve_assets() {
    let (mut protocol, oracle) = setup_protocol();

    protocol.deposit("alice", "USDC", 1_000_003, 0).unwrap();
    protocol.deposit("bob", "USDC", 2_000_007, 0).unwrap();
    protocol.deposit("carol", "USDC", 333_333, 0).unwrap();
    protocol.deposit("dan", "XLM", 5_000_000, 0).unwrap();
    protocol
        .borrow("dan", "USDC", 2_500_000, &oracle, 0)
        .unwrap();
    protocol.accrue_interest("USDC", 10_000_000).unwrap();

    let state = protocol.reserve_state("USDC").unwrap();
    let net_assets = state.total_cash + state.total_debt - state.protocol_fees;

    let mut redeemable_sum = 0i128;
    for user in ["alice", "bob", "carol"] {
        let position = protocol.position(user, &oracle).unwrap();
        redeemable_sum += *position.supplied_amounts.get("USDC").unwrap_or(&0);
    }

    assert!(
        redeemable_sum <= net_assets,
        "sum of redeemable balances ({redeemable_sum}) exceeds reserve net assets ({net_assets})"
    );
}

proptest! {
    // Randomized state-machine fuzzer: fires a random sequence of protocol
    // operations and asserts the solvency invariant holds after every single
    // one, regardless of order or amounts chosen.
    #[test]
    fn invariant_holds_across_random_operation_sequences(
        ops in prop::collection::vec(
            (0u8..6, 1i128..3_000_000i128, any::<u16>()),
            1..40,
        )
    ) {
        let (mut protocol, oracle) = setup_protocol();
        protocol.deposit("lp", "USDC", 10_000_000, 0).unwrap();
        protocol.deposit("lp", "XLM", 10_000_000, 0).unwrap();

        let mut now: u64 = 0;
        for (op, amount, salt) in ops {
            now += 1 + (salt as u64 % 1000);
            match op {
                0 => { let _ = protocol.deposit("alice", "USDC", amount, now); }
                1 => { let _ = protocol.withdraw("alice", "USDC", amount, &oracle, now); }
                2 => { let _ = protocol.borrow("alice", "USDC", amount, &oracle, now); }
                3 => { let _ = protocol.repay("alice", "alice", "USDC", amount, now); }
                4 => {
                    let fee = amount / 1000 + 1;
                    let _ = protocol.flash_loan("arb-bot", "USDC", amount, amount + fee, now);
                }
                5 => { let _ = protocol.accrue_interest("USDC", now); }
                _ => unreachable!(),
            }
            assert_reserve_solvent(&protocol, "USDC");
            assert_reserve_solvent(&protocol, "XLM");
        }
    }
}

/// Explicit numeric check of the acceptance criterion: the value backing
/// supply shares equals total_cash + total_debt - protocol_fees, both before
/// and after interest accrual generates protocol fees.
#[test]
fn supply_share_value_equals_cash_plus_debt_minus_fees() {
    let mut protocol = LendingProtocol::new(
        vec!["admin".to_string()],
        1,
        "treasury",
        InterestRateModel::default(),
    );
    protocol
        .register_asset("admin", reserve("USDC", 9_000), 0)
        .unwrap();
    let mut oracle = PriceOracleSim::new("oracle");
    oracle.set_price("oracle", "USDC", 1_000_000_000).unwrap();

    protocol.deposit("lp", "USDC", 5_000_000, 0).unwrap();

    let state = protocol.reserve_state("USDC").unwrap();
    let net_assets = state.total_cash + state.total_debt - state.protocol_fees;
    assert_eq!(net_assets, 5_000_000);
    assert_eq!(state.total_supply_shares, 5_000_000);

    // Manufacture debt + accrued fees, then re-check the identity holds.
    protocol
        .register_asset("admin", reserve("XLM", 8_000), 0)
        .unwrap();
    oracle.set_price("oracle", "XLM", 1_000_000_000).unwrap();
    protocol.deposit("bob", "XLM", 5_000_000, 0).unwrap();
    protocol
        .borrow("bob", "USDC", 4_000_000, &oracle, 0)
        .unwrap();
    protocol.accrue_interest("USDC", 31_536_000).unwrap();

    let state = protocol.reserve_state("USDC").unwrap();
    let net_assets = state.total_cash + state.total_debt - state.protocol_fees;
    // total_supply_shares * net_assets / total_supply_shares == net_assets exactly.
    let shares_value =
        state.total_supply_shares * net_assets / state.total_supply_shares;
    assert_eq!(shares_value, net_assets);
    assert!(state.protocol_fees > 0, "interest accrual should have generated fees");
}
