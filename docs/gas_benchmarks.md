# Gas / Compute-Cost Benchmarks

## Methodology

`LendingProtocol` (`src/contracts/lending.rs`) is a plain-Rust simulation —
it has no `#[contract]` / `#[contractimpl]` Soroban entry points of its own,
so there is no Soroban host budget (`env.budget()`) to read a real ledger gas
number from. `benches/lending_benchmarks.rs` measures wall-clock time per
call instead, as the closest available proxy for computational/gas cost: the
two move together for the same reason Soroban's own instruction metering
does — more work per call means both a longer measured time here and a
bigger resource-fee bill once this logic runs behind a real `#[contract]`
wrapper.

The harness is a small hand-rolled `std::time::Instant` loop rather than a
benchmark framework like `criterion` — this repository's dependency graph is
fragile enough (soroban-sdk's macro crates, no committed `Cargo.lock`) that
pulling in a framework's transitive dependencies was observed to perturb
version resolution elsewhere in the tree and break the build. Trading away
criterion's statistical rigor (confidence intervals, outlier detection) for
that stability was the right call here; revisit if the dependency graph ever
gets a committed lockfile and more headroom.

Run the suite:

```sh
cargo bench --bench lending_benchmarks
```

It prints a `ns/call` figure per operation directly — there's no persisted
history to diff against automatically, so "compared against baseline" here
means comparing two runs' printed output by hand:

```sh
git checkout main && cargo bench --bench lending_benchmarks > /tmp/baseline.txt
git checkout my-branch && cargo bench --bench lending_benchmarks > /tmp/branch.txt
diff /tmp/baseline.txt /tmp/branch.txt
```

CI (see `.github/workflows/ci.yml`, job `gas-benchmarks`) runs this on every
PR and uploads the printed report as the `gas-report` workflow artifact —
download `main`'s and the PR's artifacts to do the same comparison without a
local checkout. The job does not itself fail on a regression (wall-clock
timing in shared CI runners is too noisy for a hard threshold); use the
diff to judge before merging.

## Benchmarked operations

| Benchmark | What it measures |
|---|---|
| `deposit` | Cost of a single `deposit` call against a warm 2-reserve protocol. |
| `withdraw` | Cost of a single `withdraw` call (fresh deposit set up per iteration, excluded from timing via `iter_batched`). |
| `borrow` | Cost of a single `borrow` call against posted collateral. |
| `repay` | Cost of a single `repay` call against an existing debt position. |
| `liquidate` | Cost of a single `liquidate` call against an undercollateralized position (post-crash). |
| `deposit_scaling_by_reserve_count` | How `deposit` cost scales as the number of *other* registered reserves grows (1/5/10/20) — flags accidental O(n)-in-reserve-count regressions. |

## Optimization notes from benchmark analysis

These fall out of reading the hot paths exercised by the benchmarks above,
not from asserted numbers (there is no committed baseline yet — the first
`cargo bench` run on `main` establishes one):

- **`String`-keyed `BTreeMap`s everywhere** (`reserves`, `reserve_configs`,
  `accounts`, all keyed by `String` asset/user ids). Every `deposit`/
  `borrow`/`repay` does at least one `String` clone (e.g.
  `lending.rs:381-384`) to insert or look up. For a real on-chain deployment
  this becomes `Symbol`/`Address`-keyed storage, which is both cheaper to
  hash/compare and avoids the heap allocation entirely.
- **Whole-position recomputation in `position()`** (`lending.rs:1060+`)
  iterates every supplied *and* every borrowed asset on every call, including
  from inside `borrow`/`liquidate`'s health-factor checks. For accounts with
  many open positions this is the dominant cost — the `deposit_scaling`
  benchmark's "warm protocol with N reserves" variant is the regression
  canary for this class of cost growing with position count rather than
  reserve count; a follow-up benchmark parameterized by *the caller's* open
  position count (rather than total registered reserves) would isolate it
  more directly.
- **`ProtocolSnapshot`/`snapshot()`** (`lending.rs:1123-1126`) clones every
  reserve, config, and the full multisig state on each call — fine for
  tests/CLI introspection, but something to keep out of any hot
  transaction path if it's ever called there.

None of these are correctness bugs — they're the first places to look if
`cargo bench` shows a regression after a change to the hot paths above.
