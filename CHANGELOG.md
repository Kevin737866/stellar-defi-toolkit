# Changelog

All notable changes to the Stellar DeFi Toolkit will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added - Circuit Breaker System

#### New Features
- **Circuit Breaker Contract** (`src/contracts/circuit_breaker.rs`)
  - Automatic protection against extreme price volatility
  - Configurable thresholds for single and consecutive deviations
  - Per-asset circuit breaker status tracking
  - Rate limiting on price updates (5-minute minimum interval)
  - Admin controls for manual trip, reset, and configuration
  - Comprehensive event logging for all circuit breaker actions

- **Enhanced Price Oracle** (`src/contracts/price_oracle.rs`)
  - Integrated circuit breaker checks in price aggregation
  - Automatic halt on 10% single-update price deviation
  - Consecutive deviation tracking (trips after 3x 5%+ moves)
  - `is_operational()` method to check circuit breaker status
  - `get_circuit_breaker_status()` for detailed state queries
  - `reset_circuit_breaker()` admin function
  - `set_circuit_breaker_enabled()` to toggle protection

#### Protection Mechanisms
- **Single Deviation Threshold**: 10% (1000 basis points)
  - Immediate circuit breaker trip on extreme price moves
  - Prevents flash crash liquidations
  
- **Consecutive Deviation Detection**: 5% (500 basis points)
  - Tracks consecutive price movements above threshold
  - Trips after 3 consecutive deviations
  - Protects against sustained volatility attacks

- **Rate Limiting**: 5-minute minimum between updates
  - Prevents rapid price manipulation
  - Ensures time for validation between updates

- **Cooldown Period**: 30 minutes
  - Mandatory wait time after circuit breaker trips
  - Allows admin investigation before reset

#### Data Structures
- `CircuitBreakerStatus` enum: Active, Tripped, Recovery states
- `CircuitBreakerState` struct: Tracks status, deviations, timestamps
- `CircuitBreakerConfig` struct: Configurable thresholds and parameters
- `CircuitBreakerTrip` struct: Records trip events with full context

#### Events
- `CB_TRIPPED`: Published when circuit breaker trips
- `CB_RESET`: Published when circuit breaker is reset
- `CB_ENABLED`: Published when circuit breaker is enabled/disabled
- `CB_RECOVERY`: Published when entering recovery mode
- `RATE_LIMITED`: Published when update is rate limited

#### Documentation
- **Circuit Breaker Guide** (`docs/circuit_breaker_guide.md`)
  - Comprehensive usage documentation
  - Integration examples for dependent contracts
  - Admin procedures and best practices
  - Troubleshooting guide
  - Configuration reference

- **Circuit Breaker Demo** (`examples/circuit_breaker_demo.rs`)
  - Demonstrates normal operation scenarios
  - Shows circuit breaker triggering conditions
  - Examples of admin operations
  - Integration patterns

#### Tests
- **Circuit Breaker Test Suite** (`tests/circuit_breaker_tests.rs`)
  - Initialization tests
  - Single deviation trip tests
  - Consecutive deviation trip tests
  - Rate limiting tests
  - Reset functionality tests
  - Operational status tests

#### Security Enhancements
- Prevents liquidations during flash crashes
- Protects against oracle price manipulation
- Reduces systemic risk from cascading failures
- Provides audit trail of price anomalies
- Enables safe recovery after market disruptions

#### Integration Points
- Price oracle automatically checks circuit breaker before returning prices
- Lending protocol protected from volatile price-based liquidations
- Stablecoin minting/burning halted during extreme volatility
- Synthetic protocol operations suspended when circuit breaker trips
- Position manager respects circuit breaker status

#### Admin Functions
```rust
// Reset circuit breaker after investigation
price_oracle.reset_circuit_breaker(env, asset_address);

// Enable/disable circuit breaker
price_oracle.set_circuit_breaker_enabled(env, true);

// Check status
let status = price_oracle.get_circuit_breaker_status(env, asset_address);
```

#### Developer Experience
- Clear panic messages when circuit breaker is tripped
- `is_operational()` method for graceful handling
- Comprehensive event logging for monitoring
- Detailed state information for debugging

### Changed
- **Price Oracle**: Now includes circuit breaker checks in `get_price()`
- **Contracts Module**: Added circuit breaker to exports
- **README**: Updated with circuit breaker documentation
- **Project Structure**: Added circuit breaker module

### Security
- Added automatic protection against extreme price volatility
- Implemented rate limiting to prevent rapid price manipulation
- Added consecutive deviation detection for sustained attacks
- Provided admin controls with proper authorization checks

## [0.1.0] - Previous Release

### Added
- Token contracts with ERC-20-like functionality
- Liquidity pools with AMM functionality
- Lending and borrowing protocol
- Staking contracts with reward distribution
- Governance contracts
- Oracle system with multi-source aggregation
- CLI tools for contract deployment
- Comprehensive testing suite

---

## Notes

### Circuit Breaker Design Decisions

1. **Why 10% single deviation threshold?**
   - Balances protection with normal market volatility
   - Prevents false positives during legitimate price discovery
   - Aligned with traditional finance circuit breaker standards

2. **Why 3 consecutive deviations?**
   - Distinguishes between volatility and sustained attacks
   - Provides early warning through deviation alerts
   - Allows normal operation during brief volatility spikes

3. **Why 5-minute rate limiting?**
   - Matches oracle heartbeat interval
   - Prevents rapid manipulation attempts
   - Allows timely updates for legitimate price changes

4. **Why 30-minute cooldown?**
   - Provides sufficient time for admin investigation
   - Allows market conditions to stabilize
   - Prevents premature resumption during ongoing volatility

### Future Enhancements

Planned improvements for the circuit breaker system:

- **Gradual Recovery Mode**: Limited operations with tighter thresholds after cooldown
- **Dynamic Thresholds**: Adjust based on historical volatility patterns
- **Multi-Asset Correlation**: Trip if multiple correlated assets show extreme moves
- **Automated Recovery**: Automatic reset after extended stable period
- **Governance Integration**: Community-controlled parameter adjustments
- **Advanced Analytics**: Volatility prediction and risk scoring
- **Cross-Protocol Coordination**: Shared circuit breaker status across protocols

### Migration Guide

For existing deployments:

1. Deploy new circuit breaker contract
2. Update price oracle contract with circuit breaker integration
3. Configure circuit breaker thresholds for each asset
4. Update dependent contracts to check `is_operational()`
5. Set up monitoring for circuit breaker events
6. Document reset procedures for operators
7. Test circuit breaker behavior in staging environment

### Breaking Changes

None - Circuit breaker is additive and can be disabled if needed.

### Deprecations

None

---

For more information, see:
- [Circuit Breaker Guide](docs/circuit_breaker_guide.md)
- [Risk Management Framework](docs/synthetic_protocol_risk_management.md)
- [Contributing Guide](CONTRIBUTING.md)
