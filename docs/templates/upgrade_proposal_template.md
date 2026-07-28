# SDT-YYYY-NNN: <Title>

<!--
Copy this file into a GitHub issue labelled `governance`.
Every section is required. A proposal missing any section is closed without a vote.
Process: docs/upgrade_governance_process.md
-->

| Field | Value |
|-------|-------|
| Proposal ID | `SDT-YYYY-NNN` |
| Class | C1 / C2 / C3 / C4 / C5 (see process §2) |
| Author | `<github handle>` / `<stellar address>` |
| Proposer voting power | `<amount>` (`<x.xx>` % of supply — must be ≥ 0.1 %) |
| Target contracts | `<contract names and addresses>` |
| Network | Testnet first / Mainnet |
| Public window opens | `YYYY-MM-DD` |
| Earliest vote open | `YYYY-MM-DD` (≥ 5 days after publication; 14 for C4/C5) |
| Voting period | `<n>` days |
| Quorum required | `<n>` % |
| Timelock | `<n>` days |
| Status | Draft / Under review / Voting / Queued / Executed / Rejected / Withdrawn |

---

## 1. Summary

<!-- One paragraph. What changes, and why, in plain language a token holder can act on. -->

## 2. Motivation

<!--
The problem, with evidence. Not "this would be better" — what is wrong now.

Include where applicable:
- Metrics demonstrating the problem, with the observation window
- The incident or near-miss that prompted it
- The risk-register item addressed (economic_risk_analysis.md §6.2)
- Why the current parameters are inadequate rather than merely non-ideal
- What happens if this proposal does not pass
-->

## 3. Specification

### 3.1 Parameter changes

| Parameter | File:line | Current | Proposed | Δ |
|-----------|-----------|--------:|---------:|--:|
| `EXAMPLE_BPS` | `src/contracts/example.rs:25` | `1000` (10 %) | `1500` (15 %) | +5 pp |

### 3.2 Code changes

<!-- C4+ only. Delete if not applicable. -->

- Commit: `<full sha>`
- Diff: `<PR link>`
- Build command: `cargo build --release --target wasm32-unknown-unknown`
- Toolchain: `rustc <version>`, `soroban-sdk <version>`
- **WASM hash:** `<hex>`
- Reproduction: `<exact steps for a third party to rebuild and compare the hash>`

### 3.3 Storage layout

<!-- C4+ only. -->

- Layout changed: yes / no
- Migration required: yes / no
- Migration is idempotent: yes / no
- Reverse migration tested: yes / no — **if no, this proposal is not mainnet-eligible**

## 4. Economic impact

<!-- Required for C2+. Use the framework in docs/economic_risk_analysis.md. -->

### 4.1 Affected modules

| Module | Affected | How |
|--------|:--------:|-----|
| Lending | ☐ | |
| Stablecoin | ☐ | |
| Synthetic | ☐ | |
| Vault | ☐ | |
| Circuit breaker | ☐ | |
| Governance | ☐ | |

### 4.2 Capital efficiency

<!-- Change in leverage available, utilisation targets, or yield to suppliers. Show the arithmetic. -->

### 4.3 Liquidation risk

<!--
Required if the change touches any collateral ratio, threshold, penalty, bonus,
or close factor. Must address:
- Does liquidation remain profitable for the liquidator after realistic slippage?
- Does the toxic-zone identity c' = (c − f(1+b))/(1−f) still improve positions
  across the whole permitted liquidation band?
- Does the cushion between "liquidatable" and "insolvent" widen or narrow?
-->

| Scenario | Before | After |
|----------|-------:|------:|
| Drawdown tolerated from default ratio | | |
| Liquidator gross margin | | |
| Cushion to insolvency | | |

### 4.4 Oracle dependency

<!-- Does this change how much the protocol depends on price feeds, or how fast a feed error propagates? -->

### 4.5 Systemic risk

<!-- Cross-module effects. Which other modules read a value this change touches? -->

## 5. Risks and mitigations

| Risk | Likelihood | Impact | Detection | Mitigation |
|------|-----------|--------|-----------|------------|
| | | | | |

### 5.1 What would make this proposal wrong

<!--
State the conditions under which this change turns out to have been a mistake.
A proposal whose author cannot name these has not been thought through.
-->

## 6. Test evidence

- [ ] Unit tests added or updated: `<paths>`
- [ ] Integration tests pass: `cargo test`
- [ ] Testnet deployment: `<contract address>`, ledger `<seq>`
- [ ] Testnet exercise performed: `<what was done and observed>`
- [ ] Simulation / stress results: `<link or summary>`
- [ ] Boundary values tested: `<min, max, zero, overflow>`

## 7. Rollback plan

| Field | Value |
|-------|-------|
| Reversible | yes / no |
| Route | Multisig parameter revert / WASM re-install / governance proposal / forward-fix only |
| Time to effect | `<minutes / hours / days>` |
| Previous WASM hash | `<hex>` |
| Rollback transaction pre-built and signed | yes / no |
| State snapshot ledger sequence | `<seq>` |
| Rehearsed on testnet | yes / no |

### 7.1 Rollback triggers

<!-- The specific, observable conditions that trigger rollback without further discussion. -->

## 8. Verification checklist

<!-- The change-specific checks. Generic checks are in the process doc §7. -->

### Immediate (15 min)

- [ ] Execution transaction succeeded; events present
- [ ] Parameters read back from storage match the proposal
- [ ] <change-specific check>

### Short-term (4 h)

- [ ] Share prices did not decrease
- [ ] No position newly liquidatable
- [ ] <change-specific check>

### Sustained (7 d)

- [ ] Predicted effect observed — quantitatively
- [ ] CHANGELOG.md, economic_risk_analysis.md, module docs updated
- [ ] Retrospective published

## 9. Review record

<!-- Filled in during Stage 2. -->

| Reviewer | Role | Verdict | Date | Notes |
|----------|------|---------|------|-------|
| | | Approved / Conditions / Needs work / Rejected | | |

- Independent reviewer (no code contribution to this change): `<handle>` — C4+ only
- External audit: `<firm>`, `<report link>`, commit audited `<sha>` — C4+ only

## 10. Execution record

<!-- Filled in after Stage 4. -->

| Field | Value |
|-------|-------|
| On-chain proposal ID | |
| Votes for / against | |
| Total supply at execution | |
| Quorum reached | |
| Voting deadline | |
| Timelock expiry | |
| Execution transaction | |
| Execution ledger | |
| Executed by | |
| Verification owner | |
| Verification complete | ☐ 15 min ☐ 4 h ☐ 7 d |
