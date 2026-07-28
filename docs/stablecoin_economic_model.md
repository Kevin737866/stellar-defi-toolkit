# Stellar Stablecoin Economic Model

## Overview

This document describes the economic model for the decentralized stablecoin built on Stellar using Soroban smart contracts. The stablecoin is an over-collateralized system designed to maintain a 1:1 peg with the US dollar through multiple stabilization mechanisms.

## Architecture

### Core Components

1. **Stablecoin Contract** - The main token contract implementing SEP-41 standards
2. **Collateral Vaults** - Over-collateralized positions for minting stablecoins
3. **Price Oracle** - Reliable price feeds for collateral valuation
4. **Stability Pool** - Backstop mechanism for liquidations and peg defense
5. **Governance System** - Decentralized parameter management
6. **Arbitrage Incentives** - Market-based peg maintenance

## Economic Parameters

### Collateral Requirements

| Parameter | Value | Description |
|-----------|-------|-------------|
| Minimum Collateral Ratio | 110% | Minimum over-collateralization required |
| Default Collateral Ratio | 150% | Standard ratio for new positions |
| Maximum Collateral Ratio | 500% | Maximum allowed ratio |
| Minimum Debt Position | 100 SUSD | Minimum stablecoin debt per vault |
| Maximum Debt Position | 10,000 SUSD | Maximum stablecoin debt per vault |

### Fee Structure

| Fee Type | Rate | Description |
|----------|------|-------------|
| Minting Fee | 0.5% | Fee charged when minting stablecoins |
| Redemption Fee | 0.5% | Fee charged when burning stablecoins |
| Liquidation Penalty | 10% | Penalty applied to liquidated positions |
| Stability Pool Reward | 5% APY | Rewards for stability pool providers |
| Arbitrage Reward | 0.5-2% | Variable rewards for peg maintenance |

## Stability Mechanisms

### 1. Over-Collateralization

The system maintains over-collateralized positions to ensure stability:

- **Dynamic Collateral Ratios**: Different collateral types have different risk profiles
- **Real-Time Monitoring**: Continuous monitoring of collateral ratios
- **Automatic Liquidations**: Positions below minimum ratio are automatically liquidated

### 2. Stability Pool

The stability pool acts as a first line of defense:

- **Liquidation Coverage**: Uses deposits to liquidate undercollateralized positions
- **Reward Distribution**: Distributes liquidation gains to depositors
- **Early Withdrawal Penalties**: Discourages premature withdrawals during stress periods

### 3. Price Oracle Integration

Reliable price feeds are critical for system stability:

- **Multi-Source Aggregation**: Prices from multiple sources are aggregated
- **Time-Weighted Average Prices (TWAP)**: Prevents manipulation
- **Deviation Alerts**: Automatic alerts for unusual price movements

### 4. Arbitrage Incentives

Market-based mechanisms maintain the peg:

- **Opportunity Detection**: Automatic detection of arbitrage opportunities
- **Sliding Scale Rewards**: Higher rewards for larger deviations
- **Performance Tracking**: Tracks arbitrageur performance

## Risk Management

### Systemic Risks

1. **Collateral Price Volatility**
   - Mitigated through diversified collateral types
   - Dynamic collateral ratios based on volatility
   - Circuit breakers for extreme price movements

2. **Bank Runs**
   - Stability pool provides immediate liquidity
   - Redemption fees discourage panic withdrawals
   - Emergency shutdown procedures

3. **Oracle Failures**
   - Multiple price sources prevent single points of failure
   - Manual override capabilities through governance
   - Price deviation alerts

### Risk Parameters

| Risk Metric | Target | Maximum |
|-------------|--------|---------|
| System Collateral Ratio | 180% | 150% |
| Stability Pool Size | 20% of supply | 10% of supply |
| Single Collateral Concentration | 30% | 50% |
| Daily Liquidation Volume | 5% of TVL | 15% of TVL |

## Economic Risk Analysis

This section analyses the model above against the behaviour actually implemented
in [stablecoin.rs](../src/contracts/stablecoin.rs) and
[stability_pool.rs](../src/contracts/stability_pool.rs). Where the two diverge,
the contract is authoritative. Cross-module context is in
[economic_risk_analysis.md](./economic_risk_analysis.md).

### Collateral Ratio Analysis

