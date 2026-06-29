pub const DEFAULT_UNSTAKE_COOLDOWN: u64 =
    7 * 24 * 60 * 60; // 7 days

    pub struct StakePosition {
    pub owner: Address,

    pub amount: i128,

    pub unstake_requested_at: Option<u64>,

    pub pending_unstake_amount: i128,
}