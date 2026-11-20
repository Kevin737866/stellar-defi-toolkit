use stellar_defi_toolkit::contracts::vault::YieldVaultContract;
use stellar_defi_toolkit::types::vault::{
    MarketConditions, RebalanceReason, RebalanceThresholds, StrategyType, VaultStrategy,
};
use soroban_sdk::{testutils::Address as _, Address, Env, Symbol};

fn setup() -> (Env, YieldVaultContract) {
    let env = Env::default();
    let asset = Address::generate(&env);
    let share = Address::generate(&env);
    let mut vault = YieldVaultContract::new(&env, asset, share);
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    vault = vault.initialize(admin, treasury, 1000).unwrap();
    (env, vault)
}

fn strategy(env: &Env, apy: u32, name: &str) -> VaultStrategy {
    VaultStrategy {
        name: Symbol::new(env, name),
        contract_address: Address::generate(env),
        strategy_type: StrategyType::Lending,
        estimated_apy: apy,
        allocated_amount: 0,
        active: true,
    }
}

fn thresholds() -> RebalanceThresholds {
    RebalanceThresholds::default()
}

#[test]
fn yield_differential_above_threshold_triggers() {
    let (env, mut vault) = setup();
    vault.add_strategy(strategy(&env, 500, "A")).unwrap();
    vault.add_strategy(strategy(&env, 1500, "B")).unwrap();

    let mut conditions = MarketConditions::empty(2);
    conditions.strategy_yields_bps = vec![500, 1500];

    let triggers = vault.evaluate_rebalance(&conditions, &thresholds()).unwrap();
    let yield_trigger = triggers
        .iter()
        .find(|t| t.reason == RebalanceReason::YieldDifferential)
        .expect("yield trigger evaluated");
    assert!(yield_trigger.triggered, "1000bps delta must cross 200bps threshold");
    assert_eq!(yield_trigger.target_strategy_index, 1);
    assert!(yield_trigger.rationale.contains("yield"));
}

#[test]
fn yield_differential_below_threshold_does_not_trigger() {
    let (env, mut vault) = setup();
    vault.add_strategy(strategy(&env, 500, "A")).unwrap();
    vault.add_strategy(strategy(&env, 550, "B")).unwrap();

    let mut conditions = MarketConditions::empty(2);
    conditions.strategy_yields_bps = vec![500, 550];

    let triggers = vault.evaluate_rebalance(&conditions, &thresholds()).unwrap();
    let yield_trigger = triggers
        .iter()
        .find(|t| t.reason == RebalanceReason::YieldDifferential)
        .unwrap();
    assert!(!yield_trigger.triggered, "50bps delta must not cross 200bps threshold");
}

#[test]
fn risk_score_reduction_triggers() {
    let (env, mut vault) = setup();
    vault.add_strategy(strategy(&env, 500, "A")).unwrap();
    vault.add_strategy(strategy(&env, 500, "B")).unwrap();

    let mut conditions = MarketConditions::empty(2);
    conditions.strategy_yields_bps = vec![500, 500];
    conditions.strategy_risk_scores = vec![8000, 3000];

    let triggers = vault.evaluate_rebalance(&conditions, &thresholds()).unwrap();
    let risk = triggers
        .iter()
        .find(|t| t.reason == RebalanceReason::RiskScoreChange)
        .unwrap();
    assert!(risk.triggered, "5000bps risk drop must cross 1000bps threshold");
    assert_eq!(risk.target_strategy_index, 1);
}

#[test]
fn utilization_drop_triggers() {
    let (env, mut vault) = setup();
    vault.add_strategy(strategy(&env, 500, "A")).unwrap();
    vault.add_strategy(strategy(&env, 500, "B")).unwrap();

    let mut conditions = MarketConditions::empty(2);
    conditions.strategy_yields_bps = vec![500, 500];
    conditions.strategy_utilization = vec![9500, 2000];

    let triggers = vault.evaluate_rebalance(&conditions, &thresholds()).unwrap();
    let util = triggers
        .iter()
        .find(|t| t.reason == RebalanceReason::UtilizationChange)
        .unwrap();
    assert!(util.triggered, "7500bps utilization drop must cross 2000bps threshold");
}

