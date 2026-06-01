//! Staking contract implementation for Stellar DeFi Toolkit
//! 
//! Provides comprehensive staking functionality for token holders to earn rewards
//! on the Stellar blockchain with time-based reward distribution.

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol, token};

const PRECISION: i128 = 1_000_000_000; // 1e9 for precision in calculations

/// Storage keys for the staking contract
#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    StakingToken,
    RewardToken,
    TotalStaked,
    RewardRate,
    LastUpdateTime,
    RewardPerTokenStored,
    UserStake(Address),
    UserRewardPerTokenPaid(Address),
    UserRewards(Address),
    RewardsDuration,
    PeriodFinish,
    Initialized,
}

/// Staking contract for token staking and reward distribution
#[contract]
pub struct StakingContract;

#[contractimpl]
impl StakingContract {
    /// Initialize the staking contract
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// * `admin` - The admin address who can manage the contract
    /// * `staking_token` - The token that users will stake
    /// * `reward_token` - The token distributed as rewards
    /// * `reward_duration` - Duration of reward period in ledgers
    pub fn initialize(
        env: Env,
        admin: Address,
        staking_token: Address,
        reward_token: Address,
        reward_duration: u32,
    ) {
        if env.storage().instance().has(&DataKey::Initialized) {
            panic!("Contract already initialized");
        }

        admin.require_auth();

            env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::StakingToken, &staking_token);
        env.storage().instance().set(&DataKey::RewardToken, &reward_token);
        env.storage().instance().set(&DataKey::TotalStaked, &0i128);
        env.storage().instance().set(&DataKey::RewardRate, &0i128);
        env.storage().instance().set(&DataKey::LastUpdateTime, &0u32);
        env.storage().instance().set(&DataKey::RewardPerTokenStored, &0i128);
        env.storage().instance().set(&DataKey::RewardsDuration, &reward_duration);
        env.storage().instance().set(&DataKey::PeriodFinish, &0u64);
        env.storage().instance().set(&DataKey::Initialized, &true);

