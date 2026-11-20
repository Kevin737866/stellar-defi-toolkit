//! Tests for the yield vault: deposit/withdrawal queue (#145), emergency
//! withdrawal (#149), and keeper compounding (#150).

use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env, Symbol, Vec};

use stellar_defi_toolkit::contracts::vault::{YieldVaultContract, YieldVaultContractClient};
use stellar_defi_toolkit::types::vault::{QueueKind, StrategyType, VaultStrategy};

const BASE_TIME: u64 = 1_700_000_000;
const HOUR: u64 = 3_600;

struct Fixture<'a> {
    env: Env,
    admin: Address,
    treasury: Address,
    client: YieldVaultContractClient<'a>,
}

fn setup() -> Fixture<'static> {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(BASE_TIME);

    let contract_id = env.register_contract(None, YieldVaultContract);
    let client = YieldVaultContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let asset = Address::generate(&env);
    let share_token = Address::generate(&env);

    client.initialize(&admin, &treasury, &asset, &share_token, &1_000);

    Fixture {
        env,
        admin,
        treasury,
        client,
    }
}

fn strategy(env: &Env, name: &str, apy_bps: u32) -> VaultStrategy {
    VaultStrategy {
        name: Symbol::new(env, name),
        contract_address: Address::generate(env),
        strategy_type: StrategyType::Lending,
        estimated_apy_bps: apy_bps,
        allocated_amount: 0,
        active: true,
    }
}

/// Seed the vault through a depositor so TVL-based thresholds become active.
fn seed(fixture: &Fixture, amount: u64) -> Address {
    let user = Address::generate(&fixture.env);
    fixture.client.deposit(&user, &amount);
    user
}

// ─── Queue: threshold routing (#145) ─────────────────────────────────────────

#[test]
fn small_deposit_executes_immediately() {
    let f = setup();
    let user = Address::generate(&f.env);

    let result = f.client.deposit(&user, &1_000);

    assert!(!result.queued);
    assert_eq!(result.amount_deposited, 1_000);
    assert_eq!(result.shares_minted, 1_000);
    assert_eq!(result.request_id, 0);
    assert_eq!(f.client.balance_of(&user), 1_000);
    assert_eq!(f.client.queue_summary().pending_deposits, 0);
}

#[test]
fn large_deposit_is_queued_with_position_and_estimate() {
    let f = setup();
    seed(&f, 1_000_000);
    let whale = Address::generate(&f.env);

    // Threshold defaults to 1% of TVL = 10_000, so 50_000 must queue.
    let result = f.client.deposit(&whale, &50_000);

    assert!(result.queued);
    assert_eq!(result.amount_deposited, 0);
    assert_eq!(result.shares_minted, 0);
    assert_ne!(result.request_id, 0);
    assert_eq!(result.queue_position, 1);
    assert_eq!(result.estimated_processing_at, BASE_TIME + HOUR);
    assert_eq!(f.client.balance_of(&whale), 0, "shares mint only on processing");

    let summary = f.client.queue_summary();
    assert_eq!(summary.pending_deposits, 1);
    assert_eq!(summary.queued_deposit_amount, 50_000);
    assert_eq!(summary.processing_interval, HOUR);
}

#[test]
fn queue_position_tracks_arrival_order() {
    let f = setup();
    seed(&f, 1_000_000);
    let first = Address::generate(&f.env);
    let second = Address::generate(&f.env);
    let third = Address::generate(&f.env);

    let r1 = f.client.deposit(&first, &50_000);
    let r2 = f.client.deposit(&second, &60_000);
    let r3 = f.client.deposit(&third, &70_000);

    assert_eq!(r1.queue_position, 1);
    assert_eq!(r2.queue_position, 2);
    assert_eq!(r3.queue_position, 3);
    assert_eq!(f.client.queue_position_of(&r2.request_id), 2);
    assert_eq!(f.client.queue_position_of(&r1.request_id), 1);
}

#[test]
fn deposits_and_withdrawals_queue_independently() {
    let f = setup();
    let holder = seed(&f, 1_000_000);
    let whale = Address::generate(&f.env);

    let deposit = f.client.deposit(&whale, &50_000);
    let withdrawal = f.client.withdraw(&holder, &500_000);

    assert!(deposit.queued);
    assert!(withdrawal.queued);
    assert_eq!(deposit.queue_position, 1);
    assert_eq!(withdrawal.queue_position, 1, "separate queue");

    let summary = f.client.queue_summary();
    assert_eq!(summary.pending_deposits, 1);
    assert_eq!(summary.pending_withdrawals, 1);
    assert_eq!(summary.queued_withdrawal_shares, 500_000);
}

