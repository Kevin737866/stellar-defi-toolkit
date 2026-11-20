//! Yield farming vault contract for the Stellar DeFi Toolkit.
//!
//! The vault accepts deposits of a single underlying asset, issues SEP-41 style
//! share tokens, and allocates capital across pluggable yield strategies.
//!
//! ## Features
//! - Strategy registration, switching, and APY-based optimisation
//! - Auto-compounding harvest with a performance fee
//! - Deposit/withdrawal queue that smooths large TVL changes (issue #145)
//! - Emergency withdrawal with a pro-rata redistributed penalty (issue #149)
//! - Permissionless keeper compounding with a caller reward (issue #150)
//!
//! ## Access control
//! - **Admin**: strategy management, fee/treasury settings, pause, queue
//!   configuration, and queue processing. Gated by `require_admin`, which
//!   compares the stored admin address with the caller passed to the
//!   admin-only entry points.
//! - **Keeper**: `compound` and `harvest` are open to any caller and rate
//!   limited by a minimum interval.
//! - **User**: `deposit`, `withdraw`, `emergency_withdraw`,
//!   `cancel_queue_request` operate on the caller-supplied `Address`.
//!
//! Shares are tracked in a per-owner ledger (`SHARES`), so a withdrawal is
//! always capped by the shares the caller actually owns.
//! ## Access Control
//! - **Admin**: `add_strategy`, `switch_strategy`, `optimize_strategy`,
//!   `collect_fees`, `pause`, `unpause`, `emergency_exit`, `set_performance_fee`,
//!   `set_treasury` — `require_admin()` only checks that an admin has been
//!   configured, not that the caller *is* the admin (the source even flags this:
//!   `// In production: verify env.invoker() == self.admin`). See
//!   `docs/ACCESS_CONTROL_MATRIX.md`.
//! - **Keeper**: `harvest` — open to any caller, rate-limited only by
//!   `MIN_HARVEST_INTERVAL`, not by auth.
//! - **User**: `deposit`, `withdraw` — no identity check at all, compounded by the
//!   lack of a per-user share ledger (only a single aggregate `total_shares`).

use soroban_sdk::{contract, Address, Env};
use crate::types::vault::{
    VaultInfo, VaultStrategy, VaultStats, HarvestResult, DepositResult,
    WithdrawResult, MarketConditions, RebalanceOutcome, RebalanceReason,
    RebalanceThresholds, RebalanceTrigger,
};

use soroban_sdk::{contract, contractimpl, Address, Env, Symbol, Vec};

use crate::types::vault::{
    CompoundReceipt, DepositResult, EmergencyWithdrawReceipt, HarvestResult,
    QueueEntry, QueueKind, QueueProcessResult, VaultDataKey, VaultEvent, VaultInfo,
    VaultQueueSummary, VaultStats, VaultStrategy, WithdrawResult,
};

// ─── Constants ───────────────────────────────────────────────────────────────

/// Default performance fee: 10% (1000 bps)
const DEFAULT_PERFORMANCE_FEE_BPS: u32 = 1000;
/// Maximum performance fee: 30% (3000 bps)
const MAX_PERFORMANCE_FEE_BPS: u32 = 3000;
/// Minimum harvest interval: 1 hour in seconds
const MIN_HARVEST_INTERVAL: u64 = 3600;
/// Default queue threshold: 1% of TVL (100 bps)
const DEFAULT_QUEUE_THRESHOLD_BPS: u32 = 100;
/// Default queue processing interval: 1 hour in seconds
const DEFAULT_QUEUE_INTERVAL: u64 = 3600;
/// Minimum queue processing interval: 10 minutes
const MIN_QUEUE_INTERVAL: u64 = 600;
/// Default emergency withdrawal penalty: 5% (500 bps)
pub const DEFAULT_EMERGENCY_PENALTY_BPS: u32 = 500;
/// Maximum emergency withdrawal penalty: 20% (2000 bps)
const MAX_EMERGENCY_PENALTY_BPS: u32 = 2000;
/// Maximum number of strategies a vault may register
const MAX_STRATEGIES: u32 = 10;
/// Seconds per year, used for APY maths
const SECONDS_PER_YEAR: u64 = 365 * 24 * 3600;
/// Maximum number of strategies compounded in a single keeper call
const MAX_COMPOUND_STRATEGIES: u32 = 10;

// ─── Storage Keys ────────────────────────────────────────────────────────────

const ADMIN: Symbol = Symbol::short("ADMIN");
const TREASURY: Symbol = Symbol::short("TREASURY");
const ASSET: Symbol = Symbol::short("ASSET");
const SHARE_TOKEN: Symbol = Symbol::short("SHARETOK");
const TOTAL_SHARES: Symbol = Symbol::short("TOTSHARES");
const TOTAL_ASSETS: Symbol = Symbol::short("TOTASSETS");
const STRATEGIES: Symbol = Symbol::short("STRATS");
const ACTIVE_IDX: Symbol = Symbol::short("ACTIVEIDX");
const PERF_FEE: Symbol = Symbol::short("PERFFEE");
const PAUSED: Symbol = Symbol::short("PAUSED");
const LAST_HARVEST: Symbol = Symbol::short("LASTHARV");
const FEES: Symbol = Symbol::short("FEES");
const QUEUE: Symbol = Symbol::short("QUEUE");
const NEXT_REQ_ID: Symbol = Symbol::short("NEXTREQID");
const QUEUE_THRESHOLD: Symbol = Symbol::short("QTHRESH");
const QUEUE_INTERVAL: Symbol = Symbol::short("QINTERVAL");
const NEXT_PROCESS: Symbol = Symbol::short("NEXTPROC");
const EMERG_PENALTY: Symbol = Symbol::short("EMERPEN");
const KEEPER_REWARD: Symbol = Symbol::short("KEEPRWRD");
const COMPOUND_INTERVAL: Symbol = Symbol::short("CMPINT");
const LAST_COMPOUND: Symbol = Symbol::short("LASTCMP");

// ─── Contract ────────────────────────────────────────────────────────────────

/// Automated yield farming vault with multi-strategy optimisation.
#[contract]
pub struct YieldVaultContract;
pub struct YieldVaultContract {
    /// Underlying asset token address (what users deposit)
    pub asset_token: Address,
    /// Vault share token address (SEP-41 compliant)
    pub share_token: Address,
    /// Total shares issued
    pub total_shares: u64,
    /// Total assets under management
    pub total_assets: u64,
    /// Active strategies
    pub strategies: Vec<VaultStrategy>,
    /// Active strategy index
    pub active_strategy_index: u32,
    /// Performance fee in basis points
    pub performance_fee_bps: u32,
    /// Treasury address for fee collection
    pub treasury: Option<Address>,
    /// Admin address
    pub admin: Option<Address>,
    /// Whether the vault is paused
    pub paused: bool,
    /// Last harvest timestamp
    pub last_harvest: u64,
    /// Accumulated uncollected fees
    pub accumulated_fees: u64,
    /// Rebalance history, most recent last
    pub rebalance_log: Vec<RebalanceOutcome>,
    /// Contract address
    pub address: Option<Address>,
}