        env.events().publish(
            (Symbol::new(&env, "initialized"),),
            (admin, staking_token, reward_token, reward_duration),
        );
    }

    /// Stake tokens into the contract
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// * `user` - The user staking tokens
    /// * `amount` - The amount of tokens to stake
    pub fn stake(env: Env, user: Address, amount: i128) {
        user.require_auth();

        if amount <= 0 {
            panic!("Amount must be greater than 0");
        }

        Self::update_reward(&env, &user);

        let staking_token: Address = env.storage().instance().get(&DataKey::StakingToken).unwrap();
        let contract_address = env.current_contract_address();

        // Transfer tokens from user to contract
        let token_client = token::Client::new(&env, &staking_token);
        token_client.transfer(&user, &contract_address, &amount);

        // Update user's staked balance
        let user_stake_key = DataKey::UserStake(user.clone());
        let current_stake: i128 = env.storage().instance().get(&user_stake_key).unwrap_or(0);
        let new_stake = current_stake + amount;
        env.storage().instance().set(&user_stake_key, &new_stake);

        // Update total staked
        let total_staked: i128 = env.storage().instance().get(&DataKey::TotalStaked).unwrap();
        env.storage().instance().set(&DataKey::TotalStaked, &(total_staked + amount));

        env.events().publish(
            (Symbol::new(&env, "staked"),),
            (user, amount),
        );
    }

    /// Unstake tokens from the contract
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// * `user` - The user unstaking tokens
    /// * `amount` - The amount of tokens to unstake
    pub fn unstake(env: Env, user: Address, amount: i128) {
        user.require_auth();

        if amount <= 0 {
            panic!("Amount must be greater than 0");
        }

        Self::update_reward(&env, &user);

        let user_stake_key = DataKey::UserStake(user.clone());
        let current_stake: i128 = env.storage().instance().get(&user_stake_key).unwrap_or(0);

        if current_stake < amount {
            panic!("Insufficient staked balance");
        }

        // Update user's staked balance
        let new_stake = current_stake - amount;
        env.storage().instance().set(&user_stake_key, &new_stake);

        // Update total staked
        let total_staked: i128 = env.storage().instance().get(&DataKey::TotalStaked).unwrap();
        env.storage().instance().set(&DataKey::TotalStaked, &(total_staked - amount));

        // Transfer tokens back to user
        let staking_token: Address = env.storage().instance().get(&DataKey::StakingToken).unwrap();
        let token_client = token::Client::new(&env, &staking_token);
        token_client.transfer(&env.current_contract_address(), &user, &amount);

        env.events().publish(
            (Symbol::new(&env, "unstaked"),),
            (user, amount),
        );
    }

    /// Claim accumulated rewards
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// * `user` - The user claiming rewards
    pub fn claim_rewards(env: Env, user: Address) -> i128 {
        user.require_auth();

        Self::update_reward(&env, &user);

        let user_rewards_key = DataKey::UserRewards(user.clone());
        let rewards: i128 = env.storage().instance().get(&user_rewards_key).unwrap_or(0);

        if rewards > 0 {
            env.storage().instance().set(&user_rewards_key, &0i128);

            let reward_token: Address = env.storage().instance().get(&DataKey::RewardToken).unwrap();
            let token_client = token::Client::new(&env, &reward_token);
            token_client.transfer(&env.current_contract_address(), &user, &rewards);

            env.events().publish(
                (Symbol::new(&env, "rewards_claimed"),),
                (user, rewards),
            );
        }

        rewards
    }

    /// Get the staked balance for a user
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// * `user` - The user to query
    pub fn get_staked_balance(env: Env, user: Address) -> i128 {
        let user_stake_key = DataKey::UserStake(user);
        env.storage().instance().get(&user_stake_key).unwrap_or(0)
    }

    /// Get the earned rewards for a user
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// * `user` - The user to query
    pub fn get_earned(env: Env, user: Address) -> i128 {
        let user_stake = Self::get_staked_balance(env.clone(), user.clone());
        let reward_per_token = Self::reward_per_token(env.clone());
        
        let user_reward_per_token_paid_key = DataKey::UserRewardPerTokenPaid(user.clone());
        let user_reward_per_token_paid: i128 = env.storage().instance()
            .get(&user_reward_per_token_paid_key).unwrap_or(0);
        
        let user_rewards_key = DataKey::UserRewards(user);
        let user_rewards: i128 = env.storage().instance().get(&user_rewards_key).unwrap_or(0);

        let earned = (user_stake * (reward_per_token - user_reward_per_token_paid)) / PRECISION;
        user_rewards + earned
    }

    /// Get total staked amount
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    pub fn get_total_staked(env: Env) -> i128 {
        env.storage().instance().get(&DataKey::TotalStaked).unwrap_or(0)
    }

    /// Get reward rate per ledger
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    pub fn get_reward_rate(env: Env) -> i128 {
        env.storage().instance().get(&DataKey::RewardRate).unwrap_or(0)
    }

    /// Set reward amount and duration (admin only)
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// * `admin` - The admin address
    /// * `reward_amount` - Total rewards to distribute
    pub fn notify_reward_amount(env: Env, admin: Address, reward_amount: i128) {
        admin.require_auth();

        let stored_admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        if admin != stored_admin {
            panic!("Only admin can notify reward amount");
        }

        Self::update_reward(&env, &env.current_contract_address());

        let current_ledger = env.ledger().sequence();
        let period_finish: u64 = env.storage().instance().get(&DataKey::PeriodFinish).unwrap_or(0);
        let rewards_duration: u32 = env.storage().instance().get(&DataKey::RewardsDuration).unwrap();

        let reward_rate = if (current_ledger as u64) >= period_finish {
            reward_amount / (rewards_duration as i128)
        } else {
            let remaining = (period_finish - current_ledger as u64) as i128;
            let leftover = remaining * env.storage().instance().get(&DataKey::RewardRate).unwrap_or(0i128);
            (reward_amount + leftover) / (rewards_duration as i128)
        };

        env.storage().instance().set(&DataKey::RewardRate, &reward_rate);
        env.storage().instance().set(&DataKey::LastUpdateTime, &current_ledger);
        env.storage().instance().set(&DataKey::PeriodFinish, &((current_ledger as u64) + (rewards_duration as u64)));

        env.events().publish(
            (Symbol::new(&env, "reward_added"),),
            (reward_amount, reward_rate),
        );
    }

    /// Get the last time rewards were applicable
    fn last_time_reward_applicable(env: &Env) -> u32 {
        let current_ledger = env.ledger().sequence();
        let period_finish: u64 = env.storage().instance().get(&DataKey::PeriodFinish).unwrap_or(0);
        
        if (current_ledger as u64) < period_finish {
            current_ledger
        } else {
            period_finish as u32
        }
    }

    /// Calculate reward per token
    fn reward_per_token(env: Env) -> i128 {
        let total_staked: i128 = env.storage().instance().get(&DataKey::TotalStaked).unwrap_or(0);
        
        if total_staked == 0 {
            return env.storage().instance().get(&DataKey::RewardPerTokenStored).unwrap_or(0);
        }

        let last_update_time: u32 = env.storage().instance().get(&DataKey::LastUpdateTime).unwrap_or(0);
        let reward_per_token_stored: i128 = env.storage().instance()
            .get(&DataKey::RewardPerTokenStored).unwrap_or(0);
        let reward_rate: i128 = env.storage().instance().get(&DataKey::RewardRate).unwrap_or(0);

        let last_time = Self::last_time_reward_applicable(&env);
        let time_delta = (last_time - last_update_time) as i128;

        reward_per_token_stored + ((time_delta * reward_rate * PRECISION) / total_staked)
    }

    /// Update reward calculations for a user
    fn update_reward(env: &Env, user: &Address) {
        let reward_per_token = Self::reward_per_token(env.clone());
        env.storage().instance().set(&DataKey::RewardPerTokenStored, &reward_per_token);
        env.storage().instance().set(&DataKey::LastUpdateTime, &Self::last_time_reward_applicable(env));

        if user != &env.current_contract_address() {
            let earned = Self::get_earned(env.clone(), user.clone());
            let user_rewards_key = DataKey::UserRewards(user.clone());
            env.storage().instance().set(&user_rewards_key, &earned);

            let user_reward_per_token_paid_key = DataKey::UserRewardPerTokenPaid(user.clone());
            env.storage().instance().set(&user_reward_per_token_paid_key, &reward_per_token);
        }
    }

    /// Emergency withdraw all staked tokens (forfeits rewards)
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// * `user` - The user performing emergency withdrawal
    pub fn emergency_withdraw(env: Env, user: Address) -> i128 {
        user.require_auth();

        let user_stake_key = DataKey::UserStake(user.clone());
        let staked_amount: i128 = env.storage().instance().get(&user_stake_key).unwrap_or(0);

        if staked_amount == 0 {
            panic!("No tokens staked");
        }

        // Reset user's stake
        env.storage().instance().set(&user_stake_key, &0i128);

        // Update total staked
        let total_staked: i128 = env.storage().instance().get(&DataKey::TotalStaked).unwrap();
        env.storage().instance().set(&DataKey::TotalStaked, &(total_staked - staked_amount));

        // Transfer tokens back to user
        let staking_token: Address = env.storage().instance().get(&DataKey::StakingToken).unwrap();
        let token_client = token::Client::new(&env, &staking_token);
        token_client.transfer(&env.current_contract_address(), &user, &staked_amount);

        env.events().publish(
            (Symbol::new(&env, "emergency_withdraw"),),
            (user, staked_amount),
        );

        staked_amount
    }

    /// Get contract information
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    pub fn get_info(env: Env) -> StakingInfo {
        StakingInfo {
            staking_token: env.storage().instance().get(&DataKey::StakingToken).unwrap(),
            reward_token: env.storage().instance().get(&DataKey::RewardToken).unwrap(),
            total_staked: env.storage().instance().get(&DataKey::TotalStaked).unwrap_or(0),
            reward_rate: env.storage().instance().get(&DataKey::RewardRate).unwrap_or(0),
            period_finish: env.storage().instance().get(&DataKey::PeriodFinish).unwrap_or(0),
            rewards_duration: env.storage().instance().get(&DataKey::RewardsDuration).unwrap(),
        }
    }
}