#[test]
fn small_withdrawal_executes_immediately() {
    let f = setup();
    let user = seed(&f, 1_000_000);

    let result = f.client.withdraw(&user, &5_000);

    assert!(!result.queued);
    assert_eq!(result.shares_burned, 5_000);
    assert_eq!(result.amount_withdrawn, 5_000);
    assert_eq!(f.client.balance_of(&user), 995_000);
}

#[test]
fn withdraw_more_than_owned_panics() {
    let f = setup();
    let user = seed(&f, 1_000);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client.withdraw(&user, &2_000);
    }));

    assert!(result.is_err(), "cannot withdraw more shares than owned");
}

// ─── Queue: cancellation (#145) ──────────────────────────────────────────────

#[test]
fn owner_can_cancel_queued_request() {
    let f = setup();
    seed(&f, 1_000_000);
    let whale = Address::generate(&f.env);
    let queued = f.client.deposit(&whale, &50_000);

    f.client.cancel_queue_request(&whale, &queued.request_id);

    assert_eq!(f.client.queue_summary().pending_deposits, 0);
    assert_eq!(f.client.get_owner_requests(&whale).len(), 0);
    assert_eq!(f.client.queue_position_of(&queued.request_id), 0);
}

#[test]
fn cancel_removes_entry_and_preserves_other_requests() {
    let f = setup();
    seed(&f, 1_000_000);
    let first = Address::generate(&f.env);
    let second = Address::generate(&f.env);
    let r1 = f.client.deposit(&first, &50_000);
    let r2 = f.client.deposit(&second, &60_000);

    f.client.cancel_queue_request(&first, &r1.request_id);

    assert_eq!(f.client.queue_position_of(&r2.request_id), 1, "second moves up");
    assert_eq!(f.client.balance_of(&first), 0);
    assert_eq!(f.client.queue_summary().pending_deposits, 1);
}

#[test]
fn cannot_cancel_someone_elses_request() {
    let f = setup();
    seed(&f, 1_000_000);
    let whale = Address::generate(&f.env);
    let attacker = Address::generate(&f.env);
    let queued = f.client.deposit(&whale, &50_000);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client.cancel_queue_request(&attacker, &queued.request_id);
    }));

    assert!(result.is_err());
    assert_eq!(f.client.queue_summary().pending_deposits, 1);
}

#[test]
fn cancelling_unknown_request_panics() {
    let f = setup();
    let user = Address::generate(&f.env);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client.cancel_queue_request(&user, &9_999);
    }));

    assert!(result.is_err());
}

#[test]
fn cancelled_capital_never_enters_the_vault() {
    let f = setup();
    let holder = seed(&f, 1_000_000);
    let whale = Address::generate(&f.env);
    let queued = f.client.deposit(&whale, &50_000);

    f.client.cancel_queue_request(&whale, &queued.request_id);

    let info = f.client.get_info();
    assert_eq!(info.total_assets, 1_000_000);
    assert_eq!(info.total_shares, 1_000_000);
    assert_eq!(f.client.balance_of(&holder), 1_000_000);
}

// ─── Queue: processing (#145) ────────────────────────────────────────────────

#[test]
fn process_queue_before_window_panics() {
    let f = setup();
    seed(&f, 1_000_000);
    let whale = Address::generate(&f.env);
    f.client.deposit(&whale, &50_000);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client.process_queue(&f.admin);
    }));

    assert!(result.is_err(), "must wait for the processing window");
}

#[test]
fn process_queue_executes_eligible_requests() {
    let f = setup();
    seed(&f, 1_000_000);
    let whale = Address::generate(&f.env);
    f.client.deposit(&whale, &50_000);

    f.env.ledger().set_timestamp(BASE_TIME + HOUR);
    let result = f.client.process_queue(&f.admin);

    assert_eq!(result.deposits_processed, 1);
    assert_eq!(result.deposited_amount, 50_000);
    assert_eq!(f.client.balance_of(&whale), 50_000);
    assert_eq!(f.client.queue_summary().pending_deposits, 0);
    assert_eq!(f.client.get_info().total_assets, 1_050_000);
}

