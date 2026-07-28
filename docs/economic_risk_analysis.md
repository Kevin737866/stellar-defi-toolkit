# Economic Risk Analysis

Cross-module economic risk assessment for the Stellar DeFi Toolkit.

Every figure in this document is derived from the constants and formulas actually
present in `src/`, not from an idealised design. Where the implementation differs
from the intended economics — or where a payout path is still a stub — that is
called out explicitly under **Implementation gap**. Those gaps change the risk
profile and must be closed before mainnet.

| Module | Source | Primary economic risk |
|--------|--------|-----------------------|
| Lending | [lending.rs](../src/contracts/lending.rs), [types/lending.rs](../src/types/lending.rs) | Utilisation lock-out blocking liquidations |
| Stablecoin | [stablecoin.rs](../src/contracts/stablecoin.rs), [stability_pool.rs](../src/contracts/stability_pool.rs) | Liquidation cannot restore vault health |
| Synthetic | [synthetic_protocol.rs](../src/contracts/synthetic_protocol.rs) | Liquidator payout decoupled from debt size |
| Vault | [vault.rs](../src/contracts/vault.rs) | Unverified APY input; harvest-sandwich |
| Circuit breaker | [circuit_breaker.rs](../src/contracts/circuit_breaker.rs) | Freezes liquidations during the crash it detects |

Companion documents:

- [Stablecoin Economic Model](./stablecoin_economic_model.md) — collateral ratio and depeg detail
- [Synthetic Protocol Risk Management](./synthetic_protocol_risk_management.md) — debt pool and oracle detail
- [Circuit Breaker Guide](./circuit_breaker_guide.md) — threshold derivation
- [Emergency Response Runbook](./emergency_response_runbook.md) — what to do when these risks materialise

---

## 1. Lending Protocol

### 1.1 Interest rate model

