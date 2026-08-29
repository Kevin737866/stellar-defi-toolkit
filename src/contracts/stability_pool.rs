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
//!
//! ## Access Control
//! - **Admin**: `update_params`, `pause`, `unpause`, `update_treasury` — gated by a
//!   broken `require_admin()` (compares the contract's own address, not the caller).
//!   See `docs/ACCESS_CONTROL_MATRIX.md`.
//! - **Keeper**: `process_liquidation` — intended for the lending/vault contract, but
//!   has no auth check at all.
//! - **User**: `deposit`, `withdraw`, `claim_rewards` — none call `require_auth()` on
//!   the `depositor` address; any caller can withdraw/claim on behalf of any depositor.

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
const AUTO_COMPOUND: Symbol = Symbol::short("AUTOCOMP");
const MIGRATION_VERSION: Symbol = Symbol::short("MIGVER");
const MIGRATION_EVENTS: Symbol = Symbol::short("MIGEVENTS");

// Issue #202: Withdrawal queue
const WITHDRAWAL_QUEUE: Symbol = Symbol::short("WITHQ");
const DAILY_WITHDRAWN: Symbol = Symbol::short("DAILYW");
const LAST_DAY_TIMESTAMP: Symbol = Symbol::short("LASTDAY");

// Issue #203: Liquidation distribution
const PENDING_DISTRIBUTIONS: Symbol = Symbol::short("PENDIST");
const DISTRIBUTION_HISTORY: Symbol = Symbol::short("DISTHIST");

// Issue #204: Pool cap management
const POOL_CAP: Symbol = Symbol::short("POOLCAP");
const CAP_HISTORY: Symbol = Symbol::short("CAPHIST");
const OVERRIDE_CAP: Symbol = Symbol::short("OVRCAP");

// Issue #205: Multi-token support
const TOKEN_CONFIGS: Symbol = Symbol::short("TOKCONF");
const USER_TOKEN_DEPOSITS: Symbol = Symbol::short("USRTKDEP");
const TOTAL_TOKEN_DEPOSITS: Symbol = Symbol::short("TOTTOKDEP");
const ORACLE_RATES: Symbol = Symbol::short("ORACLERATES");

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

// Issue #202: Withdrawal queue entry
#[derive(Clone, Debug)]
#[contracttype]
pub struct WithdrawalQueueEntry {
    /// User requesting withdrawal
    pub user: Address,
    /// Amount requested
    pub amount: u64,
    /// Queue position
    pub position: u64,
    /// Request timestamp
    pub requested_at: u64,
    /// Estimated processing time
    pub estimated_completion: u64,
    /// Whether this is an emergency withdrawal
    pub emergency: bool,
    /// Token address (for multi-token support)
    pub token: Address,
}

// Issue #203: Distribution record
#[derive(Clone, Debug)]
#[contracttype]
pub struct DistributionRecord {
    /// Distribution ID
    pub distribution_id: u64,
    /// Liquidation event ID
    pub liquidation_id: u64,
    /// Total amount distributed
    pub total_amount: u64,
    /// Number of recipients
    pub recipients: u64,
    /// Distribution timestamp
    pub timestamp: u64,
    /// Reward index after distribution
    pub reward_index_after: u64,
}

// Issue #204: Pool cap parameters
#[derive(Clone, Debug)]
#[contracttype]
pub struct PoolCapParams {
    /// Current dynamic cap
    pub current_cap: u64,
    /// Minimum cap
    pub min_cap: u64,
    /// Maximum cap
    pub max_cap: u64,
    /// Cap adjustment sensitivity (basis points)
    pub adjustment_sensitivity_bps: u32,
    /// Last cap adjustment timestamp
    pub last_adjustment: u64,
    /// Manual override cap (0 if not set)
    pub manual_override: u64,
    /// Reason for last cap change
    pub last_change_rationale: Symbol,
}