#[contractimpl]
impl YieldVaultContract {
    // ─── Setup ───────────────────────────────────────────────────────────────

    /// Initialize the vault.
    ///
    /// `performance_fee_bps` must not exceed [`MAX_PERFORMANCE_FEE_BPS`].
    /// Create a new yield vault
    pub fn new(_env: &Env, asset_token: Address, share_token: Address) -> Self {
        Self {
            asset_token,
            share_token,
            total_shares: 0,
            total_assets: 0,
            strategies: Vec::new(),
            active_strategy_index: 0,
            performance_fee_bps: DEFAULT_PERFORMANCE_FEE_BPS,
            treasury: None,
            admin: None,
            paused: false,
            last_harvest: 0,
            accumulated_fees: 0,
            rebalance_log: Vec::new(),
            address: None,
        }
    }

    /// Create from std string
    pub fn new_std(env: &Env, _asset_token: String, _share_token: String) -> Self {
        Self::new(
            env,
            Address::from_string(&soroban_sdk::String::from_str(env, "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF")),
            Address::from_string(&soroban_sdk::String::from_str(env, "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF")),
        )
    }

    /// Initialize vault with admin and treasury
    pub fn initialize(
        env: Env,
        admin: Address,
        treasury: Address,
        asset_token: Address,
        share_token: Address,
        performance_fee_bps: u32,
    ) {
        if env.storage().instance().has(&ADMIN) {
            panic!("vault already initialized");
        }
        if performance_fee_bps > MAX_PERFORMANCE_FEE_BPS {
            panic!("performance fee exceeds maximum");
        }

        env.storage().instance().set(&ADMIN, &admin);
        env.storage().instance().set(&TREASURY, &treasury);
        env.storage().instance().set(&ASSET, &asset_token);
        env.storage().instance().set(&SHARE_TOKEN, &share_token);
        env.storage().instance().set(&TOTAL_SHARES, &0u64);
        env.storage().instance().set(&TOTAL_ASSETS, &0u64);
        env.storage().instance().set(&ACTIVE_IDX, &0u32);
        env.storage().instance().set(&PERF_FEE, &performance_fee_bps);
        env.storage().instance().set(&PAUSED, &false);
        env.storage()
            .instance()
            .set(&LAST_HARVEST, &env.ledger().timestamp());
        env.storage().instance().set(&FEES, &0u64);
        env.storage()
            .instance()
            .set(&QUEUE_THRESHOLD, &DEFAULT_QUEUE_THRESHOLD_BPS);
        env.storage()
            .instance()
            .set(&QUEUE_INTERVAL, &DEFAULT_QUEUE_INTERVAL);
        env.storage().instance().set(&NEXT_REQ_ID, &1u64);
        env.storage()
            .instance()
            .set(&NEXT_PROCESS, &(env.ledger().timestamp() + DEFAULT_QUEUE_INTERVAL));
        env.storage()
            .instance()
            .set(&EMERG_PENALTY, &DEFAULT_EMERGENCY_PENALTY_BPS);
        env.storage().instance().set(&KEEPER_REWARD, &0u32);
        env.storage()
            .instance()
            .set(&COMPOUND_INTERVAL, &MIN_HARVEST_INTERVAL);
        env.storage().instance().set(&LAST_COMPOUND, &0u64);
    }

    // ─── Strategy management ─────────────────────────────────────────────────

    /// Register a new strategy and return its index.
    pub fn add_strategy(env: Env, admin: Address, strategy: VaultStrategy) -> u32 {
        Self::require_admin(&env, &admin);
        let mut strategies = Self::load_strategies(&env);
        if strategies.len() >= MAX_STRATEGIES {
            panic!("maximum strategies reached");
        }
        strategies.push_back(strategy);
        Self::save_strategies(&env, &strategies);
        strategies.len() - 1
    }

    /// Switch the active strategy, harvesting and moving capital across.
    pub fn switch_strategy(env: Env, admin: Address, new_index: u32) {
        Self::require_admin(&env, &admin);
        Self::require_not_paused(&env);
        Self::switch_strategy_internal(&env, new_index);
    }

