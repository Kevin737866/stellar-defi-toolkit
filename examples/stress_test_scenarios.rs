//! Stress Test Scenarios Demo
//!
//! Runs the same extreme-market scenarios exercised in
//! `tests/stress_tests.rs` as a standalone program, printing the protocol's
//! behavior at each step. Useful for manually inspecting resilience under:
//! - a 50% price crash across every collateral asset
//! - multiple simultaneous liquidations
//! - an oracle failure (missing price feed)

use stellar_defi_toolkit::{InterestRateModel, LendingProtocol, MockOracle, ReserveConfig, WAD};

fn reserve(asset: &str, collateral_factor_bps: u32) -> ReserveConfig {
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

fn setup() -> (LendingProtocol, MockOracle) {
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
    oracle.set_price("oracle", "XLM", WAD).unwrap();
    oracle.set_price("oracle", "USDC", WAD).unwrap();
    (protocol, oracle)
}

fn scenario_market_crash() {
    println!("Scenario 1: 50% price crash across all assets");
    println!("-----------------------------------------------");

    let (mut protocol, mut oracle) = setup();
    protocol.deposit("lp", "USDC", 100_000_000, 0).unwrap();
    protocol.deposit("alice", "XLM", 10_000_000, 0).unwrap();
    protocol
        .borrow("alice", "USDC", 7_800_000, &oracle, 0)
        .unwrap();

    let before = protocol.position("alice", &oracle).unwrap();
    println!("  Health factor before crash: {}", before.health_factor);

    oracle
        .simulate_crash("oracle", "XLM", WAD, 5_000, 6, 3_600)
        .unwrap();
    println!(
        "  XLM price after crash: {} (was {WAD})",
        oracle.get_price("XLM").unwrap()
    );

    let after = protocol.position("alice", &oracle).unwrap();
    println!("  Health factor after crash: {}\n", after.health_factor);
}

fn scenario_simultaneous_liquidations() {
    println!("Scenario 2: Multiple simultaneous liquidations");
    println!("-----------------------------------------------");

    let (mut protocol, oracle) = setup();
    protocol.deposit("lp", "USDC", 100_000_000, 0).unwrap();
    for user in ["alice", "bob", "carol"] {
        protocol.deposit(user, "XLM", 10_000_000, 0).unwrap();
        protocol
            .borrow(user, "USDC", 7_900_000, &oracle, 0)
            .unwrap();
    }

    let mut oracle = oracle;
    oracle.set_price("oracle", "XLM", WAD / 2).unwrap();

    for user in ["alice", "bob", "carol"] {
        let result = protocol
            .liquidate("liquidator", user, "USDC", "XLM", 1_000_000, &oracle, 1)
            .unwrap();
        println!(
            "  Liquidated {user}: repaid {}, seized {} XLM",
            result.repaid_amount, result.seized_collateral
        );
    }
    println!();
}

fn scenario_oracle_failure() {
    println!("Scenario 3: Oracle failure (missing price feed)");
    println!("-----------------------------------------------");

    let (mut protocol, mut oracle) = setup();
    protocol
        .register_asset("admin", reserve("BTC", 7_000), 0)
        .unwrap();
    protocol.deposit("dave", "BTC", 1_000_000, 0).unwrap();

    match protocol.position("dave", &oracle) {
        Ok(_) => println!("  Unexpected: position succeeded without a BTC price"),
        Err(err) => println!("  Position lookup correctly failed: {err:?}"),
    }

    oracle.set_price("oracle", "BTC", 60_000 * WAD).unwrap();
    let position = protocol.position("dave", &oracle).unwrap();
    println!(
        "  Oracle recovered — collateral value now: {}\n",
        position.collateral_value
    );
}

fn main() {
    println!("=== Stellar DeFi Toolkit - Stress Test Scenarios ===\n");
    scenario_market_crash();
    scenario_simultaneous_liquidations();
    scenario_oracle_failure();
}