#[test]
fn requests_wait_for_their_own_window() {
    let f = setup();
    seed(&f, 1_000_000);
    let whale = Address::generate(&f.env);

    // First request lands in window 1.
    f.client.deposit(&whale, &50_000);
    f.env.ledger().set_timestamp(BASE_TIME + HOUR);
    f.client.process_queue(&f.admin);

    // A second request created now targets window 2.
    f.env.ledger().set_timestamp(BASE_TIME + HOUR + 1);
    let later = f.client.deposit(&whale, &60_000);
    assert_eq!(later.estimated_processing_at, BASE_TIME + 2 * HOUR);

    let closed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client.process_queue(&f.admin);
    }));
    assert!(closed.is_err(), "the window closes once processed");

    f.env.ledger().set_timestamp(BASE_TIME + 2 * HOUR);
    let on_time = f.client.process_queue(&f.admin);
    assert_eq!(on_time.deposits_processed, 1);
}

#[test]
fn withdrawals_are_processed_before_deposits() {
    let f = setup();
    let holder = seed(&f, 1_000_000);
    let whale = Address::generate(&f.env);

    // Queue a withdrawal that exceeds 1% of TVL, then a large deposit.
    f.client.withdraw(&holder, &200_000);
    f.client.deposit(&whale, &100_000);

    f.env.ledger().set_timestamp(BASE_TIME + HOUR);
    let result = f.client.process_queue(&f.admin);

    assert_eq!(result.withdrawals_processed, 1);
    assert_eq!(result.deposits_processed, 1);
    assert_eq!(result.withdrawn_amount, 200_000);
    assert_eq!(result.deposited_amount, 100_000);
    assert_eq!(f.client.balance_of(&holder), 800_000);
    assert_eq!(f.client.balance_of(&whale), 100_000);
}

#[test]
fn queue_config_is_configurable() {
    let f = setup();
    seed(&f, 1_000_000);

    // Raise the threshold above 50% of TVL and shorten the interval.
    f.client.set_queue_config(&f.admin, &5_000, &1_200);
    assert_eq!(f.client.queue_threshold_bps(), 5_000);

    let whale = Address::generate(&f.env);
    let result = f.client.deposit(&whale, &50_000);
    assert!(!result.queued, "under the raised threshold");

    let big = f.client.deposit(&whale, &600_000);
    assert!(big.queued);
    assert_eq!(big.estimated_processing_at, BASE_TIME + 1_200);

    let summary = f.client.queue_summary();
    assert_eq!(summary.processing_interval, 1_200);
}

#[test]
fn queue_config_rejects_invalid_values() {
    let f = setup();

    let too_high = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client.set_queue_config(&f.admin, &10_001, &1_200);
    }));
    assert!(too_high.is_err());

    let too_short = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client.set_queue_config(&f.admin, &100, &60);
    }));
    assert!(too_short.is_err());
}

#[test]
fn zero_threshold_disables_queueing() {
    let f = setup();
    seed(&f, 1_000_000);
    f.client.set_queue_config(&f.admin, &0, &1_200);

    let whale = Address::generate(&f.env);
    let result = f.client.deposit(&whale, &900_000);
    assert!(!result.queued);
}

#[test]
fn time_to_next_processing_counts_down() {
    let f = setup();
    assert_eq!(f.client.time_to_next_processing(), HOUR);

    f.env.ledger().set_timestamp(BASE_TIME + HOUR / 2);
    assert_eq!(f.client.time_to_next_processing(), HOUR / 2);
}

#[test]
fn owner_requests_only_list_their_own_entries() {
    let f = setup();
    seed(&f, 1_000_000);
    let first = Address::generate(&f.env);
    let second = Address::generate(&f.env);

    f.client.deposit(&first, &50_000);
    f.client.deposit(&first, &70_000);
    f.client.deposit(&second, &90_000);

    let first_requests = f.client.get_owner_requests(&first);
    assert_eq!(first_requests.len(), 2);
    assert_eq!(f.client.get_owner_requests(&second).len(), 1);
}