// Issue #205: Multi-token configuration
#[derive(Clone, Debug)]
#[contracttype]
pub struct TokenConfig {
    /// Token address
    pub token: Address,
    /// Token symbol
    pub symbol: Symbol,
    /// Current cap for this token
    pub cap: u64,
    /// Current balance in pool
    pub balance: u64,
    /// Oracle rate to base token (scaled by 1e6)
    pub oracle_rate: u64,
    /// Whether token is enabled
    pub enabled: bool,
    /// Decimals
    pub decimals: u32,
}

// Issue #205: Multi-token user deposit
#[derive(Clone, Debug)]
#[contracttype]
pub struct UserTokenDeposit {
    /// Amount deposited for this token
    pub amount: u64,
    /// Reward index at time of deposit
    pub reward_index: u64,
    /// Deposit timestamp
    pub deposit_timestamp: u64,
    /// Rewards claimed
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

/// Per-user auto-compounding preference and last compound time.
#[derive(Clone, Debug)]
#[contracttype]
pub struct AutoCompoundConfig {
    /// Whether rewards should be reinvested automatically.
    pub enabled: bool,
    /// Minimum interval between automatic compounds in seconds.
    pub interval: u64,
    /// Last ledger timestamp at which rewards were compounded.
    pub last_compounded_at: u64,
}

// ─── Stability Pool Contract ───────────────────────────────────────────────────

/// Migration record for audit trail.
#[derive(Clone, Debug)]
#[contracttype]
pub struct MigrationRecord {
    pub migration_id: u64,
    pub user: Address,
    pub amount: u64,
    pub reward_index: u64,
    pub deposit_timestamp: u64,
    pub migrated_at: u64,
}

/// Pool health metrics for dashboard integration.
#[derive(Clone, Debug)]
#[contracttype]
pub struct PoolHealthMetrics {
    pub coverage_ratio_bps: u32,
    pub concentration_bps: u32,
    pub withdrawal_pressure_bps: u32,
    pub reward_sustainability_bps: u32,
    pub health_score: u32,
    pub pool_size: u64,
    pub total_debt: u64,
    pub depositor_count: u32,
}

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

    /// Configure automatic reinvestment of a depositor's pending rewards.
    pub fn set_auto_compound(env: Env, depositor: Address, enabled: bool, interval: u64) {
        Self::require_not_paused(&env);
        if interval == 0 {
            panic!("Compound interval must be greater than 0");
        }
        let mut configs: Map<Address, AutoCompoundConfig> = env.storage().instance()
            .get(&AUTO_COMPOUND).unwrap_or_else(|| Map::new(&env));
        configs.set(depositor.clone(), AutoCompoundConfig {
            enabled,
            interval,
            last_compounded_at: env.ledger().timestamp(),
        });
        env.storage().instance().set(&AUTO_COMPOUND, &configs);
        env.events().publish(
            (Symbol::short("AUTO_COMPOUND_UPDATED"), depositor),
            (enabled, interval),
        );
    }

    /// Return a depositor's auto-compounding configuration.
    pub fn get_auto_compound(env: Env, depositor: Address) -> AutoCompoundConfig {
        let configs: Map<Address, AutoCompoundConfig> = env.storage().instance()
            .get(&AUTO_COMPOUND).unwrap_or_else(|| Map::new(&env));
        configs.get(depositor).unwrap_or(AutoCompoundConfig {
            enabled: false,
            interval: 0,
            last_compounded_at: 0,
        })
    }

