# Circuit Breaker Operator Quick Reference Guide

## Quick Status Check

```bash
# Check if asset is operational
stellar-defi-cli circuit-breaker status --asset <ASSET_ADDRESS>

# Get health score
stellar-defi-cli circuit-breaker health --asset <ASSET_ADDRESS>

# List all tripped assets
stellar-defi-cli circuit-breaker list-tripped

# List assets in recovery
stellar-defi-cli circuit-breaker list-recovery
```

## Alert Levels

| Level | Deviation | Action Required |
|-------|-----------|-----------------|
| 🟢 **Info** | 3-5% | Monitor |
| 🟡 **Warning** | 5%+ (1-2 consecutive) | Investigate |
| 🔴 **Critical** | 5%+ (2+ consecutive) | Immediate attention |

## Health Score Interpretation

| Score | Status | Action |
|-------|--------|--------|
| 90-100 | 🟢 Excellent | Normal monitoring |
| 70-89 | 🟡 Good | Increased monitoring |
| 50-69 | 🟠 Fair | Close monitoring, prepare for intervention |
| 30-49 | 🔴 Poor | Immediate investigation required |
| 0-29 | ⚫ Critical | Emergency response |

## Circuit Breaker States

### 🟢 ACTIVE (Normal Operation)
- All operations allowed
- Normal price updates accepted
- Standard monitoring

### 🔴 TRIPPED (Operations Halted)
- All operations blocked
- Cooldown period: 30 minutes
- **Action**: Investigate cause, verify price data

### 🟡 RECOVERY (Limited Operation)
- Operations allowed with restrictions
- Maximum 2% price change per update
- Duration: 1 hour
- **Action**: Monitor closely, ensure stability

## Common Scenarios

### Scenario 1: Flash Crash (Single Large Drop)

**Symptoms:**
- Circuit breaker trips immediately
- Single update with 10%+ deviation
- Trip reason: `SINGLE_DEV`

**Response:**
1. ✅ Acknowledge alert
2. ✅ Verify price from multiple sources
3. ✅ Check if legitimate market movement
4. ✅ Wait for cooldown (30 min)
5. ✅ Monitor recovery mode
6. ✅ Document incident

**Timeline:**
```
T+0:00  - Circuit breaker trips
T+0:05  - Investigation complete
T+0:30  - Cooldown complete → Recovery mode
T+1:30  - Recovery complete → Active
```

### Scenario 2: Gradual Decline (Consecutive Deviations)

**Symptoms:**
- Multiple warning alerts
- 3 consecutive 5%+ deviations
- Trip reason: `CONSEC_DEV`

**Response:**
1. ✅ Review warning alert history
2. ✅ Analyze price trend
3. ✅ Check market conditions
4. ✅ Verify oracle health
5. ✅ Wait for stabilization
6. ✅ Monitor recovery

**Warning Signs:**
- 2 consecutive warnings → Prepare for potential trip
- Health score < 70 → Increased monitoring
- Multiple assets showing warnings → System-wide issue

### Scenario 3: Oracle Malfunction

**Symptoms:**
- Erratic price updates
- Multiple assets affected
- Inconsistent with market data

**Response:**
1. ✅ Activate global pause (if severe)
2. ✅ Investigate oracle system
3. ✅ Fix oracle issues
4. ✅ Reset affected circuit breakers
5. ✅ Resume operations gradually
6. ✅ Post-mortem analysis

### Scenario 4: System-Wide Emergency

**Symptoms:**
- Multiple assets tripping
- Coordinated attack suspected
- Protocol-wide threat

**Response:**
1. ✅ **IMMEDIATE**: Activate global pause
2. ✅ Assemble incident response team
3. ✅ Investigate root cause
4. ✅ Implement fixes
5. ✅ Test thoroughly
6. ✅ Deactivate global pause
7. ✅ Monitor closely
8. ✅ Conduct post-mortem

## Admin Commands