#[test]
fn queue_entry_exposes_kind_and_timing() {
    let f = setup();
    seed(&f, 1_000_000);
    let whale = Address::generate(&f.env);
    let queued = f.client.deposit(&whale, &50_000);

    let entry = f.client.get_queue_entry(&queued.request_id);
    assert_eq!(entry.id, queued.request_id);
    assert_eq!(entry.owner, whale);
    assert_eq!(entry.kind, QueueKind::Deposit);
    assert_eq!(entry.amount, 50_000);
    assert_eq!(entry.created_at, BASE_TIME);
    assert_eq!(entry.processable_at, BASE_TIME + HOUR);
}

// ─── Emergency withdrawal (#149) ─────────────────────────────────────────────

#[test]
fn emergency_withdraw_requires_pause() {
    let f = setup();
    let user = seed(&f, 1_000_000);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client.emergency_withdraw(&user, &500_000);
    }));

    assert!(result.is_err(), "only available while paused");
}

#[test]
fn emergency_withdraw_applies_penalty() {
    let f = setup();
    let user = seed(&f, 1_000_000);
    f.client.pause(&f.admin);

    let receipt = f.client.emergency_withdraw(&user, &400_000);

    // Default penalty is 5%.
    assert_eq!(receipt.penalty_bps, 500);
    assert_eq!(receipt.shares_burned, 400_000);
    assert_eq!(receipt.assets_withdrawn, 380_000);
    assert_eq!(receipt.penalty_amount, 20_000);
    assert_eq!(f.client.balance_of(&user), 600_000);
}

#[test]
fn emergency_penalty_raises_share_price_for_remaining_depositors() {
    let f = setup();
    let exiting = seed(&f, 1_000_000);
    let stayer = Address::generate(&f.env);
    // Raise the threshold so the second deposit executes immediately and both
    // depositors hold an equal share of the vault.
    f.client.set_queue_config(&f.admin, &10_000, &1_200);
    f.client.deposit(&stayer, &1_000_000);

    let price_before = f.client.share_price();
    f.client.pause(&f.admin);
    f.client.emergency_withdraw(&exiting, &1_000_000);

    let price_after = f.client.share_price();
    assert!(
        price_after > price_before,
        "penalty is retained by the vault: {price_before} -> {price_after}"
    );
    // 1M of 2M shares exits, paying a 50_000 penalty that stays in the vault
    // for the remaining 1M shares: 10_000 -> 10_500 bps.
    assert_eq!(price_after, 10_500);
    assert_eq!(f.client.balance_of(&stayer), 1_000_000);
}

#[test]
fn emergency_penalty_is_configurable_and_bounded() {
    let f = setup();
    let user = seed(&f, 1_000_000);

    f.client.set_emergency_penalty(&f.admin, &1_000);
    assert_eq!(f.client.emergency_penalty_bps(), 1_000);
    f.client.pause(&f.admin);

    let receipt = f.client.emergency_withdraw(&user, &100_000);
    assert_eq!(receipt.penalty_amount, 10_000);

    f.client.unpause(&f.admin);
    let too_high = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client.set_emergency_penalty(&f.admin, &2_001);
    }));
    assert!(too_high.is_err());
}

#[test]
fn emergency_withdraw_more_than_owned_panics() {
    let f = setup();
    let user = seed(&f, 1_000);
    f.client.pause(&f.admin);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client.emergency_withdraw(&user, &5_000);
    }));

    assert!(result.is_err());
}

#[test]
fn paused_vault_blocks_normal_deposits() {
    let f = setup();
    seed(&f, 1_000_000);
    f.client.pause(&f.admin);

    let user = Address::generate(&f.env);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client.deposit(&user, &1_000);
    }));

    assert!(result.is_err());
}

// ─── Keeper compounding (#150) ───────────────────────────────────────────────

#[test]
fn compound_is_rate_limited() {
    let f = setup();
    f.client.add_strategy(&f.admin, &strategy(&f.env, "LEND", 1_200));
    f.client.set_keeper_reward(&f.admin, &500);
    seed(&f, 1_000_000);
    f.env.ledger().set_timestamp(BASE_TIME + HOUR);

    assert!(f.client.can_compound());

    // First compound succeeds and starts the cooldown.
    f.client.compound(&Address::generate(&f.env), &Vec::new(&f.env));
    assert!(!f.client.can_compound());

    let too_soon = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client.compound(&Address::generate(&f.env), &Vec::new(&f.env));
    }));
    assert!(too_soon.is_err());
}