    /// Reinvest one depositor's pending rewards into the pool.
    pub fn compound(env: Env, depositor: Address) {
        Self::require_not_paused(&env);
        Self::update_rewards(&env);
        let mut configs: Map<Address, AutoCompoundConfig> = env.storage().instance()
            .get(&AUTO_COMPOUND).unwrap_or_else(|| Map::new(&env));
        let mut config = configs.get(depositor.clone()).unwrap_or_else(|| panic!("Auto-compounding is not configured"));
        if !config.enabled || env.ledger().timestamp() < config.last_compounded_at + config.interval {
            panic!("Compound interval has not elapsed");
        }
        let mut deposits = Self::get_user_deposits(&env);
        let mut deposit = deposits.get(depositor.clone()).unwrap_or_else(|| panic!("No deposit found"));
        let index = env.storage().instance().get(&REWARD_INDEX).unwrap();
        let rewards = Self::calculate_rewards(deposit.amount, deposit.reward_index, index);
        if rewards == 0 {
            panic!("No rewards to compound");
        }
        let before = deposit.amount;
        deposit.amount = deposit.amount.checked_add(rewards).unwrap_or_else(|| panic!("Compound amount overflow"));
        deposit.reward_index = index;
        deposit.rewards_claimed = deposit.rewards_claimed.checked_add(rewards).unwrap_or_else(|| panic!("Rewards overflow"));
        deposits.set(depositor.clone(), deposit.clone());
        env.storage().instance().set(&USER_DEPOSITS, &deposits);
        let mut pool = Self::get_pool_info(&env);
        pool.total_deposits = pool.total_deposits.checked_add(rewards).unwrap_or_else(|| panic!("Pool balance overflow"));
        env.storage().instance().set(&POOL_INFO, &pool);
        config.last_compounded_at = env.ledger().timestamp();
        configs.set(depositor.clone(), config);
        env.storage().instance().set(&AUTO_COMPOUND, &configs);
        env.events().publish((Symbol::short("REWARDS_COMPOUNDED"), depositor), (before, deposit.amount, rewards));
    }

    /// Compound all eligible depositors in a single call.
    pub fn compound_all(env: Env) {
        Self::require_not_paused(&env);
        let deposits = Self::get_user_deposits(&env);
        for depositor in deposits.keys() {
            let config = Self::get_auto_compound(env.clone(), depositor.clone());
            if config.enabled && env.ledger().timestamp() >= config.last_compounded_at + config.interval {
                Self::compound(env.clone(), depositor);
            }
        }
    }

    // ─── Issue #202: Withdrawal Queue Management ───────────────────────────────

    /// Request a withdrawal - large withdrawals enter queue
    ///
    /// # Arguments
    /// * `user` - Address requesting withdrawal
    /// * `amount` - Amount to withdraw
    pub fn request_withdrawal(env: Env, user: Address, amount: u64, token: Address) {
        Self::require_not_paused(&env);

        if amount == 0 {
            panic!("Amount must be greater than 0");
        }

        let pool_info = Self::get_pool_info(&env);
        let withdrawal_threshold_bps = 500; // 5% of pool triggers queue
        let large_withdrawal = (amount * 10000) / pool_info.total_deposits > withdrawal_threshold_bps;

        if !large_withdrawal {
            // Small withdrawal - process immediately
            Self::withdraw(env.clone(), user.clone(), amount);
            return;
        }

        // Check daily limit
        let current_time = env.ledger().timestamp();
        let mut last_day = env.storage().instance().get(&LAST_DAY_TIMESTAMP).unwrap_or(0u64);
        let mut daily_withdrawn = env.storage().instance().get(&DAILY_WITHDRAWN).unwrap_or(0u64);

        // Reset daily counter if new day
        if current_time - last_day >= 24 * 3600 {
            last_day = current_time;
            daily_withdrawn = 0;
            env.storage().instance().set(&LAST_DAY_TIMESTAMP, &last_day);
        }

        let daily_limit = Self::get_daily_withdrawal_limit(&env);
        if daily_withdrawn + amount > daily_limit {
            panic!("Daily withdrawal limit exceeded");
        }

        // Add to queue
        let mut queue = Self::get_withdrawal_queue(&env);
        let position = queue.len() as u64 + 1;
        let estimated_completion = current_time + (position * 3600); // 1 hour per position

        let entry = WithdrawalQueueEntry {
            user: user.clone(),
            amount,
            position,
            requested_at: current_time,
            estimated_completion,
            emergency: false,
            token,
        };

        queue.push_back(entry);
        env.storage().instance().set(&WITHDRAWAL_QUEUE, &queue);

        env.events().publish(
            (Symbol::short("WITHDRAWAL_QUEUED"), user),
            (amount, position, estimated_completion),
        );
    }

