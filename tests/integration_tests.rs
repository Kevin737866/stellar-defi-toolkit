use stellar_defi_toolkit::{
    InterestRateModel, LendingProtocol, PriceOracle, ProtocolError, ReserveConfig, WAD,
};

// ── helpers ───────────────────────────────────────────────────────────────────

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
    }
}

fn setup_protocol() -> (LendingProtocol, PriceOracle) {
    let mut protocol = LendingProtocol::new("admin", "treasury", InterestRateModel::default());
    protocol
        .register_asset("admin", reserve("XLM", 8_000), 0)
        .unwrap();
    protocol
        .register_asset("admin", reserve("USDC", 9_000), 0)
        .unwrap();

    let mut oracle = PriceOracle::new("oracle");
    oracle.set_price("oracle", "XLM", WAD, 0).unwrap();
    oracle.set_price("oracle", "USDC", WAD, 0).unwrap();

    (protocol, oracle)
}

#[test]
fn deposits_mint_supply_shares_and_track_liquidity() {
    let (mut protocol, _oracle) = setup_protocol();
    let shares = protocol.deposit("alice", "USDC", 1_000_000, 0).unwrap();
    let reserve = protocol.reserve_state("USDC").unwrap();

    assert_eq!(shares, 1_000_000);
    assert_eq!(reserve.total_cash, 1_000_000);
    assert_eq!(reserve.total_supply_shares, 1_000_000);
}

#[test]
fn overcollateralized_borrow_and_repay_flow_works() {
    let (mut protocol, oracle) = setup_protocol();

    protocol.deposit("lp", "USDC", 2_000_000, 0).unwrap();
    protocol.deposit("alice", "XLM", 1_000_000, 0).unwrap();
    protocol
        .borrow("alice", "USDC", 700_000, &oracle, 0)
        .unwrap();

    let position = protocol.position("alice", &oracle).unwrap();
    assert_eq!(position.debt_amounts["USDC"], 700_000);
    assert!(position.collateral_value >= position.debt_value);

    let repaid = protocol
        .repay("alice", "alice", "USDC", 200_000, 1)
        .unwrap();
    assert_eq!(repaid, 200_000);
    let updated = protocol.position("alice", &oracle).unwrap();
    assert!(updated.debt_amounts["USDC"] < 700_000);
}

#[test]
fn borrow_rejected_when_it_exceeds_collateral_limit() {
    let (mut protocol, oracle) = setup_protocol();
    protocol.deposit("lp", "USDC", 1_000_000, 0).unwrap();
    protocol.deposit("alice", "XLM", 100_000, 0).unwrap();

    let err = protocol
        .borrow("alice", "USDC", 200_000, &oracle, 0)
        .unwrap_err();
    assert_eq!(err, ProtocolError::InsufficientCollateral);
}

#[test]
fn interest_accrues_and_reserve_factor_splits_protocol_fees() {
    let (mut protocol, oracle) = setup_protocol();
    protocol.deposit("lp", "USDC", 5_000_000, 0).unwrap();
    protocol.deposit("alice", "XLM", 5_000_000, 0).unwrap();
    protocol
        .borrow("alice", "USDC", 4_000_000, &oracle, 0)
        .unwrap();

    let before = protocol.reserve_state("USDC").unwrap().clone();
    protocol.accrue_interest("USDC", 31_536_000).unwrap();
    let after = protocol.reserve_state("USDC").unwrap();

    assert!(after.total_debt > before.total_debt);
    assert!(after.protocol_fees > before.protocol_fees);
}

