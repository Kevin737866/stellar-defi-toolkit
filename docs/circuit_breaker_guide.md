# Circuit Breaker Guide

## Overview

The Circuit Breaker system provides automatic protection against extreme price volatility in the Stellar DeFi Toolkit. When prices move too rapidly, the circuit breaker automatically halts operations to protect users and the protocol from potential manipulation or extreme market conditions.

## Features

### Automatic Protection

- **Single-Update Threshold**: Automatically trips when price changes by 10% or more in a single update
- **Consecutive Deviation Detection**: Trips after 3 consecutive price updates with 5%+ deviation
- **Rate Limiting**: Enforces minimum 5-minute intervals between price updates
- **Per-Asset Protection**: Each asset has independent circuit breaker status

### Safety Mechanisms

1. **Immediate Halt**: Operations stop immediately when circuit breaker trips
2. **Price Freeze**: Last safe price is preserved
3. **Event Logging**: All trips are logged with full context
4. **Admin Controls**: Authorized admins can reset or disable circuit breakers

## How It Works

### Normal Operation

```
Price Update → Deviation Check → Update Allowed
                     ↓
              (< 5% deviation)
                     ↓
              Reset consecutive counter
```

### Circuit Breaker Trip

```
Price Update → Deviation Check → Circuit Breaker Trips
                     ↓
              (≥ 10% single update OR
               3 consecutive ≥ 5% updates)
                     ↓
              Operations Halted
              Event Published
              State Saved
```

## Thresholds

| Threshold | Value | Description |
|-----------|-------|-------------|
| Single Deviation | 10% (1000 bps) | Immediate trip on single large move |
| Consecutive Deviation | 5% (500 bps) | Threshold for consecutive counter |
| Consecutive Count | 3 updates | Number of consecutive deviations to trip |
| Rate Limit | 5 minutes | Minimum time between price updates |
| Cooldown Period | 30 minutes | Time before automatic recovery |

## Integration with Contracts

### Price Oracle

The price oracle automatically checks circuit breaker status before returning prices:

```rust
pub fn get_price(env: Env, asset_address: Address) -> OraclePrice {
    // Check circuit breaker status
    if !Self::is_operational(&env, asset_address.clone()) {
        panic!("Circuit breaker tripped for asset");
    }
    
    // Return price if operational
    let prices = Self::get_prices(&env);
    prices.get(asset_address.clone())
        .unwrap_or_else(|| panic!("Price not available for asset"))
}
```

### Dependent Contracts

Contracts that use price data (lending, stablecoin, synthetic protocol) should check operational status before critical operations:

```rust
// Check if oracle is operational before liquidation
if !price_oracle.is_operational(env.clone(), asset_address.clone()) {
    panic!("Oracle circuit breaker tripped - operations halted");
}
```

## Admin Functions

### Reset Circuit Breaker

Manually reset a tripped circuit breaker:

```rust
price_oracle.reset_circuit_breaker(env, asset_address);
```

### Enable/Disable Circuit Breaker

Toggle circuit breaker functionality:

```rust
// Disable circuit breaker
price_oracle.set_circuit_breaker_enabled(env, false);

// Enable circuit breaker
price_oracle.set_circuit_breaker_enabled(env, true);
```

### Check Status

Query circuit breaker status for an asset:

```rust
let status = price_oracle.get_circuit_breaker_status(env, asset_address);
match status {
    Some(state) => {
        match state.status {
            CircuitBreakerStatus::Active => println!("Operational"),
            CircuitBreakerStatus::Tripped => println!("Tripped at {}", state.tripped_at),
        }
    },
    None => println!("No circuit breaker state"),
}
```

## Events

### Circuit Breaker Tripped

Published when circuit breaker trips:

```rust
Event: ("CB_TRIPPED", asset_address)
Data: (old_price, new_price, deviation_bps)
```

### Circuit Breaker Reset

Published when circuit breaker is reset:

```rust
Event: ("CB_RESET", asset_address)
Data: ()
```

### Rate Limited

Published when price update is rate limited:

```rust
Event: ("RATE_LIMITED", asset_address)
Data: time_since_last_update
```

## Best Practices

### For Protocol Operators

1. **Monitor Events**: Set up monitoring for circuit breaker trip events
2. **Investigate Trips**: Always investigate why a circuit breaker tripped before resetting
3. **Gradual Recovery**: After reset, monitor closely for additional volatility
4. **Communication**: Notify users when circuit breakers trip

### For Integrators