### Reset Circuit Breaker
```bash
# After investigation confirms price stabilization
stellar-defi-cli circuit-breaker reset --asset <ASSET_ADDRESS>
```

**When to use:**
- Investigation complete
- Price confirmed stable
- Root cause identified and resolved

**When NOT to use:**
- Without investigation
- Price still volatile
- Oracle issues unresolved

### Force Recovery
```bash
# Manually transition to recovery mode
stellar-defi-cli circuit-breaker force-recovery --asset <ASSET_ADDRESS>
```

**When to use:**
- After thorough investigation
- Price stabilizing but not fully recovered
- Want gradual resumption

### Global Pause
```bash
# Emergency stop all operations
stellar-defi-cli circuit-breaker global-pause --enable

# Resume operations
stellar-defi-cli circuit-breaker global-pause --disable
```

**When to use:**
- System-wide emergency
- Multiple assets affected
- Protocol-level threat
- Planned maintenance

### Update Configuration
```bash
# Update circuit breaker thresholds
stellar-defi-cli circuit-breaker config update \
  --single-threshold 1000 \
  --consecutive-count 3 \
  --cooldown 1800
```

**When to use:**
- After analyzing trip patterns
- Adjusting for asset characteristics
- Responding to market conditions

## Monitoring Checklist

### Every Hour
- [ ] Check dashboard for active alerts
- [ ] Review health scores for critical assets
- [ ] Verify no tripped assets

### Every 4 Hours
- [ ] Review warning alert history
- [ ] Check for patterns in deviations
- [ ] Verify oracle health

### Daily
- [ ] Review trip history
- [ ] Analyze health score trends
- [ ] Check configuration appropriateness
- [ ] Review and clear old warnings

### Weekly
- [ ] Comprehensive trip analysis
- [ ] Configuration review
- [ ] Operator training review
- [ ] Update procedures if needed

## Alert Response Times

| Alert Level | Response Time | Action |
|-------------|---------------|--------|
| Info | 1 hour | Review during next check |
| Warning | 15 minutes | Investigate and monitor |
| Critical | 5 minutes | Immediate investigation |
| Trip | Immediate | Emergency response |

## Escalation Path

### Level 1: Operator
- Monitor alerts
- Investigate warnings
- Reset after investigation
- Document incidents

### Level 2: Senior Operator
- Handle complex incidents
- Make configuration changes
- Coordinate with development team
- Approve resets for critical assets

### Level 3: Protocol Admin
- Global pause decisions
- Major configuration changes
- Security incident response
- Post-mortem leadership

### Level 4: Emergency Response
- Protocol-wide emergencies
- Security breaches
- Coordinated attacks
- System-wide failures

## Communication Templates

### User Notification: Circuit Breaker Trip

```
⚠️ NOTICE: Circuit Breaker Activated

Asset: [ASSET_NAME]
Status: Operations temporarily halted
Reason: Price volatility protection
Expected Resume: [TIME]

We are investigating the cause and will resume operations once price stability is confirmed.

For updates: [STATUS_PAGE_URL]
```

### User Notification: Recovery Mode

```
ℹ️ UPDATE: Recovery Mode Active

Asset: [ASSET_NAME]
Status: Limited operations resumed
Restrictions: Maximum 2% price change per update
Full Resume: [TIME]

Operations are gradually resuming. Full functionality will be restored after the recovery period.
```

### User Notification: Operations Resumed

```
✅ RESOLVED: Normal Operations Resumed

Asset: [ASSET_NAME]
Status: Fully operational
Duration: [DURATION]

All operations have been restored. Thank you for your patience.

Incident Report: [REPORT_URL]
```

## Troubleshooting

### Problem: Can't Reset Circuit Breaker

**Check:**
1. Admin authentication
2. Circuit breaker enabled status
3. Global pause status
4. Transaction logs

**Solution:**
```bash
# Verify admin
stellar-defi-cli auth verify

# Check status
stellar-defi-cli circuit-breaker status --asset <ASSET>

# Check global pause
stellar-defi-cli circuit-breaker global-pause --status

# Retry reset
stellar-defi-cli circuit-breaker reset --asset <ASSET>
```