#[test]
fn compound_accrues_fees_and_keeper_reward() {
    let f = setup();
    f.client.add_strategy(&f.admin, &strategy(&f.env, "LEND", 5_000));
    f.client.set_keeper_reward(&f.admin, &500);
    seed(&f, 1_000_000);
    f.env.ledger().set_timestamp(BASE_TIME + 30 * 24 * HOUR);

    let keeper = Address::generate(&f.env);
    let receipt = f.client.compound(&keeper, &Vec::new(&f.env));

    // 50% APY on 1M over 30 days; 10% fee then 5% of the remainder to the keeper.
    assert!(receipt.gross_rewards > 0);
    assert_eq!(receipt.performance_fee, receipt.gross_rewards / 10);
    assert_eq!(receipt.keeper_reward, (receipt.gross_rewards - receipt.performance_fee) * 500 / 10_000);
    assert_eq!(receipt.keeper_reward_shares, receipt.keeper_reward);
    assert!(f.client.balance_of(&keeper) > 0, "keeper is paid in shares");
    assert_eq!(
        receipt.net_compounded,
        receipt.gross_rewards - receipt.performance_fee - receipt.keeper_reward
    );
    assert_eq!(f.client.get_stats().accumulated_fees, receipt.performance_fee);
}

#[test]
fn compound_rejects_out_of_bounds_strategy() {
    let f = setup();
    f.client.add_strategy(&f.admin, &strategy(&f.env, "LEND", 1_200));
    seed(&f, 1_000_000);
    f.env.ledger().set_timestamp(BASE_TIME + HOUR);

    let mut indices = Vec::new(&f.env);
    indices.push_back(7u32);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client.compound(&Address::generate(&f.env), &indices);
    }));

    assert!(result.is_err());
}

#[test]
fn compound_interval_is_configurable() {
    let f = setup();
    f.client.add_strategy(&f.admin, &strategy(&f.env, "LEND", 1_200));
    seed(&f, 1_000_000);

    f.client.set_compound_interval(&f.admin, &60);
    assert_eq!(f.client.compound_interval(), 60);

    f.env.ledger().set_timestamp(BASE_TIME + 60);
    assert!(f.client.can_compound());
    f.client.compound(&Address::generate(&f.env), &Vec::new(&f.env));

    f.env.ledger().set_timestamp(BASE_TIME + 120);
    assert!(f.client.can_compound());
}

#[test]
fn keeper_reward_is_bounded() {
    let f = setup();

    let too_high = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client.set_keeper_reward(&f.admin, &2_001);
    }));

    assert!(too_high.is_err());
}

#[test]
fn optimal_compound_interval_scales_with_apy() {
    let f = setup();
    f.client.add_strategy(&f.admin, &strategy(&f.env, "LOW", 100));
    f.client.add_strategy(&f.admin, &strategy(&f.env, "HIGH", 5_000));

    let low = f.client.optimal_compound_interval(&0);
    let high = f.client.optimal_compound_interval(&1);

    assert!(low > high, "higher APY justifies more frequent compounding");
}

// ─── Strategy and fee plumbing ───────────────────────────────────────────────

#[test]
fn add_strategy_indexes_sequentially() {
    let f = setup();

    assert_eq!(f.client.add_strategy(&f.admin, &strategy(&f.env, "A", 500)), 0);
    assert_eq!(f.client.add_strategy(&f.admin, &strategy(&f.env, "B", 900)), 1);
    assert_eq!(f.client.list_strategies().len(), 2);
    assert_eq!(f.client.get_info().strategy_count, 2);
}

#[test]
fn optimize_strategy_switches_to_higher_apy() {
    let f = setup();
    f.client.add_strategy(&f.admin, &strategy(&f.env, "LOW", 500));
    f.client.add_strategy(&f.admin, &strategy(&f.env, "HIGH", 2_000));
    seed(&f, 1_000_000);

    let chosen = f.client.optimize_strategy(&f.admin, &100);
    assert_eq!(chosen, 1);
    assert_eq!(f.client.get_info().active_strategy_index, Some(1));
}

#[test]
fn optimize_strategy_stays_put_below_threshold() {
    let f = setup();
    f.client.add_strategy(&f.admin, &strategy(&f.env, "LOW", 1_000));
    f.client.add_strategy(&f.admin, &strategy(&f.env, "HIGH", 1_010));
    seed(&f, 1_000_000);

    // A 1% improvement does not clear a 5% threshold.
    let chosen = f.client.optimize_strategy(&f.admin, &500);
    assert_eq!(chosen, 0);
}