/// Staking contract information
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct StakingInfo {
    pub staking_token: Address,
    pub reward_token: Address,
    pub total_staked: i128,
    pub reward_rate: i128,
    pub period_finish: u64,
    pub rewards_duration: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::{Address as _, Ledger}, token, Env};

    const DAY_IN_LEDGERS: u32 = 17280; // Approximately 24 hours worth of ledgers (5 second ledgers)

    fn create_token_contract<'a>(env: &Env, admin: &Address) -> (token::Client<'a>, token::StellarAssetClient<'a>) {
        let contract_address = env.register_stellar_asset_contract_v2(admin.clone());
        (
            token::Client::new(env, &contract_address.address()),
            token::StellarAssetClient::new(env, &contract_address.address()),
        )
    }

    fn create_staking_contract<'a>(env: &Env) -> StakingContractClient<'a> {
        StakingContractClient::new(env, &env.register_contract(None, StakingContract {}))
    }

    #[test]
    fn test_initialize() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let staking_contract = create_staking_contract(&env);
        
        let (staking_token, _) = create_token_contract(&env, &admin);
        let (reward_token, _) = create_token_contract(&env, &admin);

        staking_contract.initialize(
            &admin,
            &staking_token.address,
            &reward_token.address,
            &DAY_IN_LEDGERS,
        );

        let info = staking_contract.get_info();
        assert_eq!(info.staking_token, staking_token.address);
        assert_eq!(info.reward_token, reward_token.address);
        assert_eq!(info.total_staked, 0);
    }

    #[test]
    #[should_panic(expected = "Contract already initialized")]
    fn test_double_initialize() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let staking_contract = create_staking_contract(&env);
        
        let (staking_token, _) = create_token_contract(&env, &admin);
        let (reward_token, _) = create_token_contract(&env, &admin);

        staking_contract.initialize(
            &admin,
            &staking_token.address,
            &reward_token.address,
            &DAY_IN_LEDGERS,
        );

        // Try to initialize again - should panic
        staking_contract.initialize(
            &admin,
            &staking_token.address,
            &reward_token.address,
            &DAY_IN_LEDGERS,
        );
    }

    #[test]
    fn test_stake() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let user = Address::generate(&env);
        let staking_contract = create_staking_contract(&env);
        
        let (staking_token, staking_token_admin) = create_token_contract(&env, &admin);
        let (reward_token, _) = create_token_contract(&env, &admin);

        staking_contract.initialize(
            &admin,
            &staking_token.address,
            &reward_token.address,
            &DAY_IN_LEDGERS,
        );

        // Mint tokens to user
        staking_token_admin.mint(&user, &1000);

        // Stake tokens
        staking_contract.stake(&user, &500);

        assert_eq!(staking_contract.get_staked_balance(&user), 500);
        assert_eq!(staking_contract.get_total_staked(), 500);
    }

    #[test]
    #[should_panic(expected = "Amount must be greater than 0")]
    fn test_stake_zero_amount() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let user = Address::generate(&env);
        let staking_contract = create_staking_contract(&env);
        
        let (staking_token, _) = create_token_contract(&env, &admin);
        let (reward_token, _) = create_token_contract(&env, &admin);

        staking_contract.initialize(
            &admin,
            &staking_token.address,
            &reward_token.address,
            &DAY_IN_LEDGERS,
        );

        staking_contract.stake(&user, &0);
    }

    #[test]
    fn test_unstake() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let user = Address::generate(&env);
        let staking_contract = create_staking_contract(&env);
        
        let (staking_token, staking_token_admin) = create_token_contract(&env, &admin);
        let (reward_token, _) = create_token_contract(&env, &admin);

        staking_contract.initialize(
            &admin,
            &staking_token.address,
            &reward_token.address,
            &DAY_IN_LEDGERS,
        );

        // Mint and stake tokens
        staking_token_admin.mint(&user, &1000);
        staking_contract.stake(&user, &500);

        // Unstake tokens
        staking_contract.unstake(&user, &200);

        assert_eq!(staking_contract.get_staked_balance(&user), 300);
        assert_eq!(staking_contract.get_total_staked(), 300);
    }

    #[test]
    #[should_panic(expected = "Insufficient staked balance")]
    fn test_unstake_more_than_staked() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let user = Address::generate(&env);
        let staking_contract = create_staking_contract(&env);
        
        let (staking_token, staking_token_admin) = create_token_contract(&env, &admin);
        let (reward_token, _) = create_token_contract(&env, &admin);

        staking_contract.initialize(
            &admin,
            &staking_token.address,
            &reward_token.address,
            &DAY_IN_LEDGERS,
        );

        staking_token_admin.mint(&user, &1000);
        staking_contract.stake(&user, &500);

        // Try to unstake more than staked
        staking_contract.unstake(&user, &600);
    }

    #[test]
    fn test_rewards() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let user = Address::generate(&env);
        let staking_contract = create_staking_contract(&env);
        
        let (staking_token, staking_token_admin) = create_token_contract(&env, &admin);
        let (reward_token, reward_token_admin) = create_token_contract(&env, &admin);

        staking_contract.initialize(
            &admin,
            &staking_token.address,
            &reward_token.address,
            &DAY_IN_LEDGERS,
        );

        // Mint tokens
        staking_token_admin.mint(&user, &1000);
        reward_token_admin.mint(&staking_contract.address, &10000);

        // Stake tokens
        staking_contract.stake(&user, &500);

        // Set reward amount
        staking_contract.notify_reward_amount(&admin, &1000);

        // Advance ledger to accumulate rewards
        env.ledger().with_mut(|li| {
            li.sequence_number += 100;
        });

        // Check earned rewards
        let earned = staking_contract.get_earned(&user);
        assert!(earned > 0);

        // Claim rewards
        let claimed = staking_contract.claim_rewards(&user);
        assert_eq!(claimed, earned);
        assert_eq!(staking_contract.get_earned(&user), 0);
    }

    #[test]
    fn test_emergency_withdraw() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let user = Address::generate(&env);
        let staking_contract = create_staking_contract(&env);
        
        let (staking_token, staking_token_admin) = create_token_contract(&env, &admin);
        let (reward_token, _) = create_token_contract(&env, &admin);

        staking_contract.initialize(
            &admin,
            &staking_token.address,
            &reward_token.address,
            &DAY_IN_LEDGERS,
        );

        // Mint and stake tokens
        staking_token_admin.mint(&user, &1000);
        staking_contract.stake(&user, &500);

        // Emergency withdraw
        let withdrawn = staking_contract.emergency_withdraw(&user);
        assert_eq!(withdrawn, 500);
        assert_eq!(staking_contract.get_staked_balance(&user), 0);
        assert_eq!(staking_contract.get_total_staked(), 0);
    }

    #[test]
    #[should_panic(expected = "No tokens staked")]
    fn test_emergency_withdraw_no_stake() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let user = Address::generate(&env);
        let staking_contract = create_staking_contract(&env);
        
        let (staking_token, _) = create_token_contract(&env, &admin);
        let (reward_token, _) = create_token_contract(&env, &admin);

        staking_contract.initialize(
            &admin,
            &staking_token.address,
            &reward_token.address,
            &DAY_IN_LEDGERS,
        );

        staking_contract.emergency_withdraw(&user);
    }

    #[test]
    fn test_multiple_users_staking() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let user1 = Address::generate(&env);
        let user2 = Address::generate(&env);
        let staking_contract = create_staking_contract(&env);
        
        let (staking_token, staking_token_admin) = create_token_contract(&env, &admin);
        let (reward_token, reward_token_admin) = create_token_contract(&env, &admin);

        staking_contract.initialize(
            &admin,
            &staking_token.address,
            &reward_token.address,
            &DAY_IN_LEDGERS,
        );

        // Mint tokens to users
        staking_token_admin.mint(&user1, &1000);
        staking_token_admin.mint(&user2, &1000);
        reward_token_admin.mint(&staking_contract.address, &10000);

        // Both users stake
        staking_contract.stake(&user1, &500);
        staking_contract.stake(&user2, &300);

        assert_eq!(staking_contract.get_total_staked(), 800);
        assert_eq!(staking_contract.get_staked_balance(&user1), 500);
        assert_eq!(staking_contract.get_staked_balance(&user2), 300);

        // Set rewards
        staking_contract.notify_reward_amount(&admin, &1000);

        // Advance ledger
        env.ledger().with_mut(|li| {
            li.sequence_number += 100;
        });

        // Both users should have earned rewards proportional to their stake
        let earned1 = staking_contract.get_earned(&user1);
        let earned2 = staking_contract.get_earned(&user2);
        
        assert!(earned1 > 0);
        assert!(earned2 > 0);
        // User1 staked more, so should earn more
        assert!(earned1 > earned2);
    }
}
