//! Tests for issues #215, #216, #217, #218 – asset registry.

use stellar_defi_toolkit::contracts::asset_registry_protocol::{AssetRegistry, RegistryError};
use stellar_defi_toolkit::types::asset::{
    AssetRiskParams, ListChangeReason, MigrationStatus, ProtocolAssetMetadata, TokenStandard,
};
use std::collections::HashMap;

// ── helpers ──────────────────────────────────────────────────────────────────

fn registry() -> AssetRegistry {
    AssetRegistry::new(vec!["admin".to_string()])
}

fn usdc_meta(now: u64) -> ProtocolAssetMetadata {
    ProtocolAssetMetadata {
        asset_id: "USDC".to_string(),
        name: "USD Coin".to_string(),
        symbol: "USDC".to_string(),
        decimals: 6,
        contract_address: "CABC123".to_string(),
        standard: TokenStandard::Sep41,
        active: true,
        registered_at: now,
        last_updated_at: now,
    }
}

fn xlm_meta(now: u64) -> ProtocolAssetMetadata {
    ProtocolAssetMetadata {
        asset_id: "XLM".to_string(),
        name: "Stellar Lumens".to_string(),
        symbol: "XLM".to_string(),
        decimals: 7,
        contract_address: "".to_string(),
        standard: TokenStandard::StellarNative,
        active: true,
        registered_at: now,
        last_updated_at: now,
    }
}

