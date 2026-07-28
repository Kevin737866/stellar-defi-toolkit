# Deployment Guide

Step-by-step deployment of the toolkit's Soroban contracts to Stellar testnet
and mainnet.

**Read §1 before anything else.** The crate as currently structured does not
build to a deployable WASM artifact, and not every module under
`src/contracts/` is actually a Soroban contract. Both are addressed below, and
both must be fixed before the rest of this guide is executable as written.

**Related:** [Economic Risk Analysis](./economic_risk_analysis.md) ·
[Upgrade Governance Process](./upgrade_governance_process.md) ·
[Emergency Response Runbook](./emergency_response_runbook.md) ·
[scripts/](../scripts/)

---

## 1. Prerequisite: crate structure

### 1.1 No `cdylib` target

[Cargo.toml](../Cargo.toml) declares one library:

```toml
[lib]
name = "stellar_defi_toolkit"
path = "src/lib.rs"
```

There is no `crate-type = ["cdylib"]`. Soroban contracts must compile to a
`cdylib` to produce the `.wasm` artifact that `soroban contract deploy`
installs. As configured, `cargo build --target wasm32-unknown-unknown` produces
an `rlib`, not something the network can run.

### 1.2 Native-only dependencies block a wasm32 build even with cdylib added

The same crate depends on `tokio`, `axum`, `async-graphql`, `async-graphql-axum`
and `tower-http` — a full async HTTP/GraphQL server stack. None of these target
`wasm32-unknown-unknown`. Adding `cdylib` alone would not fix the build; the
GraphQL API layer ([src/api/](../src/api/)) and the contract logic
([src/contracts/](../src/contracts/)) must be separated into different crates
before either can be built for its actual target.

### 1.3 Not every module in `src/contracts/` is a deployable contract

Presence of `#[contract]` is necessary but not sufficient — the `impl` block
also needs `#[contractimpl]` to be wired into the contract's dispatch, and its
methods need to take `Env` and use `env.storage()`, not hold state on `self`.
Checked against the current source:

| Module | `#[contract]` | `#[contractimpl]` | Deployable as-is |
|--------|:---:|:---:|:---:|
| `stablecoin` | Yes | Yes | **Yes** |
| `stability_pool` | Yes | Yes | **Yes** |
| `synthetic_protocol` | Yes | Yes | **Yes** |
| `synthetic_governance` | Yes | Yes | **Yes** |
| `governance_v2` | Yes | Yes | **Yes** |
| `circuit_breaker` | Yes | Yes | **Yes** |
| `price_oracle` | Yes | Yes | **Yes** |
| `oracle` | Yes | Yes | **Yes** |
| `oracle_manager` | Yes | Yes | **Yes** |
| `multi_asset_oracle` | Yes | Yes | **Yes** |
| `decentralized_oracle` | Yes | Yes | **Yes** |
| `price_feed_adapters` | Yes | Yes | **Yes** |
| `asset_registry` | Yes | Yes | **Yes** |
| `position_manager` | Yes | Yes | **Yes** |
| `arbitrage` | Yes | Yes | **Yes** |
| `liquidity_pool` | Yes | Yes | **Yes** |
| `lp_token_storage` | Yes | Yes | **Yes** |
| `flash_loan` | Yes | Yes | **Yes** |
| `staking` | Yes | Yes | **Yes** |
| `pausable_token` | Yes | Yes | **Yes** |
| `soroban_token_contract` | Yes | Yes | **Yes** |
| `token_metadata_support` | Yes | Yes | **Yes** |
| `governance` (legacy) | Yes | **No** | No — needs a `#[contractimpl]` block added |
| `token` | Yes | **No** | No — needs a `#[contractimpl]` block added |
| `vault` | Yes | **No** | **No — see §1.4** |
| `lending` | No | No | **No — not a contract; see §1.5** |

### 1.4 Vault is a native object model, not a Soroban contract

