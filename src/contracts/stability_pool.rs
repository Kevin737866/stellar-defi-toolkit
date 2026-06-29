//! Stability Pool Contract for Stellar DeFi Toolkit
//!
//! Provides a mechanism for defending the stablecoin peg during market stress.
//! The stability pool acts as a backstop for liquidations and provides
//! incentives for users to deposit stablecoins.
//!
//! ## Features
//! - Deposit stablecoins to earn rewards
//! - Automatic liquidation coverage
//! - Reward distribution from liquidation gains
//! - Early withdrawal penalties
//! - Governance-controlled parameters

use crate::types::stablecoin::{
    LiquidationEvent, StabilityPoolDepositEvent, StabilityPoolInfo, StabilityPoolWithdrawalEvent,
    TreasuryInfo,
};
use soroban_sdk::{
    contract, contractimpl, unwrap::UnwrapOptimized, Address, Env, Map, Symbol, Vec,
};

// ─── Constants ───────────────────────────────────────────────────────────────

/// Reward rate for stability pool providers (5% APY)
const BASE_REWARD_RATE_BPS: u32 = 500;
/// Early withdrawal penalty (2%)
const EARLY_WITHDRAWAL_PENALTY_BPS: u32 = 200;
/// Minimum deposit period for full rewards (7 days)
const MIN_DEPOSIT_PERIOD: u64 = 7 * 24 * 3600;
/// Maximum deposit ratio of total supply (50%)
const MAX_DEPOSIT_RATIO: u32 = 5000;
/// Liquidation reward share for stability pool (80%)
const LIQUIDATION_REWARD_SHARE_BPS: u32 = 8000;

// ─── Storage Keys ─────────────────────────────────────────────────────────────

const ADMIN: Symbol = Symbol::short("ADMIN");
const PAUSED: Symbol = Symbol::short("PAUSED");
const STABLECOIN: Symbol = Symbol::short("STABLE");
const TREASURY: Symbol = Symbol::short("TREASURY");
const POOL_INFO: Symbol = Symbol::short("POOLINFO");
const USER_DEPOSITS: Symbol = Symbol::short("USERDEP");
const REWARD_INDEX: Symbol = Symbol::short("REWARDIDX");
const PARAMS: Symbol = Symbol::short("PARAMS");

// ─── User Deposit Information ─────────────────────────────────────────────────

#[derive(Clone, Debug)]
#[contracttype]
pub struct UserDeposit {
    /// Amount deposited by user
    pub amount: u64,
    /// Reward index at time of deposit
    pub reward_index: u64,
    /// Deposit timestamp
    pub deposit_timestamp: u64,
    /// Whether user has claimed rewards
    pub rewards_claimed: u64,
}

/// Stability pool parameters
#[derive(Clone, Debug)]
#[contracttype]
pub struct StabilityPoolParams {
    /// Base reward rate in basis points
    pub base_reward_rate_bps: u32,
    /// Early withdrawal penalty in basis points
    pub early_withdrawal_penalty_bps: u32,
    /// Minimum deposit period for full rewards
    pub min_deposit_period: u64,
    /// Maximum deposit ratio of total supply
    pub max_deposit_ratio: u32,
    /// Liquidation reward share for stability pool
    pub liquidation_reward_share_bps: u32,
}

// ─── Stability Pool Contract ───────────────────────────────────────────────────

/// Stability pool contract
#[contract]
pub struct StabilityPoolContract;