    /// Process withdrawal queue (FIFO)
    pub fn process_withdrawal_queue(env: Env) {
        Self::require_admin(&env);

        let mut queue = Self::get_withdrawal_queue(&env);
        let mut daily_withdrawn = env.storage().instance().get(&DAILY_WITHDRAWN).unwrap_or(0u64);
        let daily_limit = Self::get_daily_withdrawal_limit(&env);
        let current_time = env.ledger().timestamp();

        let mut processed = Vec::new(&env);

        for i in 0..queue.len() {
            let entry = queue.get(i).unwrap();
            if entry.estimated_completion > current_time {
                continue;
            }

            if daily_withdrawn + entry.amount > daily_limit {
                break;
            }

            // Process withdrawal
            Self::withdraw(env.clone(), entry.user.clone(), entry.amount);
            daily_withdrawn += entry.amount;
            processed.push_back(entry.position);
        }

        // Remove processed entries
        let mut new_queue = Vec::new(&env);
        for i in 0..queue.len() {
            let entry = queue.get(i).unwrap();
            if !processed.contains(&entry.position) {
                new_queue.push_back(entry);
            }
        }

        env.storage().instance().set(&WITHDRAWAL_QUEUE, &new_queue);
        env.storage().instance().set(&DAILY_WITHDRAWN, &daily_withdrawn);

        env.events().publish(
            Symbol::short("QUEUE_PROCESSED"),
            processed.len(),
        );
    }

    /// Emergency withdrawal with penalty - bypasses queue
    ///
    /// # Arguments
    /// * `user` - Address requesting emergency withdrawal
    /// * `amount` - Amount to withdraw
    pub fn emergency_withdraw(env: Env, user: Address, amount: u64) {
        Self::require_not_paused(&env);

        if amount == 0 {
            panic!("Amount must be greater than 0");
        }

        // Apply emergency penalty (10%)
        let penalty = (amount * 1000) / 10000;
        let withdrawal_amount = amount - penalty;

        // Process immediately regardless of queue
        Self::withdraw(env.clone(), user.clone(), withdrawal_amount);

        // Send penalty to treasury
        let treasury = env.storage().instance().get(&TREASURY).unwrap();
        env.events()
            .publish((Symbol::short("EMERGENCY_WITHDRAWAL"), user), (withdrawal_amount, penalty));
    }

    /// Get withdrawal queue status
    pub fn get_withdrawal_queue_status(env: Env) -> Vec<WithdrawalQueueEntry> {
        Self::get_withdrawal_queue(&env)
    }

    /// Get user's queue position
    pub fn get_user_queue_position(env: Env, user: Address) -> u64 {
        let queue = Self::get_withdrawal_queue(&env);
        for i in 0..queue.len() {
            let entry = queue.get(i).unwrap();
            if entry.user == user {
                return entry.position;
            }
        }
        0
    }

    // ─── Issue #203: Liquidation Distribution Automation ───────────────────────

    /// Process liquidation with automatic distribution
    ///
    /// # Arguments
    /// * `liquidation_event` - Details of the liquidation
    pub fn process_liquidation_auto_distribute(env: Env, liquidation_event: LiquidationEvent) {
        Self::require_not_paused(&env);

        let params = Self::get_params(&env);
        let pool_info = Self::get_pool_info(&env);

        if pool_info.total_deposits == 0 {
            return;
        }

        // Calculate reward for stability pool
        let stability_reward =
            (liquidation_event.penalty_amount * params.liquidation_reward_share_bps as u64) / 10000;

        if stability_reward == 0 {
            return;
        }

        // Update reward index atomically
        let mut reward_index = env.storage().instance().get(&REWARD_INDEX).unwrap();
        let reward_per_share = (stability_reward * 1000000) / pool_info.total_deposits;
        reward_index += reward_per_share;
        env.storage().instance().set(&REWARD_INDEX, &reward_index);

        // Record distribution
        let distribution_id = env.ledger().seq_num();
        let distribution = DistributionRecord {
            distribution_id,
            liquidation_id: distribution_id,
            total_amount: stability_reward,
            recipients: Self::get_depositor_count(&env),
            timestamp: env.ledger().timestamp(),
            reward_index_after: reward_index,
        };

        let mut history = Self::get_distribution_history(&env);
        history.push_back(distribution);
        env.storage().instance().set(&DISTRIBUTION_HISTORY, &history);

        env.events().publish(
            (Symbol::short("LIQUIDATION_DISTRIBUTED"), liquidation_event.vault_owner),
            (stability_reward, reward_index, distribution_id),
        );
    }

