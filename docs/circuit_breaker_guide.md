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