`YieldVaultContract` in [vault.rs:38](../src/contracts/vault.rs#L38) carries
`#[contract]` but its methods take `self`/`&mut self` and hold fields directly
(`self.admin`, `self.total_assets`, …) rather than reading/writing
`env.storage()`. That is a plain Rust object, useful for the existing unit
tests, but the Soroban runtime cannot invoke it as a contract in that shape.
Deploying the vault requires rewriting its methods to the `Env`-first,
storage-backed pattern used by `stablecoin.rs` or `synthetic_protocol.rs`
before it can go on any network.

### 1.5 Lending is an off-chain simulation core

`LendingProtocol` in [lending.rs:25](../src/contracts/lending.rs#L25) has no
`#[contract]` at all. Per [lib.rs](../src/lib.rs), it is deliberately "a
complete protocol simulation core" — it powers the CLI's `quote-rate`,
`check-liquidation`, and `liquidate --dry-run` commands (see
[CLI_LIQUIDATION.md](../CLI_LIQUIDATION.md)) entirely off-chain. **There is
currently no deployable on-chain lending contract in this repository.** Do not
attempt to deploy `lending.rs`; if on-chain lending is required, it needs to be
ported into a `#[contract]`/`#[contractimpl]` module first, following the
`stablecoin.rs` pattern.

### 1.6 Required restructuring before deployment

1. Split the workspace:
   ```toml
   # Cargo.toml (workspace root)
   [workspace]
   members = ["contracts/*", "api", "cli"]
   ```
   Move each deployable module in the table above into its own
   `contracts/<name>/` crate with:
   ```toml
   [lib]
   crate-type = ["cdylib", "rlib"]
   [dependencies]
   soroban-sdk = { version = "21.0.0" }
   ```
   and no `tokio`/`axum`/`async-graphql` dependency anywhere in that crate's
   dependency tree.
2. Move `src/api/` (the GraphQL server) into its own native crate that depends
   on the contract crates only for their generated client bindings, not their
   `soroban-sdk` contract types.
3. Add `#[contractimpl]` to `governance.rs` and `token.rs`, or remove the
   unused `#[contract]` attribute if they are not meant to be deployed.
4. Rewrite `vault.rs` to the storage-backed pattern (§1.4).
5. Either port `lending.rs` to a `#[contract]` module, or document explicitly
   that lending is off-chain-only for this release.

**Everything from §2 onward assumes this restructuring is complete** and each
deployable contract has its own crate producing `target/wasm32-unknown-unknown/release/<name>.wasm`.
The scripts in [scripts/](../scripts/) are written against that layout and will
fail fast with a pointer back to this section if it isn't in place yet.

---

## 2. Prerequisites

| Requirement | Version / notes |
|-------------|------------------|
| Rust | 1.70.0+, with the `wasm32-unknown-unknown` target: `rustup target add wasm32-unknown-unknown` |
| Soroban CLI | Latest — `cargo install --locked soroban-cli` (see [soroban.stellar.org](https://soroban.stellar.org/)) |
| Stellar account | Funded on the target network — testnet via [Friendbot](https://developers.stellar.org/), mainnet via an exchange or on-ramp |
| A multisig for mainnet admin roles | See [governance/README.md](../governance/README.md#emergency-powers) — never deploy mainnet with a single-key admin |

Copy [.env.example](../.env.example) to `.env` and fill in network endpoints
and secret keys. Never commit `.env`.

---

## 3. Testnet deployment

### 3.1 Network configuration

From [.env.example](../.env.example):

| Setting | Value |
|---------|-------|
| `STELLAR_NETWORK` | `testnet` |
| `STELLAR_HORIZON_URL` | `https://horizon-testnet.stellar.org` |
| `SOROBAN_RPC_URL` | `https://soroban-testnet.stellar.org` |
| `SOROBAN_NETWORK_PASSPHRAPH` | `Test SDF Network ; September 2015` |

Register the network with the CLI once:

```bash
soroban network add testnet \
  --rpc-url https://soroban-testnet.stellar.org \
  --network-passphrase "Test SDF Network ; September 2015"

soroban keys generate deployer --network testnet
soroban keys fund deployer --network testnet   # Friendbot
```

### 3.2 Build

```bash
./scripts/build.sh
```

Builds every contract crate under `contracts/*` to
`target/wasm32-unknown-unknown/release/<name>.wasm` and prints the SHA-256 hash
of each artifact — record these hashes; they are what §7 verification and any
future upgrade proposal will reference.

### 3.3 Deploy

```bash
./scripts/deploy.sh --network testnet
```

Deploys and initialises contracts **in dependency order** (§5), writing each
resulting contract ID to `deployments/testnet.json`. Re-running the script is
not idempotent — it deploys new instances each time. Do not re-run against an
existing deployment; use the [upgrade process](./upgrade_governance_process.md)
to modify one instead.

### 3.4 Manual single-contract deployment

For deploying or re-deploying one contract by hand (useful while iterating):

```bash
soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/stablecoin.wasm \
  --source deployer \
  --network testnet
# → prints the deployed contract ID

soroban contract invoke \
  --id <CONTRACT_ID> \
  --source deployer \
  --network testnet \
  -- initialize \
  --admin <ADMIN_ADDRESS> \
  --name "Stellar USD" \
  --symbol "SUSD" \
  --oracle <ORACLE_CONTRACT_ID>
```

---

## 4. Mainnet deployment

Mainnet deployment is the same mechanical steps as testnet with a materially
different risk profile. Do not treat it as "testnet with a different
`--network` flag."

### 4.1 Security considerations

- **Admin must be a multisig, not a single key**, for every contract with an
  `ADMIN` storage slot. Deploying with a single-key admin and "migrating to a
  multisig later" is a common and avoidable mistake — set the multisig address
  as the admin in the `initialize` call itself.
- **Audit before deploy, not after.** Per the
  [governance process](./upgrade_governance_process.md#2-change-classification),
  a first mainnet deployment is at minimum C4-equivalent and should have
  external audit coverage of the exact commit being deployed, with the WASM
  hash reproducible from that commit (§3.2's hash output is what an auditor
  checks against).
- **Deploy oracle infrastructure first and validate independently** before
  pointing any economic contract at it — an oracle misconfiguration discovered
  after the stablecoin is live is far more expensive to fix. See §5 for the
  dependency order and [economic_risk_analysis.md §5](./economic_risk_analysis.md#5-circuit-breaker)
  for why oracle correctness gates everything downstream.
- **Fund a separate operations key** for routine calls (price submission,
  `harvest`, non-privileged reads) distinct from the admin multisig, so routine
  operations don't require multisig coordination and the multisig's signing
  keys aren't exposed to automation.
- **Rehearse the emergency pause path before going live.** Confirm every
  multisig signer can actually sign and broadcast a `pause` transaction under
  time pressure, on mainnet, before it's needed for real — see the
  [emergency runbook](./emergency_response_runbook.md).
- **Set conservative initial caps.** `supply_cap`, `borrow_cap`,
  `max_debt_per_user`, `max_total_debt` and similar limits should start well
  below their eventual target and be raised by governance as the deployment
  proves itself, not set to their final intended value on day one.
- **Never reuse a testnet admin key or testnet-generated secrets on mainnet.**
  Generate mainnet keys freshly, ideally on hardware signing devices for
  multisig cosigners.

### 4.2 Network configuration

Use the official SDF mainnet Horizon and Soroban RPC endpoints (see
[developers.stellar.org](https://developers.stellar.org/) for current URLs) or a
supported third-party RPC provider — do not point mainnet deployments at a
testnet-era `.env` by mistake; diff `.env` against `.env.example` before running
anything.

```bash
soroban keys add mainnet-deployer --secret-key   # paste, do not pass on the CLI history
```

### 4.3 Deploy

```bash
./scripts/deploy.sh --network mainnet --confirm
```

`--confirm` is required and prints a full dry-run summary (contracts to deploy,
in what order, with what constructor arguments) that must be reviewed before
typing `yes`. There is deliberately no `--yes`/non-interactive flag for
mainnet in `scripts/deploy.sh`.

---

## 5. Contract initialisation order

Contracts have constructor dependencies on each other's addresses. Deploying
out of order means passing a placeholder or non-existent address to
`initialize`, which is either an outright failure or a silent misconfiguration.
Order derived from each module's `initialize` signature:

```
1. Token contracts (collateral assets, SUSD's own token if separate)
       soroban_token_contract / pausable_token / token_metadata_support
              │
              ▼
2. Oracle infrastructure
       asset_registry.initialize(admin)
              │
              ▼
       multi_asset_oracle.initialize(admin, asset_registry_address)
       price_oracle / oracle.initialize(admin)
       oracle_manager.initialize(admin)
       decentralized_oracle.initialize(admin)
              │
              ▼
       circuit_breaker.initialize(admin)          ◄── wraps oracle reads
              │
              ▼
3. Stablecoin
       stablecoin.initialize(admin, name, symbol, oracle_address)
              │
              ├──────────────┬──────────────────┐
              ▼               ▼                  ▼
4. Dependents of stablecoin
   stability_pool.initialize(admin, stablecoin_address, treasury)
   arbitrage.initialize(admin, stablecoin_address, oracle_address)
   governance_v2.initialize(admin, stablecoin_address)
              │
              ▼
5. Synthetic protocol (independent of stablecoin)
       synthetic_protocol.initialize(admin)
       synthetic_protocol.list_asset(...)          ◄── per synthetic, needs its own oracle_address
              │
              ▼
       synthetic_governance.initialize(admin, governance_token_address)
              │
              ▼
6. Standalone modules (deploy whenever their inputs exist)
       staking.initialize(admin, token_address)
       flash_loan.initialize(token, admin)
       liquidity_pool.initialize(token_a, token_b, fee_bps)
       position_manager.initialize(admin)
       price_feed_adapters.initialize(admin)
```

Notes:

- `governance_token_address` for `synthetic_governance` is not produced by any
  `initialize` call above — it must be an existing SEP-41 token (often the
  protocol's own governance token, deployed via `soroban_token_contract`
  before this step).
- `synthetic_protocol.list_asset` is not part of `initialize` — each synthetic
  asset is registered afterward and needs its own oracle already deployed and
  operational (§5, step 2) before listing.
- `vault` and `lending` are excluded pending §1.4/§1.5.

`scripts/deploy.sh` encodes this order; do not reorder its stages without
updating this diagram.

---

## 6. Configuration reference

`.env` keys consumed by the deployment scripts (superset of
[.env.example](../.env.example)):

| Key | Purpose |
|-----|---------|
| `STELLAR_NETWORK` | `testnet` or `mainnet` |
| `SOROBAN_RPC_URL` | RPC endpoint for the target network |
| `SOROBAN_NETWORK_PASSPHRAPH` | Network passphrase (note: matches the existing typo in `.env.example` — keep it consistent or fix both together) |
| `STELLAR_SECRET_KEY` | Deployer key — routine deploys only, never the admin multisig key |
| `TOKEN_CONTRACT_ID`, `LIQUIDITY_POOL_CONTRACT_ID`, `STAKING_CONTRACT_ID`, `GOVERNANCE_CONTRACT_ID` | Populated by `deploy.sh` into `deployments/<network>.json`; mirror into `.env` for CLI use |
| `DEFAULT_FEE_BASIS_POINTS`, `MAX_SLIPPAGE_BASIS_POINTS` | Passed through to relevant `initialize` calls where applicable |

---

## 7. Post-deployment verification

Run after every deployment, testnet or mainnet, before announcing the
contracts as live:

```bash
./scripts/verify.sh --network <testnet|mainnet>
```

This performs, per deployed contract:

1. **Existence check** — the contract ID resolves and returns a footprint.
2. **WASM hash check** — the installed hash matches the one recorded at build
   time (§3.2). A mismatch means the wrong artifact was deployed.
3. **Admin check** — reads back the admin address and confirms it is the
   intended multisig (mainnet) or deployer (testnet), not left as a default or
   placeholder.
4. **Initialization sanity** — calls each contract's read-only getters
   (`get_params`, `get_info`, `get_protocol_stats`, `total_supply`, …) and
   confirms values match what was passed to `initialize`, not zero/default
   values that would indicate `initialize` silently no-oped.
5. **Cross-reference check** — confirms address-typed fields (e.g. the
   stablecoin's `oracle` field) point at the contract ID actually deployed in
   the prior stage, not a stale or placeholder address.
6. **Pause state** — confirms every contract starts unpaused (or intentionally
   paused, if that's the deployment plan — e.g. deploying paused and unpausing
   only after a coordinated launch).

Manual spot-check, always worth doing even though the script covers it:

```bash
soroban contract invoke --id <STABLECOIN_ID> --network testnet -- get_token_info
```

If any check fails, do not proceed to list assets, open the protocol to users,
or link the contract IDs from user-facing documentation. Treat a failed
verification the same as a failed deployment — fix and redeploy, don't patch
around it.

---

## 8. Rollback and upgrade procedures

Deployment mistakes fall into two categories with very different remedies.

### 8.1 Before any user activity

If verification (§7) fails and no user has yet interacted with the contract:
the deployed instance can simply be abandoned. Fix the issue, rebuild, and
redeploy fresh — there is no state to migrate and no upgrade process needed.
Do not publish the bad contract ID anywhere.

### 8.2 After user activity, or after any deployment has been publicly announced

Once a contract has real state (deposits, positions, votes), it is no longer a
throwaway — every change goes through the
[Upgrade Governance Process](./upgrade_governance_process.md):

- A parameter fix is a **C2/C3** change (process §2).
- A code fix requiring a new WASM install is a **C4** change and requires the
  rollback plan described in
  [process §8](./upgrade_governance_process.md#8-rollback), including a tested
  reverse migration if storage layout changed.
- An active exploit found post-deployment follows the
  [Emergency Response Runbook](./emergency_response_runbook.md#3-exploit-response)
  instead of the normal upgrade path — pause first, govern the fix after.

### 8.3 Soroban-specific upgrade mechanics

Soroban supports contract WASM upgrades in place (the contract ID is stable;
the installed code changes) via the network's upgrade host function, invoked
from within the contract by an authorised caller — the contract itself must
expose an `upgrade` entry point that calls it. **None of the modules in this
repository currently implement such an entry point.** Until one is added,
"upgrading" a deployed contract means the multisig-executed WASM re-install
path described in
[upgrade_governance_process.md §6.3–§6.4](./upgrade_governance_process.md#63-execution),
which is the same mechanism used for rollback. Track adding a proper
`upgrade()` entry point as a prerequisite for a lower-friction mainnet upgrade
story — it is currently a manual, multisig-coordinated operation either way.

---

## 9. Mainnet readiness gate

Do not deploy to mainnet until all of the following are true. This gate exists
because the gaps below are not testnet-only concerns — they are the specific
findings from [economic_risk_analysis.md §6.3](./economic_risk_analysis.md#63-pre-mainnet-gate).

- [ ] §1 restructuring complete: contracts build as `cdylib`, native
      dependencies isolated from contract crates, `vault`/`lending`/`governance`/
      `token` resolved one way or the other.
- [ ] Liquidation collateral-transfer paths implemented and verified for
      stablecoin and synthetic protocol (currently placeholder comments —
      [economic_risk_analysis.md item 1](./economic_risk_analysis.md#62-ranked-risk-register)).
- [ ] Synthetic liquidator payout reconciled with the stated penalty
      ([economic_risk_analysis.md item 2](./economic_risk_analysis.md#62-ranked-risk-register)).
- [ ] Stablecoin liquidation parameters adjusted so liquidation restores vault
      health ([economic_risk_analysis.md item 3](./economic_risk_analysis.md#62-ranked-risk-register)).
- [ ] Governance/breaker design decisions (items 4, 5, 7 in the risk register)
      explicitly ratified by governance, with published limits, or resolved in
      code.
- [ ] External audit completed against the exact commit being deployed, with
      the WASM hash matching §3.2's output.
- [ ] Admin roles are multisigs; emergency pause path rehearsed end-to-end.
- [ ] `scripts/verify.sh` passes clean against the mainnet deployment.
