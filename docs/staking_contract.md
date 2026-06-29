# Staking Contract Documentation

## Overview

The Staking Contract is a production-ready smart contract for the Stellar blockchain that enables users to stake tokens and earn rewards over time. Built using Soroban SDK, it implements a time-based reward distribution mechanism with precise calculations and comprehensive security features.

## Features

### Core Functionality

- **Token Staking**: Users can stake tokens to participate in reward distribution
- **Time-Based Rewards**: Rewards are distributed proportionally based on stake amount and duration
- **Flexible Unstaking**: Users can unstake tokens at any time while preserving earned rewards
- **Reward Claiming**: Separate reward claiming mechanism for better control
- **Emergency Withdrawal**: Users can withdraw all staked tokens instantly (forfeits unclaimed rewards)

### Admin Features

- **Reward Management**: Admin can set reward amounts and durations
- **Multiple Reward Periods**: Support for consecutive reward periods
- **Contract Initialization**: One-time setup with configurable parameters

### Security Features

- **Authorization Checks**: All user actions require proper authentication
- **Overflow Protection**: Safe arithmetic operations throughout
- **Initialization Guard**: Prevents double initialization
- **Balance Validation**: Comprehensive checks for all operations

## Architecture

### Storage Structure

The contract uses Soroban's instance storage with the following keys:

```rust
pub enum DataKey {
    Admin,                          // Contract administrator
    StakingToken,                   // Token users stake
    RewardToken,                    // Token distributed as rewards
    TotalStaked,                    // Total amount staked across all users
    RewardRate,                     // Rewards distributed per ledger
    LastUpdateTime,                 // Last reward calculation timestamp
    RewardPerTokenStored,           // Accumulated reward per token
    UserStake(Address),             // Individual user's staked amount
    UserRewardPerTokenPaid(Address),// User's reward checkpoint
    UserRewards(Address),           // User's pending rewards
    RewardsDuration,                // Length of reward period
    PeriodFinish,                   // End of current reward period
    Initialized,                    // Initialization flag
}
```

### Reward Calculation

The contract implements a sophisticated reward distribution algorithm:

1. **Reward Per Token**: Calculates accumulated rewards per staked token
   ```
   reward_per_token = reward_per_token_stored + 
                      (time_delta * reward_rate * PRECISION) / total_staked
   ```

2. **User Earned Rewards**: Calculates individual user's earned rewards
   ```
   earned = (user_stake * (reward_per_token - user_reward_per_token_paid)) / PRECISION
            + user_rewards
   ```

3. **Precision**: Uses 1e9 precision factor to handle fractional rewards accurately

## API Reference

### Initialization

#### `initialize`
```rust
pub fn initialize(
    env: Env,
    admin: Address,
    staking_token: Address,
    reward_token: Address,
    reward_duration: u32,
)
```

Initializes the staking contract with configuration parameters.

**Parameters:**
- `admin`: Address with administrative privileges
- `staking_token`: Token contract address for staking
- `reward_token`: Token contract address for rewards
- `reward_duration`: Duration of reward period in ledgers

**Requirements:**
- Can only be called once
- Requires admin authorization

**Events:**
- Emits `initialized` event

### User Functions

#### `stake`
```rust
pub fn stake(env: Env, user: Address, amount: i128)
```

Stakes tokens into the contract to earn rewards.

**Parameters:**
- `user`: Address of the staker
- `amount`: Amount of tokens to stake (must be > 0)

**Requirements:**
- User must authorize the transaction
- User must have sufficient token balance
- User must have approved the contract to transfer tokens

**Effects:**
- Transfers tokens from user to contract
- Updates user's staked balance
- Updates total staked amount
- Updates reward calculations

**Events:**
- Emits `staked` event

#### `unstake`
```rust
pub fn unstake(env: Env, user: Address, amount: i128)
```

Unstakes tokens from the contract.