#[contractimpl]
impl StabilityPoolContract {
    /// Initialize the stability pool
    ///
    /// # Arguments
    /// * `admin` - Admin address for governance
    /// * `stablecoin_address` - Address of the stablecoin token
    /// * `treasury_address` - Address for fee collection
    pub fn initialize(
        env: Env,
        admin: Address,
        stablecoin_address: Address,
        treasury_address: Address,
    ) {
        if env.storage().instance().has(&ADMIN) {
            panic!("Already initialized");
        }

        env.storage().instance().set(&ADMIN, &admin);
        env.storage().instance().set(&PAUSED, &false);
        env.storage()
            .instance()
            .set(&STABLECOIN, &stablecoin_address);
        env.storage().instance().set(&TREASURY, &treasury_address);

        // Initialize pool info
        let pool_info = StabilityPoolInfo {
            total_deposits: 0,
            reward_per_share: 0,
            last_update: env.ledger().timestamp(),
        };
        env.storage().instance().set(&POOL_INFO, &pool_info);

        // Initialize parameters
        let params = StabilityPoolParams {
            base_reward_rate_bps: BASE_REWARD_RATE_BPS,
            early_withdrawal_penalty_bps: EARLY_WITHDRAWAL_PENALTY_BPS,
            min_deposit_period: MIN_DEPOSIT_PERIOD,
            max_deposit_ratio: MAX_DEPOSIT_RATIO,
            liquidation_reward_share_bps: LIQUIDATION_REWARD_SHARE_BPS,
        };
        env.storage().instance().set(&PARAMS, &params);

        // Initialize empty user deposits
        let user_deposits: Map<Address, UserDeposit> = Map::new(&env);
        env.storage().instance().set(&USER_DEPOSITS, &user_deposits);

        // Initialize reward index
        env.storage().instance().set(&REWARD_INDEX, &0u64);

        env.events().publish(
            Symbol::short("STABILITY_POOL_INITIALIZED"),
            (stablecoin_address, treasury_address),
        );
    }

    /// Deposit stablecoins into the stability pool
    ///
    /// # Arguments
    /// * `depositor` - Address making the deposit
    /// * `amount` - Amount to deposit
    pub fn deposit(env: Env, depositor: Address, amount: u64) {
        Self::require_not_paused(&env);

        if amount == 0 {
            panic!("Amount must be greater than 0");
        }

        // Check deposit limits
        let stablecoin_supply = Self::get_stablecoin_supply(&env);
        let params = Self::get_params(&env);
        let max_deposit = (stablecoin_supply * params.max_deposit_ratio as u64) / 10000;

        let pool_info = Self::get_pool_info(&env);
        if pool_info.total_deposits + amount > max_deposit {
            panic!("Deposit would exceed maximum pool size");
        }

        // Update rewards first
        Self::update_rewards(&env);

        // Get or create user deposit
        let mut user_deposits = Self::get_user_deposits(&env);
        let mut user_deposit = user_deposits.get(depositor.clone()).unwrap_or(UserDeposit {
            amount: 0,
            reward_index: 0,
            deposit_timestamp: env.ledger().timestamp(),
            rewards_claimed: 0,
        });

        // Update user deposit
        let current_reward_index = env.storage().instance().get(&REWARD_INDEX).unwrap();
        user_deposit.amount += amount;
        user_deposit.reward_index = current_reward_index;
        user_deposit.deposit_timestamp = env.ledger().timestamp();

        user_deposits.set(depositor.clone(), user_deposit);
        env.storage().instance().set(&USER_DEPOSITS, &user_deposits);

        // Update pool info
        let mut pool_info = Self::get_pool_info(&env);
        pool_info.total_deposits += amount;
        env.storage().instance().set(&POOL_INFO, &pool_info);

        // In production: Transfer stablecoins from user to this contract
        env.events().publish(
            (Symbol::short("STABILITY_DEPOSIT"), depositor.clone()),
            (amount, pool_info.total_deposits, current_reward_index),
        );
    }

    /// Withdraw from the stability pool
    ///
    /// # Arguments
    /// * `depositor` - Address making the withdrawal
    /// * `amount` - Amount to withdraw
    pub fn withdraw(env: Env, depositor: Address, amount: u64) {
        Self::require_not_paused(&env);

        if amount == 0 {
            panic!("Amount must be greater than 0");
        }

        // Update rewards first
        Self::update_rewards(&env);

        let mut user_deposits = Self::get_user_deposits(&env);
        let mut user_deposit = user_deposits
            .get(depositor.clone())
            .unwrap_or_else(|| panic!("No deposit found"));

        if user_deposit.amount < amount {
            panic!("Insufficient deposit balance");
        }

        let params = Self::get_params(&env);
        let current_time = env.ledger().timestamp();
        let deposit_age = current_time - user_deposit.deposit_timestamp;

        // Calculate withdrawal amount and penalty
        let (withdrawal_amount, penalty) = if deposit_age < params.min_deposit_period {
            let penalty_amount = (amount * params.early_withdrawal_penalty_bps as u64) / 10000;
            (amount - penalty_amount, penalty_amount)
        } else {
            (amount, 0)
        };

        // Calculate rewards
        let current_reward_index = env.storage().instance().get(&REWARD_INDEX).unwrap();
        let rewards_earned = Self::calculate_rewards(
            user_deposit.amount,
            user_deposit.reward_index,
            current_reward_index,
        );

        // Update user deposit
        user_deposit.amount -= amount;
        if user_deposit.amount == 0 {
            user_deposits.remove(depositor.clone());
        } else {
            user_deposits.set(depositor.clone(), user_deposit);
        }
        env.storage().instance().set(&USER_DEPOSITS, &user_deposits);

        // Update pool info
        let mut pool_info = Self::get_pool_info(&env);
        pool_info.total_deposits -= amount;
        env.storage().instance().set(&POOL_INFO, &pool_info);

        // Send penalty to treasury if applicable
        if penalty > 0 {
            let treasury = env.storage().instance().get(&TREASURY).unwrap();
            // In production: Transfer penalty to treasury
            env.events()
                .publish((Symbol::short("PENALTY_SENT"), treasury), penalty);
        }

        // In production: Transfer withdrawal amount and rewards to user
        env.events().publish(
            (Symbol::short("STABILITY_WITHDRAWAL"), depositor.clone()),
            (
                withdrawal_amount,
                rewards_earned,
                penalty,
                pool_info.total_deposits,
            ),
        );
    }

