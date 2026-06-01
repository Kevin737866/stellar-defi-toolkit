# Circuit Breaker V2 - Enhanced Price Volatility Protection

## Overview

The Circuit Breaker V2 system provides comprehensive protection against extreme price volatility in DeFi protocols. This enhanced version includes advanced features like recovery mode, warning alerts, global pause, and health scoring.

## Key Features

### 🛡️ Multi-Layer Protection

1. **Single Deviation Protection** (10% threshold)
   - Immediate halt on extreme price movements
   - Prevents flash crash exploitation

2. **Consecutive Deviation Detection** (3x 5% threshold)
   - Identifies sustained volatility trends
   - Catches gradual but dangerous price movements

3. **Rate Limiting** (5-minute minimum interval)
   - Prevents manipulation through rapid updates
   - Ensures price stability between updates

### 🔄 Recovery Mode

- **Automatic Transition**: After 30-minute cooldown
- **Gradual Resumption**: 2% maximum price change during recovery
- **Duration**: 1 hour before returning to normal operations
- **Benefits**: Prevents immediate re-trips, allows market stabilization

### ⚠️ Warning Alert System

- **Early Detection**: Alerts at 3% deviation
- **Three Alert Levels**:
  - **Info**: 3-5% deviation
  - **Warning**: 5%+ deviation (1-2 consecutive)
  - **Critical**: 5%+ deviation (2+ consecutive)
- **Proactive Monitoring**: Enables intervention before circuit breaker trips

### 🚨 Global Pause

- **Emergency Stop**: Halt all assets simultaneously
- **Admin Controlled**: Quick response to system-wide threats
- **Use Cases**:
  - Protocol-wide security incidents
  - Major market disruptions
  - Planned maintenance
  - Oracle failures

### 📊 Health Scoring

- **0-100 Score**: Quantifies asset stability
- **Factors Considered**:
  - Consecutive deviations
  - Trip history
  - Current status
  - Time since last trip
- **Data-Driven Decisions**: Prioritize monitoring and intervention

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Circuit Breaker V2                        │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
│  │   Active     │→ │   Tripped    │→ │  Recovery    │      │
│  │   Status     │  │   Status     │  │    Mode      │      │
│  └──────────────┘  └──────────────┘  └──────────────┘      │
│         ↑                                      │             │
│         └──────────────────────────────────────┘             │
│                                                               │
│  ┌─────────────────────────────────────────────────────┐    │
│  │           Warning Alert System                       │    │
│  │  • Info Alerts (3-5%)                               │    │
│  │  • Warning Alerts (5%+, 1-2 consecutive)            │    │
│  │  • Critical Alerts (5%+, 2+ consecutive)            │    │
│  └─────────────────────────────────────────────────────┘    │
│                                                               │
│  ┌─────────────────────────────────────────────────────┐    │
│  │           Health Scoring Engine                      │    │
│  │  • Deviation tracking                                │    │
│  │  • Trip history analysis                             │    │
│  │  • Time-based recovery                               │    │
│  └─────────────────────────────────────────────────────┘    │
│                                                               │
└─────────────────────────────────────────────────────────────┘
                            │
                            ↓
        ┌───────────────────────────────────────┐
        │      Integrated Protocols              │
        ├───────────────────────────────────────┤
        │  • Price Oracle                        │
        │  • Lending Protocol                    │
        │  • Stablecoin Protocol                 │
        │  • Synthetic Asset Protocol            │
        └───────────────────────────────────────┘
```

## State Machine

```
                    ┌─────────────┐
                    │   ACTIVE    │
                    │  (Normal)   │
                    └──────┬──────┘
                           │
                    Price Deviation
                    ≥ 10% OR 3x 5%
                           │
                           ↓
                    ┌─────────────┐
                    │   TRIPPED   │
                    │  (Halted)   │
                    └──────┬──────┘
                           │
                    Cooldown: 30 min
                           │
                           ↓
                    ┌─────────────┐
                    │  RECOVERY   │
                    │ (Limited)   │
                    └──────┬──────┘
                           │
                    Duration: 1 hour
                           │
                           ↓
                    ┌─────────────┐
                    │   ACTIVE    │
                    │  (Normal)   │
                    └─────────────┘