#[test]
fn liquidation_seizes_collateral_when_health_factor_falls_below_one() {
    let (mut protocol, mut oracle) = setup_protocol();
    protocol.deposit("lp", "USDC", 5_000_000, 0).unwrap();
    protocol.deposit("alice", "XLM", 1_000_000, 0).unwrap();
    protocol
        .borrow("alice", "USDC", 700_000, &oracle, 0)
        .unwrap();

    oracle.set_price("oracle", "XLM", 700_000_000, 1).unwrap();
    let position = protocol.position("alice", &oracle).unwrap();
    assert!(position.health_factor < WAD);

    let result = protocol
        .liquidate("bob", "alice", "USDC", "XLM", 300_000, &oracle, 1)
        .unwrap();

    assert!(result.repaid_amount > 0);
    assert!(result.seized_collateral > 0);

    let updated = protocol.position("alice", &oracle).unwrap();
    assert!(updated.debt_value < position.debt_value);
}

#[test]
fn flash_loans_charge_fee_and_credit_protocol_cut() {
    let (mut protocol, _oracle) = setup_protocol();
    protocol.deposit("lp", "USDC", 10_000_000, 0).unwrap();

    let receipt = protocol
        .flash_loan("arb-bot", "USDC", 1_000_000, 1_001_000, 1)
        .unwrap();
    let reserve = protocol.reserve_state("USDC").unwrap();

    assert!(receipt.fee_paid > 0);
    assert_eq!(
        receipt.fee_paid,
        receipt.protocol_fee + receipt.supplier_fee
    );
    assert!(reserve.protocol_fees >= receipt.protocol_fee);
}

#[test]
fn admin_controls_guard_configuration_and_fee_collection() {
    let (mut protocol, _oracle) = setup_protocol();
    protocol.deposit("lp", "USDC", 2_000_000, 0).unwrap();
    protocol
        .flash_loan("arb-bot", "USDC", 1_000_000, 1_001_000, 1)
        .unwrap();

    let err = protocol
        .collect_protocol_fees("alice", "USDC", 100)
        .unwrap_err();
    assert_eq!(err, ProtocolError::Unauthorized);

    let collected = protocol
        .collect_protocol_fees("admin", "USDC", 100)
        .unwrap();
    assert!(collected > 0);
}

#[test]
fn disabling_collateral_is_blocked_if_it_would_break_health_factor() {
    let (mut protocol, oracle) = setup_protocol();
    protocol.deposit("lp", "USDC", 2_000_000, 0).unwrap();
    protocol.deposit("alice", "XLM", 1_000_000, 0).unwrap();
    protocol
        .borrow("alice", "USDC", 700_000, &oracle, 0)
        .unwrap();

    let err = protocol
        .set_collateral_enabled("alice", "XLM", false, &oracle)
        .unwrap_err();
    assert_eq!(err, ProtocolError::HealthFactorTooLow);
}

// ── Emergency pause tests ─────────────────────────────────────────────────────

#[test]
fn pause_blocks_all_user_operations() {
    let (mut protocol, oracle) = setup_protocol();
    protocol.deposit("lp", "USDC", 2_000_000, 0).unwrap();
    protocol.deposit("alice", "XLM", 1_000_000, 0).unwrap();

    // Pause the protocol
    protocol.pause("admin").unwrap();
    assert!(protocol.is_paused());

    // Every user-facing operation must return ProtocolPaused
    assert_eq!(
        protocol.deposit("alice", "USDC", 100, 1).unwrap_err(),
        ProtocolError::ProtocolPaused
    );
    assert_eq!(
        protocol.withdraw("alice", "XLM", 100, &oracle, 1).unwrap_err(),
        ProtocolError::ProtocolPaused
    );
    assert_eq!(
        protocol.borrow("alice", "USDC", 100, &oracle, 1).unwrap_err(),
        ProtocolError::ProtocolPaused
    );
    assert_eq!(
        protocol.repay("alice", "alice", "USDC", 100, 1).unwrap_err(),
        ProtocolError::ProtocolPaused
    );
    assert_eq!(
        protocol
            .liquidate("bob", "alice", "USDC", "XLM", 100, &oracle, 1)
            .unwrap_err(),
        ProtocolError::ProtocolPaused
    );
    assert_eq!(
        protocol.flash_loan("arb", "USDC", 100, 200, 1).unwrap_err(),
        ProtocolError::ProtocolPaused
    );
    assert_eq!(
        protocol
            .set_collateral_enabled("alice", "XLM", false, &oracle)
            .unwrap_err(),
        ProtocolError::ProtocolPaused
    );
}