1. **Handle Panics**: Wrap oracle calls in error handling to gracefully handle circuit breaker trips
2. **Check Status**: Use `is_operational()` before critical operations
3. **User Feedback**: Inform users when operations are halted due to circuit breaker
4. **Retry Logic**: Implement appropriate retry logic with backoff

### For Users

1. **Understand Protection**: Circuit breakers protect you from extreme volatility
2. **Wait for Reset**: Operations resume after admin review and reset
3. **Monitor Status**: Check circuit breaker status before large operations

## Example Scenarios

### Scenario 1: Flash Crash Protection

```
Time 0:00 - Price: $100
Time 0:05 - Price: $95 (5% drop) - Alert, counter = 1
Time 0:10 - Price: $90 (5.3% drop) - Alert, counter = 2
Time 0:15 - Price: $85 (5.6% drop) - CIRCUIT BREAKER TRIPS
Time 0:15+ - All operations halted
Time 0:45 - Admin investigates and resets
Time 0:45+ - Operations resume
```

### Scenario 2: Single Large Move

```
Time 0:00 - Price: $100
Time 0:05 - Price: $88 (12% drop) - CIRCUIT BREAKER TRIPS IMMEDIATELY
Time 0:05+ - All operations halted
Time 0:35 - Cooldown period ends
Time 0:35 - Admin reviews and resets
Time 0:35+ - Operations resume
```

### Scenario 3: Normal Volatility

```
Time 0:00 - Price: $100
Time 0:05 - Price: $103 (3% increase) - Normal operation
Time 0:10 - Price: $101 (1.9% decrease) - Normal operation
Time 0:15 - Price: $104 (3% increase) - Normal operation
All operations continue normally
```

## Configuration

Circuit breaker parameters can be adjusted by modifying constants in `circuit_breaker.rs`:

```rust
/// Circuit breaker triggers at 10% single-update deviation
const CIRCUIT_BREAKER_THRESHOLD_BPS: u32 = 1000;

/// Circuit breaker triggers after 3 consecutive deviations > 5%
const CONSECUTIVE_DEVIATION_THRESHOLD: u32 = 3;

/// Minimum deviation to count as consecutive (5%)
const MIN_CONSECUTIVE_DEVIATION_BPS: u32 = 500;

/// Cooldown period after circuit breaker trips (30 minutes)
const CIRCUIT_BREAKER_COOLDOWN: u64 = 1800;

/// Minimum time between price updates (5 minutes)
const MIN_UPDATE_INTERVAL: u64 = 300;
```

## Threshold Justification and Risk Analysis

Every threshold below is a trade-off between two failure modes: tripping on a
real price move (which freezes liquidations while losses accumulate) and failing
to trip on a manipulated one (which lets a bad price drive liquidations and
minting). This section states why each value was chosen, what it costs when it
is wrong, and when to change it. Cross-module context is in
[economic_risk_analysis.md](./economic_risk_analysis.md).

### Complete parameter set