    /// Switch to the highest-APY strategy when it beats the current one by at
    /// least `threshold_bps`. Returns the new active index.
    pub fn optimize_strategy(env: Env, admin: Address, threshold_bps: u32) -> u32 {
        Self::require_admin(&env, &admin);
        Self::require_not_paused(&env);

        let strategies = Self::load_strategies(&env);
        let active = Self::active_strategy_index(&env);
        let optimal = Self::optimal_strategy_index(&strategies);
        if optimal == active || strategies.is_empty() {
            return active;
        self.strategies.push(strategy);
        Ok((self.strategies.len() - 1) as u32)
    }

    /// Switch to a different strategy (admin only)
    ///
    /// Withdraws all funds from the current strategy, then deposits into the new one.
    pub fn switch_strategy(&mut self, new_index: u32) -> Result<(), String> {
        self.require_admin()?;
        self.require_not_paused()?;
 
        if new_index as usize >= self.strategies.len() {
            return Err("Strategy index out of bounds".to_string());
        }
 
        if new_index == self.active_strategy_index {
            return Err("Already using this strategy".to_string());
        }
 
        // 1. Harvest pending rewards from current strategy before switching
        let _ = self.harvest_from_strategy(self.active_strategy_index);
 
        // 2. Withdraw all assets from current strategy
        let withdrawn = self.withdraw_from_strategy(self.active_strategy_index, self.total_assets)?;
 
        // 3. Deposit into new strategy
        self.deposit_into_strategy(new_index, withdrawn)?;
 
        self.active_strategy_index = new_index;
        Ok(())
    }

    /// Get the best strategy by estimated APY
    pub fn get_optimal_strategy_index(&self) -> u32 {
        if self.strategies.is_empty() {
            return 0;
        }

        let current_apy = strategies
            .get(active)
            .map(|s| s.estimated_apy_bps)
            .unwrap_or(0);
        let optimal_apy = strategies
            .get(optimal)
            .map(|s| s.estimated_apy_bps)
            .unwrap_or(0);

        if Self::improvement_bps(current_apy, optimal_apy) >= threshold_bps {
            Self::switch_strategy_internal(&env, optimal);
            return optimal;
        }
        active
    }

    // ─── Deposits and withdrawals ────────────────────────────────────────────

    /// Deposit assets into the vault.
    ///
    /// Deposits at or below the queue threshold execute immediately. Larger
    /// deposits are queued so strategy execution is not disrupted by a sudden
    /// TVL change.
    pub fn deposit(env: Env, depositor: Address, amount: u64) -> DepositResult {
        Self::require_not_paused(&env);
        if amount == 0 {
            panic!("deposit amount must be greater than 0");
        }

        if Self::should_queue(&env, amount) {
            let entry = Self::enqueue(&env, depositor.clone(), QueueKind::Deposit, amount, 0);
            Self::emit(
                &env,
                VaultEvent::DepositQueued(depositor.clone(), amount, entry.id),
            );
            return DepositResult {
                depositor: depositor.clone(),
                amount_deposited: 0,
                shares_minted: 0,
                share_price_bps: Self::share_price_bps(&env),
                queued: true,
                request_id: entry.id,
                queue_position: Self::queue_position(&env, QueueKind::Deposit, entry.id),
                estimated_processing_at: entry.processable_at,
            };
        }

        let shares = Self::execute_deposit(&env, depositor.clone(), amount);
        Self::emit(
            &env,
            VaultEvent::DepositExecuted(depositor.clone(), amount, shares),
        );
        DepositResult {
            depositor,
            amount_deposited: amount,
            shares_minted: shares,
            share_price_bps: Self::share_price_bps(&env),
            queued: false,
            request_id: 0,
            queue_position: 0,
            estimated_processing_at: 0,
        }
    }
        let current_apy = self
            .strategies
            .get(self.active_strategy_index as usize)
            .map(|s| s.estimated_apy)
            .unwrap_or(0);

        let optimal_apy = self
            .strategies
            .get(optimal as usize)
            .map(|s| s.estimated_apy)
            .unwrap_or(0);

    /// Withdraw assets by burning shares.
    ///
    /// Withdrawals at or below the queue threshold execute immediately.
    /// Larger withdrawals are queued and bypass-checked only when the vault is
    /// paused, in which case `emergency_withdraw` must be used.
    pub fn withdraw(env: Env, withdrawer: Address, shares: u64) -> WithdrawResult {
        Self::require_not_paused(&env);
        if shares == 0 {
            panic!("shares amount must be greater than 0");
        }
        let balance = Self::shares_of(&env, &withdrawer);
        if shares > balance {
            panic!("insufficient shares");
        }

        let assets = Self::assets_for_shares(&env, shares);
        if Self::should_queue(&env, assets) {
            let entry = Self::enqueue(
                &env,
                withdrawer.clone(),
                QueueKind::Withdrawal,
                shares,
                assets,
            );
            Self::emit(
                &env,
                VaultEvent::WithdrawQueued(withdrawer.clone(), shares, entry.id),
            );
            return WithdrawResult {
                withdrawer: withdrawer.clone(),
                shares_burned: 0,
                amount_withdrawn: 0,
                share_price_bps: Self::share_price_bps(&env),
                queued: true,
                request_id: entry.id,
                queue_position: Self::queue_position(&env, QueueKind::Withdrawal, entry.id),
                estimated_processing_at: entry.processable_at,
            };
        }

        let assets = Self::execute_withdraw(&env, withdrawer.clone(), shares);
        Self::emit(
            &env,
            VaultEvent::WithdrawExecuted(withdrawer.clone(), shares, assets),
        );
        WithdrawResult {
            withdrawer,
            shares_burned: shares,
            amount_withdrawn: assets,
            share_price_bps: Self::share_price_bps(&env),
            queued: false,
            request_id: 0,
            queue_position: 0,
            estimated_processing_at: 0,
        }
    }

    // ─── Queue (issue #145) ──────────────────────────────────────────────────

    /// Cancel a pending queue request owned by `owner`.
    pub fn cancel_queue_request(env: Env, owner: Address, request_id: u64) {
        let mut queue = Self::load_queue(&env);
        let mut index: Option<u32> = None;
        for (i, entry) in queue.iter().enumerate() {
            if entry.id == request_id {
                if entry.owner != owner {
                    panic!("request does not belong to caller");
                }
                index = Some(i as u32);
                break;
            }
        }
        let index = index.unwrap_or_else(|| panic!("request not found"));
        queue.remove(index);
        Self::save_queue(&env, &queue);
        Self::emit(&env, VaultEvent::QueueRequestCancelled(owner, request_id));
    }
    // ─── Rebalancing Triggers ─────────────────────────────────────────────────

    /// Evaluate every enabled market-condition trigger without moving funds
    ///
    /// Returns one entry per condition that was checked. Entries with
    /// `triggered == false` record why the threshold was not crossed, so callers
    /// can log a full rationale either way.
    pub fn evaluate_rebalance(
        &self,
        conditions: &MarketConditions,
        thresholds: &RebalanceThresholds,
    ) -> Result<Vec<RebalanceTrigger>, String> {
        if self.strategies.is_empty() {
            return Ok(Vec::new());
        }

        let strategy_count = self.strategies.len();
        Self::validate_conditions(conditions, strategy_count)?;

        let current = self.active_strategy_index as usize;
        let current_yield = Self::series_value(&conditions.strategy_yields_bps, current);
        let current_risk = Self::series_value(&conditions.strategy_risk_scores, current);
        let current_util = Self::series_value(&conditions.strategy_utilization, current);

        let mut triggers = Vec::new();

        for candidate in 0..strategy_count {
            if candidate == current {
                continue;
            }

            let candidate_yield = Self::series_value(&conditions.strategy_yields_bps, candidate);
            let candidate_risk = Self::series_value(&conditions.strategy_risk_scores, candidate);
            let candidate_util = Self::series_value(&conditions.strategy_utilization, candidate);

            if thresholds.yield_enabled {
                let delta = candidate_yield.saturating_sub(current_yield);
                triggers.push(RebalanceTrigger {
                    reason: RebalanceReason::YieldDifferential,
                    target_strategy_index: candidate as u32,
                    current_strategy_index: current as u32,
                    current_value: current_yield,
                    target_value: candidate_yield,
                    threshold_bps: thresholds.yield_differential_bps,
                    rationale: format!(
                        "yield {} -> {}bps delta {}bps vs threshold {}bps",
                        current_yield, candidate_yield, delta, thresholds.yield_differential_bps
                    ),
                    triggered: delta >= thresholds.yield_differential_bps,
                });
            }

            if thresholds.risk_enabled {
                let delta = current_risk.saturating_sub(candidate_risk);
                triggers.push(RebalanceTrigger {
                    reason: RebalanceReason::RiskScoreChange,
                    target_strategy_index: candidate as u32,
                    current_strategy_index: current as u32,
                    current_value: current_risk,
                    target_value: candidate_risk,
                    threshold_bps: thresholds.risk_score_change_bps,
                    rationale: format!(
                        "risk {} -> {}bps reduction {}bps vs threshold {}bps",
                        current_risk, candidate_risk, delta, thresholds.risk_score_change_bps
                    ),
                    triggered: delta >= thresholds.risk_score_change_bps,
                });
            }

            if thresholds.utilization_enabled {
                let delta = current_util.saturating_sub(candidate_util);
                triggers.push(RebalanceTrigger {
                    reason: RebalanceReason::UtilizationChange,
                    target_strategy_index: candidate as u32,
                    current_strategy_index: current as u32,
                    current_value: current_util,
                    target_value: candidate_util,
                    threshold_bps: thresholds.utilization_change_bps,
                    rationale: format!(
                        "utilization {} -> {}bps reduction {}bps vs threshold {}bps",
                        current_util, candidate_util, delta, thresholds.utilization_change_bps
                    ),
                    triggered: delta >= thresholds.utilization_change_bps,
                });
            }
        }

        Ok(triggers)
    }

    /// Rebalance into the first strategy whose condition crosses its threshold
    ///
    /// Triggers are evaluated in priority order: yield, then risk, then
    /// utilization. Returns `Ok(None)` when no threshold is crossed.
    pub fn rebalance(
        &mut self,
        conditions: &MarketConditions,
        thresholds: &RebalanceThresholds,
    ) -> Result<Option<RebalanceOutcome>, String> {
        self.require_admin()?;
        self.require_not_paused()?;

        let triggers = self.evaluate_rebalance(conditions, thresholds)?;
        let trigger = match triggers.iter().find(|t| t.triggered) {
            Some(t) => t.clone(),
            None => return Ok(None),
        };

        let outcome = self.apply_rebalance(&trigger)?;
        Ok(Some(outcome))
    }

    /// Force a rebalance into `target_index`, bypassing every threshold
    ///
    /// Manual override is always available to the admin, including when market
    /// conditions would not otherwise justify a move.
    pub fn manual_rebalance(&mut self, target_index: u32) -> Result<RebalanceOutcome, String> {
        self.require_admin()?;
        self.require_not_paused()?;

        if target_index as usize >= self.strategies.len() {
            return Err("Strategy index out of bounds".to_string());
        }
        if target_index == self.active_strategy_index {
            return Err("Already using this strategy".to_string());
        }

        let current = self.active_strategy_index;
        let trigger = RebalanceTrigger {
            reason: RebalanceReason::ManualOverride,
            target_strategy_index: target_index,
            current_strategy_index: current,
            current_value: Self::strategy_yield(&self.strategies, current as usize),
            target_value: Self::strategy_yield(&self.strategies, target_index as usize),
            threshold_bps: 0,
            rationale: format!("manual override {} -> {}", current, target_index),
            triggered: true,
        };

        self.apply_rebalance(&trigger)
    }

    /// Rebalance log, most recent last
    pub fn get_rebalance_log(&self) -> Vec<RebalanceOutcome> {
        self.rebalance_log.clone()
    }

    fn apply_rebalance(&mut self, trigger: &RebalanceTrigger) -> Result<RebalanceOutcome, String> {
        let previous = self.active_strategy_index;
        let assets_moved = self.total_assets;

        self.switch_strategy(trigger.target_strategy_index)?;

        let outcome = RebalanceOutcome {
            trigger: trigger.clone(),
            previous_strategy_index: previous,
            new_strategy_index: self.active_strategy_index,
            assets_moved,
        };
        self.rebalance_log.push(outcome.clone());

        Ok(outcome)
    }

    fn validate_conditions(conditions: &MarketConditions, strategy_count: usize) -> Result<(), String> {
        if conditions.strategy_yields_bps.len() != strategy_count
            || conditions.strategy_risk_scores.len() != strategy_count
            || conditions.strategy_utilization.len() != strategy_count
        {
            return Err("Market conditions length must match strategy count".to_string());
        }
        Ok(())
    }

    fn series_value(series: &[u32], index: usize) -> u32 {
        series.get(index).copied().unwrap_or(0)
    }

    fn strategy_yield(strategies: &[VaultStrategy], index: usize) -> u32 {
        strategies.get(index).map(|s| s.estimated_apy).unwrap_or(0)
    }

    // ─── Deposit / Withdrawal ─────────────────────────────────────────────────

    /// Process every pending request whose delay has elapsed.
    ///
    /// Processed withdrawals release capital first so they are not blocked by
    /// the deposits they share a window with.
    pub fn process_queue(env: Env, admin: Address) -> QueueProcessResult {
        Self::require_admin(&env, &admin);
        Self::require_not_paused(&env);

        let now = env.ledger().timestamp();
        if !Self::window_is_open(&env) {
            panic!("queue processing window not reached");
        }

        let queue = Self::load_queue(&env);
        let mut remaining = Vec::new(&env);
        let mut withdrawals = Vec::new(&env);
        let mut deposits = Vec::new(&env);

        for entry in queue.iter() {
            if entry.processable_at > now {
                remaining.push_back(entry);
            } else if entry.kind == QueueKind::Withdrawal {
                withdrawals.push_back(entry);
            } else {
                deposits.push_back(entry);
            }
        }

        let mut result = QueueProcessResult {
            deposits_processed: 0,
            withdrawals_processed: 0,
            deposited_amount: 0,
            withdrawn_amount: 0,
            processed_at: now,
        };

        for entry in withdrawals.iter() {
            let assets = Self::execute_withdraw(&env, entry.owner.clone(), entry.amount);
            result.withdrawals_processed += 1;
            result.withdrawn_amount += assets;
            Self::emit(
                &env,
                VaultEvent::WithdrawExecuted(entry.owner, entry.amount, assets),
            );
        }

        for entry in deposits.iter() {
            let shares = Self::execute_deposit(&env, entry.owner.clone(), entry.amount);
            result.deposits_processed += 1;
            result.deposited_amount += entry.amount;
            Self::emit(
                &env,
                VaultEvent::DepositExecuted(entry.owner, entry.amount, shares),
            );
        }

        let interval = Self::queue_interval(&env);
        env.storage().instance().set(&NEXT_PROCESS, &(now + interval));
        Self::save_queue(&env, &remaining);
        Self::emit(
            &env,
            VaultEvent::QueueProcessed(result.deposits_processed, result.withdrawals_processed),
        );
        result
    }

    /// Look up a queue entry by id.
    pub fn get_queue_entry(env: Env, request_id: u64) -> QueueEntry {
        for entry in Self::load_queue(&env).iter() {
            if entry.id == request_id {
                return entry;
            }
        }
        panic!("request not found");
    }

    /// Pending queue requests owned by `owner`, in processing order.
    pub fn get_owner_requests(env: Env, owner: Address) -> Vec<QueueEntry> {
        let mut result = Vec::new(&env);
        for entry in Self::load_queue(&env).iter() {
            if entry.owner == owner {
                result.push_back(entry);
            }
        }
        result
    }

    /// Summary of both queues, including the next processing window.
    pub fn queue_summary(env: Env) -> VaultQueueSummary {
        let mut summary = VaultQueueSummary {
            pending_deposits: 0,
            pending_withdrawals: 0,
            queued_deposit_amount: 0,
            queued_withdrawal_shares: 0,
            next_processing_at: Self::next_processing_at(&env),
            processing_interval: Self::queue_interval(&env),
        };
        for entry in Self::load_queue(&env).iter() {
            match entry.kind {
                QueueKind::Deposit => {
                    summary.pending_deposits += 1;
                    summary.queued_deposit_amount += entry.amount;
                }
                QueueKind::Withdrawal => {
                    summary.pending_withdrawals += 1;
                    summary.queued_withdrawal_shares += entry.amount;
                }
            }
        }
        summary
    }

    /// 1-based position of a request within its queue, or `0` when the request
    /// is not pending.
    pub fn queue_position_of(env: Env, request_id: u64) -> u32 {
        for entry in Self::load_queue(&env).iter() {
            if entry.id == request_id {
                return Self::queue_position(&env, entry.kind, request_id);
            }
        }
        0
    }

    /// Number of seconds until the next queue processing window.
    pub fn time_to_next_processing(env: Env) -> u64 {
        Self::next_processing_at(&env)
            .saturating_sub(env.ledger().timestamp())
    }

    /// Update the queue threshold (basis points of TVL) and interval (seconds).
    pub fn set_queue_config(env: Env, admin: Address, threshold_bps: u32, interval: u64) {
        Self::require_admin(&env, &admin);
        if threshold_bps > 10_000 {
            panic!("threshold cannot exceed 100%");
        }
        if interval < MIN_QUEUE_INTERVAL {
            panic!("interval below minimum");
        }
        env.storage().instance().set(&QUEUE_THRESHOLD, &threshold_bps);
        env.storage().instance().set(&QUEUE_INTERVAL, &interval);
        // Re-anchor the schedule so the new interval takes effect immediately.
        env.storage()
            .instance()
            .set(&NEXT_PROCESS, &(env.ledger().timestamp() + interval));
    }

    /// Current queue threshold in basis points of TVL.
    pub fn queue_threshold_bps(env: Env) -> u32 {
        env.storage().instance().get(&QUEUE_THRESHOLD).unwrap_or(0)
    }

    // ─── Emergency withdrawal (issue #149) ───────────────────────────────────

    /// Exit the vault immediately while it is paused, paying a penalty that is
    /// redistributed pro-rata to the remaining depositors.
    ///
    /// The penalty is converted into extra assets left behind by the exiting
    /// depositor, so the share price of everyone still in the vault rises.
    pub fn emergency_withdraw(env: Env, owner: Address, shares: u64) -> EmergencyWithdrawReceipt {
        if !Self::paused(&env) {
            panic!("emergency withdrawal requires a paused vault");
        }
        if shares == 0 {
            panic!("shares amount must be greater than 0");
        }
        let balance = Self::shares_of(&env, &owner);
        if shares > balance {
            panic!("insufficient shares");
        }
        if shares == Self::total_shares(&env) && !Self::load_queue(&env).is_empty() {
            // Exiting the whole vault would leave queued users with nothing.
            panic!("cannot exit the entire vault while requests are queued");
        }

        let gross = Self::assets_for_shares(&env, shares);
        let penalty_bps = Self::emergency_penalty_bps(env.clone());
        let penalty = gross * penalty_bps as u64 / 10_000;
        let payout = gross - penalty;

        Self::burn_shares(&env, &owner, shares);
        Self::set_total_shares(&env, Self::total_shares(&env) - shares);
        // `penalty` stays in the vault, so remaining shares are now worth more.
        Self::set_total_assets(&env, Self::total_assets(&env) - payout);

        let receipt = EmergencyWithdrawReceipt {
            owner: owner.clone(),
            shares_burned: shares,
            assets_withdrawn: payout,
            penalty_amount: penalty,
            penalty_bps,
            timestamp: env.ledger().timestamp(),
        };
        Self::emit(
            &env,
            VaultEvent::EmergencyWithdrawn(owner, shares, payout, penalty, penalty_bps),
        );
        receipt
    }

    /// Update the emergency withdrawal penalty in basis points.
    pub fn set_emergency_penalty(env: Env, admin: Address, penalty_bps: u32) {
        Self::require_admin(&env, &admin);
        if penalty_bps > MAX_EMERGENCY_PENALTY_BPS {
            panic!("penalty exceeds maximum");
        }
        env.storage().instance().set(&EMERG_PENALTY, &penalty_bps);
    }

    /// Current emergency withdrawal penalty in basis points.
    pub fn emergency_penalty_bps(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&EMERG_PENALTY)
            .unwrap_or(DEFAULT_EMERGENCY_PENALTY_BPS)
    }

