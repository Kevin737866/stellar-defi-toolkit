# Synthetic Asset Protocol Risk Management

## Overview

This document outlines the comprehensive risk management framework for the synthetic asset protocol on Stellar. The protocol implements multiple layers of risk mitigation to ensure system stability and user protection.

## Risk Categories

### 1. Market Risk

#### Price Volatility Risk
- **Description**: Risk from extreme price movements in underlying assets
- **Mitigation**: 
  - Dynamic collateral ratios based on asset volatility
  - Real-time price monitoring with circuit breakers
  - Position size limits for volatile assets
  - Automated rebalancing triggers

#### Liquidity Risk
- **Description**: Risk of insufficient liquidity for liquidations
- **Mitigation**:
  - Stability pool with guaranteed liquidity
  - Diversified oracle sources
  - Gradual liquidation mechanisms
  - Emergency liquidity providers

### 2. Collateral Risk

#### Under-Collateralization Risk
- **Description**: Positions becoming under-collateralized due to price drops
- **Mitigation**:
  - Real-time collateral ratio monitoring
  - Automated liquidation at 120% ratio
  - Margin call warnings at 130% ratio
  - Position auto-closure at 110% ratio

#### Concentration Risk
- **Description**: Risk from too much exposure to single asset/oracle
- **Mitigation**:
  - Maximum 20% exposure per asset type
  - Maximum 10% exposure per oracle
  - Asset type diversification requirements
  - Dynamic fee adjustments for concentration

### 3. Oracle Risk

#### Oracle Failure Risk
- **Description**: Risk of price feed manipulation or failure
- **Mitigation**:
  - Multi-oracle aggregation (minimum 3 sources)
  - Confidence-weighted price calculation
  - Oracle reputation system
  - Automatic failover mechanisms
  - Price deviation alerts and circuit breakers

#### Stale Price Risk
- **Description**: Risk from outdated price information
- **Mitigation**:
  - Maximum 1 hour price age
  - Real-time timestamp validation
  - Automatic oracle deactivation
  - Freshness requirements for critical assets

### 4. Operational Risk

#### Smart Contract Risk
- **Description**: Risk from bugs or exploits in contract code
- **Mitigation**:
  - Comprehensive formal verification
  - Multiple independent security audits
  - Bug bounty programs
  - Gradual rollout with monitoring
  - Emergency pause mechanisms

#### Governance Risk
- **Description**: Risk from malicious governance actions
- **Mitigation**:
  - Time-delayed execution (48 hours)
  - Multi-signature requirements for critical changes
  - Quorum requirements for parameter changes
  - Emergency pause with multi-sig override

### 5. Systemic Risk

#### Cascade Failure Risk
- **Description**: Risk of cascading liquidations across positions
- **Mitigation**:
  - Circuit breakers during market stress
  - Position limits per user
  - System-wide liquidation throttling
  - Emergency shutdown procedures
  - Insurance fund for extreme scenarios

## Risk Parameters

### Global Risk Parameters

```rust
pub struct RiskParameters {
    /// Global minimum collateral ratio (150%)
    pub global_min_ratio: u32,
    /// Maximum debt per user ($1M)
    pub max_debt_per_user: u64,
    /// Maximum total protocol debt ($100M)
    pub max_total_debt: u64,
    /// Liquidation threshold (120%)
    pub liquidation_threshold: u32,
    /// Emergency pause threshold (50%)
    pub emergency_pause_threshold: u32,
    /// Minimum oracle confidence (80%)
    pub min_oracle_confidence: u32,
}
```

### Asset-Specific Parameters

