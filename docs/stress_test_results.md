# Stress Test Results: Extreme Market Conditions

`tests/stress_tests.rs` (and the runnable walkthrough in
`examples/stress_test_scenarios.rs`) simulate extreme market conditions
against `LendingProtocol` using `MockOracle` (see `src/contracts/oracle.rs`
for the oracle API itself — programmatic price setting, trend/spike/crash
scenarios, and staleness simulation). All scenarios are deterministic — no
wall-clock dependency — so these are exact, reproducible results, not
sampled measurements.

Run them with:

```sh
cargo test --test stress_tests
cargo run --example stress_test_scenarios
```

## Scenario 1: 50% price crash across all collateral assets

A borrower posts XLM collateral at the protocol's default 80% collateral
factor (85% liquidation threshold) and borrows USDC near that limit. Both
XLM and USDC are then crashed 50% over six 10-minute steps (a 1-hour total
window), via `MockOracle::simulate_crash`.

**Result:** the borrower's health factor drops below the healthy threshold
established at open time; `LendingProtocol::position` correctly reports the
deteriorated health factor rather than the pre-crash valuation on every call
(there is no cached/stale valuation — `position()` is fully recomputed from
current oracle prices each time it's called).

**Resilience finding:** the protocol's default single-update circuit-breaker
(20% max deviation) does **not** block this scenario, because it's evaluated
per-update against the previous *accepted* price — spreading a 50% move over
6 steps keeps every individual step under ~14.3%, comfortably under the 20%
threshold. A true single-block 50% crash (no gradual steps) *does* trip the
breaker unless an admin explicitly disables it first — see
`circuit_breaker_tests.rs::test_market_crash_scenario_trips_breaker_without_bypass`
for both cases side by side.

## Scenario 2: Multiple simultaneous liquidations

Three borrowers (Alice, Bob, Carol) each post XLM collateral and borrow USDC
near the collateral limit. A single sharp 50% XLM crash (circuit breaker
disabled, modeling a true flash crash) makes all three liquidatable in the
same block.

**Result:** a single liquidator clears all three positions back-to-back at
the same `now` timestamp. Each `liquidate` call succeeds independently
(`repaid_amount > 0`, `seized_collateral > 0`), and reserve-level bookkeeping
(`total_cash` for both XLM and USDC) stays non-negative and consistent after
all three liquidations — the protocol does not require serializing
liquidations across borrowers or any special-casing for "the same asset
liquidated multiple times in one block."

## Scenario 3: Oracle failure

Rather than a feed that goes *stale* (still covered directly against the
oracle in `circuit_breaker_tests.rs::test_get_price_stale_after_max_age`),
this scenario models a feed that never reports at all for a given asset
(e.g. a newly listed market with no price source wired up yet).

**Result:** `LendingProtocol::position` propagates
`ProtocolError::MissingPrice(asset)` rather than treating the un-priced
collateral as worthless or panicking — the caller gets a clear, typed error
instead of a silently-wrong valuation. Once the feed comes online (a single
`set_price` call), the position prices correctly on the very next read.

**Known gap (not fixed by this change, out of scope for these tests):**
`LendingProtocol`'s oracle-consuming methods (`position`, `borrow`,
`liquidate`) call `PriceOracleSim::get_price` (no staleness check) rather
than `get_price_at(asset, now)`, so a feed that goes *stale* — as opposed to
one that never reported — is **not** currently caught at the protocol level,
only at the oracle layer directly. `MockOracle` can simulate and detect this
correctly today (see `circuit_breaker_tests.rs`); wiring that check into
`LendingProtocol`'s call sites is a follow-up, not part of this change.