#[test]
fn disabled_trigger_is_not_evaluated() {
    let (env, mut vault) = setup();
    vault.add_strategy(strategy(&env, 100, "A")).unwrap();
    vault.add_strategy(strategy(&env, 2000, "B")).unwrap();

    let mut conditions = MarketConditions::empty(2);
    conditions.strategy_yields_bps = vec![100, 2000];

    let mut t = thresholds();
    t.yield_enabled = false;

    let triggers = vault.evaluate_rebalance(&conditions, &t).unwrap();
    assert!(
        !triggers.iter().any(|x| x.reason == RebalanceReason::YieldDifferential),
        "disabled yield trigger must not be evaluated"
    );
}

#[test]
fn rebalance_moves_funds_and_records_rationale() {
    let (env, mut vault) = setup();
    vault.add_strategy(strategy(&env, 500, "A")).unwrap();
    vault.add_strategy(strategy(&env, 1500, "B")).unwrap();
    let user = Address::generate(&env);
    vault.deposit(user, 1_000).unwrap();

    let mut conditions = MarketConditions::empty(2);
    conditions.strategy_yields_bps = vec![500, 1500];

    let outcome = vault
        .rebalance(&conditions, &thresholds())
        .unwrap()
        .expect("rebalance fires");
    assert_eq!(outcome.previous_strategy_index, 0);
    assert_eq!(outcome.new_strategy_index, 1);
    assert_eq!(outcome.assets_moved, 1_000);
    assert_eq!(outcome.trigger.reason, RebalanceReason::YieldDifferential);
    assert!(!outcome.trigger.rationale.is_empty());
    assert_eq!(vault.active_strategy_index, 1);
    assert_eq!(vault.get_rebalance_log().len(), 1);
}

#[test]
fn rebalance_noop_when_no_threshold_crossed() {
    let (env, mut vault) = setup();
    vault.add_strategy(strategy(&env, 500, "A")).unwrap();
    vault.add_strategy(strategy(&env, 500, "B")).unwrap();

    let mut conditions = MarketConditions::empty(2);
    conditions.strategy_yields_bps = vec![500, 500];

    assert!(vault.rebalance(&conditions, &thresholds()).unwrap().is_none());
    assert!(vault.get_rebalance_log().is_empty());
}

#[test]
fn manual_override_works_without_market_conditions() {
    let (env, mut vault) = setup();
    vault.add_strategy(strategy(&env, 500, "A")).unwrap();
    vault.add_strategy(strategy(&env, 100, "B")).unwrap();
    let user = Address::generate(&env);
    vault.deposit(user, 500).unwrap();

    // Strategy B yields *less*, so no automatic trigger would fire.
    let outcome = vault.manual_rebalance(1).unwrap();
    assert_eq!(outcome.trigger.reason, RebalanceReason::ManualOverride);
    assert_eq!(outcome.new_strategy_index, 1);
    assert_eq!(outcome.assets_moved, 500);
}

#[test]
fn manual_override_rejects_invalid_target() {
    let (env, mut vault) = setup();
    vault.add_strategy(strategy(&env, 500, "A")).unwrap();

    assert!(vault.manual_rebalance(5).is_err(), "out of bounds target rejected");
    assert!(vault.manual_rebalance(0).is_err(), "same strategy rejected");
}

#[test]
fn mismatched_conditions_length_is_rejected() {
    let (env, mut vault) = setup();
    vault.add_strategy(strategy(&env, 500, "A")).unwrap();
    vault.add_strategy(strategy(&env, 500, "B")).unwrap();

    let conditions = MarketConditions::empty(3);
    assert!(vault.evaluate_rebalance(&conditions, &thresholds()).is_err());
}

#[test]
fn rebalance_requires_admin() {
    let (env, mut vault) = setup();
    vault.add_strategy(strategy(&env, 500, "A")).unwrap();
    vault.add_strategy(strategy(&env, 1500, "B")).unwrap();
    vault.admin = None;

    let mut conditions = MarketConditions::empty(2);
    conditions.strategy_yields_bps = vec![500, 1500];
    assert!(vault.rebalance(&conditions, &thresholds()).is_err());
    assert!(vault.manual_rebalance(1).is_err());
}
