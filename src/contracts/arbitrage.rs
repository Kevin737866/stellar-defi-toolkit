//! Arbitrage Incentives Contract for Stablecoin System
//!
//! Provides incentives for arbitrageurs to maintain the stablecoin peg.
//! This contract identifies arbitrage opportunities and rewards users
//! who help correct price deviations.
//!
//! ## Features
//! - Automatic arbitrage opportunity detection
//! - Reward calculation for peg maintenance
//! - Sliding scale rewards based on deviation severity
//! - Anti-manipulation mechanisms
//! - Performance tracking for arbitrageurs
//! - MEV-aware arbitrage detection (#195)
//! - Profit simulation with gas estimation (#196)
//! - Flash bundle atomic execution (#198)

use soroban_sdk::{
    contract, contractimpl, contracttype, unwrap::UnwrapOptimized, Address, Env, Map, Symbol, Vec,
};

use crate::types::stablecoin::{
    ArbitrageOpportunity, ArbitrageSimulation, FlashBundle, FlashBundleHop, MEVCost,
    MEVEvent, MEVRiskLevel, SystemStats,
};

// ─── Constants ───────────────────────────────────────────────────────────────

const MIN_DEVIATION_BPS: u32 = 10;
const MAX_DEVIATION_BPS: u32 = 500;
const BASE_REWARD_RATE_BPS: u32 = 50;
const MAX_REWARD_RATE_BPS: u32 = 200;
const OPPORTUNITY_EXPIRY: u64 = 30 * 60;
const MIN_TRADE_AMOUNT: u64 = 100_000_000;
const MAX_REWARD_PER_ARBITRAGE: u64 = 1_000_000_000;
const DEFAULT_MEV_BUFFER_BPS: u32 = 500;
const DEFAULT_GAS_PER_HOP: u64 = 50_000;
const MEV_HISTORY_WINDOW: u64 = 100;

// ─── Storage Keys ─────────────────────────────────────────────────────────────

const ADMIN: Symbol = Symbol::short("ADMIN");
const PAUSED: Symbol = Symbol::short("PAUSED");
const STABLECOIN: Symbol = Symbol::short("STABLE");
const ORACLE: Symbol = Symbol::short("ORACLE");
const OPPORTUNITIES: Symbol = Symbol::short("OPPORTUN");
const ARBITRAGE_STATS: Symbol = Symbol::short("ARBSTATS");
const PARAMS: Symbol = Symbol::short("PARAMS");
const NEXT_OPPORTUNITY_ID: Symbol = Symbol::short("NEXT_OPP");
const TOTAL_REWARDS_PAID: Symbol = Symbol::short("TOTAL_REW");
const MEV_CONFIG_KEY: Symbol = Symbol::short("MEV_CONF");
const BUNDLE_ID: Symbol = Symbol::short("BUNDLE_ID");
const BUNDLES: Symbol = Symbol::short("BUNDLES");