    /// Claim rewards from the stability pool
    ///
    /// # Arguments
    /// * `depositor` - Address claiming rewards
    pub fn claim_rewards(env: Env, depositor: Address) {
        Self::require_not_paused(&env);

        // Update rewards first
        Self::update_rewards(&env);

        let mut user_deposits = Self::get_user_deposits(&env);
        let mut user_deposit = user_deposits
            .get(depositor.clone())
            .unwrap_or_else(|| panic!("No deposit found"));

        let current_reward_index = env.storage().instance().get(&REWARD_INDEX).unwrap();
        let rewards_earned = Self::calculate_rewards(
            user_deposit.amount,
            user_deposit.reward_index,
            current_reward_index,
        );

        if rewards_earned == 0 {
            panic!("No rewards to claim");
        }

        // Update user's reward index
        user_deposit.reward_index = current_reward_index;
        user_deposit.rewards_claimed += rewards_earned;
        user_deposits.set(depositor.clone(), user_deposit);
        env.storage().instance().set(&USER_DEPOSITS, &user_deposits);

        // In production: Transfer rewards to user
        env.events().publish(
            (Symbol::short("REWARDS_CLAIMED"), depositor.clone()),
            rewards_earned,
        );
    }

    /// Process liquidation and distribute rewards
    ///
    /// # Arguments
    /// * `liquidation_event` - Details of the liquidation
    pub fn process_liquidation(env: Env, liquidation_event: LiquidationEvent) {
        Self::require_not_paused(&env);

        let params = Self::get_params(&env);
        let pool_info = Self::get_pool_info(&env);

        if pool_info.total_deposits == 0 {
            return; // No deposits to distribute to
        }

        // Calculate reward for stability pool
        let stability_reward =
            (liquidation_event.penalty_amount * params.liquidation_reward_share_bps as u64) / 10000;

        if stability_reward == 0 {
            return;
        }

        // Update reward index
        let mut reward_index = env.storage().instance().get(&REWARD_INDEX).unwrap();
        let reward_per_share = (stability_reward * 1000000) / pool_info.total_deposits; // Scale for precision
        reward_index += reward_per_share;
        env.storage().instance().set(&REWARD_INDEX, &reward_index);

        env.events().publish(
            (
                Symbol::short("LIQUIDATION_PROCESSED"),
                liquidation_event.vault_owner,
            ),
            (stability_reward, reward_index),
        );
    }

    /// Get user deposit information
    pub fn get_user_deposit(env: Env, user: Address) -> UserDeposit {
        let user_deposits = Self::get_user_deposits(&env);
        user_deposits.get(user).unwrap_or(UserDeposit {
            amount: 0,
            reward_index: 0,
            deposit_timestamp: 0,
            rewards_claimed: 0,
        })
    }

    /// Get pending rewards for a user
    pub fn get_pending_rewards(env: Env, user: Address) -> u64 {
        // Update rewards first
        Self::update_rewards(&env);

        let user_deposit = Self::get_user_deposit(env.clone(), user);
        if user_deposit.amount == 0 {
            return 0;
        }

        let current_reward_index = env.storage().instance().get(&REWARD_INDEX).unwrap();
        Self::calculate_rewards(
            user_deposit.amount,
            user_deposit.reward_index,
            current_reward_index,
        )
    }

