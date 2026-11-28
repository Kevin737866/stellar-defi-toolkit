use soroban_sdk::{contracttype, Address};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StakePosition {
    pub owner: Address,

    pub amount: i128,

    pub reward_debt: i128,

    pub pending_rewards: i128,

    pub last_claim_at: u64,

    pub auto_compound: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RewardPool {
    pub total_staked: i128,

    pub reward_rate_per_second: i128,

    pub acc_reward_per_share: i128,

    pub last_reward_timestamp: u64,

    pub fee_pool_balance: i128,

    pub last_distribution_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeDistributedEvent {
    pub amount: i128,

    pub acc_reward_per_share: i128,

    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeesClaimedEvent {
    pub owner: Address,

    pub amount: i128,

    pub compounded: bool,

    pub timestamp: u64,
}