```

## API Reference

### Core Functions

#### `initialize(admin: Address)`
Initialize the circuit breaker with an admin address.

```rust
circuit_breaker.initialize(admin_address);
```

#### `is_operational(asset_address: Address) -> bool`
Check if operations are allowed for an asset.

```rust
if circuit_breaker.is_operational(asset_address) {
    // Proceed with operation
} else {
    // Circuit breaker tripped - halt operations
}
```

#### `check_price_update(asset_address: Address, old_price: u64, new_price: u64) -> bool`
Validate a price update and potentially trip the circuit breaker.

```rust
let allowed = circuit_breaker.check_price_update(
    asset_address,
    old_price,
    new_price
);

if allowed {
    // Update price
} else {
    // Reject update
}
```

### Monitoring Functions

#### `get_health_score(asset_address: Address) -> u32`
Get the health score (0-100) for an asset.

```rust
let score = circuit_breaker.get_health_score(asset_address);

if score < 70 {
    // Asset health is concerning
    alert_operators();
}
```

#### `get_warning_alerts() -> Vec<WarningAlert>`
Retrieve recent warning alerts.

```rust
let alerts = circuit_breaker.get_warning_alerts();

for alert in alerts {
    match alert.level {
        AlertLevel::Critical => handle_critical_alert(alert),
        AlertLevel::Warning => handle_warning_alert(alert),
        AlertLevel::Info => log_info_alert(alert),
    }
}
```

#### `get_tripped_assets() -> Vec<Address>`
Get all assets with tripped circuit breakers.

```rust
let tripped = circuit_breaker.get_tripped_assets();

for asset in tripped {
    investigate_and_resolve(asset);
}
```

#### `get_recovery_assets() -> Vec<Address>`
Get all assets in recovery mode.

```rust
let recovering = circuit_breaker.get_recovery_assets();

for asset in recovering {
    monitor_closely(asset);
}
```

### Admin Functions

#### `reset(asset_address: Address)`
Reset a tripped circuit breaker (admin only).

```rust
// After investigation and confirmation of price stabilization
circuit_breaker.reset(asset_address);
```

#### `force_recovery(asset_address: Address)`
Force transition to recovery mode (admin only).

```rust
// Manually transition to recovery after investigation
circuit_breaker.force_recovery(asset_address);
```

#### `set_global_pause(paused: bool)`
Enable or disable global pause (admin only).

```rust
// Emergency: pause all operations
circuit_breaker.set_global_pause(true);

// After resolution: resume operations
circuit_breaker.set_global_pause(false);
```

#### `update_config(config: CircuitBreakerConfig)`
Update circuit breaker configuration (admin only).

```rust
let new_config = CircuitBreakerConfig {
    single_deviation_threshold: 1500, // 15%
    consecutive_deviation_count: 4,
    min_consecutive_deviation: 400,   // 4%
    cooldown_period: 2400,            // 40 minutes
    min_update_interval: 600,         // 10 minutes
    recovery_max_change: 300,         // 3%
    enabled: true,
};

circuit_breaker.update_config(new_config);
```

## Integration Guide

### Price Oracle Integration

```rust
impl PriceOracleContract {
    pub fn get_price(env: Env, asset_address: Address) -> OraclePrice {
        // Check circuit breaker before returning price
        if !circuit_breaker.is_operational(env.clone(), asset_address.clone()) {
            panic!("Circuit breaker tripped for asset");
        }
        
        // Return price if operational
        let prices = Self::get_prices(&env);
        prices.get(asset_address).unwrap()
    }
    
