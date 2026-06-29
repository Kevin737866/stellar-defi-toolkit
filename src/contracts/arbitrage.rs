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

use crate::types::stablecoin::{AlertSeverity, ArbitrageOpportunity, OraclePrice, SystemStats};
use soroban_sdk::{
    contract, contractimpl, unwrap::UnwrapOptimized, Address, Env, Map, Symbol, Vec,
};

// ─── Constants ───────────────────────────────────────────────────────────────

/// Minimum price deviation to trigger arbitrage (0.1%)
const MIN_DEVIATION_BPS: u32 = 10;
/// Maximum deviation before emergency measures (5%)
const MAX_DEVIATION_BPS: u32 = 500;
/// Base reward rate (0.5% of trade volume)
const BASE_REWARD_RATE_BPS: u32 = 50;
/// Maximum reward rate (2% of trade volume)
const MAX_REWARD_RATE_BPS: u32 = 200;
/// Opportunity expiration time (30 minutes)
const OPPORTUNITY_EXPIRY: u64 = 30 * 60;
/// Minimum trade amount (100 stablecoins)
const MIN_TRADE_AMOUNT: u64 = 100_000_000;
/// Maximum reward per arbitrage (1000 stablecoins)
const MAX_REWARD_PER_ARBITRAGE: u64 = 1_000_000_000;

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

// ─── Arbitrage Parameters ───────────────────────────────────────────────────

#[derive(Clone, Debug)]
#[contracttype]
pub struct ArbitrageParams {
    /// Minimum deviation to trigger arbitrage
    pub min_deviation_bps: u32,
    /// Maximum deviation before emergency
    pub max_deviation_bps: u32,
    /// Base reward rate
    pub base_reward_rate_bps: u32,
    /// Maximum reward rate
    pub max_reward_rate_bps: u32,
    /// Opportunity expiration time
    pub opportunity_expiry: u64,
    /// Minimum trade amount
    pub min_trade_amount: u64,
    /// Maximum reward per arbitrage
    pub max_reward_per_arbitrage: u64,
}

/// Arbitrageur performance statistics
#[derive(Clone, Debug)]
#[contracttype]
pub struct ArbitrageurStats {
    /// Arbitrageur address
    pub address: Address,
    /// Total arbitrage count
    pub total_arbitrages: u32,
    /// Total volume processed
    pub total_volume: u64,
    /// Total rewards earned
    pub total_rewards: u64,
    /// Success rate (basis points)
    pub success_rate: u32,
    /// Last arbitrage timestamp
    pub last_arbitrage: u64,
    /// Average profit per arbitrage
    pub avg_profit: u64,
}

/// Arbitrage execution record
#[derive(Clone, Debug)]
#[contracttype]
pub struct ArbitrageExecution {
    /// Unique execution ID
    pub execution_id: u64,
    /// Opportunity ID
    pub opportunity_id: u64,
    /// Arbitrageur address
    pub arbitrageur: Address,
    /// Trade amount
    pub trade_amount: u64,
    /// Reward paid
    pub reward_paid: u64,
    /// Execution timestamp
    pub timestamp: u64,
    /// Whether execution was successful
    pub successful: bool,
}

// ─── Arbitrage Contract ─────────────────────────────────────────────────────

/// Arbitrage incentives contract
#[contract]
pub struct ArbitrageContract;

#[contractimpl]
impl ArbitrageContract {
    /// Initialize the arbitrage contract
    ///
    /// # Arguments
    /// * `admin` - Admin address for governance
    /// * `stablecoin_address` - Address of the stablecoin
    /// * `oracle_address` - Address of the price oracle
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
        env.storage()
            .instance()
            .set(&STABLECOIN, &stablecoin_address);
        env.storage().instance().set(&ORACLE, &oracle_address);
        env.storage().instance().set(&NEXT_OPPORTUNITY_ID, &1u64);
        env.storage().instance().set(&TOTAL_REWARDS_PAID, &0u64);

        // Initialize parameters
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

        // Initialize empty storage
        // TODO: Fix type serialization issues
        // let opportunities: Map<u64, ArbitrageOpportunity> = Map::new(&env);
        // env.storage().instance().set(&OPPORTUNITIES, &opportunities);

        // let arbitrage_stats: Map<Address, ArbitrageurStats> = Map::new(&env);
        // env.storage().instance().set(&ARBITRAGE_STATS, &arbitrage_stats);

