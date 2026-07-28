# Governance Module

Reference for the governance components of the Stellar DeFi Toolkit and the
upgrade authority they exercise.

**The normative process lives in
[docs/upgrade_governance_process.md](../docs/upgrade_governance_process.md).**
This file documents what exists in code, where each piece lives, and which
authority path applies to which change.

---

## Components

| Component | Location | Role |
|-----------|----------|------|
| Timelock crate | [governance/](.) | Standalone proposal queue with a configurable timelock delay |
| Stablecoin governance | [src/contracts/governance_v2.rs](../src/contracts/governance_v2.rs) | Token-weighted voting over stablecoin parameters |
| Synthetic governance | [src/contracts/synthetic_governance.rs](../src/contracts/synthetic_governance.rs) | Token-weighted voting over synthetic protocol parameters |
| Legacy governance | [src/contracts/governance.rs](../src/contracts/governance.rs) | Earlier proposal/vote implementation with `cancel_proposal` |
| Lending multisig | [src/types/lending.rs](../src/types/lending.rs) | `MultiSigConfig` + `AdminProposal` — threshold approval, no timelock |

There is no single governance contract. Each protocol carries its own authority
path, and they do not share a voting token or a proposal registry. Any change
spanning modules requires a coordinated proposal in each — track them under one
`SDT-YYYY-NNN` identifier.

---

## Timelock crate

[`governance/src/proposal.rs`](./src/proposal.rs) defines the queue-and-wait
lifecycle:

```rust
pub struct Proposal {
    pub id: u64,
    pub proposer: Address,
    pub title: String,
    pub description: String,
    pub status: ProposalStatus,
    pub created_at: u64,
    pub voting_end: u64,
    pub queued_at: Option<u64>,      // set when the proposal enters the timelock
    pub executable_at: Option<u64>,  // queued_at + TimelockDelay
}
```

[`governance/src/storage.rs`](./src/storage.rs) keys state by
`DataKey::Proposal(u64)` and `DataKey::TimelockDelay`, so the delay is a stored,
governable value rather than a compile-time constant — unlike `EXECUTION_DELAY`
in the contract modules.

```
created ──▶ voting (until voting_end) ──▶ queued (queued_at)
                                              │
                                    executable_at = queued_at + delay
                                              ▼
                                          executed
```

**Status:** `governance/main.rs` is empty and
`governance/src/test/timelock_tests.rs` contains no tests. The crate is a
skeleton — the types are defined, the queue/execute logic and its tests are not
written. It is not wired into the protocol contracts, which use their own
`EXECUTION_DELAY` constant instead. Completing this crate and routing the
contract modules through it is the cleanest path to a uniform, governable
timelock.

---

## Voting parameters

Identical in `governance_v2.rs` and `synthetic_governance.rs`:

| Parameter | Constant | Value |
|-----------|----------|-------|
| Proposal threshold | `MIN_PROPOSAL_THRESHOLD_BPS` | 0.1 % of total supply |
| Voting period | `DEFAULT_VOTING_PERIOD` | 7 days (min 3, max 30) |
| Quorum | `DEFAULT_QUORUM_BPS` | 10 % of total supply (min 5 %, settable to 50 %) |
| Execution delay | `EXECUTION_DELAY` | 2 days |
| Passing rule | — | `votes_for > votes_against`, quorum on total votes cast |

Adjustable at runtime through `update_params`, within the min/max bounds. Raising
them is itself a governance action — a C5 change under the process document.

---

## Proposal types