#[test]
fn harvest_requires_minimum_interval() {
    let f = setup();
    f.client.add_strategy(&f.admin, &strategy(&f.env, "LEND", 1_200));
    seed(&f, 1_000_000);

    let too_soon = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client.harvest();
    }));
    assert!(too_soon.is_err());
}

#[test]
fn harvest_compounds_and_takes_fee() {
    let f = setup();
    f.client.add_strategy(&f.admin, &strategy(&f.env, "LEND", 1_200));
    seed(&f, 1_000_000);
    f.env.ledger().set_timestamp(BASE_TIME + HOUR);

    let result = f.client.harvest();

    assert!(result.raw_rewards > 0);
    assert_eq!(result.performance_fee, result.raw_rewards / 10);
    assert_eq!(result.net_rewards, result.raw_rewards - result.performance_fee);
    assert_eq!(result.compounded_amount, result.net_rewards);
    assert_eq!(result.new_total_assets, 1_000_000 + result.net_rewards);
}

#[test]
fn performance_fee_is_bounded_and_collectable() {
    let f = setup();

    let too_high = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client.set_performance_fee(&f.admin, &3_001);
    }));
    assert!(too_high.is_err());

    f.client.add_strategy(&f.admin, &strategy(&f.env, "LEND", 1_200));
    seed(&f, 1_000_000);
    f.env.ledger().set_timestamp(BASE_TIME + HOUR);
    let harvest = f.client.harvest();

    assert_eq!(f.client.collect_fees(&f.admin), harvest.performance_fee);
    assert_eq!(f.client.get_stats().accumulated_fees, 0);
}

#[test]
fn collect_fees_without_accrual_panics() {
    let f = setup();

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client.collect_fees(&f.admin);
    }));

    assert!(result.is_err());
}

#[test]
fn pause_and_unpause_toggle_state() {
    let f = setup();
    assert!(!f.client.get_info().paused);

    f.client.pause(&f.admin);
    assert!(f.client.get_info().paused);

    f.client.unpause(&f.admin);
    assert!(!f.client.get_info().paused);

    let double_pause = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client.unpause(&f.admin);
    }));
    assert!(double_pause.is_err());
}

#[test]
fn initialize_twice_panics() {
    let f = setup();

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client.initialize(
            &f.admin,
            &f.treasury,
            &Address::generate(&f.env),
            &Address::generate(&f.env),
            &1_000,
        );
    }));

    assert!(result.is_err());
}

#[test]
fn initialization_rejects_excessive_fee() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, YieldVaultContract);
    let client = YieldVaultContractClient::new(&env, &contract_id);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.initialize(
            &Address::generate(&env),
            &Address::generate(&env),
            &Address::generate(&env),
            &Address::generate(&env),
            &5_000,
        );
    }));

    assert!(result.is_err());
}

// ─── Share pricing ───────────────────────────────────────────────────────────

#[test]
fn share_price_reflects_growth() {
    let f = setup();
    assert_eq!(f.client.share_price(), 10_000);

    f.client.add_strategy(&f.admin, &strategy(&f.env, "LEND", 5_000));
    seed(&f, 1_000_000);
    f.env.ledger().set_timestamp(BASE_TIME + 30 * 24 * HOUR);
    f.client.harvest();

    assert!(f.client.share_price() > 10_000);
}

#[test]
fn preview_matches_execution() {
    let f = setup();
    let user = Address::generate(&f.env);

    let previewed_shares = f.client.preview_deposit(&10_000);
    let result = f.client.deposit(&user, &10_000);
    assert_eq!(previewed_shares, result.shares_minted);

    // Keep the withdrawal under the queue threshold so it executes at once.
    f.client.set_queue_config(&f.admin, &0, &1_200);
    let previewed_assets = f.client.preview_withdraw(&5_000);
    let withdrawn = f.client.withdraw(&user, &5_000);
    assert_eq!(previewed_assets, withdrawn.amount_withdrawn);
}

#[test]
fn deposits_reject_zero() {
    let f = setup();
    let user = Address::generate(&f.env);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client.deposit(&user, &0);
    }));

    assert!(result.is_err());
}