All values from [circuit_breaker.rs:19](../src/contracts/circuit_breaker.rs#L19):

| Constant | Value | Meaning |
|----------|-------|---------|
| `CIRCUIT_BREAKER_THRESHOLD_BPS` | 1000 | 10 % single-update trip |
| `CONSECUTIVE_DEVIATION_THRESHOLD` | 3 | Consecutive deviations to trip |
| `MIN_CONSECUTIVE_DEVIATION_BPS` | 500 | 5 % counts toward the consecutive counter |
| `WARNING_THRESHOLD_BPS` | 300 | 3 % raises a warning alert |
| `MIN_UPDATE_INTERVAL` | 300 s | Minimum spacing between price updates |
| `CIRCUIT_BREAKER_COOLDOWN` | 1800 s | Minimum freeze duration |
| `RECOVERY_MODE_DURATION` | 3600 s | Constrained-update window after reset |
| `RECOVERY_MAX_CHANGE_BPS` | 200 | 2 % max move per update in recovery |
| `MAX_TRIP_HISTORY` | 100 | Retained trip records |

### Statistical basis

With `MIN_UPDATE_INTERVAL = 300 s`, each observation is a 5-minute bar. There are
105 120 such bars per year. For XLM-class collateral at 60–100 % annualised
volatility, the standard deviation of a 5-minute return is approximately
0.5–0.8 %. Each threshold can therefore be expressed in σ:

| Threshold | Value | Approx. σ | Expected organic frequency |
|-----------|------:|----------:|----------------------------|
| Warning | 3 % | 4–6 σ | A few times per year in stressed markets |
| Consecutive leg | 5 % | 6–10 σ | Rare individually; plausible in a crash |
| Single-update trip | 10 % | 13–20 σ | Effectively never from organic trading |

The 10 % single-update threshold is deliberately set beyond the range that
organic price action reaches in five minutes. **A single-update trip should be
read as near-certain evidence of a feed fault or manipulation, not as a market
move**, and triaged accordingly.

The 3× 5 % consecutive rule covers what the single-update rule misses: a real,
sustained crash arriving as a sequence of individually plausible legs. Three
compounding 5 % legs is a 14.3 % cumulative move over at least 15 minutes.
Requiring three rather than two avoids tripping on the ordinary two-bar
sequences that occur in volatile-but-functioning markets; requiring more than
three would let a 20 %+ move complete before the breaker engages.

### Why 300 s between updates

The rate limit does two things. It caps pre-trip price velocity at 10 % per
5 minutes, bounding how far a manipulated feed can travel before the single-update
rule catches it. And it bounds oracle *lag*: the reported price may be up to
5 minutes stale.

That lag is itself a risk. Liquidations execute against a price up to 5 minutes
old, so in a fast market a liquidator can seize collateral at a stale-favourable
price, or a position can be liquidated on a price the market has already
retraced. Lowering `MIN_UPDATE_INTERVAL` reduces lag but raises trip sensitivity,
because the same absolute move spread over fewer seconds looks larger per bar.
Do not tune it without re-deriving the σ table above.

### Cost of a false trip

A trip freezes price reads, and every downstream operation needing a price —
liquidation above all — reverts for at least the 1800 s cooldown plus operator
response time.

For collateral at 80 % annualised volatility, the 1-σ move over a 30-minute
freeze is roughly 1.7 %; a 3-σ adverse move is about 5 %. A lending reserve
configured with `liquidation_threshold_bps = 8000` and a 5 % liquidation bonus
holds a 16 % cushion between `HF = 1` and the point where liquidation stops
improving the position. **One false trip is comfortably survivable. Two
consecutive freezes, or one freeze during a genuine 20 % move, are not.**

This is why the cooldown is 1800 s and not, say, 4 hours: it is long enough for a
15-minute page plus 15 minutes of triage, and short enough that the expected
drawdown over the freeze stays well inside the protocol's collateral cushion.

### Cost of an attack

The breaker is a denial-of-service surface. An adversary who can move the
*reported* price 10 % once, or 5 % three times, halts the protocol for 30 minutes
on demand. The cost of doing so is set entirely by the oracle's aggregation
design, not by the breaker:

| Oracle design | Cost to force a trip |
|---------------|----------------------|
| Single reporter | Compromise of one key — effectively free |
| N-of-M median, no staking | Compromise of ⌈M/2⌉ keys |
| N-of-M median with slashing | Compromise cost + slashed stake |

**The breaker's value is bounded above by the oracle's own security.** It defends
against faults. Against an adversary who controls the feed it defends against
nothing and hands them a pause button. Deploying the breaker over a
single-reporter oracle makes the system strictly more fragile, not less.

### Recovery mode arithmetic

After a reset, `RECOVERY_MODE_DURATION = 3600 s` and
`RECOVERY_MAX_CHANGE_BPS = 200` allow at most 12 updates of 2 % each, compounding
to roughly 27 % of re-convergence in the first hour.

If the true market moved more than 27 % during the freeze, the oracle remains
knowingly stale for over an hour after reset. During that window liquidations
execute at prices above the true market and the shortfall accrues to the
protocol. **For post-freeze moves beyond ~27 %, operators must widen
`RECOVERY_MAX_CHANGE_BPS` by governance rather than let the ramp run**, accepting
a faster but more honest re-pricing. This decision belongs in the
[emergency runbook](./emergency_response_runbook.md), not in an ad-hoc call.

### The central conflict: the breaker freezes liquidations during a crash

```
t+00:00  price 100    baseline
t+05:00  price  94    −6.0 %   counter = 1, warning fires
t+10:00  price  88    −6.4 %   counter = 2, operators paged
t+15:00  price  83    −5.7 %   counter = 3 → CIRCUIT BREAKER TRIPS
t+15:00  ─────────────────────  price reads revert; liquidation impossible
t+45:00  cooldown ends; true market price is now 68
t+45:00+ reset; positions re-price 32 % below the frozen value
```

Positions that were at 118 % when the breaker tripped re-emerge at ~80 % —
insolvent, and (for the stablecoin) in the band where liquidation actively
lowers the collateral ratio further. **The breaker converts a liquidatable loss
into bad debt.**

This is not a defect in the breaker; it is its intended trade-off — halt rather
than act on a possibly-false price — meeting a liquidation engine that has no
degraded mode. Options, in order of preference:

1. **Liquidation-only mode.** During a trip, allow liquidation against the last
   known-good price with an enlarged bonus, while continuing to block minting,
   borrowing and redemption. Losses are then bounded by the staleness of the
   frozen price rather than by the full crash.
2. **Asymmetric thresholds.** Trip readily on upward moves — which inflate
   collateral values and enable over-borrowing, where being wrong is expensive —
   and reluctantly on downward moves, where freezing defers a loss that only
   grows.
3. **Volatility-scaled thresholds.** Widen the trip threshold when realised
   volatility is already high, so a genuine market-wide crash does not read as a
   feed fault.

### When to change these values

| Symptom | Likely cause | Action |
|---------|--------------|--------|
| Trips during ordinary volatility | Threshold too tight for this asset's σ | Raise per-asset threshold; re-derive the σ table first |
| Trips only on one asset | Thin feed or single reporter | Fix the oracle, not the breaker |
| Real crashes complete before tripping | `CONSECUTIVE_DEVIATION_THRESHOLD` too high | Lower to 2 for high-volatility collateral |
| Bad debt accrues during freezes | Cooldown too long, or no liquidation-only mode | Shorten cooldown; implement option 1 above |
| Repeated trips from the same source | Manipulation attempt | Escalate per the [runbook](./emergency_response_runbook.md); do not simply widen the threshold |

### Risk summary

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Freeze prevents liquidation during a real crash | Medium | **High** | Liquidation-only mode (not yet implemented) |
| DoS via induced trips | Medium | Medium | Depends entirely on oracle aggregation strength |
| Recovery ramp too slow after a large move | Medium | Medium | Governance override of `RECOVERY_MAX_CHANGE_BPS` |
| Stale price within `MIN_UPDATE_INTERVAL` exploited | Low | Medium | Shorten interval for volatile assets |
| Admin key compromise disables the breaker | Low | **High** | Multisig on `set_circuit_breaker_enabled` |
| Threshold miscalibrated for a newly listed asset | Medium | Medium | Derive σ before listing; see the [deployment guide](./deployment_guide.md) |

## Security Considerations

1. **Admin Key Security**: Circuit breaker reset requires admin privileges - protect admin keys
2. **Oracle Security**: Circuit breaker protects against price manipulation but doesn't replace oracle security
3. **Denial of Service**: Malicious actors could attempt to trigger circuit breakers - monitor for patterns
4. **Recovery Process**: Establish clear procedures for investigating and resetting circuit breakers

## Monitoring and Alerts

### Recommended Monitoring

- Circuit breaker trip events
- Consecutive deviation counter increases
- Rate limiting events
- Time since last price update
- Circuit breaker status per asset

### Alert Thresholds

- **Critical**: Circuit breaker trips
- **Warning**: 2 consecutive deviations (approaching trip threshold)
- **Info**: Single deviation > 5%

## Troubleshooting

### Circuit Breaker Won't Reset

**Problem**: Admin calls reset but circuit breaker remains tripped

**Solutions**:
1. Verify admin authentication
2. Check if circuit breaker is enabled
3. Review transaction logs for errors

### Frequent False Positives

**Problem**: Circuit breaker trips too often during normal volatility

**Solutions**:
1. Review threshold settings
2. Consider increasing single deviation threshold
3. Adjust consecutive deviation count
4. Improve oracle data quality

### Operations Halted Unexpectedly

**Problem**: Operations stop without clear circuit breaker trip

**Solutions**:
1. Check circuit breaker status for all relevant assets
2. Review event logs for trip events
3. Verify oracle operational status
4. Check for rate limiting

## Future Enhancements

Potential improvements to the circuit breaker system:

1. **Gradual Recovery Mode**: Allow limited operations with tighter thresholds after cooldown
2. **Dynamic Thresholds**: Adjust thresholds based on historical volatility
3. **Multi-Asset Correlation**: Trip circuit breaker if multiple correlated assets show extreme moves
4. **Automated Recovery**: Automatic reset after extended cooldown with stable prices
5. **Governance Integration**: Allow governance to adjust parameters without code changes

## References

- [Price Oracle Documentation](./price_oracle_guide.md)
- [Risk Management Framework](./synthetic_protocol_risk_management.md)
- [Oracle Manager Guide](./oracle_manager_guide.md)
