# Security Audit Checklist & Threat Model

This document is the protocol-wide threat model and pre-audit checklist for the
Stellar DeFi Toolkit. It exists to (1) enumerate the attack surface across every
contract, (2) give contributors and auditors a concrete checklist to run before
shipping or reviewing fund-handling changes, and (3) track known findings until
they're remediated.

Companion document: [`ACCESS_CONTROL_MATRIX.md`](ACCESS_CONTROL_MATRIX.md) — the
per-function permission reference this threat model draws its attack-surface
inventory from. Vulnerability disclosure process: [`.github/SECURITY.md`](../.github/SECURITY.md).

> **Status note:** an initial audit pass performed while writing this checklist found
> that several of the threats below are not hypothetical — they are the *current*
> state of specific contracts in this codebase. Each threat category below lists
> concrete findings alongside the general guidance, so this checklist doubles as a
> live punch list. See [Appendix A](#appendix-a-current-known-findings-by-severity)
> for the consolidated, severity-ranked list.

---

## 1. Threat Model

### 1.1 Assets at risk

- User-deposited collateral (native/token balances held by `lending.rs`,
  `stablecoin.rs`, `synthetic_protocol.rs`, `liquidity_pool.rs`, `vault.rs`)
- Protocol-owned treasury / fee accumulations
- Minted synthetic assets and the protocol stablecoin's peg
- Oracle price integrity (every contract that reads a price depends on it)
- Governance control of protocol parameters and upgrade paths
- Staked oracle/keeper bonds (`decentralized_oracle.rs`)

### 1.2 Attacker profiles

| Attacker | Capability | Primary targets |
|---|---|---|
| Unprivileged external account | Can call any public contract function, hold funds, submit transactions | Any function missing `require_auth()` |
| Malicious or compromised "admin" | Controls (or spoofs, where checks are broken) a stored admin address | Parameter tuning, pausing, fee sweeps, collateral whitelist |
| Malicious oracle / price feeder | Registered or spoofable price source | Price manipulation → liquidations, undercollateralized minting |
| Flash-loan-funded attacker | Large, transient capital within a single transaction | Oracle manipulation, governance vote manipulation, liquidation sniping |
| Governance attacker | Accumulates or borrows voting power | Malicious parameter/upgrade proposals |
| MEV searcher / front-runner | Observes mempool, reorders transactions | `initialize()` front-running, liquidation sniping, sandwich attacks on swaps |

### 1.3 Attack surfaces, by category

#### Reentrancy

Soroban's host-enforced call model reduces classic EVM-style reentrancy risk, but
logical reentrancy (state read before an external call, written after) is still
possible wherever a contract calls another contract or token mid-function.

- **Where to look:** `flash_loan.rs` (has an explicit `Executing` reentrancy-guard flag
  — verify it's checked/set/cleared correctly on every path, including error paths),
  `vault.rs::harvest`/`switch_strategy` (external strategy calls), `lending.rs::liquidate`
  and `::flash_loan` (state mutated around simulated external calls), any function that
  calls into `stability_pool.rs` from another contract (e.g. `process_liquidation`).
- **Checklist:**
  - [ ] External calls (token transfers, cross-contract calls, strategy calls) happen
        **after** all local state mutations (checks-effects-interactions).
  - [ ] Reentrancy guard flags are set before the external call and cleared in all
        exit paths, including panics/early returns.
  - [ ] No function relies on `total_supply`/balance snapshots taken before an
        external call that could change them.

#### Integer overflow / underflow / precision loss

- **Where to look:** fee/interest calculations using `bps_mul`/`mul_div`/`wad_div`
  (`lending.rs`, `utils/`), reward-index accumulation (`stability_pool.rs`,
  `staking.rs`), collateral-ratio math (`stablecoin.rs`, `synthetic_protocol.rs`).
  `token.rs::mint`/`burn` already check `u64` overflow/underflow and return `Err`
  rather than panicking or wrapping — use that as the reference pattern.
- **Checklist:**
  - [ ] All arithmetic on user-supplied amounts uses checked/saturating operations,
        not raw `+`/`-`/`*`.
  - [ ] Division happens after multiplication (not before) to preserve precision,
        and rounding direction favors the protocol, not the caller, in
        fee/interest/collateral-ratio math.
  - [ ] Basis-point (`bps`) and WAD-scaled (`1e18`/fixed-point) values are validated
        against their expected ranges before use (e.g. fee bps ≤ 10,000).

#### Oracle manipulation

The most common real-world DeFi exploit vector, and the area with the most findings
in this codebase.

- **Where to look:** every contract in the oracle family (`oracle.rs`,
  `price_oracle.rs`, `decentralized_oracle.rs`, `multi_asset_oracle.rs`,
  `oracle_manager.rs`, `price_feed_adapters.rs`, `asset_registry.rs`) and every
  consumer of their prices (`lending.rs`, `stablecoin.rs`, `synthetic_protocol.rs`,
  `arbitrage.rs`).
- **Checklist:**
  - [ ] Price-submitting functions authenticate the caller as a genuinely registered
        source (`require_auth()` + membership check — **not membership check alone**).
  - [ ] Aggregation requires a minimum number of independent, weighted sources rather
        than trusting a single feed.
  - [ ] Staleness checks (`get_price_at`-style) are enforced on every consumer path,
        not just available as an opt-in.
  - [ ] Circuit breakers trip on both single-update and cumulative/consecutive
        deviation, and consumers actually check `is_operational`/breaker status before
        acting on a price.
  - [ ] TWAP or multi-block price averaging is used wherever a single-block spot price
        could be manipulated within one flash-loan-funded transaction.
- **Current findings:** `price_oracle.rs::update_price`,
  `oracle_manager.rs::submit_price`, `decentralized_oracle.rs::submit_price`,
  `multi_asset_oracle.rs::submit_price`, and `asset_registry.rs::update_price` all
  accept a source/oracle address as a parameter without calling `require_auth()` on
  it — `price_oracle.rs` and `oracle_manager.rs` at least check list membership, but
  `asset_registry.rs::update_price` has **no check of any kind**. Any caller can
  currently submit an arbitrary price under any address's name.

#### Flash loan attacks

- **Where to look:** `flash_loan.rs`, `lending.rs::flash_loan`, any function that
  reads a spot price or a total-supply/reserve snapshot without TWAP protection
  (compounds with the oracle-manipulation category above).
- **Checklist:**
  - [ ] Flash-loan fee/repayment verification happens against actual post-callback
        balances, not a value cached before the callback.
  - [ ] No privileged action (governance vote, liquidation eligibility, collateral
        ratio check) can be satisfied using capital that exists only within the
        attacker's own transaction.
  - [ ] Flash-loan-fundable functions are identified and paired with a TWAP or
        multi-block requirement wherever they gate on price or balance.
- **Current findings:** `flash_loan.rs` simulates transfers via `log!` rather than
  performing real token movement, so its repayment check is not yet a meaningful
  guard against real capital; treat this as unimplemented, not as a passing check,
  until real token transfers are wired in.

#### Governance attacks

- **Where to look:** `governance.rs`, `governance_v2.rs`, `synthetic_governance.rs`.
- **Checklist:**
  - [ ] Voting power is derived from an actual, non-spoofable token balance/snapshot
        — never self-reported by the caller or hardcoded.
  - [ ] Proposal execution requires quorum computed against a real total-supply
        figure, not a mocked constant.
  - [ ] A timelock/execution delay separates a passing vote from its execution,
        giving users an exit window.
  - [ ] Emergency/admin fast-paths that bypass the proposal flow are minimized,
        time-boxed, and clearly flagged as temporary (see `governance_v2.rs`'s
        `update_params` comment, which already self-identifies this issue).
  - [ ] Double-voting and delegation-hijacking are actually prevented in storage, not
        just in a stubbed helper function.
- **Current findings:** all three governance contracts either mock voting power to a
  constant (`governance_v2.rs`, `synthetic_governance.rs`) or always return `0`
  (`governance.rs`), and `synthetic_governance.rs`'s double-vote guard
  (`has_voted`) always returns `false`. Until real token-balance-based voting power
  is wired in, none of these contracts provide real Sybil resistance or quorum
  guarantees — treat the voting layer as advisory/off-chain-coordinated only.

#### Access control (see full detail in the matrix)

- **Checklist:**
  - [ ] Every privileged function calls `require_auth()` on the identity it checks
        against — comparing addresses without `require_auth()` is not authentication.
  - [ ] `require_admin()`-style helpers compare the **caller's authenticated
        address**, never `env.current_contract_address()`.
  - [ ] `initialize()` either restricts who may call it (e.g. a deploy-time
        constructor argument) or is treated as front-runnable and documented as such.
  - [ ] Re-initialization is guarded (`has(&ADMIN)`-style checks) on every contract
        that stores privileged state.
  - [ ] An admin rotation path exists, or the single-admin-forever model is an
        explicit, documented risk acceptance.
- **Current findings:** see [`ACCESS_CONTROL_MATRIX.md`](ACCESS_CONTROL_MATRIX.md#appendix-enforcement-gaps-found-during-this-audit)
  for the full, function-by-function list — this is the single largest category of
  findings in the codebase today.

#### Denial of service / griefing

- **Where to look:** unbounded loops over user-controlled collections (`Vec`/`Map`
  iteration in `get_all_assets`, `get_active_proposals`, alert lists), storage growth
  (price history, event logs, alert lists — check they're capped, as
  `circuit_breaker.rs` and `price_oracle.rs` already attempt via fixed retention
  windows).
- **Checklist:**
  - [ ] Any loop over a collection has a bounded size (either a hard cap enforced at
        insertion, like `MAX_ASSETS`/`MAX_ORACLES`, or pagination).
  - [ ] No function can be permanently bricked by a single malicious input (e.g. an
        unbounded `Vec` insert with no cap).

#### Front-running / MEV

- **Checklist:**
  - [ ] `initialize()` calls that set an admin/critical config are deployed and
        initialized atomically (e.g. in the same transaction as contract creation),
        not left open as a separate, raceable call.
  - [ ] Swap functions enforce caller-supplied slippage bounds (`min_out`/`max_in`) —
        already present in `liquidity_pool.rs`, verify on any new swap path.
  - [ ] Liquidation rewards don't create a winner-take-all race that centralizes
        keeper participation (consider partial-fill or auction mechanisms for
        high-value liquidations).

---

## 2. Pre-Audit Self-Assessment Template

Copy this into your PR description (or a tracking issue) for any change that touches
fund custody, price data, or privileged parameters:

```markdown
### Security Self-Assessment

**What changed:** <brief description>

**Attack surface touched:**
- [ ] New or modified external call (token transfer, cross-contract call)
- [ ] New or modified privileged (Admin/Governance) function
- [ ] New or modified price-consuming or price-reporting logic
- [ ] New or modified arithmetic on user-supplied amounts
- [ ] New or modified loop over a user-growable collection

**Checklist categories reviewed (check all that apply, per section 1.3 above):**
- [ ] Reentrancy
- [ ] Integer overflow/underflow/precision
- [ ] Oracle manipulation
- [ ] Flash loan attacks
- [ ] Governance attacks
- [ ] Access control (cross-check `docs/ACCESS_CONTROL_MATRIX.md`)
- [ ] Denial of service / griefing
- [ ] Front-running / MEV

**New external dependencies or trust assumptions introduced:** <none, or describe>

**Known residual risk (if any) and why it's acceptable to ship:** <describe, or N/A>
```

---

## 3. Mitigation Strategy Reference

| Threat | Primary mitigation | Secondary mitigation |
|---|---|---|
| Reentrancy | Checks-effects-interactions ordering | Explicit reentrancy guard flag (see `flash_loan.rs::Executing`) |
| Overflow/underflow | Checked arithmetic, `Result`-returning math (see `token.rs::mint`/`burn`) | Fuzz/property tests on arithmetic helpers |
| Oracle manipulation | `require_auth()`-backed source registration + multi-source weighted aggregation | TWAP, circuit breakers, staleness checks |
| Flash loan attacks | TWAP/multi-block price and balance checks for privileged actions | Real (non-simulated) same-transaction repayment verification |
| Governance attacks | Real token-balance-based voting power + timelock + quorum | Minimize/time-box admin fast-paths; require multisig for emergency actions |
| Access control gaps | `require_auth()` on every identity parameter; correct `require_admin()` pattern | Two-step admin rotation, re-initialization guards |
| DoS / griefing | Bounded collections, capped retention windows | Pagination for read paths over large collections |
| Front-running / MEV | Atomic init-at-deploy, slippage bounds | Commit-reveal or batch auction for sensitive actions where relevant |

---

## 4. Review Cadence

- **Quarterly:** full pass over this checklist and the access control matrix against
  the current `main` branch; update Appendix A with newly found/newly fixed items.
  Next review due: **2026-10-28** (quarter after this document's creation).
- **Per-release:** any release that touches a contract in
  `docs/ACCESS_CONTROL_MATRIX.md`'s "broken"/"none" enforcement rows must either fix
  the gap or explicitly carry it forward in release notes as a known issue.
- **Per-PR:** the self-assessment template in Section 2 for any fund-handling,
  price-handling, or privileged-parameter change.
- **Ad hoc:** immediately after any incident, near-miss, or externally reported
  vulnerability (see [`.github/SECURITY.md`](../.github/SECURITY.md) for the
  disclosure process) — do not wait for the next quarterly cycle.

---

## Appendix A: Current Known Findings (by severity)

This list is generated from the initial audit pass performed alongside this
checklist (2026-07-28) and should be updated as items are fixed or new ones found.
Full per-function detail lives in
[`ACCESS_CONTROL_MATRIX.md`](ACCESS_CONTROL_MATRIX.md#appendix-enforcement-gaps-found-during-this-audit).

**Critical**

1. Broken `require_admin()` pattern (compares contract address, not caller) across
   12+ contracts — every "Admin only" function in those contracts is effectively
   unauthenticated or permanently unreachable.
2. Missing `require_auth()` on user-identity parameters across nearly all
   fund-moving functions (`deposit`, `withdraw`, `mint`, `redeem`, `stake`, `vote`,
   `delegate`, etc.) — any account can act on behalf of any other account.
3. `lp_token_storage.rs::mint` has no auth check — unlimited free LP token minting.
4. `asset_registry.rs::update_price` has no auth or membership check — anyone can
   set an arbitrary asset price.
5. `lending.rs` references an undefined `ensure_admin()` function and has an
   unreachable multisig-proposal flow (no `propose_*` entry point) — compile/runtime
   blockers layered on top of the access-control gap.

**High**

6. Governance voting power is mocked/stubbed in all three governance contracts —
   no real quorum or Sybil resistance yet.
7. `vault.rs` has no per-user share ledger — `withdraw` only checks the global
   share total, not the caller's actual holdings.
8. `flash_loan.rs` simulates transfers rather than moving real tokens, so its
   repayment check is not a meaningful safety guard yet.
9. Duplicate method names within the same `impl` block in 6 files
   (`stability_pool.rs`, `price_oracle.rs`, `synthetic_protocol.rs`,
   `synthetic_governance.rs`, `asset_registry.rs`, `governance_v2.rs`) — compile
   blockers that must be resolved before any of the above fixes can be validated.

**Medium**

10. No admin rotation/transfer function exists in any contract — a single
    permanently-fixed admin per contract, set by whoever calls `initialize()` first.
11. `soroban_token_contract.rs` and `pausable_token.rs` have no re-initialization
    guard on `initialize()`, despite otherwise-correct auth.
12. Several "keeper" functions intended for a registered/staked set
    (`decentralized_oracle.rs`'s oracle operations) are fully open instead of
    membership-gated.

None of these are fixed by the documentation changes in this PR — they are recorded
here, and in the access control matrix, specifically so they can be triaged and
scheduled as dedicated follow-up work rather than lost.