**Parameters:**
- `user`: Address of the unstaker
- `amount`: Amount of tokens to unstake (must be > 0)

**Requirements:**
- User must authorize the transaction
- User must have sufficient staked balance

**Effects:**
- Transfers tokens from contract to user
- Updates user's staked balance
- Updates total staked amount
- Updates reward calculations
- Preserves earned rewards

**Events:**
- Emits `unstaked` event

#### `claim_rewards`
```rust
pub fn claim_rewards(env: Env, user: Address) -> i128
```

Claims all accumulated rewards for the user.

**Parameters:**
- `user`: Address claiming rewards

**Returns:**
- Amount of rewards claimed

**Requirements:**
- User must authorize the transaction

**Effects:**
- Transfers reward tokens to user
- Resets user's pending rewards
- Updates reward calculations

**Events:**
- Emits `rewards_claimed` event

#### `emergency_withdraw`
```rust
pub fn emergency_withdraw(env: Env, user: Address) -> i128
```

Withdraws all staked tokens immediately, forfeiting unclaimed rewards.

**Parameters:**
- `user`: Address performing emergency withdrawal

**Returns:**
- Amount of tokens withdrawn

**Requirements:**
- User must authorize the transaction
- User must have staked tokens

**Effects:**
- Transfers all staked tokens to user
- Resets user's staked balance
- Forfeits all unclaimed rewards
- Updates total staked amount

**Events:**
- Emits `emergency_withdraw` event

### View Functions

#### `get_staked_balance`
```rust
pub fn get_staked_balance(env: Env, user: Address) -> i128
```

Returns the amount of tokens staked by a user.

#### `get_earned`
```rust
pub fn get_earned(env: Env, user: Address) -> i128
```

Returns the amount of rewards earned by a user (including unclaimed).

#### `get_total_staked`
```rust
pub fn get_total_staked(env: Env) -> i128
```

Returns the total amount of tokens staked in the contract.

#### `get_reward_rate`
```rust
pub fn get_reward_rate(env: Env) -> i128
```

Returns the current reward rate per ledger.

#### `get_info`
```rust
pub fn get_info(env: Env) -> StakingInfo
```

Returns comprehensive contract information.

**Returns:**
```rust
pub struct StakingInfo {
    pub staking_token: Address,
    pub reward_token: Address,
    pub total_staked: i128,
    pub reward_rate: i128,
    pub period_finish: u64,
    pub rewards_duration: u32,
}
```

### Admin Functions

#### `notify_reward_amount`
```rust
pub fn notify_reward_amount(env: Env, admin: Address, reward_amount: i128)
```

Sets the reward amount for the current or next reward period.

**Parameters:**
- `admin`: Admin address (must match stored admin)
- `reward_amount`: Total rewards to distribute over the period

**Requirements:**
- Admin must authorize the transaction
- Caller must be the contract admin
- Contract must have sufficient reward tokens

**Effects:**
- Calculates new reward rate
- Updates reward period timing
- Updates reward calculations

**Events:**
- Emits `reward_added` event

## Usage Examples

### Basic Staking Flow

```rust
use soroban_sdk::{Env, Address};

// 1. Initialize the contract
staking_contract.initialize(
    &admin,
    &staking_token_address,
    &reward_token_address,
    &17280, // 1 day in ledgers
);

// 2. Admin sets rewards
staking_contract.notify_reward_amount(&admin, &10_000_000_000);

// 3. User stakes tokens
staking_contract.stake(&user, &1_000_000_000);

// 4. Time passes, rewards accumulate...

// 5. Check earned rewards
let earned = staking_contract.get_earned(&user);

// 6. Claim rewards
let claimed = staking_contract.claim_rewards(&user);

// 7. Unstake tokens
staking_contract.unstake(&user, &500_000_000);
```

### Multiple Users