Constants from [stablecoin.rs:25](../src/contracts/stablecoin.rs#L25):

| Constant | Raw | Value |
|----------|-----|-------|
| `MIN_COLLATERAL_RATIO` | `11000` | 110 % |
| `DEFAULT_COLLATERAL_RATIO` | `15000` | 150 % |
| `MAX_COLLATERAL_RATIO` | `50000` | 500 % |
| `LIQUIDATION_PENALTY_BPS` | `1000` | 10 % |
| `MIN_DEBT` | `100_000_000` | 100 SUSD (7 decimals) |
| `MAX_DEBT` | `10_000_000_000` | 10 000 SUSD |

#### Capital efficiency

| Collateral ratio | SUSD per $1 collateral | Leverage | Drawdown to 110 % floor |
|------------------|-----------------------:|---------:|------------------------:|
| 500 % (max) | $0.200 | 1.25× | 78.0 % |
| 200 % | $0.500 | 2.00× | 45.0 % |
| 150 % (default) | $0.667 | 3.00× | 26.7 % |
| 130 % | $0.769 | 4.33× | 15.4 % |
| 110 % (floor) | $0.909 | 11.00× | 0.0 % |

Drawdown tolerance is `1 − MCR / CR`. The 150 % default buys a 26.7 % adverse
move — roughly a 3-σ weekly move for XLM-class collateral. That is a defensible
default. The 110 % floor buys nothing: a vault sitting at the floor is one tick
from insolvency, and the `MIN_DEBT` of 100 SUSD is small enough that dust vaults
at the floor are economically unattractive to liquidate at all once gas and
slippage are counted.

#### Critical: the penalty equals the floor

`calculate_collateral_ratio` returns `collateral_value × 10000 / debt_amount`,
and `liquidate` proceeds only when that value is **below**
`min_collateral_ratio`. It then seizes

```rust
let penalty_multiplier = 10000 + LIQUIDATION_PENALTY_BPS;      // 11000
let collateral_to_liquidate = (collateral_value_needed * penalty_multiplier) / 10000;
```

Let `c` be the vault's collateral ratio and `f` the fraction of debt repaid.
Post-liquidation:

```
c' = (c − f·(1 + penalty)) / (1 − f)
```

`c'` exceeds `c` only when `c > 1 + penalty = 1.10`. But `c > 1.10` is exactly
the range in which liquidation is rejected. **Within the permitted range
(`c < 1.10`), every liquidation lowers the vault's collateral ratio further.**

| Ratio at liquidation | `f` = 25 % | `f` = 50 % | `f` = 100 % |
|----------------------|-----------:|-----------:|------------:|
| 110 % (boundary) | 110.0 % | 110.0 % | n/a — exactly exhausts collateral |
| 108 % | 107.3 % | 106.0 % | insolvent |
| 105 % | 103.3 % | 100.0 % | insolvent |
| 100 % | 96.7 % | 90.0 % | insolvent |

The mechanism extracts collateral but never repairs the position. **Recommended
fix:** decouple the parameters — set `MIN_COLLATERAL_RATIO = 13000` (130 %)
against the existing 10 % penalty, which makes `c' > c` across the whole
`[1.10, 1.30]` liquidation band and preserves a 13.3 % drawdown buffer from the
150 % default.

#### No close factor, no partial-liquidation protection

`liquidate` accepts any `stablecoin_amount` the vault's collateral can cover.
Unlike the lending module (50 % close factor, [lending.rs:59](../src/contracts/lending.rs#L59)),
a single liquidator can retire an entire vault in one transaction. At the floor
ratio the owner's residual equity is zero, so the design has no notion of a
minimally invasive liquidation.

#### Bad-debt boundary

The protocol takes a loss once `c < 1.00`. Starting from a 150 % vault, that
requires a 33.3 % collateral drawdown; from a 110 % vault, 9.1 %. Because the
oracle updates at most every 300 s and the circuit breaker halts reads after
three consecutive ≥5 % moves, a fast crash can traverse the 110 % → 100 % band
while the oracle is frozen. See
[economic_risk_analysis.md §5.3](./economic_risk_analysis.md#53-conflict-with-liquidation--the-central-systemic-finding).

### Depegging Scenarios

#### Scenario A — premium, SUSD trades above $1

Arbitrage path: deposit collateral → mint SUSD → sell at premium.

At the 150 % default ratio the trade returns `premium / 1.5` on capital
committed, less the 0.5 % minting fee:

| Market premium | Gross return on capital | Net of 0.5 % mint fee |
|----------------|------------------------:|----------------------:|
| 0.5 % | 0.33 % | −0.17 % |
| 1.0 % | 0.67 % | 0.17 % |
| 2.0 % | 1.33 % | 0.83 % |
| 5.0 % | 3.33 % | 2.83 % |

Below roughly a 0.75 % premium the trade is unprofitable before any collateral
price risk over the holding period is counted. **Expect premia of 1–2 % to
persist under demand shocks.** Remedies: lower the minting fee toward 0.1 %,
or allow arbitrageurs to mint at a reduced collateral ratio when the peg is
above target so less capital is trapped.

#### Scenario B — discount, SUSD trades below $1

There is no redemption backstop. `redeem`
([stablecoin.rs:185](../src/contracts/stablecoin.rs#L185)) requires
`vault.debt_amount >= stablecoin_amount` for the **caller's own vault** — it is
debt repayment, not the Liquity-style redemption that lets any holder swap SUSD
for $1 of collateral from the riskiest vault.

Consequences:

- A holder with no vault has no protocol-guaranteed exit at $1. The only bid is
  the secondary market.
- The discount is bounded only by vault owners' willingness to buy SUSD cheaply
  to close their own debt — an incentive that weakens exactly as their vaults
  approach the floor.
- **The system has no hard price floor.** This is the largest single peg risk in
  the design and it is structural, not parametric.

Recommended fix: add a redemption function that lets any holder burn SUSD
against the lowest-collateral-ratio vault at oracle price minus the redemption
fee. That converts the fee into the peg's lower bound (`$1 − fee`) and provides
continuous deleveraging pressure on the riskiest vaults.

#### Scenario C — collateral crash

| Time | Collateral | Median vault CR | System state |
|------|-----------:|----------------:|--------------|
| t+0 | 100 | 150 % | Normal |
| t+5 m | 94 | 141 % | Warning: circuit-breaker deviation 1 |
| t+10 m | 88 | 132 % | Deviation 2; operators paged |
| t+15 m | 83 | 124 % | Deviation 3 → **breaker trips, prices frozen** |
| t+45 m | 68 (true) | 102 % (unseen) | Cooldown ends; oracle still reporting 83 |
| t+45 m+ | 68 | 102 % | Reset; mass liquidation into a ratio-decreasing mechanism |

The vaults that most need liquidating become visible only after the freeze, by
which time liquidation lowers rather than raises their collateral ratio. The
combination of Scenario C with the penalty-equals-floor finding above is the
protocol's worst realistic path to bad debt.

#### Scenario D — stability pool depletion

`stability_pool.rs` sets `MAX_DEPOSIT_RATIO = 5000` (a single depositor may hold
at most 50 % of the pool), `BASE_REWARD_RATE_BPS = 500` (5 % APY),
`EARLY_WITHDRAWAL_PENALTY_BPS = 200` (2 %) and `MIN_DEPOSIT_PERIOD = 7 days`.

A 2 % early-withdrawal penalty against a 5 % APY means a depositor who has held
for more than about 20 weeks can exit at any time with the penalty fully covered
by accrued rewards. The penalty therefore does not deter a run by long-standing
depositors — precisely the cohort holding most of the pool. Sizing the penalty
against the reward rate (e.g. penalty ≥ 6 months of rewards, or a penalty that
scales with pool utilisation) would make the deterrent durable.

### Implementation Gaps Affecting These Economics

Both `redeem` and `liquidate` end at placeholder comments:

```rust
// In production: Burn stablecoins from user
// In production: Transfer collateral to user
// In production: Handle liquidation rewards and transfers
```

No token movement occurs — only accounting state and events are updated. **Actual
liquidator profit is currently zero**, so none of the incentives modelled in this
section are live. Every economic conclusion here is conditional on those transfer
paths being implemented and tested. This is tracked as item 1 in the
[cross-module risk register](./economic_risk_analysis.md#62-ranked-risk-register)
and is a mainnet blocker.

### Summary

| Finding | Severity | Status |
|---------|----------|--------|
| Liquidation transfers unimplemented | Critical | Mainnet blocker |
| `LIQUIDATION_PENALTY_BPS` equals `MIN_COLLATERAL_RATIO` margin — liquidation never repairs a vault | High | Open |
| No redemption backstop — no hard peg floor | High | Open (design) |
| No close factor — full vault seizure permitted | Medium | Open |
| Mint-side arbitrage unprofitable below ~0.75 % premium | Medium | Tune fee |
| Early-withdrawal penalty out-earned by rewards after ~20 weeks | Low | Tune |

## Governance Model

### Proposal Types

1. **Parameter Updates**: Modify system parameters
2. **Collateral Management**: Add/remove collateral types
3. **Emergency Actions**: System shutdowns and pauses
4. **Protocol Upgrades**: Smart contract upgrades

### Voting Mechanics

- **Quorum Requirements**: Minimum participation for decisions
- **Voting Periods**: 7 days default voting period
- **Execution Delays**: 2-day delay before execution
- **Delegation**: Token holders can delegate voting power

## Tokenomics

### Stablecoin Supply Dynamics

```
Total Supply = Minted Stablecoins - Burned Stablecoins
```

### Collateral Dynamics

```
Total Collateral Value = Σ(Collateral Amount × Oracle Price)
System Collateral Ratio = Total Collateral Value / Total Supply
```

### Reward Distribution

1. **Stability Pool Rewards**
   ```
   Daily Rewards = Pool Deposits × (Reward Rate / 365)
   User Rewards = User Deposit × (Reward Index / Pool Share)
   ```

2. **Arbitrage Rewards**
   ```
   Reward = Trade Amount × Reward Rate × Deviation Factor
   ```

## Economic Scenarios

### Bull Market Scenario

- **Collateral Values Increase**: Higher collateral ratios
- **More Minting**: Increased stablecoin supply
- **Lower Liquidations**: Reduced system stress
- **Higher Yields**: Increased arbitrage opportunities

### Bear Market Scenario

- **Collateral Values Decrease**: Lower collateral ratios
- **More Redemptions**: Decreased stablecoin supply
- **Higher Liquidations**: Increased system stress
- **Stability Pool Usage**: Higher rewards for providers

### Black Swan Event

- **Rapid Collateral Devaluation**: Mass liquidations
- **Stability Pool Depletion**: System uses remaining mechanisms
- **Emergency Shutdown**: Controlled wind-down procedures
- **Proportional Redemptions**: Fair distribution of remaining assets

## Performance Metrics

### Key Performance Indicators

1. **Peg Stability**: Deviation from $1.00 target
2. **Collateral Ratio**: System-wide over-collateralization
3. **Liquidity Depth**: Available stablecoin liquidity
4. **Market Confidence**: Trading volume and spread
5. **System Health**: Composite risk score

### Monitoring Dashboard

- Real-time collateral ratios
- Price feed status
- Liquidation rates
- Stability pool utilization
- Arbitrage activity

## Security Considerations

### Smart Contract Security

1. **Audited Contracts**: All contracts undergo professional audits
2. **Formal Verification**: Critical functions verified mathematically
3. **Bug Bounties**: Incentivized vulnerability disclosure
4. **Gradual Rollouts**: Phased deployment with monitoring

### Economic Security

1. **Diversified Collateral**: Multiple asset types reduce concentration risk
2. **Circuit Breakers**: Automatic pauses on extreme conditions
3. **Governance Safeguards**: Time delays and quorum requirements
4. **Insurance Fund**: Reserve for extreme scenarios

## Regulatory Compliance

### Design Principles

1. **Decentralization**: No single point of control
2. **Transparency**: All operations on-chain and verifiable
3. **Privacy-First**: Minimal data collection
4. **Jurisdiction-Agnostic**: Global accessibility

### Compliance Measures

- **AML/KYC Integration**: Optional compliance layers
- **Reporting Standards**: Standardized financial reporting
- **Regulatory Engagement**: Proactive regulator communication
- **Legal Framework**: Clear terms of service

## Future Development

### Roadmap

1. **Phase 1**: Core stablecoin functionality
2. **Phase 2**: Advanced stability mechanisms
3. **Phase 3**: Cross-chain integration
4. **Phase 4**: DeFi ecosystem integration

### Research Areas

- **Algorithmic Stabilization**: Advanced algorithmic mechanisms
- **Synthetic Assets**: Expansion into other asset classes
- **Yield Generation**: Automated yield strategies
- **Layer 2 Integration**: Scaling solutions

## Conclusion

The Stellar stablecoin economic model is designed to provide a robust, scalable, and stable digital currency that maintains its peg through multiple complementary mechanisms. The system balances decentralization, security, and usability while providing strong incentives for participation and stability maintenance.

The multi-layered approach to stability, combined with robust governance and risk management, creates a resilient system capable of withstanding various market conditions while maintaining user confidence and system integrity.