    // ─── Harvest and compounding (issue #150) ────────────────────────────────

    /// Harvest the active strategy and auto-compound the rewards.
    pub fn harvest(env: Env) -> HarvestResult {
        Self::require_not_paused(&env);
        let now = env.ledger().timestamp();
        let last_harvest = Self::last_harvest(&env);
        if now < last_harvest + MIN_HARVEST_INTERVAL {
            panic!("harvest too soon");
        }

        let mut active = Vec::new(&env);
        active.push_back(Self::active_strategy_index(&env));
        let raw_rewards = Self::harvest_from_strategies(&env, &active);
        if raw_rewards == 0 {
            return HarvestResult {
                raw_rewards: 0,
                performance_fee: 0,
                net_rewards: 0,
                compounded_amount: 0,
                new_total_assets: Self::total_assets(&env),
            };
        }

        let fee_bps = Self::performance_fee_bps(&env);
        let performance_fee = raw_rewards * fee_bps as u64 / 10_000;
        let net_rewards = raw_rewards - performance_fee;

        Self::set_fees(&env, Self::fees(&env) + performance_fee);
        Self::set_total_assets(&env, Self::total_assets(&env) + net_rewards);
        Self::set_last_harvest(&env, now);
        Self::emit(&env, VaultEvent::Compounded(env.current_contract_address(), raw_rewards, 0, net_rewards));

        HarvestResult {
            raw_rewards,
            performance_fee,
            net_rewards,
            compounded_amount: net_rewards,
            new_total_assets: Self::total_assets(&env),
        }
    }