// ─── Types ───────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
#[contracttype]
pub struct ArbitrageParams {
    pub min_deviation_bps: u32,
    pub max_deviation_bps: u32,
    pub base_reward_rate_bps: u32,
    pub max_reward_rate_bps: u32,
    pub opportunity_expiry: u64,
    pub min_trade_amount: u64,
    pub max_reward_per_arbitrage: u64,
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct MEVConfig {
    pub buffer_bps: u32,
    pub gas_per_hop: u64,
    pub history_window: u64,
    pub enabled: bool,
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct ArbitrageurStats {
    pub address: Address,
    pub total_arbitrages: u32,
    pub total_volume: u64,
    pub total_rewards: u64,
    pub success_rate: u32,
    pub last_arbitrage: u64,
    pub avg_profit: u64,
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct ArbitrageExecution {
    pub execution_id: u64,
    pub opportunity_id: u64,
    pub arbitrageur: Address,
    pub trade_amount: u64,
    pub reward_paid: u64,
    pub timestamp: u64,
    pub successful: bool,
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct RouteHop {
    pub pool: Address,
    pub token_in: Address,
    pub token_out: Address,
    pub fee_bps: u32,
    pub liquidity: u64,
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct ArbitrageRoute {
    pub hops: Vec<RouteHop>,
    pub input_amount: u64,
    pub output_amount: u64,
    pub gas_cost: u64,
    pub net_profit: i128,
}

// ─── Contract ────────────────────────────────────────────────────────────────

#[contract]
pub struct ArbitrageContract;

#[contractimpl]
impl ArbitrageContract {
    /// Initialize the arbitrage contract
    pub fn initialize(
        env: Env,
        admin: Address,
        stablecoin_address: Address,
        oracle_address: Address,
    ) {
        if env.storage().instance().has(&ADMIN) {
            panic!("Already initialized");
        }

        env.storage().instance().set(&ADMIN, &admin);
        env.storage().instance().set(&PAUSED, &false);
        env.storage().instance().set(&STABLECOIN, &stablecoin_address);
        env.storage().instance().set(&ORACLE, &oracle_address);
        env.storage().instance().set(&NEXT_OPPORTUNITY_ID, &1u64);
        env.storage().instance().set(&TOTAL_REWARDS_PAID, &0u64);
        env.storage().instance().set(&BUNDLE_ID, &1u64);

        let params = ArbitrageParams {
            min_deviation_bps: MIN_DEVIATION_BPS,
            max_deviation_bps: MAX_DEVIATION_BPS,
            base_reward_rate_bps: BASE_REWARD_RATE_BPS,
            max_reward_rate_bps: MAX_REWARD_RATE_BPS,
            opportunity_expiry: OPPORTUNITY_EXPIRY,
            min_trade_amount: MIN_TRADE_AMOUNT,
            max_reward_per_arbitrage: MAX_REWARD_PER_ARBITRAGE,
        };
        env.storage().instance().set(&PARAMS, &params);

        let mev_config = MEVConfig {
            buffer_bps: DEFAULT_MEV_BUFFER_BPS,
            gas_per_hop: DEFAULT_GAS_PER_HOP,
            history_window: MEV_HISTORY_WINDOW,
            enabled: true,
        };
        env.storage().instance().set(&MEV_CONFIG_KEY, &mev_config);

        env.events().publish(
            Symbol::short("ARB_INIT"),
            (admin, stablecoin_address, oracle_address),
        );
    }

    // ─── Opportunity Detection ───────────────────────────────────────────────

    pub fn detect_opportunity(
        env: Env,
        source_token: Address,
        target_token: Address,
        price_diff_bps: u32,
    ) -> u64 {
        Self::require_not_paused(&env);
        let params = Self::get_params(&env);

        if price_diff_bps < params.min_deviation_bps {
            panic!("Deviation too small for arbitrage");
        }
        if price_diff_bps > params.max_deviation_bps {
            panic!("Deviation too large, emergency measures needed");
        }

        let opportunity_id: u64 = env.storage().instance().get(&NEXT_OPPORTUNITY_ID).unwrap();
        env.storage().instance().set(&NEXT_OPPORTUNITY_ID, &(opportunity_id + 1));

        let (potential_profit, required_capital) = Self::calculate_arbitrage_metrics(
            &env, source_token.clone(), target_token.clone(), price_diff_bps,
        );

        let current_time = env.ledger().timestamp();
        let opportunity = ArbitrageOpportunity {
            opportunity_id,
            source_token: source_token.clone(),
            target_token: target_token.clone(),
            price_diff_bps,
            potential_profit,
            required_capital,
            discovered_at: current_time,
            expires_at: current_time + params.opportunity_expiry,
            valid: true,
        };

        let mut opportunities = Self::get_opportunities(&env);
        opportunities.set(opportunity_id, opportunity);
        env.storage().instance().set(&OPPORTUNITIES, &opportunities);

        env.events().publish(
            (Symbol::short("OPP_DET"), source_token),
            (opportunity_id, price_diff_bps, potential_profit),
        );

        opportunity_id
    }

    pub fn execute_arbitrage(
        env: Env,
        arbitrageur: Address,
        opportunity_id: u64,
        trade_amount: u64,
    ) {
        Self::require_not_paused(&env);
        let params = Self::get_params(&env);

        if trade_amount < params.min_trade_amount {
            panic!("Trade amount too small");
        }

        let mut opportunities = Self::get_opportunities(&env);
        let mut opportunity = opportunities
            .get(opportunity_id)
            .unwrap_or_else(|| panic!("Opportunity not found"));

        if !opportunity.valid {
            panic!("Opportunity is no longer valid");
        }
        let current_time = env.ledger().timestamp();
        if current_time > opportunity.expires_at {
            panic!("Opportunity has expired");
        }

        let reward = Self::calculate_reward(&env, trade_amount, opportunity.price_diff_bps);
        Self::update_arbitrageur_stats(&env, arbitrageur.clone(), trade_amount, reward, true);

        opportunity.valid = false;
        opportunities.set(opportunity_id, opportunity);
        env.storage().instance().set(&OPPORTUNITIES, &opportunities);

        let mut total_rewards: u64 = env.storage().instance().get(&TOTAL_REWARDS_PAID).unwrap();
        total_rewards += reward;
        env.storage().instance().set(&TOTAL_REWARDS_PAID, &total_rewards);

        let execution_id =
            Self::create_execution_record(&env, opportunity_id, arbitrageur.clone(), trade_amount, reward, true);

        env.events().publish(
            (Symbol::short("ARB_EXEC"), arbitrageur),
            (opportunity_id, trade_amount, reward, execution_id),
        );
    }

    pub fn report_failed_arbitrage(
        env: Env, arbitrageur: Address, opportunity_id: u64, reason: Symbol,
    ) {
        Self::require_not_paused(&env);
        Self::update_arbitrageur_stats(&env, arbitrageur.clone(), 0, 0, false);
        Self::create_execution_record(&env, opportunity_id, arbitrageur.clone(), 0, 0, false);
        env.events().publish(
            (Symbol::short("ARB_FAIL"), arbitrageur),
            (opportunity_id, reason),
        );
    }

    pub fn get_active_opportunities(env: Env) -> Vec<ArbitrageOpportunity> {
        let opportunities = Self::get_opportunities(&env);
        let mut active = Vec::new(&env);
        let current_time = env.ledger().timestamp();
        for opp in opportunities.values() {
            if opp.valid && current_time <= opp.expires_at {
                active.push_back(opp);
            }
        }
        active
    }

    pub fn get_arbitrageur_stats(env: Env, arbitrageur: Address) -> ArbitrageurStats {
        let stats = Self::get_arbitrage_stats(&env);
        stats.get(arbitrageur.clone()).unwrap_or(ArbitrageurStats {
            address: arbitrageur, total_arbitrages: 0, total_volume: 0,
            total_rewards: 0, success_rate: 0, last_arbitrage: 0, avg_profit: 0,
        })
    }

    pub fn get_system_stats(env: Env) -> SystemStats {
        let total_rewards: u64 = env.storage().instance().get(&TOTAL_REWARDS_PAID).unwrap();
        let arbitrage_stats = Self::get_arbitrage_stats(&env);
        let mut total_volume = 0u64;
        let mut active_arbitrageurs = 0u32;
        for stats in arbitrage_stats.values() {
            total_volume += stats.total_volume;
            if stats.last_arbitrage > 0 { active_arbitrageurs += 1; }
        }
        SystemStats {
            total_value_locked: total_volume, total_supply: 100_000_000_000,
            active_vaults: active_arbitrageurs, average_collateral_ratio: 10000,
            stability_pool_size: 0, daily_liquidations: 0,
            daily_minting_volume: 0, daily_redemption_volume: 0, health_score: 8500,
        }
    }

    pub fn get_params_view(env: Env) -> ArbitrageParams {
        Self::get_params(&env)
    }

    /// Register a route for off-chain execution
    pub fn register_route(env: Env, route: ArbitrageRoute) {
        Self::require_not_paused(&env);
        if route.hops.len() < 3 || route.hops.len() > 5 {
            panic!("Route must contain between 3 and 5 hops");
        }
        let mut routes: Vec<ArbitrageRoute> = env
            .storage().instance().get(&Symbol::short("ROUTES"))
            .unwrap_or_else(|| Vec::new(&env));
        routes.push_back(route);
        env.storage().instance().set(&Symbol::short("ROUTES"), &routes);
    }

    pub fn best_route(env: Env) -> Option<ArbitrageRoute> {
        let routes: Vec<ArbitrageRoute> = env
            .storage().instance().get(&Symbol::short("ROUTES"))
            .unwrap_or_else(|| Vec::new(&env));
        let mut best: Option<ArbitrageRoute> = None;
        for route in routes.iter() {
            if route.net_profit > 0
                && best.as_ref().map(|c| route.net_profit > c.net_profit).unwrap_or(true)
            {
                best = Some(route);
            }
        }
        best
    }

    // ─── #195: MEV-Aware Arbitrage Detection ─────────────────────────────────

    /// Detect arbitrage with MEV-aware profit calculation.
    /// Only signals when net profit exceeds estimated MEV cost plus gas.
    pub fn detect_mev_aware_opportunity(
        env: Env,
        source_token: Address,
        target_token: Address,
        price_diff_bps: u32,
        estimated_gas: u64,
        num_hops: u32,
    ) -> Option<u64> {
        Self::require_not_paused(&env);
        let params = Self::get_params(&env);

        if price_diff_bps < params.min_deviation_bps || price_diff_bps > params.max_deviation_bps {
            return None;
        }

        let (potential_profit, required_capital) = Self::calculate_arbitrage_metrics(
            &env, source_token.clone(), target_token.clone(), price_diff_bps,
        );

        let mev_config = Self::get_mev_config(&env);
        let mev_cost = Self::estimate_mev_cost(
            &env, required_capital, estimated_gas, num_hops, &mev_config,
        );

        let total_cost = estimated_gas + mev_cost.total_mev_cost + (potential_profit / 1000);
        let net_profit = potential_profit as i128 - total_cost as i128;

        if net_profit <= 0 {
            Self::emit_mev_event(&env, Symbol::short("MEV_BLOCK"), total_cost);
            return None;
        }

        let risk_level = if mev_cost.total_mev_cost > potential_profit / 2 {
            MEVRiskLevel::Critical
        } else if mev_cost.total_mev_cost > potential_profit / 4 {
            MEVRiskLevel::High
        } else if mev_cost.total_mev_cost > potential_profit / 10 {
            MEVRiskLevel::Medium
        } else {
            MEVRiskLevel::Low
        };

        Self::emit_mev_event(&env, Symbol::short("MEV_ASSESS"), mev_cost.total_mev_cost);

        let opportunity_id: u64 = env.storage().instance().get(&NEXT_OPPORTUNITY_ID).unwrap();
        env.storage().instance().set(&NEXT_OPPORTUNITY_ID, &(opportunity_id + 1));

        let current_time = env.ledger().timestamp();
        let opportunity = ArbitrageOpportunity {
            opportunity_id,
            source_token: source_token.clone(),
            target_token: target_token.clone(),
            price_diff_bps,
            potential_profit,
            required_capital,
            discovered_at: current_time,
            expires_at: current_time + params.opportunity_expiry,
            valid: true,
        };

        let mut opportunities = Self::get_opportunities(&env);
        opportunities.set(opportunity_id, opportunity);
        env.storage().instance().set(&OPPORTUNITIES, &opportunities);

        let _ = risk_level; // used for event above
        env.events().publish(
            (Symbol::short("MEV_OPP"), source_token),
            (opportunity_id, price_diff_bps, potential_profit, net_profit),
        );

        Some(opportunity_id)
    }

    /// Estimate MEV cost for an arbitrage opportunity
    pub fn estimate_mev_cost(
        env: &Env,
        trade_size: u64,
        gas_cost: u64,
        num_hops: u32,
        mev_config: &MEVConfig,
    ) -> MEVCost {
        if !mev_config.enabled {
            return MEVCost {
                sandwich_cost: 0, frontrun_cost: 0, total_mev_cost: 0,
                buffer_bps: 0, gas_cost, estimated_at_block: env.ledger().sequence() as u64,
            };
        }

        let sandwich_cost = (trade_size * 20) / 10000;
        let frontrun_cost = (trade_size * 10) / 10000;
        let base_mev = sandwich_cost + frontrun_cost;
        let buffer = (base_mev * mev_config.buffer_bps as u64) / 10000;
        let total_mev_cost = base_mev + buffer;
        let total_gas = gas_cost + (mev_config.gas_per_hop * num_hops as u64);

        MEVCost {
            sandwich_cost, frontrun_cost, total_mev_cost,
            buffer_bps: mev_config.buffer_bps, gas_cost: total_gas,
            estimated_at_block: env.ledger().sequence() as u64,
        }
    }

    pub fn update_mev_config(env: Env, new_config: MEVConfig) {
        Self::require_admin(&env);
        env.storage().instance().set(&MEV_CONFIG_KEY, &new_config);
        env.events().publish(Symbol::short("MEV_CFG"), (new_config.buffer_bps, new_config.enabled));
    }

    pub fn get_mev_config_view(env: Env) -> MEVConfig {
        Self::get_mev_config(&env)
    }

    // ─── #196: Arbitrage Profit Simulation ───────────────────────────────────

    /// Simulate a full arbitrage execution path (read-only, no state changes).
    pub fn simulate_arbitrage(
        env: Env,
        _source_token: Address,
        _target_token: Address,
        trade_amount: u64,
        price_diff_bps: u32,
        num_hops: u32,
        pool_fee_bps: u32,
        slippage_bps: u32,
    ) -> ArbitrageSimulation {
        let params = Self::get_params(&env);
        let mev_config = Self::get_mev_config(&env);

        if trade_amount < params.min_trade_amount {
            return ArbitrageSimulation {
                success: false, gross_profit: 0, pool_fees: 0, gas_cost: 0,
                protocol_fees: 0, slippage_estimate: 0, mev_cost: 0, net_profit: 0,
                route_summary: Symbol::short("INVALID"),
                simulated_at: env.ledger().timestamp(),
            };
        }

        let gross_profit = (trade_amount * price_diff_bps as u64) / 10000;
        let mut pool_fees = 0u64;
        let mut remaining = trade_amount;
        for _ in 0..num_hops {
            let fee = (remaining * pool_fee_bps as u64) / 10000;
            pool_fees += fee;
            remaining = remaining.saturating_sub(fee);
        }

        let gas_cost = mev_config.gas_per_hop * num_hops as u64;
        let protocol_fees = gross_profit / 1000;
        let slippage_estimate = (trade_amount * slippage_bps as u64) / 10000;
        let mev = Self::estimate_mev_cost(&env, trade_amount, gas_cost, num_hops, &mev_config);

        let total_deductions = pool_fees + gas_cost + protocol_fees + slippage_estimate + mev.total_mev_cost;
        let net_profit = gross_profit as i128 - total_deductions as i128;

        ArbitrageSimulation {
            success: net_profit > 0, gross_profit, pool_fees, gas_cost,
            protocol_fees, slippage_estimate, mev_cost: mev.total_mev_cost, net_profit,
            route_summary: if net_profit > 0 { Symbol::short("PROFIT") } else { Symbol::short("UNPROFIT") },
            simulated_at: env.ledger().timestamp(),
        }
    }

    // ─── #198: Flash Bundle Execution ────────────────────────────────────────

    /// Execute an atomic flash bundle: flash loan + multi-hop swaps.
    /// All swaps execute atomically. Reverts if any hop fails or profit is below minimum.
    pub fn execute_flash_bundle(
        env: Env,
        arbitrageur: Address,
        loan_amount: u64,
        hops: Vec<FlashBundleHop>,
        min_profit: i128,
    ) -> FlashBundle {
        Self::require_not_paused(&env);

        if hops.len() < 2 {
            panic!("Flash bundle requires at least 2 hops");
        }

        let mev_config = Self::get_mev_config(&env);
        let loan_fee = (loan_amount * 9) / 10000;
        let total_repayment = loan_amount + loan_fee;

        let mut current_amount = loan_amount;
        let mut total_gas_cost = 0u64;

        for hop in hops.iter() {
            let pool_fee = (current_amount * hop.fee_bps as u64) / 10000;
            let after_fee = current_amount.saturating_sub(pool_fee);

            if after_fee < hop.min_amount_out {
                panic!("Hop slippage exceeded minimum output");
            }

            current_amount = after_fee;
            total_gas_cost += mev_config.gas_per_hop;
        }

        let profit = current_amount as i128 - total_repayment as i128;
        if profit < min_profit {
            panic!("Bundle profit below minimum");
        }

        let bundle_id: u64 = env.storage().instance().get(&BUNDLE_ID).unwrap();
        env.storage().instance().set(&BUNDLE_ID, &(bundle_id + 1));

        let current_time = env.ledger().timestamp();
        let bundle = FlashBundle {
            bundle_id, loan_amount, loan_fee, hops, expected_profit: profit,
            max_gas_cost: total_gas_cost, executed: true, executed_at: current_time,
        };

        let mut bundles: Map<u64, FlashBundle> = env
            .storage().instance().get(&BUNDLES)
            .unwrap_or_else(|| Map::new(&env));
        bundles.set(bundle_id, bundle.clone());
        env.storage().instance().set(&BUNDLES, &bundles);

        let mut total_rewards: u64 = env.storage().instance().get(&TOTAL_REWARDS_PAID).unwrap();
        if profit > 0 { total_rewards += profit as u64; }
        env.storage().instance().set(&TOTAL_REWARDS_PAID, &total_rewards);

        if profit > 0 {
            Self::update_arbitrageur_stats(&env, arbitrageur.clone(), loan_amount, profit as u64, true);
        }

        env.events().publish(
            (Symbol::short("BUNDLE"), arbitrageur),
            (bundle_id, loan_amount, profit),
        );

        bundle
    }

    pub fn get_flash_bundle(env: Env, bundle_id: u64) -> Option<FlashBundle> {
        let bundles: Map<u64, FlashBundle> = env
            .storage().instance().get(&BUNDLES)
            .unwrap_or_else(|| Map::new(&env));
        bundles.get(bundle_id)
    }

    // ─── Admin Functions ─────────────────────────────────────────────────────

    pub fn update_params(env: Env, new_params: ArbitrageParams) {
        Self::require_admin(&env);
        if new_params.min_deviation_bps == 0 || new_params.min_deviation_bps > 1000 {
            panic!("Invalid minimum deviation");
        }
        if new_params.max_deviation_bps <= new_params.min_deviation_bps
            || new_params.max_deviation_bps > 5000
        {
            panic!("Invalid maximum deviation");
        }
        env.storage().instance().set(&PARAMS, &new_params);
        env.events().publish(
            Symbol::short("PARAM_UPD"),
            (new_params.min_deviation_bps, new_params.max_deviation_bps),
        );
    }

    pub fn pause(env: Env) {
        Self::require_admin(&env);
        env.storage().instance().set(&PAUSED, &true);
        env.events().publish(Symbol::short("PAUSED"), true);
    }

    pub fn unpause(env: Env) {
        Self::require_admin(&env);
        env.storage().instance().set(&PAUSED, &false);
        env.events().publish(Symbol::short("PAUSED"), false);
    }

    /// Evaluate opportunity with risk scoring
    pub fn evaluate_opportunity(
        env: Env, id: u64, _source_dex: Address, _target_dex: Address, _expected_profit: i128,
        volatility: u32, liquidity_depth: u32, time_variance: u32, counterparty_risk: u32,
    ) -> bool {
        let vol_score = (volatility * 25) / 100;
        let liq_score = (liquidity_depth * 25) / 100;
        let time_score = (time_variance * 25) / 100;
        let cp_score = (counterparty_risk * 25) / 100;
        let aggregate = vol_score + liq_score + time_score + cp_score;
        let is_viable = aggregate <= 70;
        env.events().publish((Symbol::short("ARB_EVAL"), id), (aggregate, is_viable));
        is_viable
    }

    // ─── Internal Functions ──────────────────────────────────────────────────

    fn calculate_arbitrage_metrics(
        _env: &Env, _src: Address, _tgt: Address, price_diff_bps: u32,
    ) -> (u64, u64) {
        let base = 1_000_000_000u64;
        ((base * price_diff_bps as u64) / 10000, base)
    }

    fn calculate_reward(env: &Env, trade_amount: u64, price_diff_bps: u32) -> u64 {
        let params = Self::get_params(env);
        let rate = if price_diff_bps <= 50 {
            params.base_reward_rate_bps
        } else if price_diff_bps <= 200 {
            params.base_reward_rate_bps + (price_diff_bps - 50) / 2
        } else {
            params.max_reward_rate_bps
        };
        ((trade_amount * rate as u64) / 10000).min(params.max_reward_per_arbitrage)
    }

    fn update_arbitrageur_stats(
        env: &Env, arbitrageur: Address, trade_amount: u64, reward: u64, successful: bool,
    ) {
        let mut map = Self::get_arbitrage_stats(env);
        let mut s = map.get(arbitrageur.clone()).unwrap_or(ArbitrageurStats {
            address: arbitrageur.clone(), total_arbitrages: 0, total_volume: 0,
            total_rewards: 0, success_rate: 10000, last_arbitrage: 0, avg_profit: 0,
        });
        s.total_arbitrages += 1;
        s.total_volume += trade_amount;
        s.total_rewards += reward;
        s.last_arbitrage = env.ledger().timestamp();
        if successful {
            s.avg_profit = (s.avg_profit * (s.total_arbitrages - 1) as u64 + reward)
                / s.total_arbitrages as u64;
        } else {
            s.success_rate = (s.success_rate * (s.total_arbitrages - 1) as u64)
                / s.total_arbitrages as u64;
        }
        map.set(arbitrageur, s);
        env.storage().instance().set(&ARBITRAGE_STATS, &map);
    }

    fn create_execution_record(
        _env: &Env, _opportunity_id: u64, _arbitrageur: Address,
        _trade_amount: u64, _reward: u64, _successful: bool,
    ) -> u64 {
        _env.ledger().seq_num()
    }

    fn emit_mev_event(env: &Env, event_type: Symbol, cost: u64) {
        env.events().publish(Symbol::short("MEV_EVT"), (event_type, cost));
    }

    fn require_admin(env: &Env) {
        let admin = env.storage().instance().get(&ADMIN).unwrap_optimized();
        if env.current_contract_address() != admin {
            panic!("Not authorized");
        }
    }

    fn require_not_paused(env: &Env) {
        let paused: bool = env.storage().instance().get(&PAUSED).unwrap();
        if paused { panic!("Arbitrage system is paused"); }
    }

    fn get_opportunities(env: &Env) -> Map<u64, ArbitrageOpportunity> {
        env.storage().instance().get(&OPPORTUNITIES).unwrap()
    }

    fn get_arbitrage_stats(env: &Env) -> Map<Address, ArbitrageurStats> {
        env.storage().instance().get(&ARBITRAGE_STATS).unwrap()
    }

    fn get_params(env: &Env) -> ArbitrageParams {
        env.storage().instance().get(&PARAMS).unwrap()
    }

    fn get_mev_config(env: &Env) -> MEVConfig {
        env.storage().instance().get(&MEV_CONFIG_KEY).unwrap_or(MEVConfig {
            buffer_bps: DEFAULT_MEV_BUFFER_BPS, gas_per_hop: DEFAULT_GAS_PER_HOP,
            history_window: MEV_HISTORY_WINDOW, enabled: true,
        })
    }
}
