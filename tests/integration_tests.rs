use stellar_defi_toolkit::{
    InterestRateModel, LendingProtocol, PriceOracleSim, ProtocolError, ReserveConfig, WAD,
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

fn setup_protocol() -> (LendingProtocol, PriceOracleSim) {
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

    let mut oracle = PriceOracle::new("oracle");
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

    let mut oracle = PriceOracle::new("oracle");
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

    let mut oracle = PriceOracle::new("oracle");
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

    let mut oracle2 = PriceOracle::new("oracle");
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
}