    /// Compound one or more strategies on behalf of the vault.
    ///
    /// Callable by anyone once the compounding interval has elapsed. The keeper
    /// is paid `keeper_reward_bps` of the harvested rewards in vault shares, so
    /// gas costs are covered without draining the asset balance.
    pub fn compound(env: Env, keeper: Address, strategy_indices: Vec<u32>) -> CompoundReceipt {
        Self::require_not_paused(&env);
        let now = env.ledger().timestamp();
        let interval = Self::compound_interval(env.clone());
        let last = Self::last_compound(&env);
        if now < last + interval {
            panic!("compound too soon");
        }

        let strategies = Self::load_strategies(&env);
        let strategy_count = strategies.len();

        let mut indices = Vec::new(&env);
        if strategy_indices.is_empty() {
            if strategy_count > 0 {
                indices.push_back(Self::active_strategy_index(&env));
            }
        } else {
            if strategy_indices.len() > MAX_COMPOUND_STRATEGIES {
                panic!("too many strategies in one call");
            }
            for index in strategy_indices.iter() {
                if index >= strategy_count {
                    panic!("strategy index out of bounds");
                }
                indices.push_back(index);
            }
        }

        let gross_rewards = Self::harvest_from_strategies(&env, &indices);
        let fee_bps = Self::performance_fee_bps(&env);
        let performance_fee = gross_rewards * fee_bps as u64 / 10_000;
        let after_fee = gross_rewards - performance_fee;
        let keeper_reward = after_fee * Self::keeper_reward_bps(env.clone()) as u64 / 10_000;
        let net_compounded = after_fee - keeper_reward;

        let mut keeper_shares = 0u64;
        if keeper_reward > 0 {
            let total_shares = Self::total_shares(&env);
            let total_assets = Self::total_assets(&env);
            keeper_shares = if total_shares == 0 || total_assets == 0 {
                keeper_reward
            } else {
                keeper_reward * total_shares / total_assets
            };
            Self::mint_shares(&env, &keeper, keeper_shares);
            Self::set_total_shares(&env, total_shares + keeper_shares);
        }

        Self::set_fees(&env, Self::fees(&env) + performance_fee);
        Self::set_total_assets(&env, Self::total_assets(&env) + gross_rewards);
        Self::set_last_compound(&env, now);
        Self::set_last_harvest(&env, now);

        let receipt = CompoundReceipt {
            keeper: keeper.clone(),
            strategies: indices,
            gross_rewards,
            keeper_reward,
            keeper_reward_shares: keeper_shares,
            performance_fee,
            net_compounded,
            new_total_assets: Self::total_assets(&env),
            timestamp: now,
        };
        Self::emit(
            &env,
            VaultEvent::Compounded(keeper, gross_rewards, keeper_reward, net_compounded),
        );
        receipt
    }

