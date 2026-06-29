# Circuit Breaker System - Complete Guide

## 📚 Documentation Index

This directory contains comprehensive documentation for the enhanced Circuit Breaker system. Choose the guide that best fits your needs:

### For Developers
- **[Circuit Breaker Enhancements](./CIRCUIT_BREAKER_ENHANCEMENTS.md)** - Technical implementation details, architecture, and integration guide
- **[Circuit Breaker V2 README](./docs/CIRCUIT_BREAKER_V2_README.md)** - Complete API reference, usage examples, and best practices

### For Operators
- **[Operator Quick Reference Guide](./docs/CIRCUIT_BREAKER_OPERATOR_GUIDE.md)** - Day-to-day operations, troubleshooting, and emergency procedures

### For Project Management
- **[Pull Request Summary](./PULL_REQUEST_SUMMARY.md)** - PR details, changes overview, and review checklist
- **[Implementation Summary](./IMPLEMENTATION_SUMMARY.md)** - Project overview, achievements, and next steps

### For Testing
- **[Comprehensive Tests](./tests/circuit_breaker_comprehensive_tests.rs)** - Test suite with 15+ scenarios

## 🚀 Quick Start

### What is the Circuit Breaker?

The Circuit Breaker is a safety mechanism that automatically halts operations when price volatility exceeds safe thresholds. Think of it as an emergency brake for your DeFi protocol.

### Key Features

1. **🛡️ Multi-Layer Protection**
   - Single deviation protection (10% threshold)
   - Consecutive deviation detection (3x 5%)
   - Rate limiting (5-minute intervals)

2. **🔄 Recovery Mode**
   - Automatic transition after cooldown
   - Gradual resumption with restrictions
   - Prevents immediate re-trips

3. **⚠️ Warning Alerts**
   - Early detection at 3% deviation
   - Three severity levels
   - Proactive monitoring

4. **🚨 Global Pause**
   - Emergency stop for all assets
   - System-wide protection
   - Admin-controlled

5. **📊 Health Scoring**
   - 0-100 score per asset
   - Data-driven decisions
   - Predictive maintenance

### How It Works

```
Normal Operation (ACTIVE)
         ↓
Price Deviation Detected
         ↓
    ┌────┴────┐
    │         │
  < 10%     ≥ 10%
    │         │
    ↓         ↓
Continue   TRIP
    ↓         ↓
Monitor   Halt Ops
    ↓         ↓
    │    30 min cooldown
    │         ↓
    │    RECOVERY
    │    (2% max change)
    │         ↓
    │    1 hour duration
    │         ↓
    └────→ ACTIVE
```

## 📖 Documentation Guide

### I want to...

#### ...understand the technical implementation
→ Read [Circuit Breaker Enhancements](./CIRCUIT_BREAKER_ENHANCEMENTS.md)
- Architecture and design decisions
- Integration points with other contracts
- State machine and transitions
- Event system details

#### ...integrate the circuit breaker into my contract
→ Read [Circuit Breaker V2 README](./docs/CIRCUIT_BREAKER_V2_README.md)
- API reference with examples
- Integration patterns
- Best practices
- Configuration options

#### ...operate the circuit breaker system
→ Read [Operator Guide](./docs/CIRCUIT_BREAKER_OPERATOR_GUIDE.md)
- Quick status checks
- Alert response procedures
- Troubleshooting guide
- Emergency procedures

#### ...review the implementation for PR
→ Read [Pull Request Summary](./PULL_REQUEST_SUMMARY.md)
- Changes overview
- Testing status
- Review checklist
- Deployment plan

#### ...understand project status and next steps
→ Read [Implementation Summary](./IMPLEMENTATION_SUMMARY.md)
- What was accomplished
- Current status
- Known limitations
- Next steps

## 🎯 Common Use Cases

### Use Case 1: Check if Operations are Allowed

```rust
// Before any critical operation
if !circuit_breaker.is_operational(env.clone(), asset_address.clone()) {
    panic!("Circuit breaker tripped - operations halted");
}

// Proceed with operation
execute_operation();
```

