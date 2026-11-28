//! Staking Contract
//!
//! Implements staking with lock-up periods and different APY tiers.
//! Also distributes synthetic protocol fees to stakers proportionally to stake.
//!
//! ## Access Control
//! One of the few contracts in this codebase with fully correct enforcement — see
//! `docs/ACCESS_CONTROL_MATRIX.md`.
//! - **Admin**: `set_tier`, `deposit_fees` — enforced via `admin.require_auth()`.
//! - **User**: `stake`, `withdraw`, `claim_fees`, `set_auto_compound` — enforced via `user.require_auth()`.

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Map, Symbol};

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct LockUpTier {
    pub duration_secs: u64,
    pub apy_bps: u32,
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct StakePosition {
    pub amount: u64,
    pub start_time: u64,
    pub lock_up_duration: u64,
    pub apy_bps: u32,
}

/// Per-staker fee accounting used for proportional fee distribution.
#[derive(Clone, Debug)]
#[contracttype]
pub struct FeeAccount {
    /// Fee accumulator snapshot at the last settlement for this staker
    pub fee_index_snapshot: u128,
    /// Fees earned but not yet claimed
    pub pending_fees: u64,
    /// Whether accrued fees are automatically re-staked into the position
    pub auto_compound: bool,
    /// Timestamp of the last fee claim
    pub last_claim: u64,
}

const ADMIN: Symbol = Symbol::short("ADMIN");
const STAKING_TOKEN: Symbol = Symbol::short("TOKEN");
const TIERS: Symbol = Symbol::short("TIERS");
const STAKES: Symbol = Symbol::short("STAKES");
const FEE_POOL: Symbol = Symbol::short("FEEPOOL");
const FEE_INDEX: Symbol = Symbol::short("FEEIDX");
const FEE_ACCTS: Symbol = Symbol::short("FEEACCTS");
const TOTAL_STAKED: Symbol = Symbol::short("TOTSTAKE");
const LAST_DIST: Symbol = Symbol::short("LASTDIST");

/// Fee claims are available at most once per week.
pub const FEE_CLAIM_INTERVAL_SECS: u64 = 7 * 24 * 3600;

/// Fixed-point scale for the fee accumulator.
const FEE_INDEX_SCALE: u128 = 1_000_000_000_000;

#[contract]
pub struct StakingContract;

impl StakingContract {
    /// Settle a staker's accrued fees into `pending_fees` using the current
    /// fee index, given their currently staked amount.
    fn settle_fees(
        env: &Env,
        user: &Address,
        staked_amount: u64,
        index: u128,
    ) -> FeeAccount {
        let mut accts: Map<Address, FeeAccount> = env
            .storage()
            .instance()
            .get(&FEE_ACCTS)
            .unwrap_or_else(|| Map::new(env));
        let mut acct = accts.get(user.clone()).unwrap_or(FeeAccount {
            fee_index_snapshot: index,
            pending_fees: 0,
            auto_compound: false,
            last_claim: 0,
        });

        let delta = index.saturating_sub(acct.fee_index_snapshot);
        let earned = (staked_amount as u128 * delta) / FEE_INDEX_SCALE;
        acct.pending_fees = acct.pending_fees.saturating_add(earned as u64);
        acct.fee_index_snapshot = index;

        accts.set(user.clone(), acct.clone());
        env.storage().instance().set(&FEE_ACCTS, &accts);
        acct
    }

    fn total_staked(env: &Env) -> u64 {
        env.storage().instance().get(&TOTAL_STAKED).unwrap_or(0)
    }

    fn fee_index(env: &Env) -> u128 {
        env.storage().instance().get(&FEE_INDEX).unwrap_or(0)
    }
}

#[contractimpl]
impl StakingContract {
    /// Initialize the staking contract
    pub fn initialize(env: Env, admin: Address, token: Address) {
        if env.storage().instance().has(&ADMIN) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&ADMIN, &admin);
        env.storage().instance().set(&STAKING_TOKEN, &token);

        let mut tiers = Map::<u64, LockUpTier>::new(&env);
        // Default tiers:
        // No lock-up: 2% APY
        tiers.set(0, LockUpTier { duration_secs: 0, apy_bps: 200 });
        // 30 days lock-up: 5% APY
        tiers.set(30 * 24 * 3600, LockUpTier { duration_secs: 30 * 24 * 3600, apy_bps: 500 });
        // 90 days lock-up: 10% APY
        tiers.set(90 * 24 * 3600, LockUpTier { duration_secs: 90 * 24 * 3600, apy_bps: 1000 });
        // 365 days lock-up: 25% APY
        tiers.set(365 * 24 * 3600, LockUpTier { duration_secs: 365 * 24 * 3600, apy_bps: 2500 });

        env.storage().instance().set(&TIERS, &tiers);

        let stakes = Map::<Address, StakePosition>::new(&env);
        env.storage().instance().set(&STAKES, &stakes);

        env.storage().instance().set(&FEE_POOL, &0u64);
        env.storage().instance().set(&FEE_INDEX, &0u128);
        env.storage().instance().set(&TOTAL_STAKED, &0u64);
        env.storage().instance().set(&LAST_DIST, &0u64);
        let accts = Map::<Address, FeeAccount>::new(&env);
        env.storage().instance().set(&FEE_ACCTS, &accts);
    }

    /// Add or update a lock-up tier
    pub fn set_tier(env: Env, duration_secs: u64, apy_bps: u32) {
        let admin: Address = env.storage().instance().get(&ADMIN).unwrap();
        admin.require_auth();

        let mut tiers: Map<u64, LockUpTier> = env.storage().instance().get(&TIERS).unwrap();
        tiers.set(duration_secs, LockUpTier { duration_secs, apy_bps });
        env.storage().instance().set(&TIERS, &tiers);
    }

    /// Deposit synthetic protocol fees (mint/burn/exchange fees) into the fee pool
    /// and distribute them proportionally to current stakers.
    pub fn deposit_fees(env: Env, amount: u64) {
        let admin: Address = env.storage().instance().get(&ADMIN).unwrap();
        admin.require_auth();
        if amount == 0 {
            panic!("Amount must be greater than 0");
        }

        let total = Self::total_staked(&env);
        let pool: u64 = env.storage().instance().get(&FEE_POOL).unwrap_or(0);
        env.storage().instance().set(&FEE_POOL, &pool.saturating_add(amount));

        // Without stakers, fees remain in the pool until stake exists.
        if total > 0 {
            let index = Self::fee_index(&env);
            let increment = (amount as u128 * FEE_INDEX_SCALE) / total as u128;
            env.storage()
                .instance()
                .set(&FEE_INDEX, &index.saturating_add(increment));
        }

        env.events()
            .publish((Symbol::short("FEE_DEP"), admin), amount);
    }

    /// Stake tokens for a given lock-up duration
    pub fn stake(env: Env, user: Address, amount: u64, duration_secs: u64) {
        user.require_auth();
        if amount == 0 {
            panic!("Amount must be greater than 0");
        }

        let tiers: Map<u64, LockUpTier> = env.storage().instance().get(&TIERS).unwrap();
        let tier = tiers.get(duration_secs).unwrap_or_else(|| panic!("Invalid lock-up duration"));

        let mut stakes: Map<Address, StakePosition> = env.storage().instance().get(&STAKES).unwrap();

        if stakes.has(user.clone()) {
            panic!("User already has an active stake. Please withdraw first.");
        }

        // Settle fees against the (empty) prior position so the snapshot is current.
        let index = Self::fee_index(&env);
        Self::settle_fees(&env, &user, 0, index);

        let current_time = env.ledger().timestamp();
        let position = StakePosition {
            amount,
            start_time: current_time,
            lock_up_duration: duration_secs,
            apy_bps: tier.apy_bps,
        };

        stakes.set(user.clone(), position);
        env.storage().instance().set(&STAKES, &stakes);
        env.storage()
            .instance()
            .set(&TOTAL_STAKED, &Self::total_staked(&env).saturating_add(amount));

        env.events().publish((Symbol::short("STAKED"), user), (amount, duration_secs));
    }

    /// Enable or disable auto-compounding of fee rewards into the stake position.
    pub fn set_auto_compound(env: Env, user: Address, enabled: bool) {
        user.require_auth();
        let index = Self::fee_index(&env);
        let staked = Self::staked_amount(&env, &user);
        let mut acct = Self::settle_fees(&env, &user, staked, index);
        acct.auto_compound = enabled;

        let mut accts: Map<Address, FeeAccount> = env.storage().instance().get(&FEE_ACCTS).unwrap();
        accts.set(user.clone(), acct);
        env.storage().instance().set(&FEE_ACCTS, &accts);

        env.events()
            .publish((Symbol::short("AUTOCMP"), user), enabled);
    }

    /// Claim accrued synthetic fees. Claims are rate-limited to once per week.
    /// If auto-compound is enabled the fees are added to the staked principal
    /// instead of being paid out. Returns the amount claimed or compounded.
    pub fn claim_fees(env: Env, user: Address) -> u64 {
        user.require_auth();

        let now = env.ledger().timestamp();
        let index = Self::fee_index(&env);
        let staked = Self::staked_amount(&env, &user);
        let mut acct = Self::settle_fees(&env, &user, staked, index);

        if acct.last_claim != 0 && now < acct.last_claim + FEE_CLAIM_INTERVAL_SECS {
            panic!("Fee claim not yet available");
        }

        let claimable = acct.pending_fees;
        if claimable == 0 {
            return 0;
        }

        let pool: u64 = env.storage().instance().get(&FEE_POOL).unwrap_or(0);
        let paid = claimable.min(pool);
        env.storage().instance().set(&FEE_POOL, &(pool - paid));

        acct.pending_fees = claimable - paid;
        acct.last_claim = now;

        if acct.auto_compound {
            // Compound into principal; the compounded amount becomes stakable stake.
            let mut stakes: Map<Address, StakePosition> =
                env.storage().instance().get(&STAKES).unwrap();
            if let Some(mut position) = stakes.get(user.clone()) {
                position.amount = position.amount.saturating_add(paid);
                stakes.set(user.clone(), position);
                env.storage().instance().set(&STAKES, &stakes);
                env.storage()
                    .instance()
                    .set(&TOTAL_STAKED, &Self::total_staked(&env).saturating_add(paid));
            }
        }

        let mut accts: Map<Address, FeeAccount> = env.storage().instance().get(&FEE_ACCTS).unwrap();
        accts.set(user.clone(), acct.clone());
        env.storage().instance().set(&FEE_ACCTS, &accts);

        env.events().publish(
            (Symbol::short("FEE_CLM"), user),
            (paid, acct.auto_compound),
        );

        paid
    }

    /// Fees currently claimable by `user`, without mutating state.
    pub fn pending_fees(env: Env, user: Address) -> u64 {
        let index = Self::fee_index(&env);
        let staked = Self::staked_amount(&env, &user);
        let accts: Map<Address, FeeAccount> = env
            .storage()
            .instance()
            .get(&FEE_ACCTS)
            .unwrap_or_else(|| Map::new(&env));
        let (mut pending, snapshot) = match accts.get(user) {
            Some(a) => (a.pending_fees, a.fee_index_snapshot),
            None => (0, index),
        };
        let delta = index.saturating_sub(snapshot);
        pending = pending.saturating_add(((staked as u128 * delta) / FEE_INDEX_SCALE) as u64);
        pending
    }

    /// Calculate pending rewards
    pub fn pending_rewards(env: Env, user: Address) -> u64 {
        let stakes: Map<Address, StakePosition> = env.storage().instance().get(&STAKES).unwrap();
        if let Some(position) = stakes.get(user) {
            let current_time = env.ledger().timestamp();
            let elapsed = current_time.saturating_sub(position.start_time);

            let annual_rewards = (position.amount as u128 * position.apy_bps as u128) / 10000;
            let rewards = (annual_rewards * elapsed as u128) / (365 * 24 * 3600);

            rewards as u64
        } else {
            0
        }
    }

    /// Withdraw stake and rewards
    pub fn withdraw(env: Env, user: Address) -> u64 {
        user.require_auth();

        // Settle fees against the current stake before removing it.
        let index = Self::fee_index(&env);
        let staked = Self::staked_amount(&env, &user);
        let mut acct = Self::settle_fees(&env, &user, staked, index);

        let mut stakes: Map<Address, StakePosition> = env.storage().instance().get(&STAKES).unwrap();
        let position = stakes.get(user.clone()).unwrap_or_else(|| panic!("No active stake found"));

        let current_time = env.ledger().timestamp();

        if current_time < position.start_time + position.lock_up_duration {
            panic!("Lock-up period has not ended");
        }

        let rewards = Self::pending_rewards(env.clone(), user.clone());
        let total_payout = position.amount + rewards;

        stakes.remove(user.clone());
        env.storage().instance().set(&STAKES, &stakes);
        env.storage()
            .instance()
            .set(&TOTAL_STAKED, &Self::total_staked(&env).saturating_sub(position.amount));

        // Zero the stake for fee purposes; unclaimed fees remain claimable.
        acct.fee_index_snapshot = index;
        let mut accts: Map<Address, FeeAccount> = env.storage().instance().get(&FEE_ACCTS).unwrap();
        accts.set(user.clone(), acct);
        env.storage().instance().set(&FEE_ACCTS, &accts);

        env.events().publish((Symbol::short("WITHDRAWN"), user), total_payout);

        total_payout
    }

    fn staked_amount(env: &Env, user: &Address) -> u64 {
        let stakes: Map<Address, StakePosition> = env
            .storage()
            .instance()
            .get(&STAKES)
            .unwrap_or_else(|| Map::new(env));
        stakes.get(user.clone()).map(|p| p.amount).unwrap_or(0)
    }
}