    /// Estimated optimal compounding interval, in seconds, given the reward
    /// rate of `strategy_index`.
    ///
    /// The estimate targets a reward pool worth `target_reward_bps` of TVL per
    /// compound, which keeps the keeper reward meaningful relative to gas.
    pub fn optimal_compound_interval(env: Env, strategy_index: u32) -> u64 {
        let strategies = Self::load_strategies(&env);
        let apy_bps = strategies
            .get(strategy_index)
            .map(|s| s.estimated_apy_bps)
            .unwrap_or(0);
        Self::optimal_interval_for_apy(apy_bps)
    }

    /// Set the keeper reward, in basis points of post-fee rewards.
    pub fn set_keeper_reward(env: Env, admin: Address, reward_bps: u32) {
        Self::require_admin(&env, &admin);
        if reward_bps > 2_000 {
            panic!("keeper reward exceeds maximum");
        }
        env.storage().instance().set(&KEEPER_REWARD, &reward_bps);
    }

    /// Set the minimum interval between compounding calls.
    pub fn set_compound_interval(env: Env, admin: Address, interval: u64) {
        Self::require_admin(&env, &admin);
        if interval == 0 {
            panic!("interval must be greater than 0");
        }
        env.storage().instance().set(&COMPOUND_INTERVAL, &interval);
    }