    pub fn update_price(
        env: Env,
        asset_address: Address,
        old_price: u64,
        new_price: u64
    ) {
        // Validate price update with circuit breaker
        if !circuit_breaker.check_price_update(
            env.clone(),
            asset_address.clone(),
            old_price,
            new_price
        ) {
            panic!("Price update rejected by circuit breaker");
        }
        
        // Update price
        Self::set_price(&env, asset_address, new_price);
    }
}
```

### Lending Protocol Integration

```rust
impl LendingProtocol {
    pub fn liquidate(
        env: Env,
        borrower: Address,
        collateral_asset: Address
    ) {
        // Check circuit breaker before liquidation
        if !circuit_breaker.is_operational(
            env.clone(),
            collateral_asset.clone()
        ) {
            panic!("Circuit breaker tripped - liquidations halted");
        }
        
        // Proceed with liquidation
        Self::execute_liquidation(&env, borrower, collateral_asset);
    }
}
```

### Stablecoin Protocol Integration

```rust
impl StablecoinProtocol {
    pub fn mint(
        env: Env,
        user: Address,
        collateral_token: Address,
        amount: u64
    ) {
        // Check circuit breaker before minting
        if !circuit_breaker.is_operational(
            env.clone(),
            collateral_token.clone()
        ) {
            panic!("Circuit breaker tripped - minting halted");
        }
        
        // Proceed with minting
        Self::execute_mint(&env, user, collateral_token, amount);
    }
}
```

## Event System

### Events Published

| Event | Data | Description |
|-------|------|-------------|
| `CB_INITIALIZED` | `admin: Address` | Circuit breaker initialized |
| `CB_TRIPPED` | `(old_price, new_price, deviation_bps, reason)` | Circuit breaker tripped |
| `CB_RESET` | `()` | Circuit breaker reset |
| `CB_RECOVERY` | `()` | Transitioned to recovery mode |
| `CB_ACTIVE` | `()` | Returned to active status |
| `CB_WARNING` | `(deviation_bps, consecutive_count, level)` | Warning alert created |
| `RATE_LIMITED` | `time_since_update` | Price update rate limited |
| `RECOVERY_VIOLATION` | `deviation_bps` | Recovery mode limit exceeded |
| `GLOBAL_PAUSE` | `paused: bool` | Global pause state changed |
| `CONFIG_UPDATED` | `enabled: bool` | Configuration updated |
| `WARNINGS_CLEARED` | `()` | Warning alerts cleared |

### Event Monitoring Example

```rust
// Subscribe to circuit breaker events
env.events().subscribe(
    "CB_TRIPPED",
    |event| {
        let (asset, old_price, new_price, deviation, reason) = event.data;
        
        // Alert operators
        send_alert(format!(
            "Circuit breaker tripped for {}: {}% deviation",
            asset,
            deviation / 100
        ));
        
        // Log for analysis
        log_trip_event(asset, old_price, new_price, deviation, reason);
        
        // Initiate investigation
        investigate_price_anomaly(asset);
    }
);

