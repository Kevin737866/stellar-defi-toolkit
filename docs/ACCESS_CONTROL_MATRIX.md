# Access Control Matrix

This document is the single source of truth for **who can call what** across every
contract in the toolkit. It maps **Contract × Role × Action**, and — critically —
distinguishes between the **designed** permission model (what the code and comments
intend) and the **as-enforced** behavior (what actually happens on-chain today).

> **This distinction is not academic.** An initial audit pass performed while writing
> this document found that a large share of "Admin only" and "User only" checks in this
> codebase do not actually authenticate the caller (see
> [Enforcement Gap Appendix](#appendix-enforcement-gaps-found-during-this-audit) and the
> [Security Audit Checklist](SECURITY_AUDIT_CHECKLIST.md)). Treat every `⚠️ broken` tag
> below as a live finding, not a hypothetical.

## Roles

| Role | Definition |
|---|---|
| **Admin** | A single privileged address, set once at `initialize()`, intended to manage protocol parameters, pausing, and emergency actions. |
| **Governance** | Token-weighted voting participants (proposers, voters, executors) in the on-chain governance contracts. A superset of "User" scoped to proposal/vote actions. |
| **Keeper** | A permissionless or semi-permissionless automated caller (bot, oracle feeder, liquidator, arbitrageur) that triggers time- or condition-sensitive operations. Anyone can act as a keeper; the role describes *function*, not identity. |
| **User** | Any account interacting with the protocol on its own behalf (depositing, borrowing, minting, staking, swapping). |

## How to read the "Enforcement" column

- **✅ enforced** — the function calls `Address::require_auth()` on the relevant
  identity, and (for Admin) compares it against the stored admin/role address.
- **⚠️ broken** — the function *intends* to gate on a role, but the check does not
  cryptographically authenticate the caller (most commonly: `require_admin()` compares
  `env.current_contract_address()` to the stored admin instead of calling
  `admin.require_auth()`), so the gate is either permanently unreachable or provides no
  real protection.
- **❌ none** — no auth check of any kind exists, though the parameter naming/doc intent
  implies one role or another.
- **— (view)** — read-only, no state change, open to anyone by design.

---

## Contract × Role × Action Matrix

### `token.rs` — `TokenContract`

Plain in-memory Rust struct, **not a deployed Soroban `#[contract]`** — no `Env`, no
`require_auth` capability exists in this file at all.

| Action | Function | Role (designed) | Enforcement |
|---|---|---|---|
| Mint tokens | `mint` | Admin | ❌ none |
| Burn tokens | `burn` | User (self) | ❌ none |
| Transfer | `transfer` | User (`from`) | ❌ none |
| Approve allowance | `approve` | User (`owner`) | ❌ none |
| Spend allowance | `transfer_from` | User (`spender`) | ❌ none |
| Read balance / allowance / info | `balance_of`, `allowance`, `get_info` | Anyone | — (view) |

### `soroban_token_contract.rs` — `SorobanTokenContract`

The only token implementation with fully correct auth.

| Action | Function | Role (designed) | Enforcement |
|---|---|---|---|
| Initialize | `initialize` | Admin (self-declared) | ✅ enforced (`admin.require_auth()`) — but **no re-init guard**, callable repeatedly |
| Mint | `mint` | Admin | ✅ enforced (`require_auth` + stored-admin equality) |
| Transfer | `transfer` | User (`from`) | ✅ enforced (`from.require_auth()`) |
| Read balance / supply | `balance`, `total_supply` | Anyone | — (view) |

### `pausable_token.rs` — `PausableTokenContract`

| Action | Function | Role (designed) | Enforcement |
|---|---|---|---|
| Initialize | `initialize` | Admin (self-declared) | ✅ enforced, but **no re-init guard** |
| Pause / Unpause | `pause`, `unpause` | Admin | ✅ enforced (`require_auth` + equality) |
| Transfer | `transfer` | User (`from`) | ✅ enforced (`from.require_auth()`) |
| Read paused state | `is_paused` | Anyone | — (view) |

### `staking.rs` — `StakingContract`

Correctly implemented alongside `soroban_token_contract.rs` and `pausable_token.rs`.

| Action | Function | Role (designed) | Enforcement |
|---|---|---|---|
| Initialize | `initialize` | — (bootstrap) | ❌ none (first caller becomes admin) |
| Configure lock-up tier | `set_tier` | Admin | ✅ enforced (`admin.require_auth()`) |
| Stake | `stake` | User | ✅ enforced (`user.require_auth()`) |
| Withdraw | `withdraw` | User | ✅ enforced (`user.require_auth()`) |
| View pending rewards | `pending_rewards` | Anyone | — (view) |

### `circuit_breaker.rs` — `CircuitBreakerContract`

The only "admin registry" contract with a correct `require_admin()`.

| Action | Function | Role (designed) | Enforcement |
|---|---|---|---|
| Initialize | `initialize` | — (bootstrap) | ❌ none (first caller becomes admin) |
| Report price update | `check_price_update` | Keeper (oracle/feeder) | ❌ none |
| Manual trip / reset | `manual_trip`, `reset` | Admin | ✅ enforced (`admin.require_auth()`) |
| Update config | `update_config` | Admin | ✅ enforced |
| Global pause | `set_global_pause` | Admin | ✅ enforced |
| Force recovery | `force_recovery` | Admin | ✅ enforced |
| Clear warning alerts | `clear_warning_alerts` | Admin | ✅ enforced |
| Read status / history / health | `is_operational`, `get_status`, `get_trip_history`, `get_health_score`, etc. | Anyone | — (view) |

### `liquidity_pool.rs` — `LiquidityPool` (on-chain contract)

No admin concept — fully permissionless pool. The file also contains an unrelated,
never-deployed simulation struct `LiquidityPoolContract` used in tests, whose "admin"
setters have no auth at all (`⚠️` — see appendix).

| Action | Function | Role (designed) | Enforcement |
|---|---|---|---|
| Initialize pool | `initialize` | — (bootstrap) | ❌ none |
| Add / remove liquidity | `add_liquidity`, `remove_liquidity` | User (`provider`) | ✅ enforced (`provider.require_auth()`) |
| Swap | `swap`, `swap_a_for_b`, `swap_b_for_a` | User | ✅ enforced |
| Claim fees | `claim_fees` | User (`provider`) | ✅ enforced |
| Read position / fees | `get_position`, `get_collected_fees`, `get_liquidity_position` | Anyone | — (view) |

### `lp_token_storage.rs` — `LpTokenStorage`

No admin concept.

| Action | Function | Role (designed) | Enforcement |
|---|---|---|---|
| Mint LP tokens | `mint` | Pool contract only | ❌ none — most severe finding in this file |
| Burn LP tokens | `burn` | User (`from`) | ✅ enforced (`from.require_auth()`) |
| Read balance / supply | `balance`, `total_supply` | Anyone | — (view) |

### `stability_pool.rs` — `StabilityPoolContract`

| Action | Function | Role (designed) | Enforcement |
|---|---|---|---|
| Initialize | `initialize` | — (bootstrap) | ❌ none |
| Deposit / withdraw / claim | `deposit`, `withdraw`, `claim_rewards` | User (`depositor`) | ❌ none |
| Process liquidation credit | `process_liquidation` | Keeper (lending/vault contract) | ❌ none |
| Update params, pause/unpause, update treasury | `update_params`, `pause`, `unpause`, `update_treasury` | Admin | ⚠️ broken (`require_admin` compares contract address, not caller) |
| Read pool info / deposits / rewards | `get_user_deposit`, `get_pending_rewards`, `get_pool_info`, `get_params` | Anyone | — (view) |

### `stablecoin.rs` — `StablecoinContract`

| Action | Function | Role (designed) | Enforcement |
|---|---|---|---|
| Initialize | `initialize` | — (bootstrap) | ❌ none |
| Mint / redeem | `mint`, `redeem` | User (vault owner) | ❌ none |
| Liquidate vault | `liquidate` | Keeper (liquidator) | ❌ none |
| Stability pool deposit/withdraw | `deposit_stability_pool`, `withdraw_stability_pool` | User | ❌ none |
| Add collateral type, pause/unpause, emergency shutdown, set fees | `add_collateral`, `pause`, `unpause`, `emergency_shutdown`, `set_minting_fee`, `set_redemption_fee` | Admin | ⚠️ broken |
| Read supply / vault / ratio / pool info | `total_supply`, `get_vault`, `get_collateral_ratio`, `get_stability_pool_info`, `get_token_info` | Anyone | — (view) |

### `vault.rs` — `YieldVaultContract`

Plain Rust struct; imports `soroban_sdk` but never calls `require_auth`.

| Action | Function | Role (designed) | Enforcement |
|---|---|---|---|
| Initialize | `initialize` | — (bootstrap) | ❌ none, no re-init guard |
| Add / switch / optimize strategy, collect fees, pause/unpause, emergency exit, set fee/treasury | `add_strategy`, `switch_strategy`, `optimize_strategy`, `collect_fees`, `pause`, `unpause`, `emergency_exit`, `set_performance_fee`, `set_treasury` | Admin | ⚠️ broken (`require_admin` only checks admin is *set*, not caller identity) |
| Deposit / withdraw | `deposit`, `withdraw` | User (`depositor`/`withdrawer`) | ❌ none — compounded by no per-user share ledger (single aggregate `total_shares`) |
| Harvest | `harvest` | Keeper | ❌ none (rate-limited by `MIN_HARVEST_INTERVAL`, not by auth) |
| Read share price / info / stats | `get_share_price`, `get_info`, `get_stats`, `preview_deposit`, `preview_withdraw` | Anyone | — (view) |

### `lending.rs` — `LendingProtocol`

Plain Rust struct simulation (no Soroban auth primitives available). Multisig
admin model via `MultiSigConfig { admins: Vec<String>, threshold }`.

| Action | Function | Role (designed) | Enforcement |
|---|---|---|---|
| Deposit / withdraw / toggle collateral | `deposit`, `withdraw`, `set_collateral_enabled` | User | ❌ none (string-identified caller, no signature) |
| Borrow / repay | `borrow`, `repay` | User | ❌ none |
| Liquidate | `liquidate` | Keeper (liquidator) | ❌ none |
| Flash loan | `flash_loan` | Keeper/User | ❌ none |
| Pause / unpause, set rate models, caps, reserve factor | `pause`, `unpause`, `set_default_interest_rate_model`, `set_asset_interest_rate_model`, `set_supply_cap`, `set_borrow_cap`, `set_reserve_factor` | Admin | ⚠️ broken — calls `ensure_admin()`, which is **referenced but never defined** anywhere in the codebase |
| Approve/execute/cancel multisig proposal | `approve_admin_proposal`, `execute_admin_proposal`, `cancel_admin_proposal` | Admin (multisig member) | ⚠️ logic present but **unreachable** — no `propose_*` function ever inserts into `proposals`, so nothing can reach quorum |
| Register asset, update reserve config, set close factor, collect fees | `register_asset`, `update_reserve_config`, `set_close_factor`, `collect_protocol_fees` | Admin (single-sig fast-path) | ⚠️ gated by `threshold == 1`; with `threshold > 1` these are **permanently unreachable** (same proposal-flow gap) |
| Read admins/treasury/paused/reserves/positions | `admins`, `threshold`, `treasury`, `is_paused`, `reserve_state`, `position`, `snapshot`, `events` | Anyone | — (view) |

### `flash_loan.rs` — `FlashLoanContract`

| Action | Function | Role (designed) | Enforcement |
|---|---|---|---|
| Initialize | `initialize` | — (bootstrap) | ❌ none |
| Flash loan / liquidate via flash loan | `flash_loan`, `liquidate_with_flash_loan` | Keeper/User | ❌ none (simulated transfers only) |
| Set fee, pause, resume | `set_fee`, `pause`, `resume` | Admin | ⚠️ broken — `require_admin()` is a documented no-op (`// In production: env.invoker().require_auth();`) |
| Read info / calculate fee | `get_info`, `calculate_fee`, `arbitrage_safeguard` | Anyone | — (view) |

### `arbitrage.rs` — `ArbitrageContract`

| Action | Function | Role (designed) | Enforcement |
|---|---|---|---|
| Initialize | `initialize` | — (bootstrap) | ❌ none |
| Detect opportunity | `detect_opportunity` | Keeper (bot/oracle) | ❌ none |
| Execute arbitrage / report failure | `execute_arbitrage`, `report_failed_arbitrage` | Keeper/User (`arbitrageur`) | ❌ none |
| Update params, pause/unpause | `update_params`, `pause`, `unpause` | Admin | ⚠️ broken |
| Read opportunities / stats / params | `get_active_opportunities`, `get_arbitrageur_stats`, `get_system_stats`, `get_params` | Anyone | — (view) |

### `oracle.rs` — `PriceOracle` (correct) and `PriceOracleSim` (internal sim)

`PriceOracle` is the only oracle-family contract with a fully correct admin check.

| Action | Function | Role (designed) | Enforcement |
|---|---|---|---|
| Initialize | `initialize` | — (bootstrap) | ❌ none |
| Set price | `set_price` (`PriceOracle`) | Admin | ✅ enforced (`caller.require_auth()` + equality) |
| Set price / sanity config (`PriceOracleSim`, used internally by `lending.rs`) | `set_price`, `set_price_at`, `set_sanity_config` | Admin | ❌ none (string-equality only, no cryptographic auth exists in this struct) |
| Read price(s) | `get_price`, `get_price_at`, `admin` | Anyone | — (view) |

### `price_oracle.rs` — `PriceOracleContract`

| Action | Function | Role (designed) | Enforcement |
|---|---|---|---|
| Initialize | `initialize` | — (bootstrap) | ❌ none |
| Update price | `update_price` | Keeper (registered source) | ❌ none — only checks address is *listed*, never authenticates it |
| Add/remove source, update weight/threshold, reset/enable circuit breaker, pause/unpause | `add_price_source`, `remove_price_source`, `update_source_weight`, `set_price_update_threshold`, `reset_circuit_breaker`, `set_circuit_breaker_enabled`, `pause`, `unpause` | Admin | ⚠️ broken |
| Read price / TWAP / sources / alerts | `get_price`, `get_twap`, `get_price_sources`, `get_deviation_alerts`, `is_operational` | Anyone | — (view) |

### `decentralized_oracle.rs` — `DecentralizedOracle`

| Action | Function | Role (designed) | Enforcement |
|---|---|---|---|
| Initialize | `initialize` | — (bootstrap) | ❌ none |
| Register oracle (stake) | `register_oracle` | Keeper (oracle operator) | ❌ none |
| Submit price | `submit_price` | Keeper (registered oracle) | ❌ none |
| Request unbond / withdraw stake | `request_unbond`, `withdraw_stake` | Keeper (self) | ❌ none — anyone can unbond/withdraw *any* oracle's stake to any timeline |
| Slash oracle, update config, pause/unpause | `slash_oracle`, `update_config`, `pause`, `unpause` | Admin | ⚠️ broken, but at least compares the *passed-in* admin argument (not `require_auth`-backed) |
| Read price / stake / reputation | `get_price`, `get_oracle_stake`, `get_oracle_reputation`, `get_oracle_addresses` | Anyone | — (view) |

### `multi_asset_oracle.rs` — `MultiAssetOracleContract`

| Action | Function | Role (designed) | Enforcement |
|---|---|---|---|
| Initialize | `initialize` | — (bootstrap) | ❌ none |
| Submit price | `submit_price` | Keeper | ❌ none |
| Clear alerts, pause/unpause, update asset registry | `clear_deviation_alerts`, `pause`, `unpause`, `update_asset_registry` | Admin | ⚠️ broken |
| Read price / TWAP / history / alerts | `get_price`, `get_twap_price`, `get_batch_prices`, `get_price_history`, `get_deviation_alerts` | Anyone | — (view) |

### `oracle_manager.rs` — `OracleManagerContract`

| Action | Function | Role (designed) | Enforcement |
|---|---|---|---|
| Initialize | `initialize` | — (bootstrap) | ❌ none |
| Submit price | `submit_price` | Keeper (registered oracle) | ❌ none |
| Register/deactivate oracle, update weight/params | `register_oracle`, `deactivate_oracle`, `update_oracle_weight`, `update_aggregation_params` | Admin | ⚠️ broken |
| Read aggregated price / oracle info / alerts | `get_aggregated_price`, `get_oracle_info`, `get_registered_oracles`, `get_price_alerts` | Anyone | — (view) |

### `price_feed_adapters.rs` — `PriceFeedAdaptersContract`

| Action | Function | Role (designed) | Enforcement |
|---|---|---|---|
| Initialize | `initialize` | — (bootstrap) | ❌ none |
| Register/activate/deactivate adapter, update settings/category config | `register_adapter`, `activate_adapter`, `deactivate_adapter`, `update_adapter_settings`, `update_category_config` | Admin | ⚠️ broken |
| Read adapter/category config, validate price | `get_category_config`, `get_adapter_config`, `get_adapters_for_category`, `validate_price`, `get_recommended_aggregation`, `get_all_adapters`, `get_all_category_configs` | Anyone | — (view) |

### `asset_registry.rs` — `AssetRegistryContract`

| Action | Function | Role (designed) | Enforcement |
|---|---|---|---|
| Initialize | `initialize` | — (bootstrap) | ❌ none |
| Update price | `update_price` | Keeper (approved source) | ❌ none — no source-membership check at all, most severe gap in this file |
| Register/update asset, price sources, whitelist, cross-chain mapping, activate/deactivate, pause/unpause | `register_asset`, `update_asset_metadata`, `update_price_config`, `add_price_source`, `remove_price_source`, `register_price_source`, `whitelist_asset`, `remove_from_whitelist`, `register_cross_chain_asset`, `activate_asset`, `deactivate_asset`, `pause`, `unpause` | Admin | ⚠️ broken |
| Read asset / price / history / whitelist / stats | `get_asset`, `get_all_assets`, `get_assets_by_category`, `get_price`, `get_price_history`, `is_whitelisted`, `get_cross_chain_asset`, `get_asset_stats` | Anyone | — (view) |

### `position_manager.rs` — `PositionManagerContract`

| Action | Function | Role (designed) | Enforcement |
|---|---|---|---|
| Initialize | `initialize` | — (bootstrap) | ❌ none |
| Create monitored position, create batch op | `create_monitored_position`, `create_batch_operation` | Admin | ⚠️ broken |
| Monitor positions, execute batch op, acknowledge alert | `monitor_positions`, `execute_batch_operation`, `acknowledge_alert` | Keeper | ❌ none — any caller can execute any user's pending batch or acknowledge any alert |
| Rebalance position | `rebalance_position` | Admin | ⚠️ broken |
| Read analytics / alerts | `get_position_analytics`, `get_user_alerts` | Anyone | — (view) |

### `synthetic_protocol.rs` — `SyntheticProtocolContract`

| Action | Function | Role (designed) | Enforcement |
|---|---|---|---|
| Initialize | `initialize` | — (bootstrap) | ❌ none |
| Mint / burn synthetic, stake / unstake | `mint_synthetic`, `burn_synthetic`, `stake`, `unstake` | User | ❌ none |
| Liquidate position | `liquidate_position` | Keeper | ❌ none |
| Update oracle price | `update_oracle_price` | Keeper (whitelisted oracle) | ❌ none — membership-only check, no auth |
| List asset, pause/unpause, update risk params | `list_asset`, `pause`, `unpause`, `update_risk_params` | Admin | ⚠️ broken |
| Read asset / position / price / stats | `get_asset`, `get_user_position`, `get_asset_price`, `get_protocol_stats`, `get_listed_assets` | Anyone | — (view) |

### `governance.rs` — `GovernanceContract` (off-chain simulation struct)

Not a deployed Soroban contract — no auth primitives exist in this file.

| Action | Function | Role (designed) | Enforcement |
|---|---|---|---|
| Create proposal | `create_proposal` | Governance (proposer, token-gated) | ❌ none; voting-power gate is moot since `get_voting_power` always returns 0 |
| Vote | `vote` | Governance (voter) | ❌ none; caller self-reports their own `voting_power` |
| Execute proposal | `execute_proposal` | Keeper (permissionless executor) | ❌ none (by design — anyone may execute a passed proposal) |
| Cancel proposal | `cancel_proposal` | Governance (original proposer) | ❌ none (checked by field equality only, not auth) |
| Update governance parameters | `update_parameters` | Admin/Governance (intended: proposal-gated) | ❌ none — directly callable by anyone despite doc comment |
| Delegate | `delegate` | Governance (delegator) | N/A — stub, no-op |
| Read proposals / voting power / delegation | `get_proposal`, `get_all_proposals`, `get_active_proposals`, `has_proposal_passed`, `get_voting_power`, `get_delegation` | Anyone | — (view) |

### `governance_v2.rs` — `GovernanceContractV2`

Real voting-period / quorum / execution-delay timelock logic exists here, but bypassable via the admin fast-path.

| Action | Function | Role (designed) | Enforcement |
|---|---|---|---|
| Initialize | `initialize` | — (bootstrap) | ❌ none |
| Create proposal | `create_proposal` | Governance (proposer ≥ threshold voting power) | ❌ none |
| Vote | `vote` | Governance (voter) | ❌ none |
| Execute proposal | `execute_proposal` | Keeper (permissionless, after quorum + delay) | ❌ none (by design) |
| Delegate | `delegate` | Governance (delegator) | ❌ none |
| Update params (voting period/quorum/threshold/delay) | `update_params` | Admin (documented as a temporary fast-path — *"should only be callable through a successful proposal"*) | ⚠️ broken |
| Emergency pause / unpause | `emergency_pause`, `unpause` | Admin | ⚠️ broken |
| Read proposal / params / voting power | `get_proposal`, `get_active_proposals`, `get_params`, `get_voting_power` | Anyone | — (view); `get_voting_power` is a hardcoded mock |

### `synthetic_governance.rs` — `SyntheticGovernanceContract`

| Action | Function | Role (designed) | Enforcement |
|---|---|---|---|
| Initialize | `initialize` | — (bootstrap) | ❌ none |
| Create proposal | `create_proposal` | Governance (proposer) | ❌ none; voting power mocked to a constant |
| Vote | `vote` | Governance (voter) | ❌ none; double-vote guard (`has_voted`) is a stub that always returns `false` |
| Execute proposal | `execute_proposal` | Keeper (permissionless, after timelock) | ❌ none (by design) |
| Delegate | `delegate` | Governance (delegator) | ❌ none — any caller can set delegation for an arbitrary address |
| Create multisig requirement | `create_multisig_requirement` | Admin | ⚠️ broken; result isn't persisted |
| Pause/unpause, update governance params, emergency pause | `pause`, `unpause`, `update_governance_params`, `emergency_pause` | Admin | ⚠️ broken |
| Read proposal / voting power / params | `get_proposal`, `get_active_proposals`, `get_voting_power`, `get_governance_params` | Anyone | — (view) |

---

## Role Capability Rollups

### Admin capabilities (across all contracts)

Admin is the highest-privilege role in every contract. Where designed correctly, Admin
can, per contract: pause/unpause operations, tune risk/fee/interest parameters, manage
allow-lists (collateral types, price sources, adapters, oracles), trigger emergency
actions (shutdown, force recovery, manual circuit-breaker trip), and sweep protocol
fees. **As currently implemented, only `circuit_breaker.rs`, `staking.rs`,
`pausable_token.rs`, and `soroban_token_contract.rs` enforce Admin actions with real
`require_auth()` checks.** Every other contract's admin surface is either unauthenticated
or protected by the broken `require_admin()` pattern (see appendix) — meaning Admin
capability today is either non-functional or wide open, contract by contract.

### Governance capabilities (across all contracts)

Governance participants, in `governance.rs`, `governance_v2.rs`, and
`synthetic_governance.rs`, can: create proposals (subject to a minimum voting-power
threshold), vote for/against within a voting window, delegate voting power to another
address, and — after quorum and any execution delay are satisfied — trigger proposal
execution (open to any Keeper, by design). **None of the three governance contracts
verify voter/proposer/delegator identity with `require_auth()`, and voting power itself
is either mocked to a constant or a non-functional stub in all three** — so the
governance layer does not yet provide real Sybil resistance or non-repudiation. Treat
it as a UI/coordination layer only until these gaps are closed.

### Keeper capabilities (across all contracts)

Keepers are intentionally permissionless in most cases — anyone can: submit oracle
price updates (`update_price`, `submit_price` across all oracle contracts), execute a
passed governance proposal, liquidate an under-collateralized position (`lending.rs`,
`stablecoin.rs`, `synthetic_protocol.rs`), process a stability-pool liquidation credit,
harvest a vault strategy, detect/execute an arbitrage opportunity, and monitor/rebalance
tracked positions. This openness is by design for most of these (permissionless
liquidation and proposal execution are standard DeFi patterns), **but several oracle
and lending "keeper" actions should be restricted to a registered/staked set (e.g.
`decentralized_oracle.rs`'s registered oracles) and currently are not** — see the
appendix for which ones need real membership checks, not just open access.

### User capabilities (across all contracts)

Users can, depending on the contract: transfer/approve/burn tokens, deposit/withdraw
liquidity or collateral, borrow/repay debt, mint/redeem/burn synthetic assets and the
protocol stablecoin, stake/unstake for rewards, and swap between pool assets.
**With the exception of `pausable_token.rs`, `soroban_token_contract.rs`, and the
on-chain `liquidity_pool.rs`, user-facing state-changing functions across this codebase
do not call `require_auth()` on the acting address** — meaning, as written, one account
can currently perform these actions on behalf of another by simply passing that
account's address as a parameter. This is the single highest-impact class of finding
in this audit pass and should be prioritized above any Admin/Governance fix.

---

## Appendix: Enforcement Gaps Found During This Audit

This appendix exists so remediation work can be tracked against a concrete list. Full
threat-model context lives in [`SECURITY_AUDIT_CHECKLIST.md`](SECURITY_AUDIT_CHECKLIST.md).

1. **Broken `require_admin()` pattern** — `governance_v2.rs`, `arbitrage.rs`,
   `stablecoin.rs`, `stability_pool.rs`, `price_oracle.rs`, `multi_asset_oracle.rs`,
   `price_feed_adapters.rs`, `position_manager.rs`, `oracle_manager.rs`,
   `synthetic_protocol.rs`, `synthetic_governance.rs`, `asset_registry.rs`, and
   `vault.rs` all implement admin gating by comparing `env.current_contract_address()`
   (or, in `vault.rs`, simply checking `admin.is_some()`) instead of calling
   `admin.require_auth()`. Fix: store the admin `Address`, call
   `admin.require_auth()`, then compare against the stored value.
2. **Missing `require_auth()` on user-supplied identity parameters** — the large
   majority of `deposit`/`withdraw`/`mint`/`burn`/`stake`/`vote`/`delegate`-style
   functions across the codebase accept an `Address` parameter representing the acting
   party but never authenticate it. Fix: call `.require_auth()` on that parameter
   before mutating state.
3. **`lp_token_storage.rs::mint`** has no auth check whatsoever — any caller can mint
   unlimited LP tokens to any address. This should be restricted to the owning pool
   contract.
4. **`asset_registry.rs::update_price`** has no source-membership check at all (unlike
   its sibling oracle contracts, which at least check list membership).
5. **`lending.rs`** references `ensure_admin()`, which is never defined, and its
   multisig proposal-approval flow has no corresponding `propose_*` entry point —
   both are compile/runtime blockers, not just access-control gaps.
6. **Governance voting power is mocked or stubbed** in `governance.rs`,
   `governance_v2.rs`, and `synthetic_governance.rs` (constant values or
   always-`false` double-vote guards), so quorum and Sybil-resistance guarantees do
   not hold yet.
7. **No admin rotation/transfer function exists in any contract.** Every admin address
   is permanent once set at `initialize()`, and `initialize()` itself is callable by
   anyone in most contracts (first caller wins). A two-step
   `propose_admin`/`accept_admin` pattern plus a re-initialization guard is recommended
   protocol-wide.
8. **Duplicate method names in the same `impl` block** (e.g. `get_pool_info`,
   `get_price_sources`, `get_protocol_stats`, `get_governance_params`,
   `get_asset_stats`, `get_voting_power`, `get_params` each defined twice — once public,
   once private, with the same name) in `stability_pool.rs`, `price_oracle.rs`,
   `synthetic_protocol.rs`, `synthetic_governance.rs`, `asset_registry.rs`, and
   `governance_v2.rs`. These are compile-time blockers independent of access control
   and should be resolved (rename the private helper) before any of the above fixes
   can be validated by `cargo build`.

None of the above are fixed by this documentation change — they are recorded here and
in the security checklist so they can be triaged and fixed as dedicated follow-up work,
tracked function-by-function against this matrix.
