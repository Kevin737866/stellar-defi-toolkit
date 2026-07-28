# Protocol Upgrade Governance Process

The complete process for changing deployed protocol code or parameters:
proposal → review → voting → timelock → execution → verification.

This document is normative. An upgrade that skips a stage is not a valid upgrade,
and any contract state resulting from one should be treated as an incident under
the [Emergency Response Runbook](./emergency_response_runbook.md).

**Related:** [CONTRIBUTING.md](../CONTRIBUTING.md#-governance) ·
[governance/](../governance/README.md) ·
[Deployment Guide](./deployment_guide.md) ·
[Economic Risk Analysis](./economic_risk_analysis.md)

---

## 1. Governance parameters as deployed

From [governance_v2.rs:22](../src/contracts/governance_v2.rs#L22) and
[synthetic_governance.rs:22](../src/contracts/synthetic_governance.rs#L22) — both
modules use identical values:

| Parameter | Constant | Value |
|-----------|----------|-------|
| Proposal threshold | `MIN_PROPOSAL_THRESHOLD_BPS` | 10 bps = 0.1 % of total supply |
| Voting period (default) | `DEFAULT_VOTING_PERIOD` | 7 days |
| Voting period (min / max) | `MIN_` / `MAX_VOTING_PERIOD` | 3 days / 30 days |
| Quorum (default) | `DEFAULT_QUORUM_BPS` | 1000 bps = 10 % of total supply |
| Quorum (min / max settable) | `MIN_QUORUM_BPS` / cap in `update_params` | 500 bps (5 %) / 5000 bps (50 %) |
| Execution delay (timelock) | `EXECUTION_DELAY` | 2 days |
| Passing rule | `execute_proposal` | `votes_for > votes_against` (simple majority) |

Quorum is measured against **total votes cast** (`votes_for + votes_against`),
not against `votes_for` alone — an abstention-heavy proposal with a narrow margin
can still execute.

Total lead time from proposal to earliest execution, at defaults:
**7 days voting + 2 days timelock = 9 days.**

### Standing multisig path (lending module)

The lending protocol has a separate admin path that does **not** use token
voting: `MultiSigConfig { admins, threshold }` with `AdminProposal` / `AdminAction`
([types/lending.rs:239](../src/types/lending.rs#L239)). Actions available:
`SetCloseFactor`, `RegisterAsset`, `UpdateReserveConfig`, `UpdateMultiSig`,
`CollectProtocolFees`. This path has **no timelock**. It is appropriate for
operational parameters and inappropriate for anything that changes user
economics; see §3 for which route applies to what.

---

## 2. Change classification

Every change is classified before anything else happens. The class determines the
review depth, the route, and the voting parameters.

| Class | Definition | Route | Voting period | Quorum | Timelock | Reviewers |
|-------|-----------|-------|--------------:|-------:|---------:|-----------|
| **C1 — Emergency** | Active exploit, oracle failure, or imminent loss of funds | Multisig pause, ratify after | n/a | n/a | 0 | 2 signers + post-hoc governance vote within 72 h |
| **C2 — Operational** | Parameter inside an already-ratified range (e.g. reserve factor within a governed band) | Lending multisig or governance | 3 days | 5 % | 2 days | 1 maintainer + 1 risk reviewer |
| **C3 — Economic** | Any change to collateral ratios, fees, liquidation penalties, interest rate model, circuit-breaker thresholds, caps | Governance vote | 7 days | 10 % | 2 days | 2 maintainers + risk analysis update |
| **C4 — Code upgrade** | New contract WASM, changed storage layout, new module | Governance vote | 14 days | 20 % | 7 days | 2 maintainers + external audit |
| **C5 — Constitutional** | Governance parameters themselves, admin/multisig membership, treasury control | Governance vote | 30 days | 33 % | 14 days | Full maintainer set + audit |

C3–C5 exceed the contract defaults for voting period and quorum, so
`update_params` must be called to raise them for the proposal in question, or the
proposal must be conducted with the raised parameters already in force. **Do not
run a C4 or C5 change under the 7-day/10 % defaults.**

### Deciding the class

```
Is user money at immediate risk?              ── yes ──▶ C1
                    │ no
Does the change alter contract bytecode
  or storage layout?                          ── yes ──▶ C4
                    │ no
Does it change governance itself, admin keys,
  or treasury control?                        ── yes ──▶ C5
                    │ no
Does it change any economic parameter
  users rely on?                              ── yes ──▶ C3
                    │ no
                    └──────────────────────────────────▶ C2
```

When two classes are arguable, **the higher class applies**. Bundling a C2 change
with a C4 change makes the bundle C4.

---

## 3. Stage 1 — Proposal

### 3.1 Requirements

A proposal is admissible only when all of the following hold:

- [ ] Proposer holds ≥ 0.1 % of total supply in voting power (enforced on-chain
      by `create_proposal`; delegated power counts).
- [ ] A written proposal exists at
      [docs/templates/upgrade_proposal_template.md](./templates/upgrade_proposal_template.md),
      filled in completely, opened as a GitHub issue labelled `governance`.
- [ ] The proposal has a unique identifier: `SDT-<year>-<sequence>`
      (e.g. `SDT-2026-004`).
- [ ] Class (C1–C5) is stated and justified.
- [ ] For C3+: the affected sections of
      [economic_risk_analysis.md](./economic_risk_analysis.md) are updated in the
      same PR, or the proposal states explicitly why no risk section changes.
- [ ] For C4+: the exact WASM hash to be installed is published, together with a
      reproducible build recipe (`cargo build --release --target wasm32-unknown-unknown`
      plus the toolchain version) so any voter can independently reproduce the hash.
- [ ] A rollback plan exists (§8).

### 3.2 Required content

The template enforces these sections; a proposal missing any of them is closed
without a vote:

1. **Summary** — one paragraph, what changes and why.
2. **Motivation** — the problem, with evidence. Cite metrics, incidents, or the
   specific risk-register item being addressed.
3. **Specification** — exact old and new values, or exact code diff. Every
   changed constant listed with its file and line.
4. **Economic impact** — for each affected module: capital efficiency,
   liquidation behaviour, oracle dependency, systemic effect. Reuse the framework
   in [economic_risk_analysis.md](./economic_risk_analysis.md).
5. **Risks and mitigations** — what could go wrong, how it is detected, what the
   response is.
6. **Test evidence** — tests added or updated, testnet results, simulation output.
7. **Rollback plan** — §8.
8. **Verification checklist** — the specific post-execution checks for this
   change (§7).

### 3.3 Timing

A proposal must be public for **at least 5 days before the on-chain vote opens**
(C4/C5: 14 days). This is the discussion window and it is not optional — it is
the only stage at which the specification can change without restarting the
process.

---

## 4. Stage 2 — Review

### 4.1 Review criteria

Reviewers assess against these criteria and record a written verdict on the
proposal issue. "Looks good" is not a review.

| Criterion | Question | Applies to |
|-----------|----------|-----------|
| **Correctness** | Does the change do what the specification says? Are edge cases and integer boundaries handled? | All |
| **Economic soundness** | Does the change preserve liquidation profitability, collateral cushions, and arbitrage incentives? Are the toxic-zone identities in the risk analysis still satisfied? | C2+ |
| **Parameter coherence** | Do the new values remain mutually consistent — e.g. `liquidation_threshold_bps ≤ 10000/(1+bonus)`, `penalty < MCR − 1`? | C2+ |
| **Storage compatibility** | Does existing state deserialise under the new layout? Is a migration needed, and is it idempotent? | C4+ |
| **Authorisation** | Are `require_auth` / admin checks preserved on every privileged path? | All |
| **Blast radius** | Which other modules read the changed value? Has each been checked? | All |
| **Observability** | Are the events needed to verify execution actually emitted? | All |
| **Reversibility** | Can this be undone, and how fast? | All |

### 4.2 Reviewer independence

- No one may be the sole reviewer of their own proposal.
- For C4+, at least one reviewer must not have contributed code to the change.
- External audit for C4+ must cover the exact commit being proposed, not an
  earlier revision. An audit of a superseded commit does not satisfy the gate.

### 4.3 Review outcomes

| Outcome | Meaning | Next step |
|---------|---------|-----------|
| **Approved** | Ready for vote as written | Proceed to Stage 3 |
| **Approved with conditions** | Specific, enumerated changes required | Apply, re-review, restart the 5-day public window if the specification changed |
| **Needs work** | Material gaps | Back to Stage 1 |
| **Rejected** | Should not proceed in any form | Close with reasoning |

A proposal that reaches Stage 3 without a recorded "Approved" verdict from the
required number of reviewers must be cancelled, not executed.

---

## 5. Stage 3 — Voting

### 5.1 Opening the vote

```rust
let proposal_id = governance.create_proposal(
    env,
    proposer,          // must hold ≥ 0.1 % of supply
    proposal_type,     // ProposalType variant
    description,       // Symbol referencing SDT-YYYY-NNN
);
```

`description` is a `Symbol` and is therefore short. It must carry the proposal
identifier so the on-chain record can be tied to the written proposal — the
specification lives off-chain and voters must be able to find it.

### 5.2 Voting mechanics

- `vote(env, voter, proposal_id, support, reason)` — one vote per address per
  proposal, weight equal to `get_voting_power` at the time of voting.
- Voting power may be delegated with `delegate(delegator, delegate)`.
  Self-delegation is rejected.
- Votes may not be changed once cast.
- Voting closes at `voting_deadline = created_at + voting_period`.

### 5.3 Passing conditions

`execute_proposal` enforces all of:

1. `current_time > voting_deadline` — voting has ended.
2. `votes_for + votes_against >= quorum` where `quorum = total_supply · quorum_bps / 10000`.
3. `votes_for > votes_against` — strict simple majority.
4. `current_time >= voting_deadline + execution_delay` — timelock elapsed.

### 5.4 Known weaknesses in the voting implementation

These are properties of the current contract, not of the process. Treat them as
operational constraints until they are fixed.

| Weakness | Detail | Operational mitigation |
|----------|--------|------------------------|
| **Votes stored in temporary storage** | `vote()` records the double-vote guard via `env.storage().temporary()`. Soroban temporary entries expire. Once a key's TTL lapses, the same address can vote again on the same proposal. | Keep voting periods short relative to the temporary-storage TTL; reconcile `votes_for + votes_against` against the `VOTE_CAST` event log before executing any C3+ proposal. |
| **Voting power read at vote time** | Power is not snapshotted at proposal creation, so tokens can be acquired mid-vote, or borrowed and returned. | For C4/C5, cross-check large votes against balance history; consider requiring the proposer to publish a holder snapshot. |
| **No cancellation in `governance_v2`** | There is no `cancel_proposal`; a proposal found to be defective after the vote passes can still be executed by anyone. | Use `emergency_pause` to block execution, then let the proposal expire unexecuted. |
| **`emergency_pause` is admin-only and halts governance itself** | The admin can indefinitely prevent any proposal from executing. | Admin must be a multisig. Any use of `emergency_pause` against a passed proposal is an incident requiring public disclosure within 24 h. |
| **Execution is permissionless** | Any address may call `execute_proposal` once conditions are met. | Expected and desirable; ensure operators are ready at the timelock expiry rather than assuming they control timing. |

---

## 6. Stage 4 — Timelock and execution

### 6.1 The timelock window

`EXECUTION_DELAY = 2 days` runs from `voting_deadline`, not from the vote's
passing moment. During this window:

- The change is decided but not yet in force.
- Users who disagree can exit their positions. **This is the timelock's entire
  purpose** — it is not a cooling-off period for the team, it is an exit window
  for users. Any process change that shortens it reduces user protection.
- Operators verify readiness: monitoring in place, rollback rehearsed, on-call
  assigned.

The standalone `governance/` crate models this explicitly with
`queued_at` and `executable_at` on `Proposal`, and a configurable
`DataKey::TimelockDelay` ([governance/src/proposal.rs](../governance/src/proposal.rs)).

### 6.2 Pre-execution checklist

Complete within 4 hours of the timelock expiring:

- [ ] Vote tallies re-verified against `VOTE_CAST` events (see §5.4).
- [ ] Proposal state on-chain matches the written proposal (type and parameters).
- [ ] For C4: installed WASM hash matches the published hash exactly.
- [ ] Contract not paused; no active circuit-breaker trip on any affected asset.
- [ ] No open incident (see the [runbook](./emergency_response_runbook.md)).
- [ ] Rollback proposal drafted and ready to submit.
- [ ] On-call engineer confirmed available for the following 24 h.
- [ ] Monitoring dashboards open; baseline metrics recorded (§7.2).

### 6.3 Execution

```rust
governance.execute_proposal(env, executor, proposal_id);
```

For C4 code upgrades on Soroban, execution is a two-step sequence:

1. `soroban contract install --wasm <artifact>` — uploads the WASM, returns its hash.
2. The upgrade call on the target contract, referencing that hash.

Record for every execution: transaction hash, ledger sequence, executing address,
timestamp, and gas consumed.

### 6.4 Implementation gap — execution is not wired to targets

`execute_proposal_logic`
([governance_v2.rs](../src/contracts/governance_v2.rs)) matches on the proposal
type and **emits an event without calling the target contract**:

```rust
ProposalType::UpdateFees { minting_fee_bps, redemption_fee_bps } => {
    // In production: Call stablecoin contract to update fees
    env.events().publish(Symbol::short("FEES_UPDATED"), (minting_fee_bps, redemption_fee_bps));
},
```

Every variant follows this pattern. **A passed proposal currently changes no
protocol state.** Two consequences for this process:

1. Until the cross-contract calls are implemented, governance outcomes must be
   applied manually by the admin multisig, and every such application must
   reference the executed `proposal_id` in its rationale. This is a trust
   assumption that must be disclosed to token holders.
2. `ProposalType` has no `UpgradeContract` variant, so C4 code upgrades have no
   native on-chain representation at all. Use `ProposalType::Custom(Symbol)` with
   the symbol referencing the proposal ID, and execute the upgrade through the
   admin multisig. Adding a first-class `UpgradeContract { wasm_hash, target }`
   variant is a prerequisite for genuinely decentralised upgrades and should be
   proposed as a C5 change.

---

## 7. Stage 5 — Post-upgrade verification

Verification is not optional and not "monitoring for a while". It is a checklist
with a defined completion time and a named owner.

### 7.1 Immediate (within 15 minutes)

- [ ] Execution transaction succeeded; expected events present in the ledger.
- [ ] For C4: deployed WASM hash queried on-chain equals the proposed hash.
- [ ] Changed parameters read back from contract storage equal the proposed values
      — read them, do not infer them from the event.
- [ ] Admin and multisig configuration unchanged (unless this was the change).
- [ ] Contract not paused; no unexpected circuit-breaker trips.
- [ ] One read-only call per affected module returns sane values
      (`get_params`, `get_info`, `get_protocol_stats`, `position`).

### 7.2 Short-term (within 4 hours)

- [ ] One small end-to-end transaction per affected user flow, executed
      deliberately: deposit, borrow, repay, mint, redeem, harvest — whichever the
      change touches.
- [ ] Metrics compared against the pre-execution baseline:

  | Metric | Source | Expectation |
  |--------|--------|-------------|
  | Total supply / TVL per reserve | `ProtocolSnapshot` | Unchanged except by user activity |
  | Reserve utilisation | `total_debt / net_assets` | Unchanged at execution |
  | Share prices (vault, supply shares) | `get_share_price`, `net_assets/shares` | **Must not decrease** |
  | Health factor distribution | `position()` sweep | No position newly liquidatable |
  | Collateral ratios | `get_collateral_ratio` | No vault newly below its floor |
  | Circuit-breaker status | `get_circuit_breaker_status` | Operational for all assets |

- [ ] No error-rate increase in RPC or indexer logs.
- [ ] No unexpected liquidations in the hour following execution.

### 7.3 Sustained (within 7 days)

- [ ] Daily metric review against the baseline for 7 days.
- [ ] The behaviour the proposal predicted actually occurred — state it
      quantitatively, and say so plainly if it did not.
- [ ] [CHANGELOG.md](../CHANGELOG.md) updated with proposal ID, execution
      transaction, and observed effect.
- [ ] Affected documentation updated:
      [economic_risk_analysis.md](./economic_risk_analysis.md),
      module docs, [README.md](../README.md).
- [ ] Retrospective published on the proposal issue, then the issue is closed.

**A proposal is not complete until 7.3 is done.** An upgrade whose verification
was never finished is an open operational risk, and it should be tracked as such.

---

## 8. Rollback

### 8.1 Rollback triggers

Initiate rollback immediately, without waiting for consensus, on any of:

- Share price decreases with no corresponding strategy loss.
- Positions become liquidatable that were not liquidatable before execution.
- A user-facing function reverts where it previously succeeded.
- Accounting invariants break (`total_cash + total_debt` inconsistent with shares;
  `total_collateral` drifting from summed deposits).
- Any unexplained balance movement.

### 8.2 Rollback routes

| Change class | Rollback route | Realistic time to effect |
|--------------|----------------|--------------------------|
| C2 parameter | Multisig sets the prior value | Minutes |
| C3 parameter | Emergency C1 pause, then a governance proposal to restore | Minutes to pause, ~9 days to restore |
| C4 code | Re-install the previous WASM hash via multisig | Under 1 hour if pre-staged |
| C4 with storage migration | **Often not reversible.** Forward-fix only. | Hours to days |
| C5 constitutional | Usually not reversible without a fork | Indefinite |

### 8.3 Rollback preconditions

Before any C4 execution:

- [ ] Previous WASM artifact archived with its hash recorded.
- [ ] Rollback transaction pre-built and signed by the required threshold, held
      unsubmitted.
- [ ] For migrations: a state snapshot at the pre-execution ledger sequence.
- [ ] Rollback rehearsed on testnet against a state copy.

**A storage-layout change with no tested reverse migration is not eligible for
mainnet.** Say so in review and block the proposal.

---

## 9. Emergency changes (C1)

C1 exists because the 9-day path is unusable during an active exploit. It trades
process for speed and therefore carries a hard accountability requirement.

1. Any two multisig signers may invoke `pause` / `emergency_pause` immediately.
   No proposal, no vote, no timelock.
2. Follow the [Emergency Response Runbook](./emergency_response_runbook.md).
3. Public disclosure within **24 hours**, whatever the state of the investigation.
4. A ratifying governance proposal must be submitted within **72 hours**. If it
   fails, the emergency action is reversed and the signers' judgement is reviewed.
5. Unpausing is **not** a C1 action. Restoring normal operation goes through at
   minimum a C2 vote, because the pressure that justified skipping process is by
   then gone.

Emergency powers that are used without ratification stop being emergency powers
and become unaccountable admin control. The 72-hour ratification requirement is
the thing that keeps the distinction real.

---

## 10. Process summary

```
        ┌──────────────────────────────────────────────────────────┐
        │ Stage 1  PROPOSAL                                        │
        │  Classify C1–C5 · fill template · publish · 5–14 d window │
        └────────────────────────┬─────────────────────────────────┘
                                 ▼
        ┌──────────────────────────────────────────────────────────┐
        │ Stage 2  REVIEW                                          │
        │  8 criteria · independent reviewers · audit for C4+      │
        └────────────────────────┬─────────────────────────────────┘
                                 ▼
        ┌──────────────────────────────────────────────────────────┐
        │ Stage 3  VOTING            3–30 d (7 d default)          │
        │  quorum 5–33 % · votes_for > votes_against               │
        └────────────────────────┬─────────────────────────────────┘
                                 ▼
        ┌──────────────────────────────────────────────────────────┐
        │ Stage 4  TIMELOCK          2–14 d (2 d default)          │
        │  user exit window · pre-execution checklist · EXECUTE    │
        └────────────────────────┬─────────────────────────────────┘
                                 ▼
        ┌──────────────────────────────────────────────────────────┐
        │ Stage 5  VERIFICATION      15 min · 4 h · 7 d            │
        │  read back state · baseline compare · retrospective      │
        └──────────────────────────────────────────────────────────┘
                                 │
                    rollback available throughout (§8)
```

| Class | Public window | Vote | Timelock | Minimum total |
|-------|--------------:|-----:|---------:|--------------:|
| C1 | 0 | post-hoc | 0 | Immediate |
| C2 | 5 d | 3 d | 2 d | 10 days |
| C3 | 5 d | 7 d | 2 d | 14 days |
| C4 | 14 d | 14 d | 7 d | 35 days |
| C5 | 14 d | 30 d | 14 d | 58 days |

---

## 11. Open items blocking full decentralisation

| # | Item | Impact | Class to fix |
|---|------|--------|--------------|
| 1 | `execute_proposal_logic` emits events but does not call target contracts | Governance outcomes are applied by trusted admins, not by code | C4 |
| 2 | No `ProposalType::UpgradeContract` variant | Code upgrades have no on-chain representation | C4 |
| 3 | Double-vote guard uses expiring temporary storage | Vote integrity is time-bounded | C4 |
| 4 | Voting power not snapshotted at proposal creation | Vote-buying and flash-borrowed voting power | C4 |
| 5 | No `cancel_proposal` in `governance_v2` | Defective passed proposals cannot be withdrawn | C4 |
| 6 | `emergency_pause` can indefinitely block governance execution | Admin holds a veto over token holders | C5 |

Until items 1 and 2 are closed, this protocol's upgrade process is
**multisig-executed with token-holder ratification**, not token-holder-controlled.
Describe it that way in user-facing material.
