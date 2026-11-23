use soroban_sdk::{contracttype, Address, Env};

#[contracttype]
pub enum DataKey {
    Stake(Address),
    CooldownPeriod,
    RewardPool,
    RewardRate,
    LastRewardTimestamp,
    AccRewardPerShare,
    UserRewardDebt(Address),
    PendingRewards(Address),
    AutoCompound(Address),
    LastClaimTimestamp(Address),
}

pub const PRECISION: i128 = 1_000_000_000_000;
pub const WEEK_SECONDS: u64 = 7 * 24 * 60 * 60;

pub fn get_stake(env: &Env, user: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::Stake(user.clone()))
        .unwrap_or(0)
}

pub fn set_stake(env: &Env, user: &Address, amount: i128) {
    env.storage()
        .persistent()
        .set(&DataKey::Stake(user.clone()), &amount);
}

pub fn get_reward_pool(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::RewardPool)
        .unwrap_or(0)
}

pub fn set_reward_pool(env: &Env, amount: i128) {
    env.storage().instance().set(&DataKey::RewardPool, &amount);
}

pub fn get_acc_reward_per_share(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::AccRewardPerShare)
        .unwrap_or(0)
}

pub fn set_acc_reward_per_share(env: &Env, value: i128) {
    env.storage()
        .instance()
        .set(&DataKey::AccRewardPerShare, &value);
}

pub fn get_last_reward_timestamp(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::LastRewardTimestamp)
        .unwrap_or(0)
}

pub fn set_last_reward_timestamp(env: &Env, ts: u64) {
    env.storage()
        .instance()
        .set(&DataKey::LastRewardTimestamp, &ts);
}

pub fn get_user_reward_debt(env: &Env, user: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::UserRewardDebt(user.clone()))
        .unwrap_or(0)
}

pub fn set_user_reward_debt(env: &Env, user: &Address, debt: i128) {
    env.storage()
        .persistent()
        .set(&DataKey::UserRewardDebt(user.clone()), &debt);
}

pub fn get_pending_rewards(env: &Env, user: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::PendingRewards(user.clone()))
        .unwrap_or(0)
}

pub fn set_pending_rewards(env: &Env, user: &Address, amount: i128) {
    env.storage()
        .persistent()
        .set(&DataKey::PendingRewards(user.clone()), &amount);
}

pub fn is_auto_compound(env: &Env, user: &Address) -> bool {
    env.storage()
        .persistent()
        .get(&DataKey::AutoCompound(user.clone()))
        .unwrap_or(false)
}

pub fn set_auto_compound(env: &Env, user: &Address, enabled: bool) {
    env.storage()
        .persistent()
        .set(&DataKey::AutoCompound(user.clone()), &enabled);
}

pub fn get_last_claim_timestamp(env: &Env, user: &Address) -> u64 {
    env.storage()
        .persistent()
        .get(&DataKey::LastClaimTimestamp(user.clone()))
        .unwrap_or(0)
}

pub fn set_last_claim_timestamp(env: &Env, user: &Address, ts: u64) {
    env.storage()
        .persistent()
        .set(&DataKey::LastClaimTimestamp(user.clone()), &ts);
}

pub fn request_unstake(env: Env, amount: i128) {
    let _ = (env, amount);
}
