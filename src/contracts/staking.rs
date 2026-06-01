//! Staking Contract
//!
//! Implements staking with lock-up periods and different APY tiers.

use soroban_sdk::{contract, contractimpl, Address, Env, Map, Symbol};

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

const ADMIN: Symbol = Symbol::short("ADMIN");
const STAKING_TOKEN: Symbol = Symbol::short("TOKEN");
const TIERS: Symbol = Symbol::short("TIERS");
const STAKES: Symbol = Symbol::short("STAKES");

#[contract]
pub struct StakingContract;

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
    }

    /// Add or update a lock-up tier
    pub fn set_tier(env: Env, duration_secs: u64, apy_bps: u32) {
        let admin: Address = env.storage().instance().get(&ADMIN).unwrap();
        admin.require_auth();

        let mut tiers: Map<u64, LockUpTier> = env.storage().instance().get(&TIERS).unwrap();
        tiers.set(duration_secs, LockUpTier { duration_secs, apy_bps });
        env.storage().instance().set(&TIERS, &tiers);
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

        let current_time = env.ledger().timestamp();
        let position = StakePosition {
            amount,
            start_time: current_time,
            lock_up_duration: duration_secs,
            apy_bps: tier.apy_bps,
        };
        
        stakes.set(user.clone(), position);
        env.storage().instance().set(&STAKES, &stakes);
        
        env.events().publish((Symbol::short("STAKED"), user), (amount, duration_secs));
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
        
        env.events().publish((Symbol::short("WITHDRAWN"), user), total_payout);
        
        total_payout
    }
}