### Problem: Frequent False Positives

**Check:**
1. Current thresholds
2. Oracle data quality
3. Market volatility
4. Trip history patterns

**Solution:**
```bash
# Review configuration
stellar-defi-cli circuit-breaker config show

# Analyze trip history
stellar-defi-cli circuit-breaker history --asset <ASSET>

# Consider adjusting thresholds
stellar-defi-cli circuit-breaker config update \
  --single-threshold 1500 \
  --consecutive-count 4
```

### Problem: Operations Halted Unexpectedly

**Check:**
1. Circuit breaker status
2. Global pause status
3. Rate limiting
4. Recent price updates

**Solution:**
```bash
# Check all statuses
stellar-defi-cli circuit-breaker status --asset <ASSET>
stellar-defi-cli circuit-breaker global-pause --status

# Review recent events
stellar-defi-cli circuit-breaker events --recent

# Check rate limiting
stellar-defi-cli circuit-breaker rate-limit --asset <ASSET>
```

## Best Practices

### DO ✅
- Monitor continuously
- Investigate before resetting
- Document all incidents
- Communicate with users
- Follow escalation procedures
- Conduct post-mortems
- Keep procedures updated

### DON'T ❌
- Reset without investigation
- Ignore warning alerts
- Disable circuit breaker without approval
- Make configuration changes without analysis
- Skip documentation
- Forget to communicate with users

## Emergency Contacts

### On-Call Rotation
- Primary: [CONTACT]
- Secondary: [CONTACT]
- Escalation: [CONTACT]

### External Contacts
- Oracle Provider: [CONTACT]
- Security Team: [CONTACT]
- Development Team: [CONTACT]

## Quick Reference Card

```
┌─────────────────────────────────────────────────────────┐
│         CIRCUIT BREAKER QUICK REFERENCE                  │
├─────────────────────────────────────────────────────────┤
│                                                           │
│  STATUS CHECK:                                           │
│  $ stellar-defi-cli cb status --asset <ASSET>           │
│                                                           │
│  HEALTH SCORE:                                           │
│  $ stellar-defi-cli cb health --asset <ASSET>           │
│                                                           │
│  RESET (after investigation):                            │
│  $ stellar-defi-cli cb reset --asset <ASSET>            │
│                                                           │
│  GLOBAL PAUSE (emergency):                               │
│  $ stellar-defi-cli cb global-pause --enable            │
│                                                           │
│  ALERT LEVELS:                                           │
│  🟢 Info (3-5%)     → Monitor                           │
│  🟡 Warning (5%+)   → Investigate                       │
│  🔴 Critical (5%+)  → Immediate Action                  │
│                                                           │
│  HEALTH SCORES:                                          │
│  90-100 → Excellent    50-69 → Fair                     │
│  70-89  → Good         0-49  → Critical                 │
│                                                           │
│  RESPONSE TIMES:                                         │
│  Info → 1 hour         Critical → 5 minutes             │
│  Warning → 15 minutes  Trip → Immediate                 │
│                                                           │
│  EMERGENCY: Call [PHONE] or page [PAGER]                │
│                                                           │
└─────────────────────────────────────────────────────────┘
```

## Training Resources

### Required Reading
1. Circuit Breaker V2 README
2. Risk Management Framework
3. Incident Response Playbook
4. This Operator Guide

### Recommended Training
1. Circuit breaker operation (2 hours)
2. Incident response simulation (4 hours)
3. Oracle system overview (1 hour)
4. Security best practices (2 hours)

### Certification
- Complete all required reading
- Pass operator quiz (80%+)
- Complete incident simulation
- Shadow experienced operator (1 week)

---

**Version**: 2.0  
**Last Updated**: 2026-06-01  
**Next Review**: 2026-09-01

**For emergencies, call**: [EMERGENCY_NUMBER]  
**For questions, contact**: operators@stellar-defi-toolkit.com