> **Reconciliation note.** The contract enforces a hard floor of
> `MIN_COLLATERAL_RATIO = 15000` (150 %) via `risk_params.global_min_ratio`
> ([synthetic_protocol.rs:25](../src/contracts/synthetic_protocol.rs#L25)). The
> Commodities (120 %) and Forex (110 %) rows below are **unreachable as written** —
> `list_asset` will accept them but `mint_synthetic` rejects any position under
> 150 %. Either raise those rows to 150 % or lower `global_min_ratio` by
> governance. The contract is authoritative.

| Asset Type | Min Collateral Ratio | Max Collateral Ratio | Minting Fee | Position Limit | Reachable? |
|-------------|-------------------|-------------------|-------------|-------------|------------|
| Stocks | 150% | 500% | 0.5% | $500K | Yes |
| Crypto | 200% | 800% | 0.75% | $1M | Yes |
| Commodities | 120% | 400% | 0.25% | $2M | **No — below 150 % floor** |
| Forex | 110% | 300% | 0.1% | $5M | **No — below 150 % floor** |
| Indices | 180% | 600% | 0.3% | $100K | Yes |

### Oracle Risk Parameters

| Confidence Level | Weight in Aggregation | Max Price Deviation | Timeout Period |
|----------------|-------------------|-------------------|--------------|
| 95-100% | 100% | 1% | 30 minutes |
| 90-95% | 80% | 2% | 1 hour |
| 80-90% | 60% | 3% | 2 hours |
| 70-80% | 40% | 5% | 6 hours |
| <70% | 20% | 10% | 12 hours |

## Economic Risk Analysis

The sections above describe the intended risk framework. This section analyses
the economics actually implemented in
[synthetic_protocol.rs](../src/contracts/synthetic_protocol.rs). Cross-module
context is in [economic_risk_analysis.md](./economic_risk_analysis.md).

### Contract Parameters as Deployed

Set by `initialize` ([synthetic_protocol.rs:63](../src/contracts/synthetic_protocol.rs#L63))
and the module constants:

| Parameter | Raw | Value |
|-----------|-----|-------|
| `MIN_COLLATERAL_RATIO` / `global_min_ratio` | `15000` | 150 % |
| `MAX_COLLATERAL_RATIO` | `100000` | 1000 % |
| `liquidation_threshold` | `12000` | 120 % |
| `emergency_pause_threshold` | `5000` | 50 % |
| `max_debt_per_user` | `1_000_000_000_000` | $1 M |
| `max_total_debt` | `100_000_000_000_000` | $100 M |
| `DEFAULT_MINTING_FEE_BPS` | `50` | 0.5 % |
| `LIQUIDATION_PENALTY_BPS` | `1000` | 10 % |
| `MIN_ORACLE_CONFIDENCE` | `8000` | 80 % |
| `STAKING_REWARD_RATE_BPS` | `1000` | 10 % |

### Debt Pool Risk

#### The protocol does not run a shared debt pool

Positions are individually collateralised: `SyntheticPosition` carries its own
`collateral_deposits` and `debt_amount`, and `calculate_collateral_ratio` reads
only that position. There is no price-indexed pooled debt of the Synthetix kind.

This is a significant risk decision and it should be stated explicitly rather
than left implicit:

| | Shared debt pool | Per-position (this implementation) |
|---|---|---|
| Minter absorbs others' PnL | Yes | **No** |
| Contagion between minters | High | **None** |
| Capital efficiency | Higher (pooled backing) | Lower (each position over-collateralised) |
| Liquidation complexity | Low | **Per-position, must be individually profitable** |
| Concentration risk | Systemic, pooled | **Sits at the protocol cap** |

The upside is that no minter can be harmed by another minter's asset selection.
The cost is that all risk concentrates in two places: per-position liquidation
mechanics, and the global `max_total_debt` cap.

#### Capital efficiency and the liquidation buffer

| State | Collateral ratio | Synth per $1 collateral | Move to next boundary |
|-------|-----------------:|------------------------:|----------------------:|
| Mint floor | 150 % | $0.667 | −20.0 % → liquidatable |
| Liquidation trigger | 120 % | $0.833 | −16.7 % → insolvent |
| Insolvency | 100 % | $1.000 | — |

A freshly minted position absorbs a 20 % adverse move before it is liquidatable
and a further 16.7 % before the protocol takes a loss. For equity and forex
synthetics this is generous; for crypto synthetics with 80 %+ annualised
volatility, a 20 % move is roughly a 1.5-day 2-σ event and the buffer is thin.
Per-asset `min_collateral_ratio` should be scaled to the underlying's volatility
rather than left at the 150 % global floor.

#### Concentration risk at the protocol cap

`max_total_debt` is a single $100 M global limit with **no per-asset sub-cap**.
Nothing prevents the entire $100 M from being minted against one synthetic. Since
oracle error on a given asset hits every position in that asset simultaneously
(correlation = 1), the effective worst case is that the whole protocol debt is
exposed to a single price feed. Add a per-asset cap — e.g. 20 % of
`max_total_debt` — enforced in `list_asset` and re-checked in `mint_synthetic`.

`max_debt_per_user` at $1 M limits any single user to 1 % of protocol debt, which
is a reasonable per-account bound and is not the binding constraint.

#### Critical: liquidator payout is decoupled from debt

`liquidate_position`
([synthetic_protocol.rs:371](../src/contracts/synthetic_protocol.rs#L371))
computes:

```rust
let liquidation_penalty = (debt_value * LIQUIDATION_PENALTY_BPS as u64) / 10000;
let total_collateral_value = Self::get_position_collateral_value(&env, &position);
let liquidator_share = (total_collateral_value * 9000) / 10000; // 90% to liquidator
let user_share = total_collateral_value - liquidator_share;
```

`liquidation_penalty` is computed and routed to `distribute_fees`, but the
liquidator's payout is `90 % of the entire position collateral` — a figure that
never references the debt being retired. The two calculations describe
inconsistent economics:

| Collateral ratio at liquidation | Nominal payout (debt × 1.10) | Actual payout (collateral × 0.90) | Effective bonus | Borrower keeps |
|--------------------------------:|-----------------------------:|----------------------------------:|----------------:|---------------:|
| 120 % (trigger) | 1.100 × debt | 1.080 × debt | 8.0 % | 12 % of collateral |
| 119 % | 1.100 × debt | 1.071 × debt | 7.1 % | 11.9 % |
| 110 % | 1.100 × debt | 0.990 × debt | −1.0 % | 11 % |
| 100 % | 1.100 × debt | 0.900 × debt | −10.0 % | 10 % |

Two distinct failures follow:

1. **The borrower is over-penalised at every ratio.** A position liquidated one
   basis point below the 120 % trigger loses 90 % of its collateral, not the
   10 % the penalty parameter advertises. The residual returned is 12 % of
   collateral against a nominal expectation of ~9 % *of debt* plus all remaining
   equity.
2. **The liquidator is under-paid below ~111 %.** Once collateral falls under
   `debt × 1.111`, 90 % of it is worth less than the debt retired, so
   liquidation becomes loss-making and stops happening — exactly in the band
   where it is most needed.

**Recommended fix:**

```rust
let payout = min(
    debt_value * (10_000 + LIQUIDATION_PENALTY_BPS) / 10_000,
    total_collateral_value,
);
let user_share = total_collateral_value - payout;
```

This makes the penalty parameter meaningful, returns genuine residual equity to
the borrower, and keeps liquidation profitable down to `c = 1.10`.

#### Other liquidation defects

- **No close factor.** The whole position is retired in one call; there is no
  partial-liquidation path.
- **`liquidating` flag is set but never cleared.** The position is removed from
  storage at the end of the call, so the flag is moot in the happy path — but any
  future partial-liquidation path would deadlock on it.
- **Collateral maps are placeholders.** `collateral_to_liquidator` and
  `collateral_returned` are built as `Map::new(&env)` under the comment
  `// In production, handle actual collateral transfers`. **No collateral moves.**
  Liquidator profit is currently zero and every figure in the table above is
  hypothetical until the transfer paths land.
- **`asset_info.total_collateral -= position.debt_amount`** subtracts a debt
  figure from a collateral accumulator (flagged `// Simplified` in the source).
  Protocol-level collateral accounting will drift from reality on every
  liquidation.

### Oracle Dependency

#### Synthetics are the most oracle-dependent module

In lending, the oracle prices *collateral* — an error changes how much you may
borrow. Here the oracle defines the *debt itself*. A 10 % upward error on `sBTC`
inflates every sBTC minter's liability by 10 % instantly, and can move an entire
cohort from healthy to liquidatable in a single update. There is no diversifying
effect: every position in an asset shares one feed.

| Oracle error | Effect on a 150 % position | Effect on a 130 % position |
|-------------:|---------------------------|---------------------------|
| +10 % | 136 % — healthy | 118 % — **liquidatable** |
| +20 % | 125 % — healthy | 108 % — liquidatable, near insolvent |
| +25 % | 120 % — **at trigger** | 104 % — liquidatable |
| +50 % | 100 % — **insolvent** | 87 % — insolvent |

A sustained 25 % feed error is a protocol-wide solvency event for that asset.

#### Confidence is self-attested

Both `mint_synthetic` ([synthetic_protocol.rs:191](../src/contracts/synthetic_protocol.rs#L191))
and `update_oracle_price` ([synthetic_protocol.rs:498](../src/contracts/synthetic_protocol.rs#L498))
enforce `confidence >= MIN_ORACLE_CONFIDENCE` (80 %). The confidence value is
supplied by the reporting oracle in the same call as the price. A faulty or
compromised reporter simply asserts `confidence = 10000` alongside an arbitrary
price and passes the check.

**This is a data-quality filter, not a security control**, and should not be
counted as a defence in threat modelling. The controls that would make it one:

| Control | Purpose | Status |
|---------|---------|--------|
| On-chain multi-source median | Removes single-reporter authority | **Not enforced in this contract** |
| Minimum reporter count per asset | Prevents degradation to one source | **Not enforced** |
| Per-asset circuit breaker on the *synthetic* price | Bounds debt-side error, not just collateral-side | **Not present** |
| Reporter staking + slashing | Makes confidence economically backed | Not present |
| Staleness bound on synthetic prices | Caps how old a debt valuation may be | Not enforced here |

`ORACLES` maps `asset_id → Vec<Address>`, so multiple reporters can be
registered, but the price path does not require agreement among them.

#### Interaction with the circuit breaker

The circuit breaker in [circuit_breaker.rs](../src/contracts/circuit_breaker.rs)
guards collateral price feeds. If it also gates the synthetic's own feed, a trip
freezes both minting and liquidation for the asset — during which minters'
liabilities keep moving in the real world while the protocol cannot act. See
[economic_risk_analysis.md §5.3](./economic_risk_analysis.md#53-conflict-with-liquidation--the-central-systemic-finding)
for the general form of this conflict and the proposed liquidation-only mode.

### Systemic Risk

| Channel | Mechanism | Severity |
|---------|-----------|----------|
| Single-feed correlation | One oracle error hits 100 % of an asset's positions at once | **High** |
| No per-asset debt cap | Entire $100 M protocol debt may sit on one feed | **High** |
| Liquidation payout defect | Liquidation unprofitable below ~111 %, so bad positions persist | **High** |
| Breaker/liquidation conflict | Freeze defers liquidation while the loss grows | Medium |
| Shared collateral with lending and stablecoin | The same collateral asset backs three protocols | Medium |
| Fee/staking coupling | `STAKING_REWARD_RATE_BPS` = 10 % paid from fees; a fee shortfall in a quiet market makes staking rewards unfunded | Low |

### Summary of Findings

| # | Finding | Severity | Status |
|---|---------|----------|--------|
| 1 | Collateral transfers on liquidation unimplemented | Critical | Mainnet blocker |
| 2 | Liquidator receives 90 % of collateral irrespective of debt | Critical | Open |
| 3 | No per-asset debt cap under the $100 M global limit | High | Open |
| 4 | Oracle confidence is self-attested, not enforced by aggregation | High | Open |
| 5 | Doc asset table lists ratios below the 150 % contract floor | Medium | Annotated above |
| 6 | `total_collateral -= debt_amount` corrupts protocol accounting | Medium | Open |
| 7 | No close factor / partial liquidation | Medium | Open |
| 8 | `liquidating` flag never cleared | Low | Open |

## Risk Monitoring

### Real-Time Monitoring

The protocol implements continuous monitoring of:

1. **Position Health**
   - Collateral ratio tracking
   - PnL calculation
   - Time-based risk metrics
   - Automated alerts

2. **Market Conditions**
   - Price volatility analysis
   - Volume monitoring
   - Correlation tracking
   - System stress indicators

3. **Oracle Performance**
   - Price accuracy tracking
   - Response time monitoring
   - Reputation scoring
   - Failover detection

4. **System Metrics**
   - Total value locked
   - Collateralization ratios
   - Liquidation rates
   - Fee distribution
   - User concentration

### Alert System

#### Alert Types and Thresholds

1. **Critical Alerts** (Immediate Action Required)
   - Collateral ratio < 110%
   - Oracle confidence < 70%
   - System health score < 30%
   - Price deviation > 10%

2. **Warning Alerts** (Attention Required)
   - Collateral ratio 110-130%
   - Oracle confidence 70-80%
   - Position age > 30 days
   - Single asset > 20% exposure

3. **Info Alerts** (Monitoring)
   - New position created
   - Price updates received
   - Batch operations executed
   - Governance proposals created

## Risk Mitigation Strategies

### Proactive Measures

1. **Dynamic Collateral Requirements**
   ```rust
   // Adjust based on volatility
   let required_ratio = base_ratio * (1 + volatility_score);
   let max_ratio = min(50000, required_ratio * 2);
   ```

2. **Position Size Limits**
   ```rust
   // Limit exposure based on asset type and user tier
   let max_position = match asset_type {
       AssetType::Stock => 500_000_000, // $500K
       AssetType::Crypto => 1_000_000_000, // $1M
       AssetType::Commodity => 2_000_000_000, // $2M
       _ => 100_000_000, // $100K default
   };
   ```

3. **Diversification Requirements**
   ```rust
   // Check user's portfolio diversity
   let asset_type_exposure = calculate_exposure_by_type(user_positions);
   let max_exposure_per_type = TOTAL_COLLATERAL * 0.20; // 20% max
   
   for (asset_type, exposure) in asset_type_exposure {
       if exposure > max_exposure_per_type {
           reject_new_position("Insufficient diversification");
       }
   }
   ```

### Reactive Measures

1. **Circuit Breakers**
   ```rust
   // Pause operations during extreme conditions
   if system_stress_score > 8000 {
       protocol.pause();
       emit_critical_alert("System stress detected - operations paused");
   }
   ```

2. **Gradual Liquidations**
   ```rust
   // Liquidate positions gradually to prevent cascades
   let liquidation_batch_size = max(1, total_at_risk / 10);
   
   for position in at_risk_positions {
       if should_liquidate(position) {
           liquidate_position(position, liquidation_batch_size);
           delay_next_liquidation(1.hour); // Prevent cascade
       }
   }
   ```

3. **Emergency Procedures**
   ```rust
   // Multi-step emergency response
   if emergency_triggered {
       // Step 1: Pause new positions
       pause_new_minting();
       
       // Step 2: Notify users
       notify_all_users("Emergency protocol pause initiated");
       
       // Step 3: Enable withdrawals only
       enable_emergency_withdrawals_only();
       
       // Step 4: Governance decision
       trigger_emergency_governance();
   }
   ```

## Stress Testing Scenarios

### Test Cases

1. **Market Crash (-50% asset prices)**
   - Expected: Increased liquidations
   - Response: Circuit breaker activation
   - Success Metric: No system failures

2. **Oracle Failure**
   - Expected: Price feed disruption
   - Response: Failover to backup oracles
   - Success Metric: <5% price deviation

3. **Bank Run (50% withdrawals)**
   - Expected: Liquidity strain
   - Response: Withdrawal limits and fees
   - Success Metric: System remains solvent

4. **Smart Contract Exploit**
   - Expected: Unauthorized operations
   - Response: Immediate pause and investigation
   - Success Metric: No funds lost

## Insurance and Recovery

### Insurance Fund

- **Purpose**: Cover extreme losses beyond normal risk parameters
- **Funding**: 2% of all fees + initial seed capital
- **Coverage**: Catastrophic events only
- **Claims Process**: Multi-signature governance approval

### Recovery Procedures

1. **Incident Response**
   - Immediate system pause
   - Investigation team activation
   - User communication protocol
   - Evidence preservation

2. **Recovery Plan**
   - Root cause analysis
   - System patch deployment
   - User compensation framework
   - Preventive measures update

## Compliance and Regulation

### Risk Disclosure

- **Transparent Risk Metrics**: All risk parameters public
- **User Risk Warnings**: Clear risk communication
- **Audit Trails**: Complete operation logging
- **Regulatory Reporting**: Standardized risk reports

### Legal Compliance

- **KYC/AML Integration**: Optional compliance layers
- **Jurisdiction Awareness**: Geographic restrictions
- **Securities Laws**: Asset type compliance
- **Consumer Protection**: Fair practice requirements

## Performance Metrics

### Key Risk Indicators (KRIs)

| KRI | Description | Target | Alert Threshold |
|-----|-------------|--------|----------------|
| System Health Score | Overall system stability | >80% | <60% |
| Average Collateral Ratio | Collateralization quality | 180% | <130% |
| Daily Liquidation Rate | System stress | <2% | >5% |
| Oracle Deviation | Price feed accuracy | <2% | >5% |
| Concentration Risk | Diversification | <20% | >40% |

### Risk Dashboard

Real-time monitoring dashboard showing:
- System health score
- Total value locked
- Collateralization ratios
- Liquidation metrics
- Oracle performance
- User risk distribution
- Alert status

## Conclusion

This comprehensive risk management framework ensures the synthetic asset protocol maintains stability under various market conditions while providing users with clear risk visibility and protection mechanisms. The multi-layered approach combines proactive risk prevention, real-time monitoring, and reactive response capabilities to create a robust and secure synthetic asset ecosystem.