    /// Get distribution history
    pub fn get_distribution_history(env: Env) -> Vec<DistributionRecord> {
        env.storage()
            .instance()
            .get(&DISTRIBUTION_HISTORY)
            .unwrap_or_else(|| Vec::new(&env))
    }

    // ─── Issue #204: Pool Cap Management ──────────────────────────────────────

    /// Get current pool cap (dynamic or manual override)
    pub fn get_current_cap(env: Env) -> u64 {
        let cap_params = Self::get_pool_cap_params(&env);
        if cap_params.manual_override > 0 {
            return cap_params.manual_override;
        }

        // Dynamic cap based on stablecoin supply and collateral health
        let stablecoin_supply = Self::get_stablecoin_supply(&env);
        let collateral_health = Self::get_collateral_health(&env);

        // Base cap is percentage of stablecoin supply
        let params = Self::get_params(&env);
        let supply_based_cap = (stablecoin_supply * params.max_deposit_ratio as u64) / 10000;

        // Adjust based on collateral health (healthier = higher cap)
        let health_adjustment = (collateral_health * cap_params.adjustment_sensitivity_bps as u64) / 10000;
        let adjusted_cap = supply_based_cap + (supply_based_cap * health_adjustment) / 10000;

        adjusted_cap.min(cap_params.max_cap).max(cap_params.min_cap)
    }

    /// Update pool cap (admin only, with rationale)
    ///
    /// # Arguments
    /// * `new_cap` - New cap value (0 for auto)
    /// * `rationale` - Reason for cap change
    pub fn update_pool_cap(env: Env, new_cap: u64, rationale: Symbol) {
        Self::require_admin(&env);

        let mut cap_params = Self::get_pool_cap_params(&env);
        cap_params.manual_override = new_cap;
        cap_params.last_adjustment = env.ledger().timestamp();
        cap_params.last_change_rationale = rationale;

        env.storage().instance().set(&POOL_CAP, &cap_params);

        // Record cap change
        env.storage().instance().set(&CAP_HISTORY, &cap_params);

        env.events().publish(
            (Symbol::short("CAP_UPDATED"), rationale),
            (new_cap, cap_params.current_cap),
        );
    }

    /// Dynamically adjust cap based on market conditions
    pub fn adjust_cap_dynamically(env: Env) {
        Self::require_admin(&env);

        let new_cap = Self::get_current_cap(env.clone());
        let mut cap_params = Self::get_pool_cap_params(&env);
        cap_params.current_cap = new_cap;
        cap_params.last_adjustment = env.ledger().timestamp();
        cap_params.last_change_rationale = Symbol::short("Dynamic adjustment");

        env.storage().instance().set(&POOL_CAP, &cap_params);

        env.events().publish(
            Symbol::short("CAP_ADJUSTED"),
            new_cap,
        );
    }

    /// Get pool cap parameters
    pub fn get_pool_cap_params(env: Env) -> PoolCapParams {
        env.storage()
            .instance()
            .get(&POOL_CAP)
            .unwrap_or_else(|| PoolCapParams {
                current_cap: 0,
                min_cap: 1_000_000_000,
                max_cap: 100_000_000_000_000,
                adjustment_sensitivity_bps: 500,
                last_adjustment: 0,
                manual_override: 0,
                last_change_rationale: Symbol::short("Initial"),
            })
    }