        env.events().publish(
            Symbol::short("ARBITRAGE_INITIALIZED"),
            (admin, stablecoin_address, oracle_address),
        );
    }

    /// Detect and create arbitrage opportunities
    ///
    /// This function is typically called by an automated bot or oracle
    /// when price deviations are detected.
    ///
    /// # Arguments
    /// * `source_token` - Token trading away from peg
    /// * `target_token` - Token trading toward peg
    /// * `price_diff_bps` - Price deviation in basis points
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

        let opportunity_id = env.storage().instance().get(&NEXT_OPPORTUNITY_ID).unwrap();
        let next_id = opportunity_id + 1;
        env.storage().instance().set(&NEXT_OPPORTUNITY_ID, &next_id);

        // Calculate potential profit and required capital
        let (potential_profit, required_capital) = Self::calculate_arbitrage_metrics(
            &env,
            source_token.clone(),
            target_token.clone(),
            price_diff_bps,
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
            (Symbol::short("OPPORTUNITY_DETECTED"), source_token.clone()),
            (opportunity_id, price_diff_bps, potential_profit),
        );

        opportunity_id
    }

    /// Execute an arbitrage opportunity
    ///
    /// # Arguments
    /// * `arbitrageur` - Address performing the arbitrage
    /// * `opportunity_id` - ID of the opportunity to execute
    /// * `trade_amount` - Amount to trade
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

        // Calculate reward
        let reward = Self::calculate_reward(&env, trade_amount, opportunity.price_diff_bps);

        // Update arbitrageur statistics
        Self::update_arbitrageur_stats(&env, arbitrageur.clone(), trade_amount, reward, true);

        // Mark opportunity as used
        opportunity.valid = false;
        opportunities.set(opportunity_id, opportunity);
        env.storage().instance().set(&OPPORTUNITIES, &opportunities);

        // Update total rewards paid
        let mut total_rewards = env.storage().instance().get(&TOTAL_REWARDS_PAID).unwrap();
        total_rewards += reward;
        env.storage()
            .instance()
            .set(&TOTAL_REWARDS_PAID, &total_rewards);

        // Create execution record
        let execution_id = Self::create_execution_record(
            &env,
            opportunity_id,
            arbitrageur.clone(),
            trade_amount,
            reward,
            true,
        );

        // In production: Transfer reward to arbitrageur
        env.events().publish(
            (Symbol::short("ARBITRAGE_EXECUTED"), arbitrageuer.clone()),
            (opportunity_id, trade_amount, reward, execution_id),
        );
    }

    /// Report failed arbitrage attempt
    ///
    /// # Arguments
    /// * `arbitrageur` - Address that attempted the arbitrage
    /// * `opportunity_id` - ID of the opportunity
    /// * `reason` - Reason for failure
    pub fn report_failed_arbitrage(
        env: Env,
        arbitrageur: Address,
        opportunity_id: u64,
        reason: Symbol,
    ) {
        Self::require_not_paused(&env);

        // Update arbitrageur statistics (failed attempt)
        Self::update_arbitrageur_stats(&env, arbitrageur.clone(), 0, 0, false);

        // Create execution record
        Self::create_execution_record(&env, opportunity_id, arbitrageur.clone(), 0, 0, false);

        env.events().publish(
            (Symbol::short("ARBITRAGE_FAILED"), arbitrageur.clone()),
            (opportunity_id, reason),
        );
    }

    /// Get active arbitrage opportunities
    pub fn get_active_opportunities(env: Env) -> Vec<ArbitrageOpportunity> {
        let opportunities = Self::get_opportunities(&env);
        let mut active_opportunities = Vec::new(&env);
        let current_time = env.ledger().timestamp();

        for opportunity in opportunities.values() {
            if opportunity.valid && current_time <= opportunity.expires_at {
                active_opportunities.push_back(opportunity);
            }
        }

        active_opportunities
    }

    /// Get arbitrageur statistics
    pub fn get_arbitrageur_stats(env: Env, arbitrageur: Address) -> ArbitrageurStats {
        let arbitrage_stats = Self::get_arbitrage_stats(&env);
        arbitrage_stats
            .get(arbitrageur)
            .unwrap_or(ArbitrageurStats {
                address: arbitrageur,
                total_arbitrages: 0,
                total_volume: 0,
                total_rewards: 0,
                success_rate: 0,
                last_arbitrage: 0,
                avg_profit: 0,
            })
    }

    /// Get system statistics
    pub fn get_system_stats(env: Env) -> SystemStats {
        let total_rewards = env.storage().instance().get(&TOTAL_REWARDS_PAID).unwrap();
        let arbitrage_stats = Self::get_arbitrage_stats(&env);

        let mut total_volume = 0u64;
        let mut active_arbitrageurs = 0u32;

        for stats in arbitrage_stats.values() {
            total_volume += stats.total_volume;
            if stats.last_arbitrage > 0 {
                active_arbitrageurs += 1;
            }
        }

        SystemStats {
            total_value_locked: total_volume,
            total_supply: Self::get_stablecoin_supply(&env),
            active_vaults: active_arbitrageurs,
            average_collateral_ratio: 10000, // Placeholder
            stability_pool_size: 0,          // Placeholder
            daily_liquidations: 0,           // Placeholder
            daily_minting_volume: 0,         // Placeholder
            daily_redemption_volume: 0,      // Placeholder
            health_score: Self::calculate_health_score(&env),
        }
    }

    /// Get current parameters
    pub fn get_params(env: Env) -> ArbitrageParams {
        Self::get_params(&env)
    }

    // ─── Admin Functions ───────────────────────────────────────────────────────

    /// Update arbitrage parameters (admin only)
    pub fn update_params(env: Env, new_params: ArbitrageParams) {
        Self::require_admin(&env);

        // Validate parameters
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
            Symbol::short("ARBITRAGE_PARAMS_UPDATED"),
            (
                new_params.min_deviation_bps,
                new_params.max_deviation_bps,
                new_params.base_reward_rate_bps,
            ),
        );
    }

    /// Pause the arbitrage system (admin only)
    pub fn pause(env: Env) {
        Self::require_admin(&env);
        env.storage().instance().set(&PAUSED, &true);
        env.events()
            .publish(Symbol::short("ARBITRAGE_PAUSED"), true);
    }

    /// Unpause the arbitrage system (admin only)
    pub fn unpause(env: Env) {
        Self::require_admin(&env);
        env.storage().instance().set(&PAUSED, &false);
        env.events()
            .publish(Symbol::short("ARBITRAGE_PAUSED"), false);
    }

    // ─── Internal Functions ─────────────────────────────────────────────────────

    fn calculate_arbitrage_metrics(
        env: &Env,
        _source_token: Address,
        _target_token: Address,
        price_diff_bps: u32,
    ) -> (u64, u64) {
        // Simplified calculation - in production this would be more sophisticated
        let base_amount = 1_000_000_000; // 10,000 stablecoins
        let potential_profit = (base_amount * price_diff_bps as u64) / 10000;
        let required_capital = base_amount;

        (potential_profit, required_capital)
    }

    fn calculate_reward(env: &Env, trade_amount: u64, price_diff_bps: u32) -> u64 {
        let params = Self::get_params(env);

        // Scale reward based on deviation severity
        let reward_rate = if price_diff_bps <= 50 {
            params.base_reward_rate_bps
        } else if price_diff_bps <= 200 {
            params.base_reward_rate_bps + (price_diff_bps - 50) / 2
        } else {
            params.max_reward_rate_bps
        };

        let reward = (trade_amount * reward_rate as u64) / 10000;
        reward.min(params.max_reward_per_arbitrage)
    }

    fn update_arbitrageur_stats(
        env: &Env,
        arbitrageur: Address,
        trade_amount: u64,
        reward: u64,
        successful: bool,
    ) {
        let mut arbitrage_stats = Self::get_arbitrage_stats(env);
        let mut stats = arbitrage_stats
            .get(arbitrageur.clone())
            .unwrap_or(ArbitrageurStats {
                address: arbitrageur.clone(),
                total_arbitrages: 0,
                total_volume: 0,
                total_rewards: 0,
                success_rate: 10000, // 100%
                last_arbitrage: 0,
                avg_profit: 0,
            });

        stats.total_arbitrages += 1;
        stats.total_volume += trade_amount;
        stats.total_rewards += reward;
        stats.last_arbitrage = env.ledger().timestamp();

        if successful {
            stats.avg_profit = (stats.avg_profit * (stats.total_arbitrages - 1) as u64 + reward)
                / stats.total_arbitrages as u64;
        } else {
            // Update success rate (simplified)
            stats.success_rate = (stats.success_rate * (stats.total_arbitrages - 1) as u64)
                / stats.total_arbitrages as u64;
        }

        arbitrage_stats.set(arbitrageur, stats);
        env.storage()
            .instance()
            .set(&ARBITRAGE_STATS, &arbitrage_stats);
    }

    fn create_execution_record(
        env: &Env,
        opportunity_id: u64,
        arbitrageur: Address,
        trade_amount: u64,
        reward: u64,
        successful: bool,
    ) -> u64 {
        let execution_id = env.ledger().seq_num(); // Use ledger number as unique ID

        let execution = ArbitrageExecution {
            execution_id,
            opportunity_id,
            arbitrageur,
            trade_amount,
            reward_paid: reward,
            timestamp: env.ledger().timestamp(),
            successful,
        };

        // In production, store execution records for analytics
        env.events().publish(
            (Symbol::short("EXECUTION_RECORDED"), arbitrageur.clone()),
            (execution_id, opportunity_id, successful),
        );

        execution_id
    }

    fn calculate_health_score(env: &Env) -> u32 {
        // Simple health score calculation based on recent arbitrage activity
        let total_rewards = env.storage().instance().get(&TOTAL_REWARDS_PAID).unwrap();

        if total_rewards == 0 {
            return 10000; // Perfect health
        }

        // In production, this would be more sophisticated
        8500 // 85% health
    }

    fn get_stablecoin_supply(env: &Env) -> u64 {
        // In production, query the stablecoin contract
        100_000_000_000 // Mock: 10,000 stablecoins
    }

    fn require_admin(env: &Env) {
        let admin = env.storage().instance().get(&ADMIN).unwrap_optimized();
        if env.current_contract_address() != admin {
            panic!("Not authorized");
        }
    }

    fn require_not_paused(env: &Env) {
        let paused = env.storage().instance().get(&PAUSED).unwrap();
        if paused {
            panic!("Arbitrage system is paused");
        }
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
}