    /// Get pool information
    pub fn get_pool_info(env: Env) -> StabilityPoolInfo {
        Self::get_pool_info(&env)
    }

    /// Get current parameters
    pub fn get_params(env: Env) -> StabilityPoolParams {
        Self::get_params(&env)
    }

    // ─── Admin Functions ───────────────────────────────────────────────────────

    /// Update pool parameters (admin only)
    pub fn update_params(env: Env, new_params: StabilityPoolParams) {
        Self::require_admin(&env);

        // Validate parameters
        if new_params.base_reward_rate_bps > 2000 {
            panic!("Reward rate too high"); // Max 20%
        }

        if new_params.early_withdrawal_penalty_bps > 1000 {
            panic!("Penalty too high"); // Max 10%
        }

        if new_params.max_deposit_ratio > 8000 {
            panic!("Deposit ratio too high"); // Max 80%
        }

        env.storage().instance().set(&PARAMS, &new_params);

        env.events().publish(
            Symbol::short("PARAMS_UPDATED"),
            (
                new_params.base_reward_rate_bps,
                new_params.early_withdrawal_penalty_bps,
                new_params.max_deposit_ratio,
            ),
        );
    }

    /// Pause the pool (admin only)
    pub fn pause(env: Env) {
        Self::require_admin(&env);
        env.storage().instance().set(&PAUSED, &true);
        env.events()
            .publish(Symbol::short("STABILITY_POOL_PAUSED"), true);
    }

    /// Unpause the pool (admin only)
    pub fn unpause(env: Env) {
        Self::require_admin(&env);
        env.storage().instance().set(&PAUSED, &false);
        env.events()
            .publish(Symbol::short("STABILITY_POOL_PAUSED"), false);
    }

    /// Update treasury address (admin only)
    pub fn update_treasury(env: Env, new_treasury: Address) {
        Self::require_admin(&env);
        env.storage().instance().set(&TREASURY, &new_treasury);
        env.events()
            .publish(Symbol::short("TREASURY_UPDATED"), new_treasury);
    }

    // ─── Internal Functions ─────────────────────────────────────────────────────

    fn update_rewards(env: &Env) {
        let pool_info = Self::get_pool_info(env);
        let params = Self::get_params(env);
        let current_time = env.ledger().timestamp();

        if pool_info.total_deposits == 0 {
            return;
        }

        let time_elapsed = current_time - pool_info.last_update;
        if time_elapsed == 0 {
            return;
        }

        // Calculate rewards for the elapsed time
        let rewards =
            (pool_info.total_deposits * params.base_reward_rate_bps as u64 * time_elapsed)
                / (10000 * 365 * 24 * 3600);

        if rewards == 0 {
            return;
        }

        // Update reward index
        let mut reward_index = env.storage().instance().get(&REWARD_INDEX).unwrap();
        let reward_per_share = (rewards * 1000000) / pool_info.total_deposits; // Scale for precision
        reward_index += reward_per_share;
        env.storage().instance().set(&REWARD_INDEX, &reward_index);

        // Update pool info
        let mut updated_pool_info = pool_info;
        updated_pool_info.last_update = current_time;
        env.storage().instance().set(&POOL_INFO, &updated_pool_info);
    }

    fn calculate_rewards(deposit_amount: u64, deposit_index: u64, current_index: u64) -> u64 {
        if current_index <= deposit_index {
            return 0;
        }

        let index_diff = current_index - deposit_index;
        (deposit_amount * index_diff) / 1000000 // Remove scaling
    }

    fn get_stablecoin_supply(env: &Env) -> u64 {
        // In production, this would query the stablecoin contract
        // For now, return a mock value
        100_000_000_000 // 10,000 stablecoins with 7 decimals
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
            panic!("Pool is paused");
        }
    }

    fn get_user_deposits(env: &Env) -> Map<Address, UserDeposit> {
        env.storage().instance().get(&USER_DEPOSITS).unwrap()
    }

    fn get_pool_info(env: &Env) -> StabilityPoolInfo {
        env.storage().instance().get(&POOL_INFO).unwrap()
    }

    fn get_params(env: &Env) -> StabilityPoolParams {
        env.storage().instance().get(&PARAMS).unwrap()
    }
}