    // ─── Issue #205: Multi-Token Support ───────────────────────────────────────

    /// Add token to stability pool
    ///
    /// # Arguments
    /// * `token` - Token address
    /// * `symbol` - Token symbol
    /// * `oracle_rate` - Exchange rate to base token
    /// * `cap` - Token-specific cap
    pub fn add_pool_token(env: Env, token: Address, symbol: Symbol, oracle_rate: u64, cap: u64, decimals: u32) {
        Self::require_admin(&env);

        let config = TokenConfig {
            token: token.clone(),
            symbol,
            cap,
            balance: 0,
            oracle_rate,
            enabled: true,
            decimals,
        };

        let mut tokens = Self::get_token_configs(&env);
        tokens.set(token, config);
        env.storage().instance().set(&TOKEN_CONFIGS, &tokens);

        env.events().publish(
            Symbol::short("TOKEN_ADDED"),
            (token, oracle_rate, cap),
        );
    }

    /// Deposit multi-token stablecoins
    ///
    /// # Arguments
    /// * `user` - Address making deposit
    /// * `token` - Token address
    /// * `amount` - Amount to deposit
    pub fn deposit_token(env: Env, user: Address, token: Address, amount: u64) {
        Self::require_not_paused(&env);

        if amount == 0 {
            panic!("Amount must be greater than 0");
        }

        // Check token config
        let tokens = Self::get_token_configs(&env);
        let token_config = tokens.get(token.clone()).unwrap_or_else(|| panic!("Token not supported"));

        if !token_config.enabled {
            panic!("Token not enabled");
        }

        // Check token-specific cap
        if token_config.balance + amount > token_config.cap {
            panic!("Token cap exceeded");
        }

        // Check global cap
        let current_cap = Self::get_current_cap(env.clone());
        let pool_info = Self::get_pool_info(&env);
        if pool_info.total_deposits + amount > current_cap {
            panic!("Pool cap exceeded");
        }

        // Update token balance
        let mut updated_token_config = token_config;
        updated_token_config.balance += amount;
        let mut updated_tokens = tokens;
        updated_tokens.set(token.clone(), updated_token_config);
        env.storage().instance().set(&TOKEN_CONFIGS, &updated_tokens);

        // Update user token deposit
        let mut user_deposits = Self::get_user_token_deposits(&env);
        let mut user_deposit = user_deposits.get(user.clone()).unwrap_or_else(|| Vec::new(&env));

        let mut found = false;
        for i in 0..user_deposit.len() {
            let mut entry = user_deposit.get(i).unwrap();
            if entry.token == token {
                entry.amount += amount;
                user_deposit.set(i, entry);
                found = true;
                break;
            }
        }

        if !found {
            let new_entry = UserTokenDeposit {
                amount,
                reward_index: 0,
                deposit_timestamp: env.ledger().timestamp(),
                rewards_claimed: 0,
            };
            user_deposit.push_back(new_entry);
        }

        user_deposits.set(user.clone(), user_deposit);
        env.storage().instance().set(&USER_TOKEN_DEPOSITS, &user_deposits);

        // Update pool info
        let mut pool_info = Self::get_pool_info(&env);
        pool_info.total_deposits += amount;
        env.storage().instance().set(&POOL_INFO, &pool_info);

        env.events().publish(
            (Symbol::short("TOKEN_DEPOSIT"), user),
            (token, amount, pool_info.total_deposits),
        );
    }

    /// Get supported tokens
    pub fn get_supported_tokens(env: Env) -> Vec<TokenConfig> {
        let tokens = Self::get_token_configs(&env);
        let mut result = Vec::new(&env);
        for i in 0..tokens.len() {
            result.push_back(tokens.get(i).unwrap());
        }
        result
    }