**Documentation**: [API Reference](./docs/CIRCUIT_BREAKER_V2_README.md#api-reference)

### Use Case 2: Validate Price Update

```rust
// Before updating price
let allowed = circuit_breaker.check_price_update(
    env.clone(),
    asset_address.clone(),
    old_price,
    new_price
);

if !allowed {
    panic!("Price update rejected by circuit breaker");
}

// Update price
update_price(new_price);
```

**Documentation**: [Integration Guide](./CIRCUIT_BREAKER_ENHANCEMENTS.md#integration-points)

### Use Case 3: Monitor Asset Health

```rust
// Get health score
let score = circuit_breaker.get_health_score(env.clone(), asset_address.clone());

if score < 70 {
    // Alert operators
    send_alert("Asset health degraded");
}
```

**Documentation**: [Monitoring Functions](./docs/CIRCUIT_BREAKER_V2_README.md#monitoring-functions)

### Use Case 4: Handle Emergency

```rust
// Emergency: pause all operations
circuit_breaker.set_global_pause(env.clone(), true);

// Investigate and resolve issue
investigate_and_fix();

// Resume operations
circuit_breaker.set_global_pause(env.clone(), false);
```

**Documentation**: [Emergency Procedures](./docs/CIRCUIT_BREAKER_OPERATOR_GUIDE.md#scenario-4-system-wide-emergency)

## 🔍 Feature Comparison

| Feature | V1 | V2 |
|---------|----|----|
| Single Deviation Detection | ✅ | ✅ |
| Consecutive Deviation Detection | ✅ | ✅ |
| Rate Limiting | ✅ | ✅ |
| Recovery Mode | ❌ | ✅ |
| Warning Alerts | ❌ | ✅ |
| Global Pause | ❌ | ✅ |
| Health Scoring | ❌ | ✅ |
| Trip History | ✅ | ✅ (Enhanced) |
| Admin Controls | ✅ | ✅ (Enhanced) |
| Monitoring Functions | Limited | Comprehensive |

## 📊 System Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Circuit Breaker V2                        │
│                                                               │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  Core Protection                                      │   │
│  │  • Single Deviation (10%)                            │   │
│  │  • Consecutive Deviation (3x 5%)                     │   │
│  │  • Rate Limiting (5 min)                             │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                               │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  Recovery System                                      │   │
│  │  • Automatic Transition (30 min cooldown)            │   │
│  │  • Limited Operations (2% max)                       │   │
│  │  • Gradual Resumption (1 hour)                       │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                               │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  Monitoring & Alerts                                  │   │
│  │  • Warning Alerts (3 levels)                         │   │
│  │  • Health Scoring (0-100)                            │   │
│  │  • Trip History                                       │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                               │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  Emergency Controls                                   │   │
│  │  • Global Pause                                       │   │
│  │  • Force Recovery                                     │   │
│  │  • Manual Trip                                        │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                               │
└─────────────────────────────────────────────────────────────┘
                            │
                            ↓
        ┌───────────────────────────────────────┐
        │      Integrated Protocols              │
        │  • Price Oracle                        │
        │  • Lending Protocol                    │
        │  • Stablecoin Protocol                 │
        │  • Synthetic Asset Protocol            │
        └───────────────────────────────────────┘
```

## 🛠️ Development Status

### ✅ Completed
- Core circuit breaker enhancements
- Recovery mode implementation
- Warning alert system
- Global pause mechanism
- Health scoring algorithm
- Comprehensive documentation (2200+ lines)
- Test infrastructure (15+ scenarios)

### 🔄 In Progress
- Soroban SDK compatibility fixes
- Unit test implementation
- Integration test completion

### ⏳ Planned
- Security audit
- Performance optimization
- Testnet deployment
- Mainnet rollout

## 📞 Support and Resources

### Getting Help
- **Documentation Issues**: Open an issue on GitHub
- **Integration Questions**: Check the [Integration Guide](./CIRCUIT_BREAKER_ENHANCEMENTS.md#integration-points)
- **Operational Questions**: See the [Operator Guide](./docs/CIRCUIT_BREAKER_OPERATOR_GUIDE.md)
- **Emergency Support**: Contact via Discord or email

### Community
- **GitHub**: https://github.com/frankosakwe/stellar-defi-toolkit
- **Discord**: [stellar-defi-toolkit]
- **Email**: support@stellar-defi-toolkit.com

### Contributing
We welcome contributions! See [CONTRIBUTING.md](./CONTRIBUTING.md) for guidelines.

## 📝 Quick Reference

### For Developers
```rust
// Check operational status
circuit_breaker.is_operational(env, asset)

// Validate price update
circuit_breaker.check_price_update(env, asset, old_price, new_price)

// Get health score
circuit_breaker.get_health_score(env, asset)
```

### For Operators
```bash
# Check status
stellar-defi-cli cb status --asset <ASSET>

# Get health score
stellar-defi-cli cb health --asset <ASSET>

# Reset after investigation
stellar-defi-cli cb reset --asset <ASSET>

# Emergency pause
stellar-defi-cli cb global-pause --enable
```

### Alert Levels
- 🟢 **Info** (3-5%): Monitor
- 🟡 **Warning** (5%+): Investigate
- 🔴 **Critical** (5%+ consecutive): Immediate action

### Health Scores
- **90-100**: Excellent
- **70-89**: Good
- **50-69**: Fair (close monitoring)
- **0-49**: Critical (immediate attention)

## 🎓 Learning Path

### Beginner
1. Read this README
2. Review [Circuit Breaker V2 README](./docs/CIRCUIT_BREAKER_V2_README.md) - Overview section
3. Try the Quick Start examples

### Intermediate
1. Study [Circuit Breaker Enhancements](./CIRCUIT_BREAKER_ENHANCEMENTS.md)
2. Review integration examples
3. Implement basic integration

### Advanced
1. Deep dive into state machine and transitions
2. Review [Operator Guide](./docs/CIRCUIT_BREAKER_OPERATOR_GUIDE.md)
3. Implement custom monitoring
4. Contribute to the project

## 📄 License

This project is licensed under either of:
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

## 🙏 Acknowledgments

- Stellar Development Foundation for the Soroban platform
- Community members for feedback and requirements
- Security researchers for design review
- All contributors to the project

---

**Version**: 2.0  
**Status**: Implementation Complete - Testing Required  
**Last Updated**: June 1, 2026

**Ready to get started?** Choose your documentation from the index above! 🚀
