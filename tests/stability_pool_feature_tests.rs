//! Stability Pool Feature Tests
//!
//! Covers:
//! - Issue #202: Withdrawal queue management
//! - Issue #203: Liquidation distribution automation
//! - Issue #204: Pool cap management with dynamic adjustment
//! - Issue #205: Multi-token stability pools

use soroban_sdk::{Env, Address, Symbol};
use stellar_defi_toolkit::contracts::stability_pool::{
    StabilityPoolContract, StabilityPoolContractClient,
};

mod common;
use common::setup_test_env;

// ─── Issue #202: Withdrawal Queue Management ──────────────────────────────────

#[test]
fn test_small_withdrawal_processed_immediately() {
    let (env, admin) = setup_test_env();
    let user = Address::generate(&env);
    let stablecoin = Address::generate(&env);
    let treasury = Address::generate(&env);

    let contract_id = env.register_contract(None, StabilityPoolContract);
    let client = StabilityPoolContractClient::new(&env, &contract_id);

    client.initialize(&admin, &stablecoin, &treasury);
    client.deposit(&user, &1_000_000_000);

    // Small withdrawal should process immediately
    client.request_withdrawal(&user, &100_000_000, &stablecoin);

    let queue = client.get_withdrawal_queue_status();
    assert_eq!(queue.len(), 0);
}

#[test]
fn test_large_withdrawal_enters_queue() {
    let (env, admin) = setup_test_env();
    let user = Address::generate(&env);
    let stablecoin = Address::generate(&env);
    let treasury = Address::generate(&env);

    let contract_id = env.register_contract(None, StabilityPoolContract);
    let client = StabilityPoolContractClient::new(&env, &contract_id);

    client.initialize(&admin, &stablecoin, &treasury);
    client.deposit(&user, &1_000_000_000);

    // Large withdrawal (>5% of pool) should enter queue
    client.request_withdrawal(&user, &100_000_000, &stablecoin);

    let queue = client.get_withdrawal_queue_status();
    assert_eq!(queue.len(), 1);
    assert_eq!(queue.get(0).unwrap().amount, 100_000_000);
    assert_eq!(queue.get(0).user, user);
    assert!(queue.get(0).emergency == false);
}

#[test]
fn test_emergency_withdrawal_bypasses_queue() {
    let (env, admin) = setup_test_env();
    let user = Address::generate(&env);
    let stablecoin = Address::generate(&env);
    let treasury = Address::generate(&env);

    let contract_id = env.register_contract(None, StabilityPoolContract);
    let client = StabilityPoolContractClient::new(&env, &contract_id);

    client.initialize(&admin, &stablecoin, &treasury);
    client.deposit(&user, &1_000_000_000);

    // Emergency withdrawal should bypass queue
    client.emergency_withdraw(&user, &500_000_000);

    let queue = client.get_withdrawal_queue_status();
    assert_eq!(queue.len(), 0);
}

#[test]
fn test_withdrawal_queue_fifo_order() {
    let (env, admin) = setup_test_env();
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let stablecoin = Address::generate(&env);
    let treasury = Address::generate(&env);

    let contract_id = env.register_contract(None, StabilityPoolContract);
    let client = StabilityPoolContractClient::new(&env, &contract_id);

    client.initialize(&admin, &stablecoin, &treasury);
    client.deposit(&user1, &1_000_000_000);
    client.deposit(&user2, &1_000_000_000);

    // Both request large withdrawals
    client.request_withdrawal(&user1, &100_000_000, &stablecoin);
    client.request_withdrawal(&user2, &100_000_000, &stablecoin);

    let queue = client.get_withdrawal_queue_status();
    assert_eq!(queue.len(), 2);
    assert_eq!(queue.get(0).unwrap().position, 1);
    assert_eq!(queue.get(1).unwrap().position, 2);
}

// ─── Issue #203: Liquidation Distribution Automation ─────────────────────────

#[test]
fn test_liquidation_distribution_updates_reward_index() {
    let (env, admin) = setup_test_env();
    let user = Address::generate(&env);
    let stablecoin = Address::generate(&env);
    let treasury = Address::generate(&env);

    let contract_id = env.register_contract(None, StabilityPoolContract);
    let client = StabilityPoolContractClient::new(&env, &contract_id);

    client.initialize(&admin, &stablecoin, &treasury);
    client.deposit(&user, &1_000_000_000);

    let liquidation_event = soroban_sdk::types::stability_pool::LiquidationEvent {
        vault_owner: Address::generate(&env),
        liquidator: Address::generate(&env),
        collateral_address: Address::generate(&env),
        collateral_amount: 1000,
        debt_repaid: 500,
        penalty_amount: 100,
    };

    client.process_liquidation_auto_distribute(&liquidation_event);

    let history = client.get_distribution_history();
    assert_eq!(history.len(), 1);
    assert_eq!(history.get(0).unwrap().total_amount, 80); // 80% of 100 penalty
}

// ─── Issue #204: Pool Cap Management ─────────────────────────────────────────

#[test]
fn test_dynamic_cap_based_on_supply() {
    let (env, admin) = setup_test_env();
    let stablecoin = Address::generate(&env);
    let treasury = Address::generate(&env);

    let contract_id = env.register_contract(None, StabilityPoolContract);
    let client = StabilityPoolContractClient::new(&env, &contract_id);

    client.initialize(&admin, &stablecoin, &treasury);

    let cap = client.get_current_cap();
    assert!(cap > 0);
}

#[test]
fn test_manual_cap_override() {
    let (env, admin) = setup_test_env();
    let stablecoin = Address::generate(&env);
    let treasury = Address::generate(&env);

    let contract_id = env.register_contract(None, StabilityPoolContract);
    let client = StabilityPoolContractClient::new(&env, &contract_id);

    client.initialize(&admin, &stablecoin, &treasury);

    let new_cap = 50_000_000_000;
    client.update_pool_cap(&new_cap, &Symbol::short("Governance vote"));

    let cap_params = client.get_pool_cap_params();
    assert_eq!(cap_params.manual_override, new_cap);
    assert_eq!(cap_params.last_change_rationale, Symbol::short("Governance vote"));
}

// ─── Issue #205: Multi-Token Support ─────────────────────────────────────────

#[test]
fn test_add_pool_token() {
    let (env, admin) = setup_test_env();
    let stablecoin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let token = Address::generate(&env);

    let contract_id = env.register_contract(None, StabilityPoolContract);
    let client = StabilityPoolContractClient::new(&env, &contract_id);

    client.initialize(&admin, &stablecoin, &treasury);

    client.add_pool_token(&token, &Symbol::short("USDC"), &1_000_000, &100_000_000_000, &7);

    let tokens = client.get_supported_tokens();
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens.get(0).unwrap().symbol, Symbol::short("USDC"));
}

#[test]
fn test_multi_token_deposit() {
    let (env, admin) = setup_test_env();
    let user = Address::generate(&env);
    let stablecoin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let token = Address::generate(&env);

    let contract_id = env.register_contract(None, StabilityPoolContract);
    let client = StabilityPoolContractClient::new(&env, &contract_id);

    client.initialize(&admin, &stablecoin, &treasury);
    client.add_pool_token(&token, &Symbol::short("USDC"), &1_000_000, &100_000_000_000, &7);

    client.deposit_token(&user, &token, &1_000_000_000);

    let deposits = client.get_user_token_deposits(&user);
    assert_eq!(deposits.len(), 1);
    assert_eq!(deposits.get(0).unwrap().amount, 1_000_000_000);
}
