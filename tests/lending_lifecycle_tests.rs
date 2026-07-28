//! End-to-end integration tests covering the full lending lifecycle across
//! multiple users: deposit, borrow, interest accrual, liquidation, withdrawal.

mod common;

use common::{assert_reserve_solvent, reserve, setup_protocol};
use stellar_defi_toolkit::{InterestRateModel, LendingProtocol, PriceOracleSim, WAD};

/// Full lifecycle with two liquidity providers and two borrowers:
/// deposits -> borrows -> interest accrual -> partial repay -> a liquidation
/// -> final withdrawals. Solvency is checked at every step.
#[test]
fn full_lending_lifecycle_with_multiple_users() {
    let (mut protocol, mut oracle) = setup_protocol();

    // ── Two suppliers provide liquidity ──────────────────────────────────
    protocol.deposit("lp1", "USDC", 3_000_000, 0).unwrap();
    protocol.deposit("lp2", "USDC", 2_000_000, 0).unwrap();
    assert_reserve_solvent(&protocol, "USDC");

    // ── Two borrowers post collateral and borrow ─────────────────────────
    protocol.deposit("alice", "XLM", 2_000_000, 0).unwrap();
    protocol.deposit("carol", "XLM", 1_000_000, 0).unwrap();

    protocol
        .borrow("alice", "USDC", 1_000_000, &oracle, 0)
        .unwrap();
    protocol
        .borrow("carol", "USDC", 750_000, &oracle, 0)
        .unwrap();
    assert_reserve_solvent(&protocol, "USDC");
    assert_reserve_solvent(&protocol, "XLM");

    let alice_position = protocol.position("alice", &oracle).unwrap();
    assert_eq!(alice_position.debt_amounts["USDC"], 1_000_000);
    assert!(alice_position.health_factor >= WAD);

    // ── Interest accrues over a year ─────────────────────────────────────
    let one_year = 31_536_000;
    protocol.accrue_interest("USDC", one_year).unwrap();
    let after_accrual = protocol.reserve_state("USDC").unwrap().clone();
    assert!(after_accrual.total_debt > 1_500_000);
    assert!(after_accrual.protocol_fees > 0);
    assert_reserve_solvent(&protocol, "USDC");

    // ── Alice partially repays ────────────────────────────────────────────
    let alice_owed_before = protocol.position("alice", &oracle).unwrap().debt_amounts["USDC"];
    protocol
        .repay("alice", "alice", "USDC", 300_000, one_year)
        .unwrap();
    let alice_owed_after = protocol.position("alice", &oracle).unwrap().debt_amounts["USDC"];
    assert!(alice_owed_after < alice_owed_before);
    assert_reserve_solvent(&protocol, "USDC");

    // ── XLM price drops, Carol becomes liquidatable ──────────────────────
    oracle.set_price("oracle", "XLM", 810_000_000).unwrap();
    let carol_position = protocol.position("carol", &oracle).unwrap();
    assert!(carol_position.health_factor < WAD);

    let liquidation = protocol
        .liquidate("bob", "carol", "USDC", "XLM", 200_000, &oracle, one_year)
        .unwrap();
    assert!(liquidation.repaid_amount > 0);
    assert!(liquidation.seized_collateral > 0);
    assert_reserve_solvent(&protocol, "USDC");
    assert_reserve_solvent(&protocol, "XLM");

    let carol_after = protocol.position("carol", &oracle).unwrap();
    assert!(carol_after.debt_value < carol_position.debt_value);

    // ── Suppliers withdraw what they can (protocol stays solvent) ────────
    // (XLM stays at its post-liquidation price; only USDC is withdrawn here,
    // and Alice's blocked-withdrawal check below doesn't depend on it.)
    let lp1_withdrawn = protocol
        .withdraw("lp1", "USDC", 1_000_000, &oracle, one_year)
        .unwrap();
    assert_eq!(lp1_withdrawn, 1_000_000);
    assert_reserve_solvent(&protocol, "USDC");

    // Alice's remaining debt keeps her collateral locked; a full collateral
    // withdrawal must fail while debt remains outstanding.
    let alice_remaining_debt = protocol.position("alice", &oracle).unwrap().debt_amounts["USDC"];
    assert!(alice_remaining_debt > 0);
    let err = protocol
        .withdraw("alice", "XLM", 2_000_000, &oracle, one_year)
        .unwrap_err();
    assert!(matches!(
        err,
        stellar_defi_toolkit::ProtocolError::HealthFactorTooLow
    ));
}

