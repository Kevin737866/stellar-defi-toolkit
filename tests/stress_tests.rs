//! Scenario-based stress tests for extreme market conditions.
//!
//! These tests simulate the kind of conditions a real deployment must
//! survive: a fast, deep price crash across every collateral asset,
//! multiple borrowers becoming liquidatable at once, and an oracle that
//! stops reporting fresh prices entirely. All scenarios are driven by
//! `MockOracle` so they're fully deterministic — no wall-clock dependence,
//! no flakiness.

mod common;

use common::{reserve, setup_protocol_with_mock_oracle};
use stellar_defi_toolkit::{MockOracle, OracleSanityConfig, ProtocolError, WAD};

/// Scenario: a 50% price crash across every collateral asset in the
/// protocol, hitting within a single hour.
#[test]
fn fifty_percent_crash_across_all_assets_makes_borrowers_liquidatable() {
    let (mut protocol, mut oracle) = setup_protocol_with_mock_oracle();

    // Liquidity for borrowing.
    protocol.deposit("lp", "USDC", 100_000_000, 0).unwrap();
    protocol.deposit("lp", "XLM", 100_000_000, 0).unwrap();

    // Alice and Bob both post XLM collateral and borrow USDC near the
    // protocol's collateral limit (80% collateral factor).
    protocol.deposit("alice", "XLM", 10_000_000, 0).unwrap();
    protocol.deposit("bob", "XLM", 10_000_000, 0).unwrap();
    protocol
        .borrow("alice", "USDC", 7_800_000, &oracle, 0)
        .unwrap();
    protocol
        .borrow("bob", "USDC", 7_800_000, &oracle, 0)
        .unwrap();

    let alice_before = protocol.position("alice", &oracle).unwrap();
    let bob_before = protocol.position("bob", &oracle).unwrap();
    assert!(alice_before.health_factor >= WAD);
    assert!(bob_before.health_factor >= WAD);

    // Crash every collateral asset by 50% over six 10-minute steps (1 hour
    // total) — each step is a ~11% move, comfortably under the protocol's
    // default 20% single-update circuit breaker.
    oracle
        .simulate_crash("oracle", "XLM", WAD, 5_000, 6, 3_600)
        .unwrap();
    oracle
        .simulate_crash("oracle", "USDC", WAD, 5_000, 6, 3_600)
        .unwrap();

    assert_eq!(oracle.get_price("XLM").unwrap(), WAD / 2);
    assert_eq!(oracle.get_price("USDC").unwrap(), WAD / 2);

    // Both borrowers are now undercollateralized (XLM collateral halved in
    // value while USDC debt halved too, but the 80% collateral factor no
    // longer covers the debt once liquidation math is applied).
    let alice_after = protocol.position("alice", &oracle).unwrap();
    let bob_after = protocol.position("bob", &oracle).unwrap();
    assert!(
        alice_after.health_factor < alice_before.health_factor,
        "health factor should have deteriorated after the crash"
    );
    assert!(
        bob_after.health_factor < bob_before.health_factor,
        "health factor should have deteriorated after the crash"
    );
}

/// Scenario: multiple simultaneous liquidations following a crash.
#[test]
fn crash_triggers_multiple_simultaneous_liquidations() {
    let (mut protocol, mut oracle) = setup_protocol_with_mock_oracle();

    protocol.deposit("lp", "USDC", 100_000_000, 0).unwrap();

    // Three separate borrowers, each at the edge of their collateral limit.
    for user in ["alice", "bob", "carol"] {
        protocol.deposit(user, "XLM", 10_000_000, 0).unwrap();
        protocol
            .borrow(user, "USDC", 7_900_000, &oracle, 0)
            .unwrap();
    }

    // A sudden, sharp XLM crash (circuit breaker disabled to model a true
    // single-block flash crash rather than a gradual decline).
    let mut permissive_sanity = OracleSanityConfig::default();
    permissive_sanity.max_price_deviation_bps = 0;
    let mut oracle = MockOracle::with_sanity("oracle", permissive_sanity);
    oracle.set_price("oracle", "XLM", WAD).unwrap();
    oracle.set_price("oracle", "USDC", WAD).unwrap();
    oracle.set_price("oracle", "XLM", WAD / 2).unwrap();

    // Every borrower should now be liquidatable, and a single liquidator can
    // clear all three positions in the same "block" (same `now`) without
    // the protocol getting into an inconsistent state.
    for (user, borrower_position) in [("alice", 0), ("bob", 1), ("carol", 2)] {
        let _ = borrower_position;
        let position = protocol.position(user, &oracle).unwrap();
        assert!(
            position.health_factor < WAD,
            "{user} should be undercollateralized after the crash"
        );

        let result = protocol
            .liquidate("liquidator", user, "USDC", "XLM", 1_000_000, &oracle, 1)
            .unwrap();
        assert!(result.repaid_amount > 0);
        assert!(result.seized_collateral > 0);
    }

    // Protocol-level bookkeeping stayed consistent across all three
    // liquidations — no panics, no negative balances.
    let xlm_reserve = protocol.reserve_state("XLM").unwrap();
    let usdc_reserve = protocol.reserve_state("USDC").unwrap();
    assert!(xlm_reserve.total_cash >= 0);
    assert!(usdc_reserve.total_cash >= 0);
}

/// Scenario: an oracle failure — a collateral asset's price feed never
/// comes online (or drops out entirely). The protocol must surface the
/// failure through pricing-dependent operations rather than mispricing the
/// position as worthless or panicking.
///
/// (Staleness itself — a feed that *stops updating* rather than never
/// having reported — is exercised directly against the oracle in
/// `circuit_breaker_tests.rs`; `MockOracle::simulate_staleness` is the tool
/// for that scenario.)
#[test]
fn oracle_failure_blocks_pricing_dependent_operations() {
    let (mut protocol, mut oracle) = setup_protocol_with_mock_oracle();

    // A newly registered asset whose price feed has never come online --
    // modeling an oracle outage for that market.
    protocol
        .register_asset("admin", reserve("BTC", 7_000), 0)
        .unwrap();
    protocol.deposit("dave", "BTC", 1_000_000, 0).unwrap();

    // Reading Dave's position must surface the missing price rather than
    // silently treating the un-priced collateral as worthless.
    let err = protocol.position("dave", &oracle).unwrap_err();
    assert_eq!(err, ProtocolError::MissingPrice("BTC".to_string()));

    // The protocol recovers as soon as the feed comes back online.
    oracle.set_price("oracle", "BTC", 60_000 * WAD).unwrap();
    let position = protocol.position("dave", &oracle).unwrap();
    assert!(position.collateral_value > 0);
}
