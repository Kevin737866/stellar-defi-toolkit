pub enum DataKey {
    Stake(Address),
    CooldownPeriod,
}

pub fn request_unstake(
    env: Env,
    amount: i128,
)