#[test]
fn pause_does_not_block_admin_operations() {
    let (mut protocol, _oracle) = setup_protocol();
    protocol.deposit("lp", "USDC", 2_000_000, 0).unwrap();
    protocol
        .flash_loan("arb-bot", "USDC", 1_000_000, 1_001_000, 1)
        .unwrap();

    protocol.pause("admin").unwrap();

    // Admin can still collect fees while paused
    let collected = protocol
        .collect_protocol_fees("admin", "USDC", 1_000)
        .unwrap();
    assert!(collected > 0);
}

#[test]
fn unpause_restores_normal_operations() {
    let (mut protocol, _oracle) = setup_protocol();
    protocol.pause("admin").unwrap();
    assert!(protocol.is_paused());

    protocol.unpause("admin").unwrap();
    assert!(!protocol.is_paused());

    // Deposit should work again
    let shares = protocol.deposit("alice", "USDC", 500_000, 0).unwrap();
    assert!(shares > 0);
}

#[test]
fn only_admin_can_pause_and_unpause() {
    let (mut protocol, _oracle) = setup_protocol();

    assert_eq!(
        protocol.pause("alice").unwrap_err(),
        ProtocolError::Unauthorized
    );
    assert_eq!(
        protocol.unpause("alice").unwrap_err(),
        ProtocolError::Unauthorized
    );
}

#[test]
fn pause_and_unpause_are_idempotent() {
    let (mut protocol, _oracle) = setup_protocol();

    // Double-pause should not error
    protocol.pause("admin").unwrap();
    protocol.pause("admin").unwrap();
    assert!(protocol.is_paused());

    // Double-unpause should not error
    protocol.unpause("admin").unwrap();
    protocol.unpause("admin").unwrap();
    assert!(!protocol.is_paused());
}

// ── Liquidation optimisation tests ───────────────────────────────────────────

#[test]
fn liquidation_result_includes_health_factor_after() {
    let (mut protocol, mut oracle) = setup_protocol();
    protocol.deposit("lp", "USDC", 5_000_000, 0).unwrap();
    protocol.deposit("alice", "XLM", 1_000_000, 0).unwrap();
    protocol
        .borrow("alice", "USDC", 700_000, &oracle, 0)
        .unwrap();

    oracle.set_price("oracle", "XLM", 700_000_000, 1).unwrap();

    let result = protocol
        .liquidate("bob", "alice", "USDC", "XLM", 300_000, &oracle, 1)
        .unwrap();

    // health_factor_after must be present and non-negative
    assert!(result.health_factor_after >= 0);
    // After partial liquidation the position should be healthier
    assert!(result.repaid_amount > 0);
    assert!(result.seized_collateral > 0);
}

#[test]
fn liquidation_caps_seize_to_available_collateral() {
    let (mut protocol, mut oracle) = setup_protocol();
    protocol.deposit("lp", "USDC", 5_000_000, 0).unwrap();
    // Alice deposits a small amount of XLM and borrows close to the limit
    protocol.deposit("alice", "XLM", 100_000, 0).unwrap();
    protocol
        .borrow("alice", "USDC", 70_000, &oracle, 0)
        .unwrap();

    // Crash XLM price so the position is deeply underwater
    oracle.set_price("oracle", "XLM", 500_000_000, 1).unwrap();

    // Request a very large repay — should succeed by capping seize
    let result = protocol
        .liquidate("bob", "alice", "USDC", "XLM", 60_000, &oracle, 1)
        .unwrap();

    assert!(result.seized_collateral > 0);
    // Seized amount must not exceed what alice had
    assert!(result.seized_collateral <= 100_000);
}