env.events().subscribe(
    "CB_WARNING",
    |event| {
        let (deviation, consecutive, level) = event.data;
        
        if level == AlertLevel::Critical {
            // Proactive intervention
            alert_operators_critical(deviation, consecutive);
        }
    }
);
```

## Configuration

### Default Configuration

```rust
CircuitBreakerConfig {
    single_deviation_threshold: 1000,      // 10%
    consecutive_deviation_count: 3,        // 3 updates
    min_consecutive_deviation: 500,        // 5%
    cooldown_period: 1800,                 // 30 minutes
    min_update_interval: 300,              // 5 minutes
    recovery_max_change: 200,              // 2%
    enabled: true,
}
```

### Recommended Configurations

#### Conservative (Low Risk Tolerance)
```rust
CircuitBreakerConfig {
    single_deviation_threshold: 500,       // 5%
    consecutive_deviation_count: 2,        // 2 updates
    min_consecutive_deviation: 300,        // 3%
    cooldown_period: 3600,                 // 1 hour
    min_update_interval: 600,              // 10 minutes
    recovery_max_change: 100,              // 1%
    enabled: true,
}
```

#### Moderate (Balanced)
```rust
CircuitBreakerConfig {
    single_deviation_threshold: 1000,      // 10%
    consecutive_deviation_count: 3,        // 3 updates
    min_consecutive_deviation: 500,        // 5%
    cooldown_period: 1800,                 // 30 minutes
    min_update_interval: 300,              // 5 minutes
    recovery_max_change: 200,              // 2%
    enabled: true,
}
```

#### Aggressive (High Risk Tolerance)
```rust
CircuitBreakerConfig {
    single_deviation_threshold: 1500,      // 15%
    consecutive_deviation_count: 4,        // 4 updates
    min_consecutive_deviation: 700,        // 7%
    cooldown_period: 900,                  // 15 minutes
    min_update_interval: 180,              // 3 minutes
    recovery_max_change: 300,              // 3%
    enabled: true,
}
```

## Monitoring Dashboard

### Key Metrics to Track

1. **Asset Health Scores**
   - Real-time health score for each asset
   - Alert when score < 70
   - Critical alert when score < 50

2. **Active Trips**
   - Number of tripped assets
   - Time since trip
   - Trip reason

3. **Recovery Status**
   - Assets in recovery mode
   - Recovery progress
   - Time remaining

4. **Warning Alerts**
   - Recent warnings (last 24 hours)
   - Alert level distribution
   - Assets with multiple warnings

5. **Trip History**
   - Trip frequency by asset
   - Common trip reasons
   - Time-of-day patterns

### Sample Dashboard Layout

```
┌─────────────────────────────────────────────────────────────┐
│              Circuit Breaker Dashboard                       │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  System Status: ● OPERATIONAL    Global Pause: ○ OFF        │
│                                                               │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  Asset Health Scores                                  │   │
│  │  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  │   │
│  │  BTC/USD:  ████████████████████░░  95  ✓             │   │
│  │  ETH/USD:  ████████████████░░░░░░  80  ⚠             │   │
│  │  XLM/USD:  ████████░░░░░░░░░░░░░░  45  ⚠⚠           │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                               │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  Active Trips: 1                                      │   │
│  │  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  │   │
│  │  • XLM/USD - Tripped 15m ago (SINGLE_DEV)            │   │
│  │    Cooldown: 15m remaining                            │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                               │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  Recovery Mode: 0 assets                              │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                               │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  Recent Warnings (24h): 12                            │   │
│  │  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  │   │
│  │  Critical: 2  │  Warning: 5  │  Info: 5              │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                               │
└─────────────────────────────────────────────────────────────┘
```

## Operational Procedures

### Incident Response Playbook

#### 1. Circuit Breaker Trip

**Immediate Actions:**
1. Acknowledge alert
2. Check trip reason and deviation
3. Verify price data from multiple sources
4. Assess market conditions

**Investigation:**
1. Review trip history for pattern
2. Check oracle health
3. Analyze price deviation cause
4. Verify no manipulation attempt

**Resolution:**
1. If legitimate volatility: Wait for cooldown
2. If oracle issue: Fix oracle, then reset
3. If manipulation: Investigate further, consider manual intervention
4. Document incident

#### 2. Multiple Consecutive Warnings

**Immediate Actions:**
1. Monitor asset closely
2. Check health score
3. Review price trend

**Preventive Measures:**
1. Prepare for potential trip
2. Notify stakeholders
3. Consider proactive intervention

#### 3. Global Emergency

**Immediate Actions:**
1. Activate global pause
2. Assess threat scope
3. Notify all stakeholders

**Investigation:**
1. Identify root cause
2. Assess impact
3. Develop resolution plan

**Resolution:**
1. Implement fixes
2. Test thoroughly
3. Gradual resumption
4. Post-mortem analysis

## Best Practices

### For Protocol Operators

1. **Monitor Continuously**
   - Set up 24/7 monitoring
   - Configure alerts for all severity levels
   - Review dashboard regularly

2. **Respond Quickly**
   - Acknowledge alerts immediately
   - Follow incident response playbook
   - Document all actions

3. **Investigate Thoroughly**
   - Never reset without investigation
   - Verify price data from multiple sources
   - Look for patterns in trip history

4. **Communicate Clearly**
   - Notify users of circuit breaker events
   - Provide status updates
   - Explain resolution timeline

5. **Learn and Improve**
   - Conduct post-mortems
   - Update procedures based on learnings
   - Adjust thresholds if needed

### For Integrators

1. **Handle Gracefully**
   - Wrap oracle calls in error handling
   - Provide clear user feedback
   - Implement retry logic with backoff

2. **Check Before Operations**
   - Always call `is_operational()` before critical operations
   - Handle tripped state appropriately
   - Don't assume operational status

3. **Monitor Health**
   - Track health scores for relevant assets
   - Alert users of degraded health
   - Adjust risk parameters accordingly

4. **Test Thoroughly**
   - Test circuit breaker integration
   - Simulate trip scenarios
   - Verify error handling

### For Users

1. **Understand Protection**
   - Circuit breakers protect your assets
   - Temporary halts prevent larger losses
   - Operations resume after stabilization

2. **Monitor Status**
   - Check circuit breaker status before large operations
   - Review health scores for assets
   - Be aware of recovery mode restrictions

3. **Plan Accordingly**
   - Avoid large operations during high volatility
   - Be patient during circuit breaker events
   - Trust the protection mechanism

## Troubleshooting

### Common Issues

#### Issue: Circuit Breaker Won't Reset

**Symptoms:**
- Admin calls reset but circuit breaker remains tripped
- `is_operational()` still returns false

**Possible Causes:**
1. Admin authentication failure
2. Circuit breaker disabled
3. Global pause active

**Solutions:**
1. Verify admin credentials
2. Check circuit breaker enabled status
3. Check global pause status
4. Review transaction logs

#### Issue: Frequent False Positives

**Symptoms:**
- Circuit breaker trips too often
- Normal volatility triggers trips

**Possible Causes:**
1. Thresholds too conservative
2. Poor oracle data quality
3. High market volatility

**Solutions:**
1. Review and adjust thresholds
2. Improve oracle data sources
3. Consider asset-specific configuration
4. Analyze trip patterns

#### Issue: Operations Halted Unexpectedly

**Symptoms:**
- Operations stop without clear trip event
- No trip in history

**Possible Causes:**
1. Global pause active
2. Circuit breaker disabled
3. Rate limiting

**Solutions:**
1. Check global pause status
2. Verify circuit breaker enabled
3. Check last update timestamp
4. Review event logs

## Security Considerations

### Admin Key Security

- **Use Multi-Signature Wallet**: Require multiple approvals for admin actions
- **Implement Timelock**: Delay configuration changes for review
- **Rotate Keys Regularly**: Update admin keys periodically
- **Secure Storage**: Use hardware wallets for admin keys

### Attack Vector Mitigation

1. **Price Manipulation**
   - Rate limiting prevents rapid manipulation
   - Multiple oracle sources reduce single-point failure
   - Circuit breaker halts operations during anomalies

2. **Oracle Attacks**
   - Circuit breaker provides defense-in-depth
   - Warning system enables early detection
   - Manual intervention capability

3. **Flash Loan Attacks**
   - Single deviation threshold catches flash crashes
   - Rate limiting prevents exploitation
   - Recovery mode prevents immediate re-exploitation

4. **Denial of Service**
   - Monitor for malicious trip patterns
   - Implement rate limiting on admin functions
   - Global pause for emergency response

## Performance Optimization

### Gas Optimization

- Minimal storage reads/writes
- Early returns for disabled state
- Efficient state lookups
- Batch event publishing

### Storage Optimization

- Limited history (100 trips, 50 warnings)
- Automatic cleanup of old entries
- Efficient data structures
- Compressed state representation

## Changelog

### Version 2.0 (Current)

**New Features:**
- Recovery mode with gradual resumption
- Warning alert system with three severity levels
- Global pause mechanism
- Health scoring (0-100)
- Enhanced monitoring functions
- Trip count tracking
- Force recovery admin function

**Improvements:**
- Better state management
- Enhanced event system
- Improved documentation
- Comprehensive test suite

**Bug Fixes:**
- Fixed state transition edge cases
- Corrected deviation calculation
- Improved error handling

### Version 1.0

**Initial Features:**
- Single deviation detection
- Consecutive deviation detection
- Rate limiting
- Basic trip/reset functionality
- Admin controls

## Support and Resources

### Documentation
- [Circuit Breaker Guide](./circuit_breaker_guide.md)
- [Risk Management Framework](./synthetic_protocol_risk_management.md)
- [API Reference](./api_reference.md)

### Community
- Discord: [Join our community](https://discord.gg/stellar-defi-toolkit)
- Forum: [Discussion board](https://forum.stellar-defi-toolkit.com)
- GitHub: [Report issues](https://github.com/frankosakwe/stellar-defi-toolkit/issues)

### Contact
- Email: support@stellar-defi-toolkit.com
- Twitter: [@stellardefi](https://twitter.com/stellardefi)

---

**Version**: 2.0  
**Last Updated**: 2026-06-01  
**Status**: Production Ready (Pending Testing)
