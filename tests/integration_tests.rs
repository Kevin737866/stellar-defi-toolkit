use stellar_defi_toolkit::{
    InterestRateModel, LendingProtocol, OracleSanityConfig, ProtocolError, ProtocolEvent,
    PriceOracleSim, ReserveConfig, WAD,
};

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

<<<<<<< lakes1
fn setup_protocol() -> (LendingProtocol, PriceOracle) {
    let mut protocol = LendingProtocol::new(
        vec!["admin".to_string()],
        1,
        "treasury",
        InterestRateModel::default(),
    );
=======
fn setup_protocol() -> (LendingProtocol, PriceOracleSim) {
    let mut protocol = LendingProtocol::new("admin", "treasury", InterestRateModel::default());
>>>>>>> main
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

    oracle.set_price("oracle", "XLM", 700_000_000).unwrap();
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

<<<<<<< lakes1
#[test]
fn multisig_proposal_flow_works() {
    use stellar_defi_toolkit::AdminAction;
    let mut protocol = LendingProtocol::new(
        vec!["admin1".to_string(), "admin2".to_string()],
        2,
        "treasury",
        InterestRateModel::default(),
    );

    let action = AdminAction::SetCloseFactor(6_000);
    let proposal_id = protocol
        .propose_admin_action("admin1", action, 0)
        .unwrap();

    // admin2 approves
    protocol.approve_admin_proposal("admin2", proposal_id).unwrap();

    // Anyone in admin can execute
    protocol.execute_admin_proposal("admin1", proposal_id, 0).unwrap();

    // Check if executed
    let snapshot = protocol.snapshot();
    assert_eq!(snapshot.multisig.threshold, 2);
    assert_eq!(snapshot.multisig.admins.len(), 2);
=======
// ── Feature: per-asset interest rate models ──────────────────────────────────

#[test]
fn per_asset_interest_rate_model_overrides_protocol_default() {
    // Set up a protocol with a very low default rate, then give USDC a much
    // steeper model and verify that USDC accrues more interest than XLM.
    let default_model = InterestRateModel {
        base_rate: 10_000_000,   // 1 %
        slope_1: 40_000_000,     // 4 %
        slope_2: 400_000_000,    // 40 %
        optimal_utilization: 800_000_000,
    };
    let steep_model = InterestRateModel {
        base_rate: 100_000_000,  // 10 %
        slope_1: 400_000_000,    // 40 %
        slope_2: 2_000_000_000,  // 200 %
        optimal_utilization: 800_000_000,
    };

    let mut protocol = LendingProtocol::new("admin", "treasury", default_model);
    protocol
        .register_asset("admin", reserve("XLM", 8_000), 0)
        .unwrap();
    protocol
        .register_asset("admin", reserve("USDC", 9_000), 0)
        .unwrap();

    // Assign the steep model only to USDC.
    protocol
        .set_asset_interest_rate_model("admin", "USDC", Some(steep_model))
        .unwrap();

    let mut oracle = PriceOracleSim::new("oracle");
    oracle.set_price("oracle", "XLM", WAD).unwrap();
    oracle.set_price("oracle", "USDC", WAD).unwrap();

    // Provide liquidity and create borrows at ~80 % utilization for both assets.
    protocol.deposit("lp", "XLM", 5_000_000, 0).unwrap();
    protocol.deposit("lp", "USDC", 5_000_000, 0).unwrap();
    protocol.deposit("alice", "XLM", 10_000_000, 0).unwrap();
    protocol
        .borrow("alice", "XLM", 4_000_000, &oracle, 0)
        .unwrap();
    protocol
        .borrow("alice", "USDC", 4_000_000, &oracle, 0)
        .unwrap();

    let one_year = 31_536_000_u64;
    protocol.accrue_interest("XLM", one_year).unwrap();
    protocol.accrue_interest("USDC", one_year).unwrap();

    let xlm_debt = protocol.reserve_state("XLM").unwrap().total_debt;
    let usdc_debt = protocol.reserve_state("USDC").unwrap().total_debt;

    // USDC has a steeper model so it must accrue more interest.
    assert!(
        usdc_debt > xlm_debt,
        "USDC debt ({usdc_debt}) should exceed XLM debt ({xlm_debt}) due to steeper model"
    );
}

#[test]
fn clearing_per_asset_model_reverts_to_protocol_default() {
    let default_model = InterestRateModel::default();
    let steep_model = InterestRateModel {
        base_rate: 200_000_000,
        slope_1: 800_000_000,
        slope_2: 3_000_000_000,
        optimal_utilization: 800_000_000,
    };

    let mut protocol = LendingProtocol::new("admin", "treasury", default_model.clone());
    protocol
        .register_asset("admin", reserve("USDC", 9_000), 0)
        .unwrap();

    protocol
        .set_asset_interest_rate_model("admin", "USDC", Some(steep_model))
        .unwrap();
    // Clear the override — should fall back to default.
    protocol
        .set_asset_interest_rate_model("admin", "USDC", None)
        .unwrap();

    // The effective model should now equal the default.
    let effective = protocol.interest_rate_model_for("USDC").unwrap();
    assert_eq!(*effective, default_model);
}

#[test]
fn non_admin_cannot_set_asset_interest_rate_model() {
    let mut protocol = LendingProtocol::new("admin", "treasury", InterestRateModel::default());
    protocol
        .register_asset("admin", reserve("USDC", 9_000), 0)
        .unwrap();

    let err = protocol
        .set_asset_interest_rate_model("alice", "USDC", Some(InterestRateModel::default()))
        .unwrap_err();
    assert_eq!(err, ProtocolError::Unauthorized);
}

// ── Feature: supply caps ──────────────────────────────────────────────────────

#[test]
fn deposit_is_rejected_when_supply_cap_is_reached() {
    let mut protocol = LendingProtocol::new("admin", "treasury", InterestRateModel::default());
    protocol
        .register_asset("admin", reserve("USDC", 9_000), 0)
        .unwrap();

    // Set a tight supply cap of 1 000 000.
    protocol.set_supply_cap("admin", "USDC", 1_000_000).unwrap();

    // First deposit fits within the cap.
    protocol.deposit("alice", "USDC", 800_000, 0).unwrap();

    // Second deposit would push total supplied past the cap.
    let err = protocol.deposit("bob", "USDC", 300_000, 0).unwrap_err();
    assert_eq!(err, ProtocolError::SupplyCapExceeded("USDC".to_string()));
}

#[test]
fn deposit_succeeds_when_supply_cap_is_zero_uncapped() {
    let mut protocol = LendingProtocol::new("admin", "treasury", InterestRateModel::default());
    protocol
        .register_asset("admin", reserve("USDC", 9_000), 0)
        .unwrap();

    // supply_cap = 0 means no cap.
    protocol.set_supply_cap("admin", "USDC", 0).unwrap();
    protocol.deposit("alice", "USDC", 100_000_000, 0).unwrap();
    let state = protocol.reserve_state("USDC").unwrap();
    assert_eq!(state.total_cash, 100_000_000);
}

#[test]
fn non_admin_cannot_set_supply_cap() {
    let mut protocol = LendingProtocol::new("admin", "treasury", InterestRateModel::default());
    protocol
        .register_asset("admin", reserve("USDC", 9_000), 0)
        .unwrap();

    let err = protocol
        .set_supply_cap("alice", "USDC", 1_000_000)
        .unwrap_err();
    assert_eq!(err, ProtocolError::Unauthorized);
}

// ── Feature: borrow caps ──────────────────────────────────────────────────────

#[test]
fn borrow_is_rejected_when_borrow_cap_is_reached() {
    let mut protocol = LendingProtocol::new("admin", "treasury", InterestRateModel::default());
    protocol
        .register_asset("admin", reserve("XLM", 8_000), 0)
        .unwrap();
    protocol
        .register_asset("admin", reserve("USDC", 9_000), 0)
        .unwrap();

    let mut oracle = PriceOracleSim::new("oracle");
    oracle.set_price("oracle", "XLM", WAD).unwrap();
    oracle.set_price("oracle", "USDC", WAD).unwrap();

    // Provide ample liquidity.
    protocol.deposit("lp", "USDC", 10_000_000, 0).unwrap();
    // Alice deposits XLM as collateral.
    protocol.deposit("alice", "XLM", 10_000_000, 0).unwrap();

    // Cap USDC borrows at 500 000.
    protocol.set_borrow_cap("admin", "USDC", 500_000).unwrap();

    // First borrow fits.
    protocol
        .borrow("alice", "USDC", 400_000, &oracle, 0)
        .unwrap();

    // Second borrow would exceed the cap.
    let err = protocol
        .borrow("alice", "USDC", 200_000, &oracle, 0)
        .unwrap_err();
    assert_eq!(err, ProtocolError::BorrowCapExceeded("USDC".to_string()));
}

#[test]
fn borrow_succeeds_when_borrow_cap_is_zero_uncapped() {
    let mut protocol = LendingProtocol::new("admin", "treasury", InterestRateModel::default());
    protocol
        .register_asset("admin", reserve("XLM", 8_000), 0)
        .unwrap();
    protocol
        .register_asset("admin", reserve("USDC", 9_000), 0)
        .unwrap();

    let mut oracle = PriceOracleSim::new("oracle");
    oracle.set_price("oracle", "XLM", WAD).unwrap();
    oracle.set_price("oracle", "USDC", WAD).unwrap();

    protocol.deposit("lp", "USDC", 5_000_000, 0).unwrap();
    protocol.deposit("alice", "XLM", 5_000_000, 0).unwrap();

    // borrow_cap = 0 means no cap.
    protocol.set_borrow_cap("admin", "USDC", 0).unwrap();
    protocol
        .borrow("alice", "USDC", 4_000_000, &oracle, 0)
        .unwrap();
    let state = protocol.reserve_state("USDC").unwrap();
    assert_eq!(state.total_debt, 4_000_000);
}

#[test]
fn non_admin_cannot_set_borrow_cap() {
    let mut protocol = LendingProtocol::new("admin", "treasury", InterestRateModel::default());
    protocol
        .register_asset("admin", reserve("USDC", 9_000), 0)
        .unwrap();

    let err = protocol
        .set_borrow_cap("alice", "USDC", 500_000)
        .unwrap_err();
    assert_eq!(err, ProtocolError::Unauthorized);
}

// ── Feature: dynamic reserve factors ─────────────────────────────────────────

#[test]
fn reserve_factor_update_changes_protocol_fee_accrual() {
    let (mut protocol, oracle) = setup_protocol();

    // Provide liquidity and create a borrow.
    protocol.deposit("lp", "USDC", 5_000_000, 0).unwrap();
    protocol.deposit("alice", "XLM", 5_000_000, 0).unwrap();
    protocol
        .borrow("alice", "USDC", 4_000_000, &oracle, 0)
        .unwrap();

    // Accrue one year with the original 10 % reserve factor.
    protocol.accrue_interest("USDC", 31_536_000).unwrap();
    let fees_low_rf = protocol.reserve_state("USDC").unwrap().protocol_fees;

    // Reset state by creating a fresh protocol with a 50 % reserve factor.
    let mut protocol2 = LendingProtocol::new("admin", "treasury", InterestRateModel::default());
    let mut cfg = reserve("USDC", 9_000);
    cfg.reserve_factor_bps = 5_000; // 50 %
    protocol2.register_asset("admin", cfg, 0).unwrap();
    protocol2
        .register_asset("admin", reserve("XLM", 8_000), 0)
        .unwrap();

    let mut oracle2 = PriceOracleSim::new("oracle");
    oracle2.set_price("oracle", "XLM", WAD).unwrap();
    oracle2.set_price("oracle", "USDC", WAD).unwrap();

    protocol2.deposit("lp", "USDC", 5_000_000, 0).unwrap();
    protocol2.deposit("alice", "XLM", 5_000_000, 0).unwrap();
    protocol2
        .borrow("alice", "USDC", 4_000_000, &oracle2, 0)
        .unwrap();
    protocol2.accrue_interest("USDC", 31_536_000).unwrap();
    let fees_high_rf = protocol2.reserve_state("USDC").unwrap().protocol_fees;

    assert!(
        fees_high_rf > fees_low_rf,
        "50 % reserve factor ({fees_high_rf}) should collect more fees than 10 % ({fees_low_rf})"
    );
}

#[test]
fn set_reserve_factor_updates_config_and_affects_future_accrual() {
    let (mut protocol, oracle) = setup_protocol();

    protocol.deposit("lp", "USDC", 5_000_000, 0).unwrap();
    protocol.deposit("alice", "XLM", 5_000_000, 0).unwrap();
    protocol
        .borrow("alice", "USDC", 4_000_000, &oracle, 0)
        .unwrap();

    // Raise the reserve factor to 50 % mid-flight.
    protocol.set_reserve_factor("admin", "USDC", 5_000).unwrap();

    // Accrue interest — the new factor should apply.
    protocol.accrue_interest("USDC", 31_536_000).unwrap();
    let fees = protocol.reserve_state("USDC").unwrap().protocol_fees;
    assert!(fees > 0, "protocol fees should be positive after accrual");
}

#[test]
fn set_reserve_factor_rejects_value_above_10000_bps() {
    let (mut protocol, _oracle) = setup_protocol();

    let err = protocol
        .set_reserve_factor("admin", "USDC", 10_001)
        .unwrap_err();
    assert_eq!(err, ProtocolError::InvalidReserveFactor);
}

#[test]
fn non_admin_cannot_set_reserve_factor() {
    let (mut protocol, _oracle) = setup_protocol();

    let err = protocol
        .set_reserve_factor("alice", "USDC", 2_000)
        .unwrap_err();
    assert_eq!(err, ProtocolError::Unauthorized);
>>>>>>> main
}

// ── Feature: emergency pause ──────────────────────────────────────────────────

#[test]
fn admin_can_pause_and_unpause_protocol() {
    let (mut protocol, _oracle) = setup_protocol();

    assert!(!protocol.is_paused());

    protocol.pause("admin").unwrap();
    assert!(protocol.is_paused());

    protocol.unpause("admin").unwrap();
    assert!(!protocol.is_paused());
}

#[test]
fn non_admin_cannot_pause() {
    let (mut protocol, _oracle) = setup_protocol();

    let err = protocol.pause("alice").unwrap_err();
    assert_eq!(err, ProtocolError::Unauthorized);
}

#[test]
fn deposit_blocked_when_paused() {
    let (mut protocol, _oracle) = setup_protocol();
    protocol.pause("admin").unwrap();

    let err = protocol.deposit("alice", "USDC", 1_000_000, 0).unwrap_err();
    assert_eq!(err, ProtocolError::ProtocolPaused);
}

#[test]
fn withdraw_blocked_when_paused() {
    let (mut protocol, oracle) = setup_protocol();
    protocol.deposit("alice", "USDC", 1_000_000, 0).unwrap();
    protocol.pause("admin").unwrap();

    let err = protocol
        .withdraw("alice", "USDC", 500_000, &oracle, 0)
        .unwrap_err();
    assert_eq!(err, ProtocolError::ProtocolPaused);
}

#[test]
fn borrow_blocked_when_paused() {
    let (mut protocol, oracle) = setup_protocol();
    protocol.deposit("lp", "USDC", 2_000_000, 0).unwrap();
    protocol.deposit("alice", "XLM", 1_000_000, 0).unwrap();
    protocol.pause("admin").unwrap();

    let err = protocol
        .borrow("alice", "USDC", 500_000, &oracle, 0)
        .unwrap_err();
    assert_eq!(err, ProtocolError::ProtocolPaused);
}

#[test]
fn repay_blocked_when_paused() {
    let (mut protocol, oracle) = setup_protocol();
    protocol.deposit("lp", "USDC", 2_000_000, 0).unwrap();
    protocol.deposit("alice", "XLM", 1_000_000, 0).unwrap();
    protocol.borrow("alice", "USDC", 700_000, &oracle, 0).unwrap();
    protocol.pause("admin").unwrap();

    let err = protocol
        .repay("alice", "alice", "USDC", 200_000, 1)
        .unwrap_err();
    assert_eq!(err, ProtocolError::ProtocolPaused);
}

#[test]
fn liquidate_blocked_when_paused() {
    let (mut protocol, mut oracle) = setup_protocol();
    protocol.deposit("lp", "USDC", 5_000_000, 0).unwrap();
    protocol.deposit("alice", "XLM", 1_000_000, 0).unwrap();
    protocol.borrow("alice", "USDC", 700_000, &oracle, 0).unwrap();
    oracle.set_price("oracle", "XLM", 700_000_000).unwrap();
    protocol.pause("admin").unwrap();

    let err = protocol
        .liquidate("bob", "alice", "USDC", "XLM", 300_000, &oracle, 1)
        .unwrap_err();
    assert_eq!(err, ProtocolError::ProtocolPaused);
}

#[test]
fn flash_loan_blocked_when_paused() {
    let (mut protocol, _oracle) = setup_protocol();
    protocol.deposit("lp", "USDC", 10_000_000, 0).unwrap();
    protocol.pause("admin").unwrap();

    let err = protocol
        .flash_loan("arb-bot", "USDC", 1_000_000, 1_001_000, 1)
        .unwrap_err();
    assert_eq!(err, ProtocolError::ProtocolPaused);
}

#[test]
fn admin_operations_work_while_paused() {
    let (mut protocol, _oracle) = setup_protocol();
    protocol.pause("admin").unwrap();

    // Admin can still update config while paused.
    protocol
        .set_reserve_factor("admin", "USDC", 2_000)
        .unwrap();
    protocol.set_supply_cap("admin", "USDC", 0).unwrap();
}

#[test]
fn pause_and_unpause_emit_events() {
    let (mut protocol, _oracle) = setup_protocol();
    protocol.drain_events(); // clear setup events

    protocol.pause("admin").unwrap();
    protocol.unpause("admin").unwrap();

    let events = protocol.drain_events();
    assert!(events.iter().any(|e| matches!(e, ProtocolEvent::Paused { .. })));
    assert!(events.iter().any(|e| matches!(e, ProtocolEvent::Unpaused { .. })));
}

// ── Feature: event emission ───────────────────────────────────────────────────

#[test]
fn deposit_emits_deposit_event() {
    let (mut protocol, _oracle) = setup_protocol();
    protocol.drain_events();

    let shares = protocol.deposit("alice", "USDC", 1_000_000, 0).unwrap();

    let events = protocol.drain_events();
    let deposit_event = events.iter().find(|e| matches!(e, ProtocolEvent::Deposit { .. }));
    assert!(deposit_event.is_some(), "expected a Deposit event");

    if let Some(ProtocolEvent::Deposit {
        user,
        asset,
        amount,
        shares_minted,
    }) = deposit_event
    {
        assert_eq!(user, "alice");
        assert_eq!(asset, "USDC");
        assert_eq!(*amount, 1_000_000);
        assert_eq!(*shares_minted, shares);
    }
}

#[test]
fn borrow_emits_borrow_event() {
    let (mut protocol, oracle) = setup_protocol();
    protocol.deposit("lp", "USDC", 2_000_000, 0).unwrap();
    protocol.deposit("alice", "XLM", 1_000_000, 0).unwrap();
    protocol.drain_events();

    protocol.borrow("alice", "USDC", 700_000, &oracle, 0).unwrap();

    let events = protocol.drain_events();
    assert!(
        events.iter().any(|e| matches!(e, ProtocolEvent::Borrow { .. })),
        "expected a Borrow event"
    );
}

#[test]
fn repay_emits_repay_event() {
    let (mut protocol, oracle) = setup_protocol();
    protocol.deposit("lp", "USDC", 2_000_000, 0).unwrap();
    protocol.deposit("alice", "XLM", 1_000_000, 0).unwrap();
    protocol.borrow("alice", "USDC", 700_000, &oracle, 0).unwrap();
    protocol.drain_events();

    protocol.repay("alice", "alice", "USDC", 200_000, 1).unwrap();

    let events = protocol.drain_events();
    assert!(
        events.iter().any(|e| matches!(e, ProtocolEvent::Repay { .. })),
        "expected a Repay event"
    );
}

#[test]
fn liquidate_emits_liquidate_event() {
    let (mut protocol, mut oracle) = setup_protocol();
    protocol.deposit("lp", "USDC", 5_000_000, 0).unwrap();
    protocol.deposit("alice", "XLM", 1_000_000, 0).unwrap();
    protocol.borrow("alice", "USDC", 700_000, &oracle, 0).unwrap();
    oracle.set_price("oracle", "XLM", 700_000_000).unwrap();
    protocol.drain_events();

    protocol
        .liquidate("bob", "alice", "USDC", "XLM", 300_000, &oracle, 1)
        .unwrap();

    let events = protocol.drain_events();
    assert!(
        events.iter().any(|e| matches!(e, ProtocolEvent::Liquidate { .. })),
        "expected a Liquidate event"
    );
}

#[test]
fn flash_loan_emits_flash_loan_event() {
    let (mut protocol, _oracle) = setup_protocol();
    protocol.deposit("lp", "USDC", 10_000_000, 0).unwrap();
    protocol.drain_events();

    protocol
        .flash_loan("arb-bot", "USDC", 1_000_000, 1_001_000, 1)
        .unwrap();

    let events = protocol.drain_events();
    assert!(
        events.iter().any(|e| matches!(e, ProtocolEvent::FlashLoan { .. })),
        "expected a FlashLoan event"
    );
}

#[test]
fn collect_fees_emits_fees_collected_event() {
    let (mut protocol, _oracle) = setup_protocol();
    protocol.deposit("lp", "USDC", 2_000_000, 0).unwrap();
    protocol
        .flash_loan("arb-bot", "USDC", 1_000_000, 1_001_000, 1)
        .unwrap();
    protocol.drain_events();

    protocol
        .collect_protocol_fees("admin", "USDC", 100)
        .unwrap();

    let events = protocol.drain_events();
    assert!(
        events.iter().any(|e| matches!(e, ProtocolEvent::FeesCollected { .. })),
        "expected a FeesCollected event"
    );
}

#[test]
fn interest_accrual_emits_interest_accrued_event() {
    let (mut protocol, oracle) = setup_protocol();
    protocol.deposit("lp", "USDC", 5_000_000, 0).unwrap();
    protocol.deposit("alice", "XLM", 5_000_000, 0).unwrap();
    protocol.borrow("alice", "USDC", 4_000_000, &oracle, 0).unwrap();
    protocol.drain_events();

    protocol.accrue_interest("USDC", 31_536_000).unwrap();

    let events = protocol.drain_events();
    assert!(
        events.iter().any(|e| matches!(e, ProtocolEvent::InterestAccrued { .. })),
        "expected an InterestAccrued event"
    );
}

#[test]
fn drain_events_clears_the_log() {
    let (mut protocol, _oracle) = setup_protocol();
    protocol.deposit("alice", "USDC", 1_000_000, 0).unwrap();

    let first_drain = protocol.drain_events();
    assert!(!first_drain.is_empty());

    let second_drain = protocol.drain_events();
    assert!(second_drain.is_empty(), "log should be empty after drain");
}

// ── Feature: oracle price sanity checks ──────────────────────────────────────

#[test]
fn oracle_rejects_zero_price() {
    let mut oracle = PriceOracleSim::new("oracle");
    let err = oracle.set_price("oracle", "XLM", 0).unwrap_err();
    assert!(
        matches!(err, ProtocolError::OracleSanityCheckFailed(_, _)),
        "expected OracleSanityCheckFailed, got {:?}",
        err
    );
}

#[test]
fn oracle_rejects_negative_price() {
    let mut oracle = PriceOracleSim::new("oracle");
    let err = oracle.set_price("oracle", "XLM", -1).unwrap_err();
    assert!(matches!(err, ProtocolError::OracleSanityCheckFailed(_, _)));
}

#[test]
fn oracle_rejects_price_above_configured_maximum() {
    let sanity = OracleSanityConfig {
        max_price: 2_000_000_000, // $2.00 max
        max_price_deviation_bps: 0, // disable circuit-breaker for this test
        ..OracleSanityConfig::default()
    };
    let mut oracle = PriceOracleSim::with_sanity("oracle", sanity);
    let err = oracle
        .set_price("oracle", "XLM", 3_000_000_000)
        .unwrap_err();
    assert!(matches!(err, ProtocolError::OracleSanityCheckFailed(_, _)));
}

#[test]
fn oracle_circuit_breaker_rejects_large_price_jump() {
    let sanity = OracleSanityConfig {
        max_price_deviation_bps: 500, // 5 % max deviation
        max_price_age_secs: 0,        // disable staleness for this test
        ..OracleSanityConfig::default()
    };
    let mut oracle = PriceOracleSim::with_sanity("oracle", sanity);

    // Set initial price.
    oracle.set_price("oracle", "XLM", 1_000_000_000).unwrap();

    // A 50 % jump should be rejected.
    let err = oracle
        .set_price("oracle", "XLM", 1_500_000_000)
        .unwrap_err();
    assert!(
        matches!(err, ProtocolError::OracleSanityCheckFailed(_, _)),
        "expected circuit-breaker to fire, got {:?}",
        err
    );
}

#[test]
fn oracle_circuit_breaker_allows_small_price_change() {
    let sanity = OracleSanityConfig {
        max_price_deviation_bps: 2_000, // 20 % max deviation
        max_price_age_secs: 0,
        ..OracleSanityConfig::default()
    };
    let mut oracle = PriceOracleSim::with_sanity("oracle", sanity);

    oracle.set_price("oracle", "XLM", 1_000_000_000).unwrap();
    // A 10 % change is within the 20 % threshold.
    oracle.set_price("oracle", "XLM", 1_100_000_000).unwrap();
    assert_eq!(oracle.get_price("XLM").unwrap(), 1_100_000_000);
}

#[test]
fn oracle_staleness_check_rejects_old_price() {
    let sanity = OracleSanityConfig {
        max_price_age_secs: 3_600, // 1 hour
        max_price_deviation_bps: 0,
        ..OracleSanityConfig::default()
    };
    let mut oracle = PriceOracleSim::with_sanity("oracle", sanity);

    // Record price at t=0.
    oracle.set_price_at("oracle", "XLM", 1_000_000_000, 0).unwrap();

    // Reading at t=7200 (2 hours later) should fail.
    let err = oracle.get_price_at("XLM", 7_200).unwrap_err();
    assert_eq!(err, ProtocolError::OraclePriceStale("XLM".to_string()));
}

#[test]
fn oracle_staleness_check_accepts_fresh_price() {
    let sanity = OracleSanityConfig {
        max_price_age_secs: 3_600,
        max_price_deviation_bps: 0,
        ..OracleSanityConfig::default()
    };
    let mut oracle = PriceOracleSim::with_sanity("oracle", sanity);

    // Record price at t=1000.
    oracle
        .set_price_at("oracle", "XLM", 1_000_000_000, 1_000)
        .unwrap();

    // Reading at t=2000 (1000 seconds later, within 1 hour) should succeed.
    let price = oracle.get_price_at("XLM", 2_000).unwrap();
    assert_eq!(price, 1_000_000_000);
}

#[test]
fn oracle_non_admin_cannot_change_sanity_config() {
    let mut oracle = PriceOracleSim::new("oracle");
    let err = oracle
        .set_sanity_config("attacker", OracleSanityConfig::default())
        .unwrap_err();
    assert_eq!(err, ProtocolError::Unauthorized);
}

// ── Feature: optimised liquidation ───────────────────────────────────────────

#[test]
fn optimised_liquidation_produces_same_result_as_before() {
    // Verify the refactored liquidation path produces the same economic outcome
    // as the original: correct repaid amount, seized collateral, and discount.
    let (mut protocol, mut oracle) = setup_protocol();
    protocol.deposit("lp", "USDC", 5_000_000, 0).unwrap();
    protocol.deposit("alice", "XLM", 1_000_000, 0).unwrap();
    protocol.borrow("alice", "USDC", 700_000, &oracle, 0).unwrap();

    // Drop XLM price to make position liquidatable.
    oracle.set_price("oracle", "XLM", 700_000_000).unwrap();
    let position_before = protocol.position("alice", &oracle).unwrap();
    assert!(position_before.health_factor < WAD);

    let result = protocol
        .liquidate("bob", "alice", "USDC", "XLM", 300_000, &oracle, 1)
        .unwrap();

    assert!(result.repaid_amount > 0, "should have repaid some debt");
    assert!(result.seized_collateral > 0, "should have seized some collateral");
    assert!(
        result.liquidator_discount_value > 0,
        "liquidator should receive a bonus"
    );

    let position_after = protocol.position("alice", &oracle).unwrap();
    assert!(
        position_after.debt_value < position_before.debt_value,
        "debt should decrease after liquidation"
    );
}

#[test]
fn liquidation_validates_before_mutating_state() {
    // A healthy position must not be liquidatable even with the new code path.
    let (mut protocol, oracle) = setup_protocol();
    protocol.deposit("lp", "USDC", 2_000_000, 0).unwrap();
    protocol.deposit("alice", "XLM", 1_000_000, 0).unwrap();
    protocol.borrow("alice", "USDC", 500_000, &oracle, 0).unwrap();

    let err = protocol
        .liquidate("bob", "alice", "USDC", "XLM", 100_000, &oracle, 0)
        .unwrap_err();
    assert_eq!(err, ProtocolError::PositionNotLiquidatable);
}