/// A borrower who never over-extends should be able to deposit, borrow,
/// repay in full, and withdraw all collateral back out.
#[test]
fn borrower_can_fully_exit_after_repaying_in_full() {
    let (mut protocol, oracle) = setup_protocol();
    protocol.deposit("lp", "USDC", 2_000_000, 0).unwrap();
    protocol.deposit("alice", "XLM", 1_000_000, 0).unwrap();
    protocol
        .borrow("alice", "USDC", 500_000, &oracle, 0)
        .unwrap();

    // Repay the full outstanding balance (overpay; only the owed amount is taken).
    let repaid = protocol
        .repay("alice", "alice", "USDC", 10_000_000, 0)
        .unwrap();
    assert_eq!(repaid, 500_000);

    let position = protocol.position("alice", &oracle).unwrap();
    assert_eq!(position.debt_value, 0);

    // Now the full collateral can be withdrawn.
    let withdrawn = protocol
        .withdraw("alice", "XLM", 1_000_000, &oracle, 0)
        .unwrap();
    assert_eq!(withdrawn, 1_000_000);
    assert_reserve_solvent(&protocol, "XLM");
}

/// Multiple suppliers depositing/withdrawing at different times must each
/// get back (at least) what their share of the reserve entitles them to,
/// even after interest has changed the exchange rate.
#[test]
fn multiple_suppliers_share_interest_proportionally() {
    let mut protocol = LendingProtocol::new(
        vec!["admin".to_string()],
        1,
        "treasury",
        InterestRateModel::default(),
    );
    protocol
        .register_asset("admin", reserve("USDC", 9_000), 0)
        .unwrap();
    protocol
        .register_asset("admin", reserve("XLM", 8_000), 0)
        .unwrap();
    let mut oracle = PriceOracleSim::new("oracle");
    oracle.set_price("oracle", "USDC", WAD).unwrap();
    oracle.set_price("oracle", "XLM", WAD).unwrap();

    // lp1 supplies early, lp2 supplies later (after some interest has accrued).
    protocol.deposit("lp1", "USDC", 1_000_000, 0).unwrap();
    protocol.deposit("borrower", "XLM", 5_000_000, 0).unwrap();
    protocol
        .borrow("borrower", "USDC", 800_000, &oracle, 0)
        .unwrap();

    protocol.accrue_interest("USDC", 15_768_000).unwrap(); // half a year

    let lp2_shares = protocol.deposit("lp2", "USDC", 1_000_000, 15_768_000).unwrap();
    // lp2 deposits into an already-appreciated reserve, so identical cash in
    // should mint fewer shares than lp1's original 1:1 mint.
    assert!(lp2_shares < 1_000_000);

    protocol.accrue_interest("USDC", 31_536_000).unwrap(); // another half year

    let lp1_redeemable = protocol.position("lp1", &oracle).unwrap();
    let lp2_redeemable = protocol.position("lp2", &oracle).unwrap();

    // lp1 supplied earlier and for longer, so should have earned more.
    assert!(
        lp1_redeemable.supplied_amounts["USDC"] > lp2_redeemable.supplied_amounts["USDC"],
        "earlier supplier should accrue more interest"
    );
    assert_reserve_solvent(&protocol, "USDC");
}

/// A user with multiple collateral assets and multiple debts should have a
/// consistent aggregate position, and repaying one debt shouldn't affect an
/// unrelated debt in a different asset.
#[test]
fn user_can_hold_positions_across_multiple_assets() {
    let (mut protocol, oracle) = setup_protocol();
    protocol.deposit("lp", "USDC", 5_000_000, 0).unwrap();
    protocol.deposit("lp", "XLM", 5_000_000, 0).unwrap();

    // Alice supplies both assets as collateral.
    protocol.deposit("alice", "USDC", 2_000_000, 0).unwrap();
    protocol.deposit("alice", "XLM", 2_000_000, 0).unwrap();

    // ...and borrows a bit of each against the combined collateral.
    protocol.borrow("alice", "USDC", 500_000, &oracle, 0).unwrap();
    protocol.borrow("alice", "XLM", 500_000, &oracle, 0).unwrap();

    let position = protocol.position("alice", &oracle).unwrap();
    assert_eq!(position.debt_amounts["USDC"], 500_000);
    assert_eq!(position.debt_amounts["XLM"], 500_000);

    // Repaying the USDC debt in full must not touch the XLM debt.
    protocol
        .repay("alice", "alice", "USDC", 500_000, 1)
        .unwrap();
    let after = protocol.position("alice", &oracle).unwrap();
    assert_eq!(*after.debt_amounts.get("USDC").unwrap_or(&0), 0);
    assert_eq!(after.debt_amounts["XLM"], 500_000);

    assert_reserve_solvent(&protocol, "USDC");
    assert_reserve_solvent(&protocol, "XLM");
}