`InterestRateModel::default()` in [types/lending.rs:144](../src/types/lending.rs#L144),
with `WAD = 1e9`:

| Parameter | Raw value | Interpretation |
|-----------|-----------|----------------|
| `base_rate` | `20_000_000` | 2.0 % APR at zero utilisation |
| `slope_1` | `80_000_000` | +8.0 % APR across the sub-optimal band |
| `slope_2` | `1_200_000_000` | +120 % APR across the excess band |
| `optimal_utilization` | `800_000_000` | Kink at 80 % utilisation |

The piecewise rate function ([types/lending.rs:325](../src/types/lending.rs#L325)):

```
U ≤ 80 %:   borrow_apr = 0.02 + 0.10·U
U >  80 %:  borrow_apr = 0.10 + 6.00·(U − 0.80)
```

Utilisation is `total_debt / (total_cash + total_debt − protocol_fees)`
([lending.rs:397](../src/contracts/lending.rs#L397)) — accrued protocol fees are
excluded from the supply base, so `U` is marginally higher than the naive
`debt/(cash+debt)`.

Supplier yield is the borrow rate scaled by utilisation, net of the reserve
factor: `supply_apr ≈ borrow_apr · U · (1 − reserve_factor)`. Interest is added
to `total_debt` and the reserve cut to `protocol_fees`; suppliers are paid
implicitly through share price appreciation.

| Utilisation | Borrow APR | Supply APR (RF = 10 %) | Marginal rate |
|-------------|-----------:|-----------------------:|--------------:|
| 0 % | 2.0 % | 0.00 % | 10 bps per 1 % U |
| 25 % | 4.5 % | 1.01 % | 10 bps per 1 % U |
| 50 % | 7.0 % | 3.15 % | 10 bps per 1 % U |
| 80 % (kink) | 10.0 % | 7.20 % | 10 bps per 1 % U |
| 85 % | 40.0 % | 30.60 % | 600 bps per 1 % U |
| 90 % | 70.0 % | 56.70 % | 600 bps per 1 % U |
| 95 % | 100.0 % | 85.50 % | 600 bps per 1 % U |
| 100 % | 130.0 % | 117.00 % | 600 bps per 1 % U |

**Capital efficiency.** The model targets 80 % utilisation, i.e. 80 cents of
every supplied dollar is productive. At the kink the protocol clears a 2.8
percentage-point spread (10 % paid by borrowers, 7.2 % received by suppliers),
of which 1 pp is the reserve cut and 1.8 pp is the idle-capital drag inherent to
a `U < 100 %` design. That is competitive with mature lending markets and is the
right place for the kink: it leaves a 20 % cash buffer for withdrawals and
liquidation seizures.

**Assessment of the slope-2 discontinuity.** The marginal rate jumps 60× at the
kink. This is deliberate and correct in direction — it must be cheaper to repay
than to stay borrowed when the reserve runs dry — but the magnitude is
aggressive. Consequences:

- A borrower who is 1 % over the kink pays 40 % APR rather than 10 %. There is no
  gradual band; the penalty is immediate and large.
- Rate volatility near the kink is extreme. A single large borrow can triple the
  rate paid by every existing borrower in the reserve, including those who
  borrowed at 10 %.
- Combined with the 50 % close factor, a rate spike can push a marginally healthy
  position into liquidation purely through interest accrual, with no price move.

**Risk: accrual is time-discretised, not continuous.** `accrue_interest`
([lending.rs:380](../src/contracts/lending.rs#L380)) applies simple interest over
`now − last_accrual_ts` and only compounds when it is called. It is invoked on
deposit, withdraw, borrow, repay and liquidate. In a quiet reserve with no
activity, a 130 % nominal APR under-charges materially relative to continuous
compounding; in a busy reserve it over-charges relative to the nominal figure.
The effective rate a borrower pays is therefore a function of *other users'*
transaction frequency. This is an accepted simplification in most lending
protocols, but it should be documented in user-facing material rather than
presented as a fixed APR.

### 1.2 Liquidation profitability

Liquidation ([lending.rs:776](../src/contracts/lending.rs#L776)) triggers when

```
health_factor = Σ(collateral_value · liquidation_threshold_bps) / debt_value < 1
```

A liquidator repaying value `R` seizes collateral worth `R · (1 + b)` where `b`
is the collateral reserve's `liquidation_bonus_bps`. Repayment is capped at
`close_factor_bps` of the borrower's debt in that asset — default `5_000`, i.e.
50 % ([lending.rs:59](../src/contracts/lending.rs#L59)).

**Gross liquidator profit** is exactly `R · b`. Against that:

| Cost component | Typical magnitude | Notes |
|----------------|-------------------|-------|
| Soroban transaction fee | < 0.01 % | Negligible at any meaningful size |
| Exit slippage on seized collateral | 0.1 – 5 % | Dominant cost; scales with size |
| Price risk while unwinding | 0.5 – 3 % | Seized collateral is held, not atomically sold |
| Capital cost of repay asset | ~0 % | Sub-block holding period |

Break-even requires `b > slippage + price_risk`. Practical guidance:

| Collateral liquidity | Recommended `liquidation_bonus_bps` |
|----------------------|-------------------------------------|
| Deep (XLM, USDC) | 500 (5 %) |
| Medium | 750 – 1000 (7.5 – 10 %) |
| Thin / long-tail | 1000 – 1500, plus a low `supply_cap` |

A bonus set below realistic exit slippage produces a reserve where positions are
*technically* liquidatable but nobody liquidates them — the worst outcome, since
the health factor keeps deteriorating with no bidder.

**The toxic zone.** Let `c = collateral_value / debt_value` and `f` be the
fraction of debt repaid. After liquidation:

```
c' = (c − f·(1 + b)) / (1 − f)
```

`c'` improves only when `c > 1 + b`. Below that, **each liquidation makes the
position less collateralised than it was**. With `b = 5 %`, the toxic zone is
`c ∈ [1.00, 1.05]`.

For a reserve with `liquidation_threshold_bps = 8000`, `HF = 1` occurs at
`c = 1.25`. The cushion between "liquidatable" and "value-destructive" is
therefore a 16 % adverse price move (1.25 → 1.05), and insolvency arrives at a
20 % move (1.25 → 1.00). This is the real justification for the liquidation
threshold, and it is why `liquidation_threshold_bps` must exceed
`1 / (1 + bonus)` by a comfortable margin. Configuration rule:

```
liquidation_threshold_bps  ≤  10_000 / (1 + b) − volatility_cushion_bps
```

**Risk: high utilisation blocks liquidation.** Step 5 of `liquidate` requires the
collateral reserve to hold enough *cash* to hand over the seized amount
([lending.rs:836](../src/contracts/lending.rs#L836)):

```rust
if col_reserve.total_cash < seize_amount {
    return Err(ProtocolError::InsufficientLiquidity);
}
```

At 95 % utilisation only 5 % of the reserve is cash. Market stress raises
borrowing demand and utilisation at exactly the moment liquidations are needed,
so the constraint binds precisely when it is most damaging. The failure mode is
a reserve full of unliquidatable underwater positions whose debt keeps
compounding at 100 %+ APR. Mitigations, in order of preference:

1. Set `borrow_cap` per reserve so utilisation cannot approach 100 %.
2. Monitor `total_cash / seize_amount` for the largest at-risk position as a
   first-class operational metric (see the runbook).
3. Treat sustained `U > 95 %` on a collateral asset as a pageable condition.

**Risk: close factor interacts with the toxic zone.** A 50 % close factor means
two liquidations are needed to clear a position. If the first pushes `c` into the
toxic zone, the second actively harms the reserve. Consider lowering the close
factor for high-bonus (thin) collateral.

### 1.3 Oracle dependency

`OracleSanityConfig::default()` ([types/lending.rs:26](../src/types/lending.rs#L26))
rejects prices older than 3 600 s, deviating more than 2 000 bps (20 %) from the
last accepted price, or non-positive.

The 20 % deviation guard is looser than the circuit breaker's 10 %, so in a
combined deployment the circuit breaker binds first. The 1-hour staleness window
is the dominant exposure: a borrower can be liquidated on a price up to an hour
old, and conversely a position can remain nominally healthy for an hour after it
is economically insolvent. For volatile collateral, `max_price_age_secs` should
be reduced to 300–900 s to match the oracle's own `MIN_UPDATE_INTERVAL`.

### 1.4 Systemic risk

| Channel | Mechanism | Severity |
|---------|-----------|----------|
| Utilisation lock-out | High `U` starves liquidations of cash | **High** |
| Rate-spike cascade | Slope-2 jump forces liquidations without a price move | Medium |
| Shared-collateral contagion | One collateral asset backs debt across many reserves | Medium |
| Oracle staleness | 1 h window lets insolvency accumulate unrecognised | Medium |

---

## 2. Stablecoin

Full treatment in [stablecoin_economic_model.md](./stablecoin_economic_model.md);
summary here.

### 2.1 Collateral ratio analysis

From [stablecoin.rs:25](../src/contracts/stablecoin.rs#L25):

| Constant | Value | Meaning |
|----------|-------|---------|
| `MIN_COLLATERAL_RATIO` | `11000` | 110 % liquidation floor |
| `DEFAULT_COLLATERAL_RATIO` | `15000` | 150 % at mint |
| `MAX_COLLATERAL_RATIO` | `50000` | 500 % cap |
| `LIQUIDATION_PENALTY_BPS` | `1000` | 10 % |
| `MIN_DEBT` / `MAX_DEBT` | 100 / 10 000 SUSD | Per-vault bounds |

Capital efficiency at the 150 % default is 66.7 cents of stablecoin per dollar of
collateral; at the 110 % floor it is 90.9 cents. Price drawdown tolerated from
the default ratio is `1 − 110/150 = 26.7 %`.

**Critical finding — liquidation cannot restore vault health.** The penalty
multiplier equals the minimum collateral ratio: `1 + 0.10 = 1.10 = MCR`. Applying
the `c' = (c − f(1+b))/(1−f)` identity with `c = 1.10` and `b = 0.10` gives
`c' = 1.10` for every `f`. Liquidation at the floor is exactly ratio-neutral, and
for any `c < 1.10` — which is the only state in which liquidation is permitted,
since `liquidate` requires `current_ratio < min_collateral_ratio` — it is
strictly ratio-*decreasing*. The mechanism transfers collateral to liquidators
without ever repairing the vault. Fix by separating the two parameters, e.g.
`MCR = 130 %` with a 10 % penalty, which yields `c' > c` throughout `c ∈ [1.10,
1.30]`.

**No close factor.** `liquidate` accepts any `stablecoin_amount` up to the vault's
collateral. A single liquidator can take an entire vault at the floor ratio; the
owner's residual equity is zero by construction.

### 2.2 Depegging scenarios

**Premium (SUSD > $1).** Arbitrage is mint-and-sell. At the 150 % default ratio a
1 % premium returns `1 % / 1.5 = 0.67 %` on capital deployed, less the 0.5 %
minting fee — roughly 0.17 % net, before collateral price risk over the holding
period. The incentive is too weak to defend the peg from above; expect persistent
premia of 1–2 % under demand shocks.

**Discount (SUSD < $1).** There is no redemption backstop. `redeem`
([stablecoin.rs:185](../src/contracts/stablecoin.rs#L185)) only lets a vault owner
burn their *own* debt — it is repayment, not Liquity-style redemption against the
riskiest vault. A holder with no vault cannot convert SUSD to $1 of collateral at
any price. The consequence is that the system has **no hard price floor**: a
discount is bounded only by vault owners' incentive to buy back their own debt
cheaply, which disappears precisely when they are underwater. This is the single
largest peg risk in the design.

**Black swan.** A 27 %+ collateral crash inside one hour drives the median vault
below the floor. Because the oracle enforces `MIN_UPDATE_INTERVAL = 300 s` and
the circuit breaker trips after three consecutive ≥5 % moves, the oracle freezes
roughly 15 minutes into the crash — before most vaults can be liquidated (§5.3).

**Implementation gap.** `redeem` and `liquidate` both end at
`// In production: Transfer collateral to user`. No token transfers occur; only
accounting state and events are updated. Liquidator profit is currently zero, so
none of the incentives modelled above are live. Treat all stablecoin economics as
unvalidated until the transfer paths are implemented and tested.

---

## 3. Synthetic Asset Protocol

Full treatment in
[synthetic_protocol_risk_management.md](./synthetic_protocol_risk_management.md).

### 3.1 Debt pool risk

From [synthetic_protocol.rs:25](../src/contracts/synthetic_protocol.rs#L25) and
the defaults set in `initialize`:

| Parameter | Value |
|-----------|-------|
| `MIN_COLLATERAL_RATIO` / `global_min_ratio` | 15000 (150 %) |
| `MAX_COLLATERAL_RATIO` | 100000 (1000 %) |
| `liquidation_threshold` | 12000 (120 %) |
| `max_debt_per_user` | $1 M |
| `max_total_debt` | $100 M |
| `DEFAULT_MINTING_FEE_BPS` | 50 (0.5 %) |
| `LIQUIDATION_PENALTY_BPS` | 1000 (10 %) |
| `MIN_ORACLE_CONFIDENCE` | 8000 (80 %) |
| `STAKING_REWARD_RATE_BPS` | 1000 (10 %) |

Positions are individually collateralised — there is no shared, price-indexed
debt pool of the Synthetix kind, so minters do not absorb each other's PnL. That
removes the classic debt-pool contagion risk but replaces it with per-position
liquidation risk and concentration risk at the protocol level (`max_total_debt`
is a single global cap with no per-asset sub-cap).

The 150 % mint floor against a 120 % liquidation trigger gives a 20 % price
cushion (`1 − 120/150`) before a freshly minted position is liquidatable, and a
further 20 % (`120 → 100`) before it is insolvent.

**Critical finding — liquidator payout is decoupled from debt.** In
`liquidate_position` ([synthetic_protocol.rs:371](../src/contracts/synthetic_protocol.rs#L371)):

```rust
let liquidator_share = (total_collateral_value * 9000) / 10000; // 90% to liquidator
```

The liquidator receives 90 % of the position's **entire collateral value**,
independent of the debt being retired and independent of the nominal 10 %
penalty. At the 120 % trigger this is a payout of `1.20 × 0.90 = 1.08` per unit
of debt — an 8 % effective bonus. But a position liquidated at, say, 119 % pays
out `1.19 × 0.90 = 1.071`, while the borrower's 10 % residual is `0.119` — the
borrower loses 90 % of their collateral regardless of how marginal the shortfall
was. The parameters `LIQUIDATION_PENALTY_BPS` and the hard-coded `9000` describe
two different, inconsistent economics. Reconcile to
`liquidator_payout = min(debt_value · 1.10, total_collateral_value)`.

**Documentation drift.** The asset-type table in
`synthetic_protocol_risk_management.md` lists minimum collateral ratios of 110 %
(Forex) and 120 % (Commodities), both below the contract's 150 % `global_min_ratio`
floor. Those rows are unreachable as written. The table has been annotated in
that document; the contract is authoritative.

### 3.2 Oracle dependency

`mint_synthetic` and `update_oracle_price` both reject prices with
`confidence < MIN_ORACLE_CONFIDENCE` (80 %). Confidence is supplied by the
reporting oracle, so it is a self-attested value: a compromised or faulty
reporter can assert 100 % confidence on an arbitrary price. The check is a
liveness/quality filter, not a security control.

Synthetics are the most oracle-dependent module in the toolkit — unlike lending,
where the oracle prices collateral, here the oracle defines the *debt itself*.
A 10 % upward error on `sBTC` instantly inflates every sBTC minter's liability by
10 % and can make the entire cohort liquidatable in one update. Requirements:

- Multi-source median with an explicit minimum reporter count, enforced on-chain.
- A per-asset circuit breaker on the synthetic price, not just the collateral price.
- Reporter staking/slashing, so confidence is economically backed rather than declared.

**Implementation gap.** `collateral_to_liquidator` and `collateral_returned` are
constructed as empty maps with the comment
`// In production, handle actual collateral transfers`. As with the stablecoin,
liquidation economics are not yet live.

### 3.3 Systemic risk

Oracle error propagates to every position of an asset simultaneously — the
correlation across positions is 1. Combined with a global `max_total_debt` and no
per-asset cap, a single bad feed on the largest listed asset is a
protocol-solvency event rather than a position-level one.

---

## 4. Yield Vault

From [vault.rs:29](../src/contracts/vault.rs#L29):

| Constant | Value | Meaning |
|----------|-------|---------|
| `DEFAULT_PERFORMANCE_FEE_BPS` | `1000` | 10 % of harvested rewards |
| `MAX_PERFORMANCE_FEE_BPS` | `3000` | 30 % ceiling |
| `MIN_HARVEST_INTERVAL` | `3600` | 1 hour between harvests |

### 4.1 Strategy risk

`VaultStrategy.estimated_apy` is an `f64` supplied by whoever registers the
strategy ([types/vault.rs](../src/types/vault.rs)). `get_optimal_strategy_index`
and `optimize_strategy` ([vault.rs:156](../src/contracts/vault.rs#L156)) select
the active strategy purely by comparing these declared numbers — nothing measures
realised yield.

This makes strategy selection an **admin-trust problem, not an optimisation
problem**. An admin (or a compromised admin key) can route the entire vault into
a chosen strategy by declaring an implausible APY, and the vault will comply.
`optimize_strategy` compounds this by requiring only a relative improvement of
`threshold_bps` over the current declared APY.

Additional strategy exposures, none of which the vault models:

- **Counterparty risk.** `contract_address` is an arbitrary external contract.
  Vault assets are deposited into it with no allowlist, no cap, and no
  per-strategy exposure limit.
- **Correlated failure.** Nothing prevents several registered strategies from
  routing to the same underlying protocol, so "diversification" across strategies
  may be illusory.
- **Reward denomination.** `harvest` treats `raw_rewards` as being denominated in
  the vault asset and compounds them directly. A strategy paying rewards in a
  separate token requires a swap the vault does not perform, so `total_assets`
  and therefore share price would be overstated.

Recommended controls: governance-gated strategy registration, a per-strategy
allocation cap, an on-chain realised-yield measurement (delta of `total_assets`
across harvests) used in place of `estimated_apy`, and a timelock on
`switch_strategy`.

### 4.2 Yield sustainability

Net yield to depositors is `gross_strategy_yield × (1 − 0.10)` at the default
fee. Two structural questions the vault cannot currently answer:

1. **Is the yield real or emissions-funded?** Strategy APY carries no
   decomposition into fee revenue versus token emissions. Emissions-funded yield
   decays as TVL grows; fee-funded yield does not. Without the distinction, the
   vault's declared APY has no predictive value.
2. **Does the fee survive a low-yield regime?** A 10 % performance fee on a
   harvest of zero is zero, so the vault has no fixed-cost drag — good. But
   `harvest` returns early on `raw_rewards == 0` without updating `last_harvest`,
   so an unproductive strategy can be re-probed every block, wasting fees.

### 4.3 Harvest-sandwich risk

Shares are priced as `total_assets / total_shares`. `harvest`
([vault.rs:288](../src/contracts/vault.rs#L288)) adds an hour or more of accrued
rewards to `total_assets` in a single transaction, stepping the share price
discontinuously. Because `MIN_HARVEST_INTERVAL` is a fixed 3 600 s, the earliest
harvest time is exactly predictable from `last_harvest`.

An attacker deposits immediately before the harvest transaction and withdraws
immediately after, capturing a pro-rata share of an hour's yield earned entirely
by pre-existing depositors. Profit is `deposit_share × net_rewards`, and the only
cost is transaction fees — there is no deposit fee, no withdrawal fee, and no
lock-up. The attack is capital-intensive but risk-free, and it scales with the
attacker's balance.

Standard mitigations: stream rewards linearly over a release window rather than
booking them instantly, charge a withdrawal fee that decays with holding time, or
enforce a minimum holding period before withdrawal.

### 4.4 Systemic risk

The vault is a leaf node — it consumes other protocols but nothing consumes it —
so its failure does not propagate into lending or the stablecoin. Its own
exposure is the union of every registered strategy's risk, unbounded by any cap.

---

## 5. Circuit Breaker

Threshold-by-threshold derivation lives in
[circuit_breaker_guide.md](./circuit_breaker_guide.md#threshold-justification-and-risk-analysis);
this section covers its economic interactions.

### 5.1 What it costs to trip

A trip freezes price reads for the asset. Every downstream operation that needs a
price — liquidation above all — reverts for at least
`CIRCUIT_BREAKER_COOLDOWN = 1800 s` plus operator response time.

The expected cost of a *false* trip is the additional collateral drawdown over
the freeze window. For collateral with 80 % annualised volatility, the 1-σ move
over 30 minutes is roughly 1.7 %; a 3-σ adverse move is ~5 %. Against a lending
reserve holding a 16 % cushion between `HF = 1` and the toxic zone (§1.2), a
single false trip is survivable. Two consecutive freezes, or one freeze during a
genuine 20 % move, are not.

### 5.2 What it costs to attack

The breaker is a denial-of-service surface: an attacker who can move the
*reported* price 10 % once, or 5 % three times, halts the protocol for 30 minutes
at will. The cost of doing so is entirely a function of the oracle's aggregation:

| Oracle design | Cost to force a trip |
|---------------|----------------------|
| Single reporter | Compromise of one key — effectively free |
| N-of-M median, no staking | Compromise of ⌈M/2⌉ keys |
| N-of-M median with slashing | Compromise cost + slashed stake |

The breaker's value is therefore bounded above by the oracle's own security. It
protects against *faults*; it does not protect against an adversary who controls
the feed, and against such an adversary it hands them a pause button.

### 5.3 Conflict with liquidation — the central systemic finding

Consider a 27 % collateral crash, which is exactly the drawdown that exhausts the
stablecoin's 150 % default ratio:

```
t+00:00  price 100    baseline
t+05:00  price  94    −6.0 %   consecutive counter = 1, warning fires
t+10:00  price  88    −6.4 %   consecutive counter = 2
t+15:00  price  83    −5.7 %   counter = 3 → CIRCUIT BREAKER TRIPS
t+15:00  ─────────────────────  all price reads revert; liquidations impossible
t+45:00  cooldown ends; true market price is now 68
t+45:00+ operator resets; vaults are re-priced 32 % below the frozen value
```

Between t+15 and t+45 the protocol is blind. Vaults that were at 118 % when the
breaker tripped re-emerge at 80 % — deeply insolvent, with liquidation now
value-destructive under the toxic-zone identity. The breaker converts a
liquidatable loss into bad debt.

This is not a bug in the breaker; it is the intended trade-off (halt rather than
act on a possibly-false price) meeting a liquidation engine that has no
alternative mode. The resolution is not to remove the breaker but to give
liquidation a degraded path:

1. **Liquidation-only mode.** During a trip, permit liquidation against the last
   known-good price with an enlarged bonus, while continuing to block minting,
   borrowing and redemption. Losses are bounded by the staleness of the frozen
   price rather than by the full crash.
2. **Asymmetric thresholds.** Trip readily on upward moves (which inflate
   collateral and enable over-borrowing) and reluctantly on downward moves
   (where freezing defers a loss that only grows).
3. **Graduated recovery.** `RECOVERY_MODE_DURATION = 3600 s` with
   `RECOVERY_MAX_CHANGE_BPS = 200` permits at most ~27 % of re-convergence in the
   first hour after reset (12 updates × 2 %, compounded). A crash larger than
   that leaves the oracle knowingly stale for more than an hour — during which
   liquidations execute at prices above the true market, transferring the
   shortfall to the protocol. For moves beyond 27 %, operators must widen
   `RECOVERY_MAX_CHANGE_BPS` by governance rather than let the ramp run.

### 5.4 Threshold sizing summary

| Threshold | Value | Economic justification |
|-----------|-------|------------------------|
| `CIRCUIT_BREAKER_THRESHOLD_BPS` | 1000 (10 %) | ≈ 13–20 σ for a 5-minute XLM bar; effectively unreachable organically, so a trip is near-certain evidence of a feed fault or manipulation |
| `MIN_CONSECUTIVE_DEVIATION_BPS` | 500 (5 %) | ≈ 7–10 σ per leg; individually plausible in a real crash, collectively not |
| `CONSECUTIVE_DEVIATION_THRESHOLD` | 3 | Three legs ≈ 15.8 % cumulative over ≥15 min — catches sustained moves the single-update rule misses |
| `MIN_UPDATE_INTERVAL` | 300 s | Caps pre-trip price velocity at 10 % per 5 min; also bounds oracle lag, so liquidations may use a price up to 5 min stale |
| `WARNING_THRESHOLD_BPS` | 300 (3 %) | ≈ 4–6 σ; the paging threshold, giving operators ~10 min of lead time before a trip |
| `CIRCUIT_BREAKER_COOLDOWN` | 1800 s | Matches a 15-minute page-plus-triage cycle with margin; short enough that the drawdown in §5.1 stays inside the lending cushion |
| `RECOVERY_MODE_DURATION` | 3600 s | One hour of constrained updates after reset |
| `RECOVERY_MAX_CHANGE_BPS` | 200 (2 %) | Limits post-reset re-convergence to ~27 % per hour (see §5.3) |

---

## 6. Cross-module systemic risk

### 6.1 Shared dependencies

Every module ultimately depends on the price oracle, and the circuit breaker
gates the oracle. A single oracle fault therefore propagates to lending,
stablecoin and synthetics simultaneously — their risks are correlated, not
independent, and cannot be netted.

```
                    ┌─────────────────┐
                    │ Circuit Breaker │
                    └────────┬────────┘
                             │ gates
                    ┌────────▼────────┐
                    │  Price Oracle   │
                    └────────┬────────┘
          ┌──────────────────┼──────────────────┐
          ▼                  ▼                  ▼
      Lending           Stablecoin          Synthetic
          │                  │                  │
          └──────────► shared collateral ◄──────┘
                             │
                          Vault (consumer)
```

### 6.2 Ranked risk register

| # | Risk | Module | Likelihood | Impact | Status |
|---|------|--------|-----------|--------|--------|
| 1 | Liquidation payout paths are stubs | Stablecoin, Synthetic | Certain today | Critical | **Open — blocks mainnet** |
| 2 | Synthetic liquidator takes 90 % of collateral regardless of debt | Synthetic | Certain on liquidation | Critical | **Open** |
| 3 | Stablecoin liquidation cannot restore vault health (`penalty == MCR`) | Stablecoin | Certain on liquidation | High | **Open** |
| 4 | No redemption backstop → no hard peg floor | Stablecoin | High | High | **Open — design** |
| 5 | Circuit breaker freezes liquidations during a crash | Cross-module | Medium | High | **Open — design** |
| 6 | High utilisation blocks liquidation for lack of cash | Lending | Medium | High | Mitigable via `borrow_cap` |
| 7 | Vault strategy chosen from unverified declared APY | Vault | Medium | High | **Open** |
| 8 | Harvest-sandwich extracts yield from existing depositors | Vault | Medium | Medium | **Open** |
| 9 | Slope-2 rate spike forces price-independent liquidations | Lending | Medium | Medium | Accepted; monitor |
| 10 | 1-hour oracle staleness window | Lending | Medium | Medium | Tune per asset |
| 11 | Self-attested oracle confidence | Synthetic | Low | High | Needs staked reporters |

### 6.3 Pre-mainnet gate

Items 1–3 are correctness defects with direct economic consequences and must be
closed and tested before any mainnet deployment. Items 4, 5 and 7 are design
decisions that require an explicit, documented governance ruling — either accept
them with published limits, or change the design. See
[deployment_guide.md](./deployment_guide.md#mainnet-readiness-gate).

### 6.4 Monitoring

| Metric | Source | Alert |
|--------|--------|-------|
| Reserve utilisation | `total_debt / net_assets` | Warn 90 %, page 95 % |
| Collateral cash vs largest at-risk seize | `total_cash` vs computed seize | Page when ratio < 2× |
| Positions with `HF ∈ [1.0, 1.05]` | `position()` sweep | Page on any |
| Vaults with `CR ∈ [110 %, 120 %]` | `get_collateral_ratio` | Warn |
| SUSD market price | External venues | Warn ±1 %, page ±3 % |
| Circuit breaker trips | `CB_TRIPPED` events | Page on any |
| Consecutive deviation counter | `CONSEC` storage | Warn at 2 |
| Oracle age per asset | `LAST_UPD` storage | Page at 2× `MIN_UPDATE_INTERVAL` |
| Vault share price | `get_share_price()` | Page on any decrease |
| Vault deposit immediately pre-harvest | Deposit events vs `last_harvest` | Warn |

---

## Review cadence

This analysis must be re-run when any constant referenced above changes, when a
new collateral asset or synthetic is listed, and at minimum quarterly. Parameter
changes follow the process in
[upgrade_governance_process.md](./upgrade_governance_process.md).
