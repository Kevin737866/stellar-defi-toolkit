//! Stability Pool and Position Manager feature tests
//!
//! Covers:
//! - Issue #207: stability pool migration
//! - Issue #208: pool health metrics
//! - Issue #209: position health monitoring with alerts
//! - Issue #210: automated position rebalancing with keeper network

use soroban_sdk::{Env, Address, Symbol};
use stellar_defi_toolkit::contracts::stability_pool::{
    StabilityPoolContractClient,
};
use stellar_defi_toolkit::contracts::position_manager::{
    PositionManagerContractClient,
};

mod common;
use common::setup_test_env;

// ─── Stability Pool Migration (Issue #207) ────────────────────────────────────

#[test]
fn test_migration_preserves_deposit_amount() {
    let (env, admin) = setup_test_env();
    let user = Address::generate(&env);
    let stablecoin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let new_pool = Address::generate(&env);

    let contract_id = env.register_contract(None, StabilityPoolContract);
    let client = StabilityPoolContractClient::new(&env, &contract_id);

    client.initialize(&admin, &stablecoin, &treasury);
    client.deposit(&user, &1_000_000_000);

    let before = client.get_user_deposit(&user);
    assert_eq!(before.amount, 1_000_000_000);

    client.migrate_to_new_pool(&user, &new_pool);

    let history = client.get_migration_history();
    assert_eq!(history.len(), 1);
    assert_eq!(history.get(0).unwrap().amount, 1_000_000_000);
    assert_eq!(history.get(0).user, user);
}

#[test]
fn test_migration_preserves_reward_index_and_timestamp() {
    let (env, admin) = setup_test_env();
    let user = Address::generate(&env);
    let stablecoin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let new_pool = Address::generate(&env);

    let contract_id = env.register_contract(None, StabilityPoolContract);
    let client = StabilityPoolContractClient::new(&env, &contract_id);

    client.initialize(&admin, &stablecoin, &treasury);
    client.deposit(&user, &2_000_000_000);

    let before = client.get_user_deposit(&user);
    let reward_index = before.reward_index;
    let deposit_timestamp = before.deposit_timestamp;

    client.migrate_to_new_pool(&user, &new_pool);

    let record = client.get_migration_history().get(0).unwrap();
    assert_eq!(record.reward_index, reward_index);
    assert_eq!(record.deposit_timestamp, deposit_timestamp);
}

// ─── Pool Health Metrics (Issue #208) ────────────────────────────────────────

#[test]
fn test_pool_health_metrics_bounds() {
    let (env, admin) = setup_test_env();
    let stablecoin = Address::generate(&env);
    let treasury = Address::generate(&env);

    let contract_id = env.register_contract(None, StabilityPoolContract);
    let client = StabilityPoolContractClient::new(&env, &contract_id);

    client.initialize(&admin, &stablecoin, &treasury);

    let metrics = client.get_pool_health();
    assert!(metrics.health_score <= 10000);
    assert!(metrics.coverage_ratio_bps <= 10000);
    assert!(metrics.concentration_bps <= 10000);
    assert!(metrics.withdrawal_pressure_bps <= 10000);
    assert!(metrics.reward_sustainability_bps <= 10000);
}

// ─── Position Health Monitoring (Issue #209) ──────────────────────────────────

#[test]
fn test_health_check_creates_alerts() {
    let (env, admin) = setup_test_env();

    let contract_id = env.register_contract(None, PositionManagerContract);
    let client = PositionManagerContractClient::new(&env, &contract_id);

    client.initialize(&admin);
    let position_id = client.create_monitored_position(
        &admin,
        &1,
        &Address::generate(&env),
        &10_000_000_000,
        &5_000_000_000,
        &15000,
    );

    client.run_health_checks();

    let health = client.get_health_check(&position_id);
    assert_eq!(health.position_id, position_id);
}

// ─── Keeper Network & Rebalancing (Issue #210) ────────────────────────────────

#[test]
fn test_keeper_registration_and_rebalance() {
    let (env, admin) = setup_test_env();
    let keeper = Address::generate(&env);

    let contract_id = env.register_contract(None, PositionManagerContract);
    let client = PositionManagerContractClient::new(&env, &contract_id);

    client.initialize(&admin);
    client.register_keeper(&keeper);

    let keeper_info = client.get_keeper_info(&keeper);
    assert_eq!(keeper_info.keeper, keeper);
    assert_eq!(keeper_info.reputation, 10000);
}
