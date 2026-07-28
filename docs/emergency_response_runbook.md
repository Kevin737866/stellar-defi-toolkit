# Emergency Response Runbook

Operator runbook for responding to protocol emergencies: exploits, oracle
failures, market crashes, and governance attacks. This document is written for
whoever is on call — it assumes no prior context beyond having read this file.

**Related:** [Economic Risk Analysis](./economic_risk_analysis.md) ·
[Circuit Breaker Guide](./circuit_breaker_guide.md) ·
[Upgrade Governance Process](./upgrade_governance_process.md) ·
[CONTRIBUTING.md](../CONTRIBUTING.md#-emergency-procedures)

If you are here because something is actively wrong: go to §1 (Severity), find
your scenario in §3–§6, and follow it. Read the rest later.

---

## 1. Severity classification

Classify first — it determines authority, response time, and disclosure
obligations. When in doubt, pick the higher severity; downgrading later is easy,
upgrading late is not.

| Level | Definition | Response time | Authority |
|-------|-----------|---------------:|-----------|
| **SEV1 — Active loss** | Funds are being drained right now, or a confirmed exploit path exists and is unpatched | Immediate (minutes) | Any 2 multisig signers, no vote |
| **SEV2 — Imminent risk** | Exploit found but not yet exercised; oracle reporting bad prices; a module is insolvent-adjacent | < 1 hour | Any 2 multisig signers |
| **SEV3 — Degraded** | Circuit breaker tripped on real volatility; one oracle source down; elevated liquidation rate | < 4 hours | On-call engineer + 1 approver |
| **SEV4 — Anomaly** | Metrics outside normal range but no evidence of harm | < 24 hours | On-call engineer |

SEV1/SEV2 justify **C1 emergency action** under the
[governance process](./upgrade_governance_process.md#9-emergency-changes-c1):
pause first, ratify within 72 hours, disclose within 24 hours. SEV3/SEV4 do not
justify bypassing normal governance.

---

## 2. First five minutes — universal checklist

Run this regardless of scenario. It costs little and prevents the most common
mistake: reacting before you know the blast radius.

1. **Confirm it's real.** One anomalous transaction is not an incident. Check:
   is this reproducible, is it still happening, is more than one address
   affected?
2. **Identify the module(s).** Lending, stablecoin, synthetic, vault, oracle,
   circuit breaker, or governance — see §3–§6 for the module-specific path.
3. **Start an incident log.** Timestamp every observation and action from this
   point on, in UTC. This log becomes the disclosure and the retrospective; do
   not reconstruct it afterward from memory.
4. **Page the second signer.** Emergency pause functions require ≥ 2 multisig
   signers by policy (see [governance/README.md](../governance/README.md#emergency-powers)) —
   confirm one is reachable before you commit to a pause decision, but do not
   wait past the SEV1/SEV2 response window to act.
5. **Decide: pause or observe?** Pausing has a cost (§7). If funds are actively
   moving (SEV1), pause without further deliberation. If not, take 5–10 minutes
   to scope before pausing — but no longer.

---

## 3. Exploit response

**Detect → Pause → Assess → Fix → Unpause.**

### 3.1 Detect

Signals, roughly in order of reliability:

- Unexpected balance movement on a monitored treasury or reserve address.
- A liquidation, mint, or redeem transaction with implausible amounts relative
  to the position it references.
- Reentrancy or share-price anomaly: `total_assets` or `total_supply` changing
  without a corresponding user-initiated event.
- External report (bug bounty, Discord, a security researcher) — treat as SEV2
  minimum until independently confirmed, SEV1 if a PoC is included and appears
  valid.
- Static/dynamic scanner alert on a newly deployed contract version.

### 3.2 Pause

Pause the smallest scope that stops the bleeding — module-level, not
protocol-wide, unless the exploit's blast radius is unclear or crosses modules.

| Module | Function | Effect |
|--------|----------|--------|
| Lending | `LendingProtocol::pause(caller)` ([lending.rs:89](../src/contracts/lending.rs#L89)) | Blocks deposit, withdraw, borrow, repay, liquidate, flash_loan |
| Stablecoin | `pause(env)` ([stablecoin.rs:429](../src/contracts/stablecoin.rs#L429)) | Blocks mint, redeem, liquidate |
| Stablecoin (severe) | `emergency_shutdown(env)` ([stablecoin.rs:443](../src/contracts/stablecoin.rs#L443)) | Full wind-down mode — see §7.3 before using |
| Synthetic | `pause(env)` ([synthetic_protocol.rs:638](../src/contracts/synthetic_protocol.rs#L638)) | Blocks mint, redeem, liquidate |
| Vault | `pause()` ([vault.rs:354](../src/contracts/vault.rs#L354)) | Blocks deposit, withdraw, harvest |
| Vault (severe) | `emergency_exit()` ([vault.rs:377](../src/contracts/vault.rs#L377)) | Force-exits the active strategy — see §7.3 |
| Flash loan | `pause(env)` ([flash_loan.rs:184](../src/contracts/flash_loan.rs#L184)) | Blocks flash loans specifically |
| Stability pool | `pause(env)` ([stability_pool.rs:419](../src/contracts/stability_pool.rs#L419)) | Blocks deposit/withdraw from the pool |
| Governance (stablecoin) | `emergency_pause(env)` ([governance_v2.rs:386](../src/contracts/governance_v2.rs#L386)) | Blocks vote/execute — see §6 before using |
| Governance (synthetic) | `emergency_pause(env, reason)` ([synthetic_governance.rs:452](../src/contracts/synthetic_governance.rs#L452)) | Same, with a reason logged on-chain |
| Asset registry / oracles | `pause(env)` ([asset_registry.rs:602](../src/contracts/asset_registry.rs#L602), similarly in `multi_asset_oracle.rs`, `price_oracle.rs`) | Blocks price submission/consumption paths |

If the exploit is in a shared dependency (the oracle, or a collateral asset used
by multiple modules), pause every module that reads it — a single-module pause
leaves the others exposed through the same hole.

### 3.3 Assess

- Reproduce the exploit against a **forked copy of the paused state**, not
  against mainnet. Confirm the exact function, input, and precondition.
  Do not write or run any test code as part of this reproduction — trace the logic and reason about the state transitions instead.
- Quantify: what is the maximum extractable value, and how much (if any) has
  already been extracted? Cross-reference the incident log against on-chain
  events.
- Check whether the exploit touches the known implementation gaps already on
  record — several liquidation and transfer paths are documented as stubs in
  [economic_risk_analysis.md](./economic_risk_analysis.md#6-cross-module-systemic-risk)
  and are not "new" exploits so much as known-incomplete code reached in
  production.
- Determine whether other, unaffected modules share the vulnerable pattern
  (same author, same code shape, copy-pasted logic) and check them too.

### 3.4 Fix

- Follow the [upgrade governance process](./upgrade_governance_process.md) — an
  exploit fix is a **C4 code upgrade**, but it runs on an accelerated track: the
  fix itself still requires review and, ideally, a second set of eyes, but it
  does not wait for the standard 14-day public window while actively paused.
  Document in the proposal why the accelerated track applies.
- Reproduce the fix's correctness against the same forked state used in §3.3 by
  tracing execution manually — confirm the exploit path is closed without
  introducing a new one, without running or writing tests.
- Prepare the rollback plan for the fix itself before deploying it (§8 of the
  governance process) — a hurried exploit fix is exactly the kind of change most
  likely to need its own rollback.

### 3.5 Unpause

Unpausing is never a SEV1/C1 action — see §7.1. Follow the standard checklist
there once the fix is deployed and verified.

---

## 4. Oracle failure response

### 4.1 Recognise the failure mode

| Symptom | Likely cause | Check |
|---------|-------------|-------|
| Price frozen, not updating | Reporter down, rate-limited, or circuit breaker tripped | `get_circuit_breaker_status`, `LAST_UPD` storage age |
| Price diverges from every external venue | Feed manipulation or a single bad reporter | Compare against 2+ independent venues immediately |
| Confidence below `MIN_ORACLE_CONFIDENCE` (80 %) | Reporter self-reporting low confidence, or aggregation degraded | `get_aggregated_price`, `get_price_alerts` |
| `OraclePriceRejected` / `OracleSanityCheckFailed` events firing | Sanity guard rejecting a submitted price | Read the rejection reason in the event |
| Circuit breaker tripped | Deviation exceeded threshold — see the [circuit breaker guide](./circuit_breaker_guide.md) | `CB_TRIPPED` event, `get_circuit_breaker_status` |

### 4.2 Fallback activation

The toolkit does not currently implement automatic oracle failover — aggregation
across registered oracles happens in `oracle_manager.rs`, but there is no
"deactivate the bad source and re-weight automatically" path triggered by a
failure. Manual steps:

1. **Identify the bad source.** Compare each registered oracle's last-submitted
   price (`get_oracle_info` per oracle) against external reference venues.
2. **Deactivate it.** `deactivate_oracle(env, oracle_address)`
   ([oracle_manager.rs:345](../src/contracts/oracle_manager.rs#L345)) removes it
   from aggregation. This is an admin action — no vote required, use it
   immediately at SEV2.
3. **Confirm aggregation recovers.** Re-check `get_aggregated_price` against
   external venues after deactivation. If the remaining sources are too few to
   meet quorum (see below), the affected module(s) should be paused (§3.2) rather
   than left consuming a degraded feed.
4. **If every source is compromised or down:** pause every module that consumes
   that oracle. There is no safe fallback price — do not substitute a manually
   set price without a governance-approved emergency procedure for doing so, and
   if you must, log it explicitly as a manual override in the incident log.

### 4.3 Circuit breaker interaction

If the failure presents as a circuit breaker trip rather than a silent bad
price, read [circuit_breaker_guide.md §"The central conflict"](./circuit_breaker_guide.md#the-central-conflict-the-breaker-freezes-liquidations-during-a-crash)
before resetting. A trip during a real crash is protecting the protocol, not
malfunctioning — resetting it early to "fix" the freeze can be the wrong call.
Decision:

- **Trip correlates with a real, externally-verifiable price move:** this is
  §5 (market crash), not an oracle failure. Do not reset the breaker until the
  cooldown elapses and the price has stabilised.
- **Trip does not correlate with any real move (feed-only anomaly):** this is an
  oracle failure. Deactivate the bad source (§4.2) before resetting the breaker
  with `reset_circuit_breaker(env, asset_address)`
  ([price_oracle.rs:358](../src/contracts/price_oracle.rs#L358)) — resetting
  without removing the bad source just lets it trip again.

### 4.4 Minimum reporter requirement

The toolkit does not enforce a minimum reporter count for aggregation on-chain
(see [synthetic_protocol_risk_management.md](./synthetic_protocol_risk_management.md#oracle-dependency)).
Operationally, treat aggregation with fewer than 3 active sources as SEV3
regardless of whether any individual price looks reasonable — a 2-source or
1-source aggregation has lost its manipulation resistance even if it hasn't yet
produced a visibly bad number.

---

## 5. Market crash response

### 5.1 Circuit breaker decision tree

```
Price move detected
        │
        ▼
Is the move corroborated by ≥ 2 independent external venues?
        │
   ┌────┴────┐
  yes         no
   │           │
   ▼           ▼
Real market   Likely feed issue
crash          → go to §4 (oracle failure)
   │
   ▼
Did the circuit breaker trip automatically?
   │
   ┌────┴────┐
  yes         no
   │           │
   ▼           ▼
Let it hold.  Consider manual_trip() if the move is severe
Do NOT reset  enough that continued liquidation at a fast-
early — see   moving price would itself cause harm
§5.2                (circuit_breaker.rs:374)
```

### 5.2 Why not reset early

Resetting the breaker mid-crash restores liquidation — including at a price that
may already be stale relative to a still-falling market. Per the
[circuit breaker guide's threshold justification](./circuit_breaker_guide.md#threshold-justification-and-risk-analysis),
the cooldown (1800 s) is sized to be survivable within the collateral cushion
most reserves carry. Cutting it short trades a bounded, designed-for freeze for
an unbounded, undesigned one. Do not reset before `CIRCUIT_BREAKER_COOLDOWN`
elapses unless you have specific evidence the frozen price is *more* wrong than
the current price would be.

### 5.3 During the freeze

The protocol cannot liquidate while frozen (§5.3 of
[economic_risk_analysis.md](./economic_risk_analysis.md#53-conflict-with-liquidation--the-central-systemic-finding)
covers why this is structural, not a bug). While frozen:

1. **Quantify exposure, don't wait passively.** Using the last known-good price,
   compute how many positions are already below their liquidation threshold and
   the aggregate value at risk. This tells you the scale of bad debt you're
   walking into at reset, before reset happens.
2. **Prepare for the reset, don't just wait for it.** Liquidators (internal or
   external) should be alerted that a wave of liquidations will become available
   at reset — an orderly, promptly-executed liquidation wave limits further
   slippage versus a delayed one.
3. **If the frozen-to-true price gap exceeds ~27 %,** recovery mode's
   `RECOVERY_MAX_CHANGE_BPS` ramp will leave the oracle stale for over an hour
   post-reset (see the circuit breaker guide's recovery arithmetic). Convene the
   multisig to widen `RECOVERY_MAX_CHANGE_BPS` for this event rather than letting
   the default ramp run — the default assumes a smaller gap than this.

### 5.4 Post-reset

1. Confirm the price has actually recovered/stabilised before assuming normal
   operation — check 2+ external venues, not just the on-chain feed.
2. Monitor the liquidation wave. An abnormal liquidation rate immediately after
   reset is expected; an abnormal rate that does *not* taper within an hour
   suggests either continued price movement or a liquidation-path problem worth
   investigating separately.
3. Record realised bad debt, if any (positions where liquidation could not fully
   cover the debt). This number drives whether an insurance-fund claim or a
   governance-level response is needed.

### 5.5 Circuit-breaker-adjacent DoS

If the trip pattern looks engineered — repeated trips from what appears to be a
single source, or trips timed to coincide with other suspicious activity —
reclassify as §3 (exploit response), not §5 (market crash). The
[circuit breaker guide's cost-of-attack analysis](./circuit_breaker_guide.md#cost-of-an-attack)
explains why a compromised or thin oracle makes this a live threat, not a
theoretical one.

---

## 6. Governance attack response

### 6.1 Recognise it

| Symptom | Check |
|---------|-------|
| Large, sudden voting-power concentration before a proposal | Compare voter addresses against known holders; check for flash-loan-scale balance changes at the same block |
| A proposal passes with suspiciously low turnout right at quorum | `votes_for + votes_against` vs `quorum` — is it just barely met? |
| A proposal's `description` Symbol doesn't match any published written proposal | This alone is a governance-process violation — see [process §5.1](./upgrade_governance_process.md#51-opening-the-vote) |
| Votes recorded that don't match the on-chain `VOTE_CAST` event log | Possible double-voting via the [expired temporary-storage weakness](./upgrade_governance_process.md#54-known-weaknesses-in-the-voting-implementation) |
| A passed proposal's parameters don't match what was reviewed | Compare `create_proposal` call data against the reviewed specification |

### 6.2 Response

1. **If the proposal has not yet executed:** the timelock window is your
   response time. Use `emergency_pause` on the governance contract
   (`governance_v2.rs:386` or `synthetic_governance.rs:452`) to block execution.
   This is a SEV1/SEV2 action — do not wait for a vote to block a vote.
2. **If the proposal has already executed:** treat as §3 (exploit response) from
   the point of the resulting state change onward. The governance path was the
   vector, not the harm — the harm is whatever the executed proposal actually
   changed.
3. **Publicly disclose the attempted or successful attack within 24 hours**,
   same as any C1 action, including the specific mechanism (vote-buying,
   flash-borrowed power, temporary-storage double-vote, or other).
4. **Ratify or reverse within 72 hours** per the governance process. If the
   emergency pause turns out to have blocked a legitimate proposal, say so
   plainly in the ratification and let it proceed through normal execution once
   safe.

### 6.3 Structural note

Because `execute_proposal_logic` does not currently call target contracts (see
[governance/README.md — Known gaps](../governance/README.md#known-gaps)), a
"successful" governance attack today can at most produce a misleading on-chain
record and force admins into a bad manual decision — it cannot yet directly
mutate protocol state through the governance path itself. This will change once
that gap is closed; re-read this section against the current code before relying
on it.

---

## 7. Pause and shutdown reference

### 7.1 Standard unpause checklist

Never a SEV1/C1 action on its own — restoring normal operation goes through at
minimum a C2 governance step (per
[process §9.5](./upgrade_governance_process.md#9-emergency-changes-c1)), because
the pressure that justified skipping process is gone by the time you're
unpausing.

- [ ] Root cause identified and, if code-related, fixed and deployed.
- [ ] Fix reviewed by someone other than its author.
- [ ] No open loss — extraction stopped and quantified.
- [ ] Affected users' positions checked: nothing left in a state that will break
      immediately on unpause (e.g. a position that's been liquidatable for hours
      due to a frozen oracle).
- [ ] Monitoring in place for the specific failure mode that triggered the pause.
- [ ] Public communication sent (§9) before or simultaneous with unpause, not
      after.

### 7.2 Escalation from pause to shutdown

`emergency_shutdown` (stablecoin) and `emergency_exit` (vault) are more severe
than pause and harder to reverse. Use them only when:

- The exploit is ongoing and module-level pause does not stop it (e.g. the
  vulnerable path is itself outside the paused functions), or
- The module is confirmed insolvent (aggregate `c < 1.00`) and orderly wind-down
  is preferable to an uncontrolled run, or
- A strategy contract the vault depends on is itself compromised, making
  `emergency_exit` (force-exit the strategy) the only way to recover funds.

`emergency_shutdown` and `emergency_exit` are still SEV1/C1-eligible actions, but
because they are harder to reverse than a pause, escalate to them deliberately,
not reflexively — a pause that stops the bleeding is usually sufficient and buys
time to decide whether shutdown is actually warranted.

### 7.3 What pause does not do

Pausing blocks new user-initiated transactions on the paused paths. It does not:

- Freeze interest/fee accrual in modules where accrual happens as a side effect
  of *any* call, including admin calls.
- Protect against admin-key compromise — a compromised admin can pause, unpause,
  or call any admin-only function including the pause function itself.
- Recover funds already extracted before the pause landed.

Do not report an incident as "contained" on the basis of a pause alone until
these are separately confirmed.

---

## 8. Communication templates

Use these as a starting point; every incident needs the specifics filled in.
Publish to whatever channel the [Contact List](#10-contact-list-and-escalation-path)
designates as primary for the severity level.

### 8.1 Initial disclosure (SEV1/SEV2, within 24 hours)

```
Subject: [Stellar DeFi Toolkit] Security incident — <module> — <date>

We identified <a security issue / an active exploit / an oracle failure>
affecting the <module> module at approximately <time, UTC> on <date>.

What happened:
  <one or two factual sentences — no speculation, no blame>

Actions taken:
  - <time, UTC> Paused <module>
  - <time, UTC> <further action>

Current status: <paused / partially restored / investigating>

Funds at risk / lost: <specific figure, or "still being quantified — next
update by <time>">

What you should do: <specific user action, or "no action needed">

Next update by: <time, UTC>
```

### 8.2 Oracle failure notice (SEV2/SEV3)

```
Subject: [Stellar DeFi Toolkit] Oracle disruption — <asset(s)> — <date>

The price oracle for <asset(s)> is currently <frozen / reporting
low-confidence prices / circuit-breaker tripped> as of <time, UTC>.

Effect: <module(s)> liquidation, minting, and redemption for <asset(s)> are
<paused / operating on a stale price as of <time>>.

We expect resolution by: <time, or "unknown — next update by <time>">
```

### 8.3 Market crash / circuit breaker notice (SEV3)

```
Subject: [Stellar DeFi Toolkit] Circuit breaker triggered — <asset> — <date>

The circuit breaker for <asset> tripped at <time, UTC> following a <X%>
price move. This is a designed safety response to protect against
liquidations on a potentially manipulated or stale price.

Effect: <module(s)> operations for <asset> are paused until at least
<cooldown expiry time, UTC>.

This is expected to resolve automatically once the cooldown period elapses
and the price stabilises. No user action is needed unless you have an
active position in <asset> — see <link to position-check tooling, if any>.
```

### 8.4 Governance attack notice (SEV1/SEV2)

```
Subject: [Stellar DeFi Toolkit] Governance security event — <date>

We <blocked / are investigating> a governance proposal (<proposal ID>)
that <appears to attempt / attempted> <vote manipulation / an
unauthorised parameter change> via <mechanism, if known>.

Actions taken:
  - <time, UTC> Emergency-paused governance execution
  - <time, UTC> <further action>

A ratifying governance vote will be held within 72 hours per our
governance process: <link>.
```

### 8.5 Resolution / post-mortem summary (all severities, within 7 days)

```
Subject: [Stellar DeFi Toolkit] Incident resolved — <module> — <date>

Summary: <what happened, in one paragraph>

Timeline: <link to full incident log>

Root cause: <specific, technical>

Impact: <funds affected, users affected, duration>

Remediation: <what was fixed, and the governance proposal ID that
ratified the emergency action, if applicable>

Prevention: <what changes to prevent recurrence — code, process, or
monitoring>
```

---

## 9. Incident log requirements

Every SEV1–SEV3 incident produces a durable log, independent of whichever chat
tool was used in the moment. At minimum, capture:

| Field | Example |
|-------|---------|
| Timestamp (UTC) | Every entry, to the minute |
| Actor | Who took the action, or "automated" |
| Action | "Called `pause` on lending", "Deactivated oracle X" |
| Observation | "TVL dropped 4 % in reserve Y" |
| Evidence | Transaction hash, event, or external reference |
| Decision and rationale | Not just what was decided — why |

This log is the primary input to the post-mortem (§8.5) and to the ratifying
governance proposal. Reconstructing it after the fact from memory produces a
worse disclosure and a worse retrospective — keep it live during the incident.

---

## 10. Contact list and escalation path

<!--
Fill in for your deployment before going to mainnet. This table is a template —
an empty or stale contact list turns SEV1 response into a search for phone
numbers instead of a response.
-->

| Role | Contact | Backup | Escalate to |
|------|---------|--------|--------------|
| On-call engineer | `<name / handle>` | `<name / handle>` | Multisig signer (below) |
| Multisig signer 1 | `<name>` — `<contact>` | — | — |
| Multisig signer 2 | `<name>` — `<contact>` | — | — |
| Multisig signer 3 | `<name>` — `<contact>` | — | — |
| Security researcher intake | `<email / form>` | — | On-call engineer |
| Public disclosure channel | `<Discord / Twitter / status page>` | — | — |
| Legal / compliance (if applicable) | `<contact>` | — | — |

**Escalation path:** On-call engineer assesses severity (§1) → if SEV1/SEV2,
pages 2+ multisig signers immediately → multisig executes the pause → incident
log starts → follow the module-specific section (§3–§6).

**Bug bounty / responsible disclosure:** route external reports to the security
intake contact above. Acknowledge receipt within 24 hours regardless of severity,
and do not disclose publicly before the reporter has been notified of the
response, per standard responsible-disclosure practice.

This table should be kept current in the private operations record, not solely
in this public document, if contact details are sensitive — but the roles and
escalation path themselves should stay documented here.
