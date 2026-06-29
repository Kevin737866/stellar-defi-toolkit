pub struct StakePosition {
    pub owner: Address,

    pub amount: i128,

    pub reward_debt: i128,

    pub pending_rewards: i128,

    pub last_claim_at: u64,
}

pub struct RewardPool {
    pub total_staked: i128,

    pub reward_rate_per_second: i128,

    pub acc_reward_per_share: i128,

    pub last_reward_timestamp: u64,
}