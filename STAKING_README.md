# Staking Contract - Quick Start Guide

## Overview

The Staking Contract allows users to stake tokens and earn rewards over time on the Stellar blockchain. This is a production-ready implementation with comprehensive features and security measures.

## Key Features

✅ **Time-Based Rewards** - Earn rewards proportional to your stake and duration  
✅ **Flexible Staking** - Stake and unstake at any time  
✅ **Separate Claiming** - Claim rewards independently from staking  
✅ **Emergency Withdrawal** - Quick exit option (forfeits rewards)  
✅ **Admin Controls** - Configurable reward periods and amounts  
✅ **Multi-User Support** - Fair distribution among all stakers  
✅ **Event Emission** - Track all contract activities  
✅ **Comprehensive Tests** - Full test coverage included  

## Quick Start

### 1. Build the Contract

```bash
cargo build --release --target wasm32-unknown-unknown
```

### 2. Run Tests

```bash
cargo test --package stellar-defi-toolkit --lib contracts::staking::tests
```

### 3. Run Example

```bash
cargo run --example staking
```

## Basic Usage

### For Users

#### Stake Tokens
```rust
// Stake 1000 tokens
staking_contract.stake(&user_address, &1_000_000_000);
```

#### Check Rewards
```rust
// View earned rewards
let earned = staking_contract.get_earned(&user_address);
```

#### Claim Rewards
```rust
// Claim all earned rewards
let claimed = staking_contract.claim_rewards(&user_address);
```

#### Unstake Tokens
```rust
// Unstake 500 tokens (keeps rewards)
staking_contract.unstake(&user_address, &500_000_000);
```

### For Admins

#### Initialize Contract
```rust
staking_contract.initialize(
    &admin_address,
    &staking_token_address,
    &reward_token_address,
    &17280, // 1 day reward period
);
```

#### Set Rewards
```rust
// Distribute 10,000 reward tokens over the period
staking_contract.notify_reward_amount(&admin_address, &10_000_000_000);
```

## How It Works

### Reward Distribution

1. **Admin sets reward amount** for a specific duration (e.g., 10,000 tokens over 7 days)
2. **Reward rate is calculated** automatically (e.g., ~1,428 tokens per day)
3. **Users stake tokens** and start earning immediately
4. **Rewards accumulate** proportionally based on:
   - Amount staked
   - Time staked
   - Total pool size
5. **Users claim rewards** at any time without unstaking

### Example Scenario

```
Initial State:
- Reward Pool: 10,000 REWARD tokens
- Duration: 7 days (120,960 ledgers)
- Reward Rate: ~0.083 REWARD per ledger

Day 1:
- Alice stakes 1,000 USDC
- Bob stakes 500 USDC
- Total Staked: 1,500 USDC

Day 2:
- Alice earned: ~952 REWARD (66.7% of daily rewards)
- Bob earned: ~476 REWARD (33.3% of daily rewards)

Day 3:
- Alice claims her 952 REWARD
- Alice still has 1,000 USDC staked
- Bob's rewards continue accumulating

Day 4:
- Bob unstakes 200 USDC
- Bob still has 300 USDC staked
- Bob's unclaimed rewards: ~1,428 REWARD
```

## Time Conversions

Stellar uses ledgers (≈5 seconds each):

| Duration | Ledgers |
|----------|---------|
| 1 minute | 12 |
| 1 hour | 720 |
| 1 day | 17,280 |
| 1 week | 120,960 |
| 1 month | 518,400 |

## Contract Functions

### User Functions
- `stake(user, amount)` - Stake tokens
- `unstake(user, amount)` - Unstake tokens
- `claim_rewards(user)` - Claim earned rewards
- `emergency_withdraw(user)` - Withdraw all (forfeit rewards)
- `get_staked_balance(user)` - View staked amount
- `get_earned(user)` - View earned rewards

### Admin Functions
- `initialize(...)` - Set up the contract
- `notify_reward_amount(admin, amount)` - Add rewards

### View Functions
- `get_total_staked()` - Total staked in contract
- `get_reward_rate()` - Current reward rate
- `get_info()` - All contract information

## Security Features

🔒 **Authorization Checks** - All actions require proper authentication  
🔒 **Overflow Protection** - Safe math operations  
🔒 **Balance Validation** - Comprehensive checks  
🔒 **Initialization Guard** - Prevents double initialization  
🔒 **Admin Controls** - Restricted administrative functions  

## Testing

The contract includes extensive tests:

```bash
# Run all staking tests
cargo test --package stellar-defi-toolkit --lib contracts::staking

# Run specific test
cargo test --package stellar-defi-toolkit --lib contracts::staking::tests::test_rewards
```

Test coverage includes:
- ✅ Initialization
- ✅ Staking/Unstaking
- ✅ Reward calculations
- ✅ Multiple users
- ✅ Edge cases
- ✅ Error conditions
- ✅ Emergency withdrawals

## Documentation

For detailed documentation, see:
- **Full Documentation**: [docs/staking_contract.md](docs/staking_contract.md)
- **Example Code**: [examples/staking.rs](examples/staking.rs)
- **Contract Source**: [src/contracts/staking.rs](src/contracts/staking.rs)

## Common Use Cases

### 1. Liquidity Mining
Reward users for providing liquidity to your protocol.

### 2. Governance Staking
Distribute governance tokens to long-term holders.

### 3. Yield Farming
Create yield opportunities for token holders.

### 4. Protocol Incentives
Bootstrap network effects by rewarding early adopters.

## Deployment Checklist

Before deploying to production:

- [ ] Run all tests
- [ ] Conduct security audit
- [ ] Test with actual token contracts
- [ ] Verify reward token supply
- [ ] Set appropriate reward duration
- [ ] Configure admin keys securely
- [ ] Test emergency scenarios
- [ ] Document deployment parameters
- [ ] Set up monitoring and alerts

## Troubleshooting

### "Amount must be greater than 0"
- Ensure you're staking/unstaking a positive amount

### "Insufficient staked balance"
- Check your staked balance before unstaking
- Use `get_staked_balance()` to verify

### "Only admin can notify reward amount"
- Ensure you're using the correct admin address
- Verify admin authorization

### "No tokens staked"
- You must stake tokens before earning rewards
- Check if you have any staked balance

## APY Calculation

To calculate the Annual Percentage Yield:

```
APY = (reward_rate × 6,307,200 / total_staked) × 100

where:
- reward_rate = rewards per ledger
- 6,307,200 = ledgers per year (≈365.25 days)
- total_staked = total tokens staked
```

Example:
```
reward_rate = 0.083 REWARD per ledger
total_staked = 1,000,000 USDC

APY = (0.083 × 6,307,200 / 1,000,000) × 100
    = 52.35%
```

## Contributing

Contributions are welcome! Please:

1. Fork the repository
2. Create a feature branch
3. Add tests for new functionality
4. Ensure all tests pass
5. Submit a pull request

## Support

- **Issues**: [GitHub Issues](https://github.com/frankosakwe/stellar-defi-toolkit/issues)
- **Discussions**: [GitHub Discussions](https://github.com/frankosakwe/stellar-defi-toolkit/discussions)
- **Documentation**: See `/docs` directory

## License

MIT OR Apache-2.0

---

**Built with ❤️ for the Stellar ecosystem**