    /// Minimum interval between compounding calls, in seconds.
    pub fn compound_interval(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&COMPOUND_INTERVAL)
            .unwrap_or(MIN_HARVEST_INTERVAL)
    }

    /// Whether `compound` may be called at the current ledger time.
    pub fn can_compound(env: Env) -> bool {
        let now = env.ledger().timestamp();
        now >= Self::last_compound(&env).saturating_add(Self::compound_interval(env.clone()))
    }

    /// Current keeper reward in basis points.
    pub fn keeper_reward_bps(env: Env) -> u32 {
        env.storage().instance().get(&KEEPER_REWARD).unwrap_or(0)
    }

    // ─── Fees and admin ──────────────────────────────────────────────────────

    /// Transfer accumulated performance fees to the treasury.
    pub fn collect_fees(env: Env, admin: Address) -> u64 {
        Self::require_admin(&env, &admin);
        let fees = Self::fees(&env);
        if fees == 0 {
            panic!("no fees to collect");
        }
        Self::set_fees(&env, 0);
        fees
    }

    /// Pause the vault. Emergency withdrawals remain available while paused.
    pub fn pause(env: Env, admin: Address) {
        Self::require_admin(&env, &admin);
        if Self::paused(&env) {
            panic!("vault already paused");
    /// Get vault info snapshot
    pub fn get_info(&self, _env: &Env) -> VaultInfo {
        VaultInfo {
            asset_token: self.asset_token.clone(),
            share_token: self.share_token.clone(),
            total_shares: self.total_shares,
            total_assets: self.total_assets,
            share_price: self.get_share_price(),
            active_strategy_index: if self.strategies.len() > 0 { self.active_strategy_index as i32 } else { -1 },
            strategy_count: self.strategies.len(),
            performance_fee_bps: self.performance_fee_bps,
            paused: self.paused,
            last_harvest: self.last_harvest,
        }
    }

    /// Get vault statistics
    pub fn get_stats(&self) -> VaultStats {
        let current_apy = self
            .strategies
            .get(self.active_strategy_index as usize)
            .map(|s| s.estimated_apy)
            .unwrap_or(0);

        VaultStats {
            total_assets: self.total_assets,
            total_shares: self.total_shares,
            share_price: self.get_share_price(),
            current_apy: current_apy as f64,
            accumulated_fees: self.accumulated_fees,
            last_harvest: self.last_harvest,
            paused: self.paused,
        }
        env.storage().instance().set(&PAUSED, &true);
        Self::emit(&env, VaultEvent::PauseChanged(true));
    }

    /// Unpause the vault.
    pub fn unpause(env: Env, admin: Address) {
        Self::require_admin(&env, &admin);
        if !Self::paused(&env) {
            panic!("vault is not paused");
        }
        env.storage().instance().set(&PAUSED, &false);
        Self::emit(&env, VaultEvent::PauseChanged(false));
    }

    /// Withdraw all vault capital from the active strategy without harvesting.
    pub fn emergency_exit(env: Env, admin: Address) -> u64 {
        Self::require_admin(&env, &admin);
        let strategies = Self::load_strategies(&env);
        if strategies.is_empty() {
            panic!("no active strategy");
        }
        Self::total_assets(&env)
    }

    /// Update the performance fee.
    pub fn set_performance_fee(env: Env, admin: Address, fee_bps: u32) {
        Self::require_admin(&env, &admin);
        if fee_bps > MAX_PERFORMANCE_FEE_BPS {
            panic!("fee exceeds maximum");
        }
        env.storage().instance().set(&PERF_FEE, &fee_bps);
    }

    /// Update the treasury address.
    pub fn set_treasury(env: Env, admin: Address, treasury: Address) {
        Self::require_admin(&env, &admin);
        env.storage().instance().set(&TREASURY, &treasury);
    }

    // ─── Views ───────────────────────────────────────────────────────────────

    /// Vault state snapshot.
    pub fn get_info(env: Env) -> VaultInfo {
        let strategies = Self::load_strategies(&env);
        let summary = Self::queue_summary(env.clone());
        VaultInfo {
            asset_token: Self::asset_token(&env),
            share_token: Self::share_token(&env),
            total_shares: Self::total_shares(&env),
            total_assets: Self::total_assets(&env),
            share_price_bps: Self::share_price_bps(&env),
            active_strategy_index: if strategies.is_empty() {
                None
            } else {
                Some(Self::active_strategy_index(&env))
            },
            strategy_count: strategies.len(),
            performance_fee_bps: Self::performance_fee_bps(&env),
            paused: Self::paused(&env),
            last_harvest: Self::last_harvest(&env),
            pending_deposits: summary.pending_deposits,
            pending_withdrawals: summary.pending_withdrawals,
        }
    }

    /// Vault runtime statistics.
    pub fn get_stats(env: Env) -> VaultStats {
        let strategies = Self::load_strategies(&env);
        let current_apy_bps = if strategies.is_empty() {
            0
        } else {
            strategies
                .get(Self::active_strategy_index(&env))
                .map(|s| s.estimated_apy_bps)
                .unwrap_or(0)
        };
        let summary = Self::queue_summary(env.clone());
        VaultStats {
            total_assets: Self::total_assets(&env),
            total_shares: Self::total_shares(&env),
            share_price_bps: Self::share_price_bps(&env),
            current_apy_bps,
            accumulated_fees: Self::fees(&env),
            last_harvest: Self::last_harvest(&env),
            paused: Self::paused(&env),
            pending_deposits: summary.pending_deposits,
            pending_withdrawals: summary.pending_withdrawals,
        }
    }

    /// Shares a given asset amount would mint right now.
    pub fn preview_deposit(env: Env, amount: u64) -> u64 {
        Self::shares_for_assets(&env, amount)
    }

    /// Assets a given share amount would return right now.
    pub fn preview_withdraw(env: Env, shares: u64) -> u64 {
        Self::assets_for_shares(&env, shares)
    }

    /// Current share price in basis points (10000 = 1.0).
    pub fn share_price(env: Env) -> u32 {
        Self::share_price_bps(&env)
    }

    /// Shares owned by `owner`.
    pub fn balance_of(env: Env, owner: Address) -> u64 {
        Self::shares_of(&env, &owner)
    /// Simulate depositing into a strategy (cross-contract call in production)
    fn deposit_into_strategy(&self, index: u32, _amount: u64) -> Result<(), String> {
        if index as usize >= self.strategies.len() {
            return Err("Strategy index out of bounds".to_string());
        }
        // In production: invoke strategy contract via cross-contract call
        // soroban_sdk::invoke_contract(&strategy.contract_address, "deposit", (amount,))
        Ok(())
    }

    /// Simulate withdrawing from a strategy (cross-contract call in production)
    fn withdraw_from_strategy(&self, index: u32, amount: u64) -> Result<u64, String> {
        if index as usize >= self.strategies.len() {
            return Err("Strategy index out of bounds".to_string());
        }
        // In production: invoke strategy contract via cross-contract call
        // soroban_sdk::invoke_contract(&strategy.contract_address, "withdraw", (amount,))
        Ok(amount)
    }

    /// Simulate harvesting rewards from a strategy (cross-contract call in production)
    fn harvest_from_strategy(&self, index: u32) -> Result<u64, String> {
        if index as usize >= self.strategies.len() {
            return Ok(0);
        }
        let strategy = self.strategies.get(index as usize).unwrap();
        // Estimate rewards based on APY and time since last harvest
        let elapsed = self
            .get_current_timestamp()
            .saturating_sub(self.last_harvest);
        let annual_rewards =
            (self.total_assets as f64 * (strategy.estimated_apy as f64 / 10000.0)) as u64;
        let rewards = annual_rewards
            .checked_mul(elapsed)
            .unwrap_or(0)
            / (365 * 24 * 3600);
        Ok(rewards)
    }

    /// Registered strategies.
    pub fn list_strategies(env: Env) -> Vec<VaultStrategy> {
        Self::load_strategies(&env)
    }

    // ─── Internal: deposit/withdraw mechanics ────────────────────────────────

    fn execute_deposit(env: &Env, owner: Address, amount: u64) -> u64 {
        let shares = Self::shares_for_assets(env, amount);
        Self::mint_shares(env, &owner, shares);
        Self::set_total_shares(env, Self::total_shares(env) + shares);
        Self::set_total_assets(env, Self::total_assets(env) + amount);
        shares
    }

    fn execute_withdraw(env: &Env, owner: Address, shares: u64) -> u64 {
        let balance = Self::shares_of(env, &owner);
        if shares > balance {
            panic!("insufficient shares");
        }
        let assets = Self::assets_for_shares(env, shares);
        Self::burn_shares(env, &owner, shares);
        Self::set_total_shares(env, Self::total_shares(env) - shares);
        Self::set_total_assets(env, Self::total_assets(env).saturating_sub(assets));
        assets
    }

    fn shares_for_assets(env: &Env, amount: u64) -> u64 {
        let total_shares = Self::total_shares(env);
        let total_assets = Self::total_assets(env);
        if total_shares == 0 || total_assets == 0 {
            return amount;
        }
        amount.saturating_mul(total_shares) / total_assets
    }

    fn assets_for_shares(env: &Env, shares: u64) -> u64 {
        let total_shares = Self::total_shares(env);
        if total_shares == 0 {
            return 0;
        }
        shares.saturating_mul(Self::total_assets(env)) / total_shares
    }

    fn share_price_bps(env: &Env) -> u32 {
        let total_shares = Self::total_shares(env);
        if total_shares == 0 {
            return 10_000;
        }
        let price = Self::total_assets(env).saturating_mul(10_000) / total_shares;
        if price > u32::MAX as u64 {
            u32::MAX
        } else {
            price as u32
        }
    }
#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::vault::{StrategyType, VaultStrategy};
    use soroban_sdk::{Address, Env};
    use soroban_sdk::testutils::Address as _;

    fn mint_shares(env: &Env, owner: &Address, shares: u64) {
        if shares == 0 {
            return;
        }
        let current = Self::shares_of(env, owner);
        env.storage().persistent().set(&VaultDataKey::Shares(owner.clone()), &(current + shares));
    }

    fn burn_shares(env: &Env, owner: &Address, shares: u64) {
        let current = Self::shares_of(env, owner);
        let remaining = current.saturating_sub(shares);
        env.storage().persistent().set(&VaultDataKey::Shares(owner.clone()), &remaining);
    }

    fn shares_of(env: &Env, owner: &Address) -> u64 {
        env.storage()
            .persistent()
            .get(&VaultDataKey::Shares(owner.clone()))
            .unwrap_or(0)
    }

    // ─── Internal: queue mechanics ───────────────────────────────────────────

    fn should_queue(env: &Env, amount: u64) -> bool {
        let threshold_bps = Self::queue_threshold_bps(env.clone());
        let total_assets = Self::total_assets(env);
        if total_assets == 0 || threshold_bps == 0 {
            return false;
        }
        let threshold = total_assets.saturating_mul(threshold_bps as u64) / 10_000;
        amount > threshold
    }

    fn enqueue(
        env: &Env,
        owner: Address,
        kind: QueueKind,
        amount: u64,
        requested_assets: u64,
    ) -> QueueEntry {
        let now = env.ledger().timestamp();
        let id = env.storage().instance().get(&NEXT_REQ_ID).unwrap_or(1);
        env.storage().instance().set(&NEXT_REQ_ID, &(id + 1));

        let processable_at = Self::next_processing_at(env).max(now);
        let entry = QueueEntry {
            id,
            owner,
            kind,
            amount,
            requested_assets,
            created_at: now,
            processable_at,
        };
        let mut queue = Self::load_queue(env);
        queue.push_back(entry.clone());
        Self::save_queue(env, &queue);
        entry
    }

    fn queue_position(env: &Env, kind: QueueKind, request_id: u64) -> u32 {
        let mut position = 0u32;
        for entry in Self::load_queue(env).iter() {
            if entry.kind != kind {
                continue;
            }
            position += 1;
            if entry.id == request_id {
                return position;
            }
        }
        0
    }

    /// Whether a processing window is currently open.
    fn window_is_open(env: &Env) -> bool {
        match env.storage().instance().get::<Symbol, u64>(&NEXT_PROCESS) {
            Some(at) => env.ledger().timestamp() >= at,
            None => false,
        }
    }

    /// Timestamp of the next window that will accept new requests.
    ///
    /// Windows open every `queue_interval` seconds. Requests created while a
    /// window is open wait for the following one, so a user always gets a full
    /// interval to cancel before funds move.
    fn next_processing_at(env: &Env) -> u64 {
        let interval = Self::queue_interval(env);
        match env.storage().instance().get::<Symbol, u64>(&NEXT_PROCESS) {
            Some(at) if env.ledger().timestamp() >= at => at + interval,
            Some(at) => at,
            None => env.ledger().timestamp() + interval,
        }
    }

    fn queue_interval(env: &Env) -> u64 {
        env.storage()
            .instance()
            .get(&QUEUE_INTERVAL)
            .unwrap_or(DEFAULT_QUEUE_INTERVAL)
    }

    fn load_queue(env: &Env) -> Vec<QueueEntry> {
        env.storage()
            .instance()
            .get(&QUEUE)
            .unwrap_or_else(|| Vec::new(env))
    }

    fn save_queue(env: &Env, queue: &Vec<QueueEntry>) {
        env.storage().instance().set(&QUEUE, queue);
    }

    // ─── Internal: strategy mechanics ────────────────────────────────────────

    fn switch_strategy_internal(env: &Env, new_index: u32) {
        let strategies = Self::load_strategies(env);
        if new_index >= strategies.len() {
            panic!("strategy index out of bounds");
        }
        if new_index == Self::active_strategy_index(env) {
            panic!("already using this strategy");
        }
        let mut active = Vec::new(env);
        active.push_back(Self::active_strategy_index(env));
        Self::harvest_from_strategies(env, &active);
        env.storage().instance().set(&ACTIVE_IDX, &new_index);
    }

    fn optimal_strategy_index(strategies: &Vec<VaultStrategy>) -> u32 {
        let mut best_index = 0;
        let mut best_apy = 0;
        for (i, strategy) in strategies.iter().enumerate() {
            if strategy.estimated_apy_bps > best_apy {
                best_apy = strategy.estimated_apy_bps;
                best_index = i as u32;
            }
        }
        best_index
    }

    fn improvement_bps(current: u32, candidate: u32) -> u32 {
        if current == 0 {
            return if candidate > 0 { 10_000 } else { 0 };
        }
        if candidate <= current {
            return 0;
        }
        ((candidate - current) as u64 * 10_000 / current as u64) as u32
    }

    /// Rewards accrued by the given strategies since the last harvest.
    fn harvest_from_strategies(env: &Env, indices: &Vec<u32>) -> u64 {
        let strategies = Self::load_strategies(env);
        let now = env.ledger().timestamp();
        let last = Self::last_harvest(env);
        let elapsed = now.saturating_sub(last);
        if elapsed == 0 {
            return 0;
        }
        let total_assets = Self::total_assets(env);
        let mut rewards = 0u64;
        for index in indices.iter() {
            let strategy = match strategies.get(index) {
                Some(strategy) => strategy,
                None => continue,
            };
            if !strategy.active {
                continue;
            }
            let annual = total_assets / strategies.len().max(1) as u64
                * strategy.estimated_apy_bps as u64
                / 10_000;
            rewards += annual.saturating_mul(elapsed) / SECONDS_PER_YEAR;
        }
        rewards
    }

    fn optimal_interval_for_apy(apy_bps: u32) -> u64 {
        if apy_bps == 0 {
            return SECONDS_PER_YEAR;
        }
        // Harvest often enough that each run collects ~10 bps of TVL, which
        // keeps the keeper reward above the cost of the call.
        let target_reward_bps = 10u64;
        let interval = target_reward_bps * SECONDS_PER_YEAR / apy_bps as u64;
        interval.clamp(MIN_QUEUE_INTERVAL, SECONDS_PER_YEAR)
    }

    fn load_strategies(env: &Env) -> Vec<VaultStrategy> {
        env.storage()
            .instance()
            .get(&STRATEGIES)
            .unwrap_or_else(|| Vec::new(env))
    }

    fn save_strategies(env: &Env, strategies: &Vec<VaultStrategy>) {
        env.storage().instance().set(&STRATEGIES, strategies);
    }

    fn active_strategy_index(env: &Env) -> u32 {
        env.storage().instance().get(&ACTIVE_IDX).unwrap_or(0)
    }

    // ─── Internal: scalar storage accessors ──────────────────────────────────

    fn asset_token(env: &Env) -> Address {
        env.storage().instance().get(&ASSET).unwrap()
    }

    fn share_token(env: &Env) -> Address {
        env.storage().instance().get(&SHARE_TOKEN).unwrap()
    }

    fn total_shares(env: &Env) -> u64 {
        env.storage().instance().get(&TOTAL_SHARES).unwrap_or(0)
    }

    fn set_total_shares(env: &Env, value: u64) {
        env.storage().instance().set(&TOTAL_SHARES, &value);
    }

    fn total_assets(env: &Env) -> u64 {
        env.storage().instance().get(&TOTAL_ASSETS).unwrap_or(0)
    }

    fn set_total_assets(env: &Env, value: u64) {
        env.storage().instance().set(&TOTAL_ASSETS, &value);
    }

    fn performance_fee_bps(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&PERF_FEE)
            .unwrap_or(DEFAULT_PERFORMANCE_FEE_BPS)
    }

    fn paused(env: &Env) -> bool {
        env.storage().instance().get(&PAUSED).unwrap_or(false)
    }

    fn last_harvest(env: &Env) -> u64 {
        env.storage().instance().get(&LAST_HARVEST).unwrap_or(0)
    }

    fn set_last_harvest(env: &Env, value: u64) {
        env.storage().instance().set(&LAST_HARVEST, &value);
    }

    fn fees(env: &Env) -> u64 {
        env.storage().instance().get(&FEES).unwrap_or(0)
    }

    fn set_fees(env: &Env, value: u64) {
        env.storage().instance().set(&FEES, &value);
    }

    fn last_compound(env: &Env) -> u64 {
        env.storage().instance().get(&LAST_COMPOUND).unwrap_or(0)
    }

    fn set_last_compound(env: &Env, value: u64) {
        env.storage().instance().set(&LAST_COMPOUND, &value);
    }

    fn require_admin(env: &Env, admin: &Address) {
        admin.require_auth();
        let stored: Address = match env.storage().instance().get(&ADMIN) {
            Some(stored) => stored,
            None => panic!("admin not set"),
        };
        if &stored != admin {
            panic!("caller is not the admin");
        }
    }

    fn require_not_paused(env: &Env) {
        if Self::paused(env) {
            panic!("vault is paused");
        }
    }

    fn emit(env: &Env, event: VaultEvent) {
        env.events().publish((Symbol::short("VAULT"),), event);
    }
}