`ProposalType` ([src/types/stablecoin.rs:187](../src/types/stablecoin.rs#L187)):

| Variant | Purpose |
|---------|---------|
| `UpdateCollateralParameters { collateral_address, min_ratio, max_ratio }` | Adjust a collateral's ratio band |
| `UpdateFees { minting_fee_bps, redemption_fee_bps }` | Adjust protocol fees |
| `AddCollateral { collateral_address, collateral_type, min_ratio, max_ratio }` | List a new collateral |
| `RemoveCollateral { collateral_address }` | Delist a collateral |
| `UpdateOracle { new_oracle }` | Repoint the price oracle |
| `EmergencyShutdown` | Wind down the protocol |
| `Custom(Symbol)` | Anything else, executed off-chain |

**There is no `UpgradeContract` variant.** Code upgrades have no native on-chain
representation and must currently run through `Custom(Symbol)` plus admin
multisig execution. See process doc §11, item 2.

---

## Authority paths

```
   ┌─────────────────────────┐        ┌──────────────────────────┐
   │  Token-weighted voting  │        │    Admin multisig        │
   │  governance_v2 /        │        │    MultiSigConfig        │
   │  synthetic_governance   │        │    {admins, threshold}   │
   ├─────────────────────────┤        ├──────────────────────────┤
   │ 0.1 % to propose        │        │ Threshold approvals      │
   │ 7 d vote, 10 % quorum   │        │ No vote                  │
   │ 2 d timelock            │        │ NO TIMELOCK              │
   │ Permissionless execute  │        │ Admin executes           │
   └───────────┬─────────────┘        └────────────┬─────────────┘
               │                                   │
     Economic parameters,              Reserve config, close factor,
     collateral listing,               fee collection, multisig
     oracle, shutdown                  membership, emergency pause
```

`AdminAction` variants available to the lending multisig:
`SetCloseFactor`, `RegisterAsset`, `UpdateReserveConfig`, `UpdateMultiSig`,
`CollectProtocolFees`.

The multisig path has **no timelock**, so it gives users no exit window. Restrict
it to operational changes; route anything that alters user economics through the
voting path even where the multisig is technically capable of it.

---

## Emergency powers

| Power | Holder | Timelock | Ratification |
|-------|--------|---------:|--------------|
| `pause` / `unpause` | Admin | none | Required within 72 h |
| `emergency_pause` (governance) | Admin | none | Required within 72 h |
| `emergency_shutdown` (stablecoin) | Admin | none | Required within 72 h |
| Circuit breaker reset | Admin | none | Log and review |

`emergency_pause` in `governance_v2` halts governance itself, which means the
admin can indefinitely block a passed proposal from executing. This is the
sharpest centralisation edge in the system. Two requirements follow, and both are
process obligations rather than code guarantees:

1. The admin address **must** be a multisig, never a single key.
2. Any use of `emergency_pause` against a proposal that has already passed is an
   incident requiring public disclosure within 24 hours.

---

## Known gaps

| # | Gap | Where | Consequence |
|---|-----|-------|-------------|
| 1 | `execute_proposal_logic` emits events but never calls target contracts | `governance_v2.rs` | Passed proposals change no state; outcomes applied manually |
| 2 | No `UpgradeContract` proposal type | `types/stablecoin.rs` | Code upgrades unrepresented on-chain |
| 3 | Double-vote guard in expiring temporary storage | `governance_v2.rs` `vote()` | Vote integrity is time-bounded |
| 4 | Voting power read at vote time, not snapshotted | `governance_v2.rs` | Vote-buying, borrowed voting power |
| 5 | No `cancel_proposal` in `governance_v2` | `governance_v2.rs` | Defective passed proposals still executable |
| 6 | Timelock crate unimplemented and unwired | `governance/` | Timelock is a constant, not a governable value |
| 7 | No shared voting token or proposal registry across modules | all | Cross-module changes need manual coordination |

Because of gaps 1 and 2, the effective model today is **multisig execution with
token-holder ratification**. Describe it that way to users; do not call it
on-chain governance until those are closed.

---

## Working on governance code

- Changes to voting parameters, quorum, thresholds, or admin membership are **C5**
  under the process doc — 30-day vote, 33 % quorum, 14-day timelock.
- Add tests to [`src/test/`](./src/test/) for any lifecycle change; the timelock
  test module is currently empty and is the right place to start.
- New `ProposalType` variants must ship with their `execute_proposal_logic` arm
  and a test proving the target contract state actually changed — an event alone
  does not count as execution.