fn usdc_risk() -> AssetRiskParams {
    AssetRiskParams {
        asset_id: "USDC".to_string(),
        ltv_bps: 8000,
        liquidation_threshold_bps: 8500,
        liquidation_bonus_bps: 500,
        oracle_source: "pyth".to_string(),
        last_updated_at: 0,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Issue #215 – Allowlist / Blocklist
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn allowlist_asset_succeeds_for_admin() {
    let mut reg = registry();
    reg.allowlist_asset("admin", "USDC", ListChangeReason::GovernanceApproval, 1)
        .unwrap();
    assert!(reg.is_allowed("USDC"));
}

#[test]
fn allowlist_rejects_non_admin() {
    let mut reg = registry();
    let err = reg
        .allowlist_asset("eve", "USDC", ListChangeReason::GovernanceApproval, 1)
        .unwrap_err();
    assert_eq!(err, RegistryError::Unauthorized);
}

#[test]
fn allowlist_duplicate_returns_error() {
    let mut reg = registry();
    reg.allowlist_asset("admin", "USDC", ListChangeReason::GovernanceApproval, 1)
        .unwrap();
    let err = reg
        .allowlist_asset("admin", "USDC", ListChangeReason::GovernanceApproval, 2)
        .unwrap_err();
    assert_eq!(err, RegistryError::AlreadyInList);
}

#[test]
fn remove_from_allowlist_works() {
    let mut reg = registry();
    reg.allowlist_asset("admin", "USDC", ListChangeReason::GovernanceApproval, 1)
        .unwrap();
    reg.remove_from_allowlist("admin", "USDC").unwrap();
    assert!(!reg.is_allowed("USDC"));
}

#[test]
fn remove_from_allowlist_not_present_returns_error() {
    let mut reg = registry();
    let err = reg.remove_from_allowlist("admin", "USDC").unwrap_err();
    assert_eq!(err, RegistryError::NotInList);
}

#[test]
fn blocklist_asset_succeeds() {
    let mut reg = registry();
    reg.blocklist_asset("admin", "BAD", ListChangeReason::EmergencyBlock, 1)
        .unwrap();
    assert!(reg.is_blocked("BAD"));
}

#[test]
fn blocklist_takes_precedence_over_allowlist() {
    let mut reg = registry();
    reg.allowlist_asset("admin", "USDC", ListChangeReason::GovernanceApproval, 1)
        .unwrap();
    reg.blocklist_asset("admin", "USDC", ListChangeReason::EmergencyBlock, 2)
        .unwrap();
    // Even though USDC is allowlisted, the blocklist wins.
    assert!(!reg.is_allowed("USDC"));
    assert!(reg.is_blocked("USDC"));
}

#[test]
fn remove_from_blocklist_works() {
    let mut reg = registry();
    reg.blocklist_asset("admin", "BAD", ListChangeReason::EmergencyBlock, 1)
        .unwrap();
    reg.remove_from_blocklist("admin", "BAD").unwrap();
    assert!(!reg.is_blocked("BAD"));
}

#[test]
fn blocklist_rejects_non_admin() {
    let mut reg = registry();
    let err = reg
        .blocklist_asset("eve", "BAD", ListChangeReason::EmergencyBlock, 1)
        .unwrap_err();
    assert_eq!(err, RegistryError::Unauthorized);
}

#[test]
fn events_emitted_for_allowlist_changes() {
    use stellar_defi_toolkit::types::asset::ListChangeEvent;
    use stellar_defi_toolkit::contracts::asset_registry_protocol::RegistryEvent;

    let mut reg = registry();
    reg.allowlist_asset("admin", "USDC", ListChangeReason::GovernanceApproval, 1)
        .unwrap();
    reg.remove_from_allowlist("admin", "USDC").unwrap();

    let events = reg.drain_events();
    assert_eq!(events.len(), 2);
    assert!(matches!(
        &events[0],
        RegistryEvent::ListChange(ListChangeEvent::AllowlistAdded { asset_id, .. })
        if asset_id == "USDC"
    ));
    assert!(matches!(
        &events[1],
        RegistryEvent::ListChange(ListChangeEvent::AllowlistRemoved { asset_id, .. })
        if asset_id == "USDC"
    ));
}

#[test]
fn events_emitted_for_blocklist_changes() {
    use stellar_defi_toolkit::types::asset::ListChangeEvent;
    use stellar_defi_toolkit::contracts::asset_registry_protocol::RegistryEvent;

    let mut reg = registry();
    reg.blocklist_asset("admin", "BAD", ListChangeReason::EmergencyBlock, 1)
        .unwrap();
    reg.remove_from_blocklist("admin", "BAD").unwrap();

    let events = reg.drain_events();
    assert_eq!(events.len(), 2);
    assert!(matches!(
        &events[0],
        RegistryEvent::ListChange(ListChangeEvent::BlocklistAdded { asset_id, .. })
        if asset_id == "BAD"
    ));
    assert!(matches!(
        &events[1],
        RegistryEvent::ListChange(ListChangeEvent::BlocklistRemoved { asset_id, .. })
        if asset_id == "BAD"
    ));
}

// ─────────────────────────────────────────────────────────────────────────────
// Issue #216 – Asset Metadata Registry with Token Standards
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn register_asset_metadata_succeeds() {
    let mut reg = registry();
    reg.register_asset("admin", usdc_meta(1), 1).unwrap();
    let meta = reg.get_asset_metadata("USDC").unwrap();
    assert_eq!(meta.symbol, "USDC");
    assert_eq!(meta.decimals, 6);
    assert_eq!(meta.standard, TokenStandard::Sep41);
    assert_eq!(meta.contract_address, "CABC123");
}

#[test]
fn register_asset_sets_active_and_timestamps() {
    let mut reg = registry();
    reg.register_asset("admin", usdc_meta(42), 42).unwrap();
    let meta = reg.get_asset_metadata("USDC").unwrap();
    assert!(meta.active);
    assert_eq!(meta.registered_at, 42);
    assert_eq!(meta.last_updated_at, 42);
}

#[test]
fn register_asset_duplicate_returns_error() {
    let mut reg = registry();
    reg.register_asset("admin", usdc_meta(1), 1).unwrap();
    let err = reg.register_asset("admin", usdc_meta(2), 2).unwrap_err();
    assert_eq!(err, RegistryError::AlreadyRegistered);
}

#[test]
fn register_asset_rejects_non_admin() {
    let mut reg = registry();
    let err = reg
        .register_asset("eve", usdc_meta(1), 1)
        .unwrap_err();
    assert_eq!(err, RegistryError::Unauthorized);
}

#[test]
fn register_native_asset_with_contract_address_fails_validation() {
    let mut reg = registry();
    let mut bad_meta = xlm_meta(1);
    bad_meta.contract_address = "CSHOULD_NOT_EXIST".to_string();
    let err = reg.register_asset("admin", bad_meta, 1).unwrap_err();
    assert!(matches!(err, RegistryError::ValidationError(_)));
}

#[test]
fn register_sep41_without_contract_address_fails_validation() {
    let mut reg = registry();
    let mut bad_meta = usdc_meta(1);
    bad_meta.contract_address = "".to_string();
    let err = reg.register_asset("admin", bad_meta, 1).unwrap_err();
    assert!(matches!(err, RegistryError::ValidationError(_)));
}

#[test]
fn register_native_asset_without_contract_address_succeeds() {
    let mut reg = registry();
    reg.register_asset("admin", xlm_meta(1), 1).unwrap();
    let meta = reg.get_asset_metadata("XLM").unwrap();
    assert_eq!(meta.standard, TokenStandard::StellarNative);
    assert_eq!(meta.contract_address, "");
}

#[test]
fn update_asset_metadata_succeeds() {
    let mut reg = registry();
    reg.register_asset("admin", usdc_meta(1), 1).unwrap();

    let mut updated = usdc_meta(1);
    updated.name = "USD Circle Coin".to_string();
    reg.update_asset_metadata("admin", updated, 100).unwrap();

    let meta = reg.get_asset_metadata("USDC").unwrap();
    assert_eq!(meta.name, "USD Circle Coin");
    assert_eq!(meta.last_updated_at, 100);
    assert_eq!(meta.registered_at, 1); // original preserved
}

#[test]
fn update_metadata_for_unregistered_asset_returns_error() {
    let mut reg = registry();
    let err = reg
        .update_asset_metadata("admin", usdc_meta(1), 1)
        .unwrap_err();
    assert_eq!(err, RegistryError::NotRegistered);
}

#[test]
fn all_assets_returns_all_registered() {
    let mut reg = registry();
    reg.register_asset("admin", usdc_meta(1), 1).unwrap();
    reg.register_asset("admin", xlm_meta(1), 1).unwrap();
    assert_eq!(reg.all_assets().len(), 2);
}

#[test]
fn is_registered_returns_correct_values() {
    let mut reg = registry();
    assert!(!reg.is_registered("USDC"));
    reg.register_asset("admin", usdc_meta(1), 1).unwrap();
    assert!(reg.is_registered("USDC"));
}

// ─────────────────────────────────────────────────────────────────────────────
// Issue #217 – Asset Risk Parameters per Collateral Type
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn set_risk_params_succeeds() {
    let mut reg = registry();
    reg.register_asset("admin", usdc_meta(1), 1).unwrap();
    reg.set_risk_params("admin", usdc_risk(), 1).unwrap();
    let params = reg.get_risk_params("USDC").unwrap();
    assert_eq!(params.ltv_bps, 8000);
    assert_eq!(params.liquidation_threshold_bps, 8500);
    assert_eq!(params.liquidation_bonus_bps, 500);
    assert_eq!(params.oracle_source, "pyth");
}

#[test]
fn set_risk_params_records_timestamp() {
    let mut reg = registry();
    reg.register_asset("admin", usdc_meta(1), 1).unwrap();
    reg.set_risk_params("admin", usdc_risk(), 99).unwrap();
    assert_eq!(reg.get_risk_params("USDC").unwrap().last_updated_at, 99);
}

#[test]
fn set_risk_params_rejects_non_admin() {
    let mut reg = registry();
    reg.register_asset("admin", usdc_meta(1), 1).unwrap();
    let err = reg
        .set_risk_params("eve", usdc_risk(), 1)
        .unwrap_err();
    assert_eq!(err, RegistryError::Unauthorized);
}

#[test]
fn set_risk_params_requires_asset_registered() {
    let mut reg = registry();
    let err = reg.set_risk_params("admin", usdc_risk(), 1).unwrap_err();
    assert_eq!(err, RegistryError::NotRegistered);
}

#[test]
fn set_risk_params_rejects_ltv_above_threshold() {
    let mut reg = registry();
    reg.register_asset("admin", usdc_meta(1), 1).unwrap();
    let bad = AssetRiskParams {
        asset_id: "USDC".to_string(),
        ltv_bps: 9000,
        liquidation_threshold_bps: 8500, // LTV > threshold — invalid
        liquidation_bonus_bps: 500,
        oracle_source: "pyth".to_string(),
        last_updated_at: 0,
    };
    let err = reg.set_risk_params("admin", bad, 1).unwrap_err();
    assert_eq!(err, RegistryError::InvalidRiskParams);
}

#[test]
fn set_risk_params_rejects_threshold_above_10000() {
    let mut reg = registry();
    reg.register_asset("admin", usdc_meta(1), 1).unwrap();
    let bad = AssetRiskParams {
        asset_id: "USDC".to_string(),
        ltv_bps: 8000,
        liquidation_threshold_bps: 10_001,
        liquidation_bonus_bps: 500,
        oracle_source: "pyth".to_string(),
        last_updated_at: 0,
    };
    let err = reg.set_risk_params("admin", bad, 1).unwrap_err();
    assert_eq!(err, RegistryError::InvalidRiskParams);
}

#[test]
fn risk_params_queryable_returns_none_when_not_set() {
    let mut reg = registry();
    reg.register_asset("admin", usdc_meta(1), 1).unwrap();
    assert!(reg.get_risk_params("USDC").is_none());
}

#[test]
fn risk_params_can_be_updated() {
    let mut reg = registry();
    reg.register_asset("admin", usdc_meta(1), 1).unwrap();
    reg.set_risk_params("admin", usdc_risk(), 1).unwrap();

    let updated = AssetRiskParams {
        asset_id: "USDC".to_string(),
        ltv_bps: 7500,
        liquidation_threshold_bps: 8000,
        liquidation_bonus_bps: 300,
        oracle_source: "band".to_string(),
        last_updated_at: 0,
    };
    reg.set_risk_params("admin", updated, 50).unwrap();
    let params = reg.get_risk_params("USDC").unwrap();
    assert_eq!(params.ltv_bps, 7500);
    assert_eq!(params.oracle_source, "band");
}

#[test]
fn all_risk_params_returns_all_entries() {
    let mut reg = registry();
    reg.register_asset("admin", usdc_meta(1), 1).unwrap();
    reg.register_asset("admin", xlm_meta(1), 1).unwrap();
    reg.set_risk_params("admin", usdc_risk(), 1).unwrap();
    let xlm_risk = AssetRiskParams {
        asset_id: "XLM".to_string(),
        ltv_bps: 7000,
        liquidation_threshold_bps: 7500,
        liquidation_bonus_bps: 1000,
        oracle_source: "pyth".to_string(),
        last_updated_at: 0,
    };
    reg.set_risk_params("admin", xlm_risk, 1).unwrap();
    assert_eq!(reg.all_risk_params().len(), 2);
}

#[test]
fn risk_params_event_emitted() {
    use stellar_defi_toolkit::contracts::asset_registry_protocol::RegistryEvent;

    let mut reg = registry();
    reg.register_asset("admin", usdc_meta(1), 1).unwrap();
    reg.drain_events(); // clear registration event
    reg.set_risk_params("admin", usdc_risk(), 1).unwrap();

    let events = reg.drain_events();
    assert_eq!(events.len(), 1);
    assert!(matches!(
        &events[0],
        RegistryEvent::RiskParamsSet { asset_id, .. } if asset_id == "USDC"
    ));
}

// ─────────────────────────────────────────────────────────────────────────────
// Issue #218 – Asset Upgrade and Migration Path
// ─────────────────────────────────────────────────────────────────────────────

fn sample_balances() -> HashMap<String, u64> {
    let mut b = HashMap::new();
    b.insert("alice".to_string(), 600);
    b.insert("bob".to_string(), 400);
    b
}

fn sample_allowances() -> HashMap<String, HashMap<String, u64>> {
    let mut inner = HashMap::new();
    inner.insert("spender1".to_string(), 100u64);
    let mut a = HashMap::new();
    a.insert("alice".to_string(), inner);
    a
}

#[test]
fn initiate_migration_succeeds() {
    let mut reg = registry();
    reg.register_asset("admin", usdc_meta(1), 1).unwrap();
    reg.initiate_migration(
        "admin", "USDC", "OLD_CONTRACT", "NEW_CONTRACT",
        sample_balances(), sample_allowances(), 1000, 10,
    ).unwrap();
    let state = reg.get_migration_state("USDC").unwrap();
    assert_eq!(state.status, MigrationStatus::InProgress);
    assert_eq!(state.old_contract, "OLD_CONTRACT");
    assert_eq!(state.new_contract, "NEW_CONTRACT");
    assert_eq!(state.total_supply_snapshot, 1000);
}

#[test]
fn initiate_migration_rejects_non_admin() {
    let mut reg = registry();
    reg.register_asset("admin", usdc_meta(1), 1).unwrap();
    let err = reg.initiate_migration(
        "eve", "USDC", "OLD", "NEW",
        sample_balances(), sample_allowances(), 1000, 10,
    ).unwrap_err();
    assert_eq!(err, RegistryError::Unauthorized);
}

#[test]
fn initiate_migration_requires_registered_asset() {
    let mut reg = registry();
    let err = reg.initiate_migration(
        "admin", "USDC", "OLD", "NEW",
        sample_balances(), sample_allowances(), 1000, 10,
    ).unwrap_err();
    assert_eq!(err, RegistryError::NotRegistered);
}

#[test]
fn initiate_migration_rejects_balance_sum_mismatch() {
    let mut reg = registry();
    reg.register_asset("admin", usdc_meta(1), 1).unwrap();
    // balances sum to 1000 but we pass 999 as total_supply
    let err = reg.initiate_migration(
        "admin", "USDC", "OLD", "NEW",
        sample_balances(), sample_allowances(), 999, 10,
    ).unwrap_err();
    assert!(matches!(err, RegistryError::ValidationError(_)));
}

#[test]
fn initiate_migration_rejects_duplicate_active_migration() {
    let mut reg = registry();
    reg.register_asset("admin", usdc_meta(1), 1).unwrap();
    reg.initiate_migration(
        "admin", "USDC", "OLD", "NEW",
        sample_balances(), sample_allowances(), 1000, 10,
    ).unwrap();
    let err = reg.initiate_migration(
        "admin", "USDC", "OLD2", "NEW2",
        sample_balances(), sample_allowances(), 1000, 20,
    ).unwrap_err();
    assert_eq!(err, RegistryError::MigrationAlreadyActive);
}

#[test]
fn complete_migration_verifies_balances_on_new_contract() {
    let mut reg = registry();
    reg.register_asset("admin", usdc_meta(1), 1).unwrap();
    reg.initiate_migration(
        "admin", "USDC", "OLD", "NEW",
        sample_balances(), sample_allowances(), 1000, 10,
    ).unwrap();

    // Provide matching new-contract balances
    reg.complete_migration("admin", "USDC", &sample_balances(), 20).unwrap();

    let state = reg.get_migration_state("USDC").unwrap();
    assert_eq!(state.status, MigrationStatus::Completed);
    assert!(state.old_contract_paused);
    assert_eq!(state.completed_at, 20);
}

#[test]
fn complete_migration_fails_on_balance_mismatch() {
    let mut reg = registry();
    reg.register_asset("admin", usdc_meta(1), 1).unwrap();
    reg.initiate_migration(
        "admin", "USDC", "OLD", "NEW",
        sample_balances(), sample_allowances(), 1000, 10,
    ).unwrap();

    // Incorrect balances on new contract
    let mut wrong = sample_balances();
    wrong.insert("alice".to_string(), 500); // should be 600
    let err = reg.complete_migration("admin", "USDC", &wrong, 20).unwrap_err();
    assert!(matches!(err, RegistryError::ValidationError(_)));
}

#[test]
fn complete_migration_emits_audit_events() {
    use stellar_defi_toolkit::contracts::asset_registry_protocol::RegistryEvent;
    use stellar_defi_toolkit::types::asset::MigrationEvent;

    let mut reg = registry();
    reg.register_asset("admin", usdc_meta(1), 1).unwrap();
    reg.initiate_migration(
        "admin", "USDC", "OLD", "NEW",
        sample_balances(), sample_allowances(), 1000, 10,
    ).unwrap();
    reg.drain_events(); // clear prior events

    reg.complete_migration("admin", "USDC", &sample_balances(), 20).unwrap();

    let events = reg.drain_events();
    // Should emit: BalancesMigrated, AllowancesMigrated, OldContractPaused, Completed
    assert_eq!(events.len(), 4);
    assert!(matches!(
        &events[0],
        RegistryEvent::Migration(MigrationEvent::BalancesMigrated { asset_id, .. })
        if asset_id == "USDC"
    ));
    assert!(matches!(
        &events[3],
        RegistryEvent::Migration(MigrationEvent::Completed { asset_id, .. })
        if asset_id == "USDC"
    ));
}

#[test]
fn cancel_migration_works() {
    let mut reg = registry();
    reg.register_asset("admin", usdc_meta(1), 1).unwrap();
    reg.initiate_migration(
        "admin", "USDC", "OLD", "NEW",
        sample_balances(), sample_allowances(), 1000, 10,
    ).unwrap();
    reg.cancel_migration("admin", "USDC").unwrap();
    let state = reg.get_migration_state("USDC").unwrap();
    assert_eq!(state.status, MigrationStatus::Cancelled);
}

#[test]
fn cancel_migration_emits_event() {
    use stellar_defi_toolkit::contracts::asset_registry_protocol::RegistryEvent;
    use stellar_defi_toolkit::types::asset::MigrationEvent;

    let mut reg = registry();
    reg.register_asset("admin", usdc_meta(1), 1).unwrap();
    reg.initiate_migration(
        "admin", "USDC", "OLD", "NEW",
        sample_balances(), sample_allowances(), 1000, 10,
    ).unwrap();
    reg.drain_events();
    reg.cancel_migration("admin", "USDC").unwrap();

    let events = reg.drain_events();
    assert_eq!(events.len(), 1);
    assert!(matches!(
        &events[0],
        RegistryEvent::Migration(MigrationEvent::Cancelled { asset_id, .. })
        if asset_id == "USDC"
    ));
}

#[test]
fn cancel_completed_migration_returns_error() {
    let mut reg = registry();
    reg.register_asset("admin", usdc_meta(1), 1).unwrap();
    reg.initiate_migration(
        "admin", "USDC", "OLD", "NEW",
        sample_balances(), sample_allowances(), 1000, 10,
    ).unwrap();
    reg.complete_migration("admin", "USDC", &sample_balances(), 20).unwrap();
    let err = reg.cancel_migration("admin", "USDC").unwrap_err();
    assert_eq!(err, RegistryError::MigrationNotInProgress);
}

#[test]
fn migration_initiation_event_emitted() {
    use stellar_defi_toolkit::contracts::asset_registry_protocol::RegistryEvent;
    use stellar_defi_toolkit::types::asset::MigrationEvent;

    let mut reg = registry();
    reg.register_asset("admin", usdc_meta(1), 1).unwrap();
    reg.drain_events();
    reg.initiate_migration(
        "admin", "USDC", "OLD", "NEW",
        sample_balances(), sample_allowances(), 1000, 10,
    ).unwrap();

    let events = reg.drain_events();
    assert_eq!(events.len(), 1);
    assert!(matches!(
        &events[0],
        RegistryEvent::Migration(MigrationEvent::Initiated { asset_id, .. })
        if asset_id == "USDC"
    ));
}

// ─────────────────────────────────────────────────────────────────────────────
// Issue #215 – Lending protocol registry integration
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn lending_deposit_blocked_when_asset_not_allowed() {
    use stellar_defi_toolkit::{InterestRateModel, LendingProtocol, ProtocolError, ReserveConfig};

    let mut protocol = LendingProtocol::new(
        vec!["admin".to_string()], 1, "treasury", InterestRateModel::default(),
    );
    protocol.register_asset("admin", ReserveConfig {
        asset: "USDC".to_string(),
        decimals: 6,
        collateral_factor_bps: 8000,
        liquidation_threshold_bps: 8500,
        liquidation_bonus_bps: 500,
        reserve_factor_bps: 1000,
        flash_loan_fee_bps: 9,
        borrow_enabled: true,
        deposit_enabled: true,
        flash_loan_enabled: true,
        supply_cap: 0,
        borrow_cap: 0,
        interest_rate_model: None,
    }, 0).unwrap();

    // Attach registry with USDC NOT allowlisted
    let reg = AssetRegistry::new(vec!["admin".to_string()]);
    protocol.attach_asset_registry(reg);

    let err = protocol.deposit("alice", "USDC", 1_000_000, 0).unwrap_err();
    assert!(matches!(err, ProtocolError::AssetNotAllowed(_)));
}

#[test]
fn lending_deposit_succeeds_when_asset_allowed() {
    use stellar_defi_toolkit::{InterestRateModel, LendingProtocol, ReserveConfig};

    let mut protocol = LendingProtocol::new(
        vec!["admin".to_string()], 1, "treasury", InterestRateModel::default(),
    );
    protocol.register_asset("admin", ReserveConfig {
        asset: "USDC".to_string(),
        decimals: 6,
        collateral_factor_bps: 8000,
        liquidation_threshold_bps: 8500,
        liquidation_bonus_bps: 500,
        reserve_factor_bps: 1000,
        flash_loan_fee_bps: 9,
        borrow_enabled: true,
        deposit_enabled: true,
        flash_loan_enabled: true,
        supply_cap: 0,
        borrow_cap: 0,
        interest_rate_model: None,
    }, 0).unwrap();

    let mut reg = AssetRegistry::new(vec!["admin".to_string()]);
    reg.allowlist_asset("admin", "USDC", ListChangeReason::GovernanceApproval, 1).unwrap();
    protocol.attach_asset_registry(reg);

    let shares = protocol.deposit("alice", "USDC", 1_000_000, 0).unwrap();
    assert_eq!(shares, 1_000_000);
}

#[test]
fn lending_deposit_blocked_when_asset_blocklisted() {
    use stellar_defi_toolkit::{InterestRateModel, LendingProtocol, ProtocolError, ReserveConfig};

    let mut protocol = LendingProtocol::new(
        vec!["admin".to_string()], 1, "treasury", InterestRateModel::default(),
    );
    protocol.register_asset("admin", ReserveConfig {
        asset: "USDC".to_string(),
        decimals: 6,
        collateral_factor_bps: 8000,
        liquidation_threshold_bps: 8500,
        liquidation_bonus_bps: 500,
        reserve_factor_bps: 1000,
        flash_loan_fee_bps: 9,
        borrow_enabled: true,
        deposit_enabled: true,
        flash_loan_enabled: true,
        supply_cap: 0,
        borrow_cap: 0,
        interest_rate_model: None,
    }, 0).unwrap();

    let mut reg = AssetRegistry::new(vec!["admin".to_string()]);
    reg.allowlist_asset("admin", "USDC", ListChangeReason::GovernanceApproval, 1).unwrap();
    reg.blocklist_asset("admin", "USDC", ListChangeReason::EmergencyBlock, 2).unwrap();
    protocol.attach_asset_registry(reg);

    let err = protocol.deposit("alice", "USDC", 1_000_000, 0).unwrap_err();
    assert!(matches!(err, ProtocolError::AssetNotAllowed(_)));
}