```rust
// User 1 stakes 1000 tokens
staking_contract.stake(&user1, &1_000_000_000);

// User 2 stakes 500 tokens
staking_contract.stake(&user2, &500_000_000);

// After some time, rewards are distributed proportionally:
// User 1 gets 66.7% of rewards (1000/1500)
// User 2 gets 33.3% of rewards (500/1500)

let earned1 = staking_contract.get_earned(&user1);
let earned2 = staking_contract.get_earned(&user2);
```

### Consecutive Reward Periods

```rust
// First reward period
staking_contract.notify_reward_amount(&admin, &10_000_000_000);

// ... period runs ...

// Start new reward period (can be called before previous ends)
staking_contract.notify_reward_amount(&admin, &15_000_000_000);
```

## Time Calculations

### Ledger-Based Timing

Stellar uses ledgers instead of block timestamps. Key conversions:

- **1 ledger** ≈ 5 seconds
- **1 minute** ≈ 12 ledgers
- **1 hour** ≈ 720 ledgers
- **1 day** ≈ 17,280 ledgers
- **1 week** ≈ 120,960 ledgers
- **1 month** ≈ 518,400 ledgers

### APY Calculation

To calculate Annual Percentage Yield (APY):

```
APY = (reward_rate * ledgers_per_year / total_staked) * 100

where ledgers_per_year ≈ 6,307,200 (365.25 days)
```

## Security Considerations

### Best Practices

1. **Token Approvals**: Users must approve the staking contract before staking
2. **Reward Funding**: Admin must ensure contract has sufficient reward tokens
3. **Reward Duration**: Choose appropriate durations to balance gas costs and flexibility
4. **Emergency Withdrawals**: Only use in emergencies as rewards are forfeited

### Known Limitations

1. **Precision Loss**: Very small stakes or rewards may experience rounding
2. **Gas Costs**: Frequent reward updates increase transaction costs
3. **Reward Token Supply**: Contract must have rewards before distribution starts

### Audit Recommendations

Before production deployment:

1. Conduct comprehensive security audit
2. Test with various token decimals
3. Verify reward calculations with edge cases
4. Test emergency scenarios
5. Review admin key management

## Testing

The contract includes comprehensive tests covering:

- ✅ Initialization and double-initialization prevention
- ✅ Staking with valid and invalid amounts
- ✅ Unstaking with sufficient and insufficient balances
- ✅ Reward accumulation and claiming
- ✅ Emergency withdrawals
- ✅ Multiple users with proportional rewards
- ✅ Admin functions and authorization

Run tests with:
```bash
cargo test --package stellar-defi-toolkit --lib contracts::staking::tests
```

## Events

The contract emits the following events:

| Event | Parameters | Description |
|-------|-----------|-------------|
| `initialized` | admin, staking_token, reward_token, duration | Contract initialized |
| `staked` | user, amount | User staked tokens |
| `unstaked` | user, amount | User unstaked tokens |
| `rewards_claimed` | user, amount | User claimed rewards |
| `reward_added` | amount, rate | Admin added rewards |
| `emergency_withdraw` | user, amount | User emergency withdrew |

## Gas Optimization

The contract is optimized for gas efficiency:

- Uses instance storage for frequently accessed data
- Minimizes storage operations
- Batches reward calculations
- Uses efficient arithmetic operations

## Future Enhancements

Potential improvements for future versions:

1. **Multiple Reward Tokens**: Support for distributing multiple reward types
2. **Lock Periods**: Optional lock periods with bonus rewards
3. **Delegation**: Allow users to delegate staking to others
4. **Governance Integration**: Staked tokens could provide voting power
5. **Auto-Compounding**: Automatic reward reinvestment
6. **Tiered Rewards**: Different reward rates based on stake amount or duration

## Support

For questions, issues, or contributions:

- GitHub: [stellar-defi-toolkit](https://github.com/frankosakwe/stellar-defi-toolkit)
- Documentation: See `/docs` directory
- Examples: See `/examples/staking.rs`

## License

This contract is licensed under MIT OR Apache-2.0.