    /// Get user token deposits
    pub fn get_user_token_deposits(env: Env, user: Address) -> Vec<UserTokenDeposit> {
        let user_deposits = Self::get_user_token_deposits(&env);
        user_deposits.get(user).unwrap_or_else(|| Vec::new(&env))
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

    // ─── Migration (Issue #207) ────────────────────────────────────────────────

    /// Migrate user deposit to new pool version (admin-assisted one-click migration)
    ///
    /// # Arguments
    /// * `user` - User address to migrate
    /// * `new_pool_address` - Address of the new pool contract
    pub fn migrate_to_new_pool(env: Env, user: Address, new_pool_address: Address) {
        Self::require_admin(&env);

        let mut user_deposits = Self::get_user_deposits(&env);
        let user_deposit = user_deposits
            .get(user.clone())
            .unwrap_or_else(|| panic!("No deposit found for user"));

        let current_time = env.ledger().timestamp();
        let current_reward_index = env.storage().instance().get(&REWARD_INDEX).unwrap();

        // Record migration event
        let migration_id = env.ledger().seq_num();
        let record = MigrationRecord {
            migration_id,
            user: user.clone(),
            amount: user_deposit.amount,
            reward_index: user_deposit.reward_index,
            deposit_timestamp: user_deposit.deposit_timestamp,
            migrated_at: current_time,
        };

        // Store migration event
        let mut events: Vec<MigrationRecord> = env
            .storage()
            .instance()
            .get(&MIGRATION_EVENTS)
            .unwrap_or_else(|| Vec::new(&env));
        events.push_back(record);
        env.storage().instance().set(&MIGRATION_EVENTS, &events);

        // Update migration version
        env.storage().instance().set(&MIGRATION_VERSION, &2u64);

        // In production: transfer user deposit to new pool contract
        env.events().publish(
            (Symbol::short("POOL_MIGRATION"), user),
            (user_deposit.amount, user_deposit.reward_index, user_deposit.deposit_timestamp),
        );
    }

    /// Get migration history
    pub fn get_migration_history(env: Env) -> Vec<MigrationRecord> {
        env.storage()
            .instance()
            .get(&MIGRATION_EVENTS)
            .unwrap_or_else(|| Vec::new(&env))
    }

    // ─── Pool Health Metrics (Issue #208) ──────────────────────────────────────

    /// Calculate pool health metrics
    pub fn get_pool_health(env: Env) -> PoolHealthMetrics {
        let pool_info = Self::get_pool_info(&env);
        let user_deposits = Self::get_user_deposits(&env);
        let params = Self::get_params(&env);

        let pool_size = pool_info.total_deposits;
        let total_debt = Self::get_stablecoin_supply(&env);
        let depositor_count = user_deposits.len() as u32;

        // Coverage ratio: pool_size / total_debt
        let coverage_ratio_bps = if total_debt > 0 {
            (pool_size * 10000) / total_debt
        } else {
            0
        };

        // Calculate concentration (top 10 depositors)
        let mut amounts: Vec<u64> = user_deposits.values().map(|d| d.amount).collect();
        amounts.sort_by(|a, b| b.cmp(a));
        let top_10_sum: u64 = amounts.iter().take(10).sum();
        let concentration_bps = if pool_size > 0 {
            (top_10_sum * 10000) / pool_size
        } else {
            0
        };

        // Withdrawal pressure (simplified - would track pending withdrawals in production)
        let withdrawal_pressure_bps = 0;

        // Reward sustainability (simplified)
        let reward_sustainability_bps = 10000; // Default to 100% if no data

        // Calculate health score (0-10000)
        let mut health_score: u32 = 10000;
        if coverage_ratio_bps < 2000 {
            health_score = health_score.saturating_sub(4000);
        }
        if concentration_bps > 8000 {
            health_score = health_score.saturating_sub(2000);
        }
        if withdrawal_pressure_bps > 3000 {
            health_score = health_score.saturating_sub(2000);
        }
        if reward_sustainability_bps < 5000 {
            health_score = health_score.saturating_sub(2000);
        }

        PoolHealthMetrics {
            coverage_ratio_bps,
            concentration_bps,
            withdrawal_pressure_bps,
            reward_sustainability_bps,
            health_score,
            pool_size,
            total_debt,
            depositor_count,
        }
    }
}
