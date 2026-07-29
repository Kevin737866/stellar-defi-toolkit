//! Position Manager Contract — issues #211, #212, #213, #214
//!
//! Provides advanced position management for the synthetic-asset protocol:
//!
//! ## Issue #211 — Batch position operations
//! - `batch_close_positions`    — close up to 20 positions in one call
//! - `batch_rebalance_positions`— rebalance up to 20 positions in one call
//! - `batch_adjust_collateral`  — add/remove collateral for up to 20 positions
//!   All three support partial failure: individual item errors are collected and
//!   returned without rolling back the whole batch.
//!
//! ## Issue #212 — Position risk scoring (0-10000)
//! - `calculate_risk_score` — weighted composite of four factors
//!   - Collateral ratio  50 %
//!   - Asset volatility  30 %
//!   - Position age      10 %
//!   - Market conditions 10 %
//!
//! ## Issue #213 — Alert system with severity levels
//! - Alert types: `LowCollateral`, `PriceVolatility`, `LiquidationRisk`, `OracleFailure`
//! - Severities:  `Info`, `Warning`, `Critical`, `Emergency`
//! - `acknowledge_alert` — marks a single alert as seen
//! - Per-position alert history; deduplication window prevents spam
//!
//! ## Issue #214 — Position analytics / PnL
//! - `get_position_analytics` — PnL, ROI (bps), max-drawdown, Sharpe ratio
//! - `export_position_analytics` — serialisable snapshot for a single position

use soroban_sdk::{
    contract, contractimpl, contracttype,
    Address, Env, Symbol, Vec, Map,
    unwrap::UnwrapOptimized,
};
use crate::types::synthetic::SyntheticPosition;

// ─── Constants ───────────────────────────────────────────────────────────────

/// Maximum positions allowed in a single batch (issue #211)
const MAX_BATCH_SIZE: u32 = 20;

/// Minimum interval between repeated alerts for the same position+type (1 h)
const ALERT_DEDUP_WINDOW: u64 = 3600;

/// How often position metadata is refreshed (1 h)
const MONITORING_INTERVAL: u64 = 3600;

/// Minimum position size (100 USD, 8 decimals)
const MIN_POSITION_SIZE: u64 = 100_000_000;

/// Rebalancing threshold — ratio must change by at least this (1 %)
const REBALANCING_THRESHOLD: u32 = 100;

// ─── Risk-score weights (must sum to 10 000) — issue #212 ─────────────────────
/// Weight: collateral ratio component  (50 %)
const WEIGHT_COLLATERAL: u32 = 5_000;
/// Weight: asset volatility component  (30 %)
const WEIGHT_VOLATILITY: u32 = 3_000;
/// Weight: position age component      (10 %)
const WEIGHT_AGE: u32 = 1_000;
/// Weight: market conditions component (10 %)
const WEIGHT_MARKET: u32 = 1_000;

// ─── Storage Keys ─────────────────────────────────────────────────────────────

const ADMIN: Symbol              = Symbol::short("ADMIN");
const USER_POSITIONS: Symbol     = Symbol::short("USER_POS");
const POSITION_META: Symbol      = Symbol::short("POS_META");
const ALERTS: Symbol             = Symbol::short("ALERTS");
const ALERT_LAST_TS: Symbol      = Symbol::short("ALT_TS");   // dedup timestamps
const BATCH_OPS: Symbol          = Symbol::short("BATCH_OPS");
const ANALYTICS: Symbol          = Symbol::short("ANALYTICS");

// ─── Types: Position ──────────────────────────────────────────────────────────

/// Lifecycle status of a position
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum PositionStatus {
    Active,
    Warning,
    Liquidating,
    Closed,
    Frozen,
}

/// Persisted metadata for a monitored position
#[derive(Clone, Debug)]
#[contracttype]
pub struct PositionMetadata {
    pub position_id: u64,
    pub owner: Address,
    pub asset_id: u32,
    pub created_at: u64,
    pub last_updated: u64,
    pub status: PositionStatus,
    /// Composite risk score 0-10 000 (issue #212)
    pub risk_score: u32,
}

// ─── Types: Risk scoring (issue #212) ────────────────────────────────────────

/// Breakdown of a composite risk score
#[derive(Clone, Debug)]
#[contracttype]
pub struct RiskScoreBreakdown {
    /// Final weighted score 0-10 000
    pub total_score: u32,
    /// Collateral-ratio component score 0-10 000 (weight 50 %)
    pub collateral_component: u32,
    /// Volatility component score 0-10 000 (weight 30 %)
    pub volatility_component: u32,
    /// Age component score 0-10 000 (weight 10 %)
    pub age_component: u32,
    /// Market-condition component score 0-10 000 (weight 10 %)
    pub market_component: u32,
}

// ─── Types: Alerts (issue #213) ───────────────────────────────────────────────

/// Categories of position alerts
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum AlertType {
    LowCollateral,
    PriceVolatility,
    LiquidationRisk,
    OracleFailure,
}

/// How urgent an alert is
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
    Emergency,
}

/// A single position alert
#[derive(Clone, Debug)]
#[contracttype]
pub struct PositionAlert {
    pub alert_id: u64,
    pub position_id: u64,
    pub alert_type: AlertType,
    pub severity: AlertSeverity,
    /// Human-readable message stored as a short Symbol
    pub message: Symbol,
    pub timestamp: u64,
    pub acknowledged: bool,
}

// ─── Types: Batch operations (issue #211) ─────────────────────────────────────

/// One item inside a batch-close request
#[derive(Clone, Debug)]
#[contracttype]
pub struct BatchCloseItem {
    pub position_id: u64,
    pub owner: Address,
}

/// One item inside a batch-rebalance request
#[derive(Clone, Debug)]
#[contracttype]
pub struct BatchRebalanceItem {
    pub position_id: u64,
    pub owner: Address,
    /// Desired target collateral ratio (basis points)
    pub target_ratio: u32,
}

/// One item inside a batch-collateral-adjustment request
#[derive(Clone, Debug)]
#[contracttype]
pub struct BatchCollateralItem {
    pub position_id: u64,
    pub owner: Address,
    pub collateral_token: Address,
    /// Positive = add, negative = remove (stored as i64 here)
    pub delta: i64,
}

/// Result of a single item inside any batch
#[derive(Clone, Debug)]
#[contracttype]
pub struct BatchItemResult {
    pub position_id: u64,
    pub success: bool,
    /// Short error code if failed, empty Symbol if success
    pub error_code: Symbol,
}

/// Aggregated result returned from a batch call
#[derive(Clone, Debug)]
#[contracttype]
pub struct BatchResult {
    pub total: u32,
    pub succeeded: u32,
    pub failed: u32,
    pub results: Vec<BatchItemResult>,
}

// ─── Types: Analytics (issue #214) ───────────────────────────────────────────

/// Full performance analytics for a single position
#[derive(Clone, Debug)]
#[contracttype]
pub struct PositionAnalytics {
    pub position_id: u64,
    pub owner: Address,
    pub asset_id: u32,
    /// Profit / loss in USD (8 decimals). Negative = loss.
    pub pnl: i64,
    /// ROI expressed in basis points relative to initial collateral
    pub roi_bps: i32,
    /// Maximum drawdown seen over position lifetime (basis points)
    pub max_drawdown_bps: u32,
    /// Sharpe ratio scaled by 10 000 (e.g. 15 000 = 1.5)
    pub sharpe_ratio_scaled: i32,
    /// Entry price at position creation (8 decimals)
    pub entry_price: u64,
    /// Most recent mark price used for PnL (8 decimals)
    pub current_price: u64,
    /// Position size in synthetic units (8 decimals)
    pub synthetic_amount: u64,
    /// Initial collateral value at entry (8 decimals)
    pub initial_collateral: u64,
    /// Days the position has been open
    pub days_held: u32,
    /// Running sum of daily returns (scaled by 10 000) for Sharpe numerator
    pub daily_returns_sum: i64,
    /// Running sum of squared daily returns for Sharpe denominator
    pub daily_returns_sq_sum: u64,
    /// Number of daily return samples recorded
    pub daily_return_samples: u32,
    /// Snapshot timestamp
    pub last_updated: u64,
}

// ─── Contract ────────────────────────────────────────────────────────────────

#[contract]
pub struct PositionManagerContract;

#[contractimpl]
impl PositionManagerContract {

    // =========================================================================
    // Lifecycle
    // =========================================================================

    /// Initialise the position manager.  Must be called exactly once.
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&ADMIN) {
            panic!("already initialised");
        }
        env.storage().instance().set(&ADMIN, &admin);

        let empty_pos: Map<Address, Vec<SyntheticPosition>> = Map::new(&env);
        env.storage().instance().set(&USER_POSITIONS, &empty_pos);

        let empty_meta: Map<u64, PositionMetadata> = Map::new(&env);
        env.storage().instance().set(&POSITION_META, &empty_meta);

        let empty_alerts: Map<u64, Vec<PositionAlert>> = Map::new(&env);
        env.storage().instance().set(&ALERTS, &empty_alerts);

        let empty_dedup: Map<u64, u64> = Map::new(&env);
        env.storage().instance().set(&ALERT_LAST_TS, &empty_dedup);

        let empty_batch: Map<u64, BatchResult> = Map::new(&env);
        env.storage().instance().set(&BATCH_OPS, &empty_batch);

        let empty_analytics: Map<u64, PositionAnalytics> = Map::new(&env);
        env.storage().instance().set(&ANALYTICS, &empty_analytics);

        env.events().publish((Symbol::short("PM_INIT"),), admin);
    }

    /// Create a new monitored position and record its opening analytics state.
    ///
    /// `entry_price` is the oracle price at creation (8 decimals).
    pub fn create_monitored_position(
        env: Env,
        user: Address,
        asset_id: u32,
        collateral_token: Address,
        collateral_amount: u64,
        synthetic_amount: u64,
        target_ratio: u32,
        entry_price: u64,
    ) -> u64 {
        Self::require_admin(&env);

        if synthetic_amount < MIN_POSITION_SIZE {
            panic!("position too small");
        }

        let now = env.ledger().timestamp();
        let position_id = env.ledger().seq_num() as u64;

        // Build synthetic position
        let mut collateral_map: Map<Address, u64> = Map::new(&env);
        collateral_map.set(collateral_token, collateral_amount);

        let position = SyntheticPosition {
            owner: user.clone(),
            asset_id,
            synthetic_amount,
            collateral_deposits: collateral_map,
            debt_amount: synthetic_amount,
            collateral_ratio: target_ratio,
            created_at: now,
            last_updated: now,
            liquidating: false,
        };

        // Persist in user positions map
        let mut user_map = Self::load_user_positions(&env);
        let mut pos_list = user_map.get(user.clone()).unwrap_or_else(|| Vec::new(&env));
        pos_list.push_back(position);
        user_map.set(user.clone(), pos_list);
        env.storage().instance().set(&USER_POSITIONS, &user_map);

        // Compute initial risk score
        let risk = Self::compute_risk_score_internal(target_ratio, 3000, 0, 5000).total_score;

        // Persist metadata
        let meta = PositionMetadata {
            position_id,
            owner: user.clone(),
            asset_id,
            created_at: now,
            last_updated: now,
            status: PositionStatus::Active,
            risk_score: risk,
        };
        let mut meta_map = Self::load_meta(&env);
        meta_map.set(position_id, meta);
        env.storage().instance().set(&POSITION_META, &meta_map);

        // Seed analytics record (issue #214)
        let analytics = PositionAnalytics {
            position_id,
            owner: user.clone(),
            asset_id,
            pnl: 0,
            roi_bps: 0,
            max_drawdown_bps: 0,
            sharpe_ratio_scaled: 0,
            entry_price,
            current_price: entry_price,
            synthetic_amount,
            initial_collateral: collateral_amount,
            days_held: 0,
            daily_returns_sum: 0,
            daily_returns_sq_sum: 0,
            daily_return_samples: 0,
            last_updated: now,
        };
        let mut anl_map = Self::load_analytics(&env);
        anl_map.set(position_id, analytics);
        env.storage().instance().set(&ANALYTICS, &anl_map);

        env.events().publish(
            (Symbol::short("POS_CREATED"),),
            (user, position_id, asset_id),
        );
        position_id
    }

    // =========================================================================
    // Issue #211 — Batch position operations
    // =========================================================================

    /// Close up to `MAX_BATCH_SIZE` (20) positions in a single call.
    ///
    /// Each item is attempted independently; a failure on one position does
    /// **not** roll back the others (partial-failure semantics).
    pub fn batch_close_positions(
        env: Env,
        caller: Address,
        items: Vec<BatchCloseItem>,
    ) -> BatchResult {
        caller.require_auth();

        let count = items.len();
        if count > MAX_BATCH_SIZE {
            panic!("batch exceeds limit of 20");
        }

        let mut results: Vec<BatchItemResult> = Vec::new(&env);
        let mut succeeded: u32 = 0;
        let mut failed: u32 = 0;

        let mut meta_map = Self::load_meta(&env);
        let mut user_map = Self::load_user_positions(&env);
        let mut anl_map  = Self::load_analytics(&env);
        let now = env.ledger().timestamp();

        for item in items.iter() {
            // Validate ownership and status
            let ok = if let Some(mut meta) = meta_map.get(item.position_id) {
                if meta.owner != item.owner {
                    false // not owned by claimed address
                } else if meta.status == PositionStatus::Closed {
                    false // already closed
                } else if meta.status == PositionStatus::Liquidating {
                    false // in liquidation, cannot batch-close
                } else {
                    // Mark closed in metadata
                    meta.status = PositionStatus::Closed;
                    meta.last_updated = now;
                    meta_map.set(item.position_id, meta.clone());

                    // Remove position from user's list
                    if let Some(pos_list) = user_map.get(item.owner.clone()) {
                        let mut new_list: Vec<SyntheticPosition> = Vec::new(&env);
                        for p in pos_list.iter() {
                            if p.asset_id != meta.asset_id || p.owner != meta.owner {
                                new_list.push_back(p);
                            }
                        }
                        user_map.set(item.owner.clone(), new_list);
                    }

                    // Finalise analytics PnL snapshot
                    if let Some(mut anl) = anl_map.get(item.position_id) {
                        anl.days_held = ((now - anl.last_updated) / 86400) as u32;
                        anl.last_updated = now;
                        anl_map.set(item.position_id, anl);
                    }

                    true
                }
            } else {
                false // position not found
            };

            let error_code = if ok {
                Symbol::short("OK")
            } else {
                Symbol::short("ERR")
            };

            results.push_back(BatchItemResult {
                position_id: item.position_id,
                success: ok,
                error_code,
            });

            if ok { succeeded += 1; } else { failed += 1; }
        }

        env.storage().instance().set(&POSITION_META, &meta_map);
        env.storage().instance().set(&USER_POSITIONS, &user_map);
        env.storage().instance().set(&ANALYTICS, &anl_map);

        let batch_result = BatchResult {
            total: count,
            succeeded,
            failed,
            results,
        };

        env.events().publish(
            (Symbol::short("BATCH_CLOSE"),),
            (succeeded, failed),
        );

        batch_result
    }

    /// Rebalance up to 20 positions toward their requested `target_ratio`.
    ///
    /// A rebalance is skipped (partial failure) when:
    /// - position is not found / not owned by the claimed address
    /// - position is Closed or Liquidating
    /// - ratio change is below `REBALANCING_THRESHOLD`
    pub fn batch_rebalance_positions(
        env: Env,
        caller: Address,
        items: Vec<BatchRebalanceItem>,
    ) -> BatchResult {
        caller.require_auth();

        if items.len() > MAX_BATCH_SIZE {
            panic!("batch exceeds limit of 20");
        }

        let count = items.len();
        let mut results: Vec<BatchItemResult> = Vec::new(&env);
        let mut succeeded: u32 = 0;
        let mut failed: u32 = 0;

        let mut meta_map = Self::load_meta(&env);
        let mut user_map = Self::load_user_positions(&env);
        let now = env.ledger().timestamp();

        for item in items.iter() {
            let ok = if let Some(mut meta) = meta_map.get(item.position_id) {
                if meta.owner != item.owner
                    || meta.status == PositionStatus::Closed
                    || meta.status == PositionStatus::Liquidating
                {
                    false
                } else {
                    // Update collateral_ratio on the stored SyntheticPosition
                    if let Some(pos_list) = user_map.get(item.owner.clone()) {
                        let mut new_list: Vec<SyntheticPosition> = Vec::new(&env);
                        let mut changed = false;
                        for mut p in pos_list.iter() {
                            if p.asset_id == meta.asset_id && p.owner == meta.owner {
                                let current = p.collateral_ratio;
                                let target  = item.target_ratio;
                                let delta = if current > target { current - target } else { target - current };
                                if delta >= REBALANCING_THRESHOLD {
                                    p.collateral_ratio = target;
                                    p.last_updated = now;
                                    changed = true;
                                }
                            }
                            new_list.push_back(p);
                        }
                        user_map.set(item.owner.clone(), new_list);
                        if !changed {
                            false // below threshold
                        } else {
                            // Recalculate risk score
                            let breakdown = Self::compute_risk_score_internal(
                                item.target_ratio, 2000, 0, 5000,
                            );
                            meta.risk_score = breakdown.total_score;
                            meta.last_updated = now;
                            if meta.risk_score >= 8000 {
                                meta.status = PositionStatus::Warning;
                            }
                            meta_map.set(item.position_id, meta);
                            true
                        }
                    } else {
                        false
                    }
                }
            } else {
                false
            };

            let error_code = if ok { Symbol::short("OK") } else { Symbol::short("ERR") };
            results.push_back(BatchItemResult { position_id: item.position_id, success: ok, error_code });
            if ok { succeeded += 1; } else { failed += 1; }
        }

        env.storage().instance().set(&POSITION_META, &meta_map);
        env.storage().instance().set(&USER_POSITIONS, &user_map);

        let batch_result = BatchResult { total: count, succeeded, failed, results };
        env.events().publish((Symbol::short("BATCH_RBAL"),), (succeeded, failed));
        batch_result
    }

    /// Add or remove collateral for up to 20 positions in one call.
    ///
    /// `delta > 0` adds collateral; `delta < 0` removes collateral.
    /// A removal that would take collateral below zero is rejected (partial failure).
    pub fn batch_adjust_collateral(
        env: Env,
        caller: Address,
        items: Vec<BatchCollateralItem>,
    ) -> BatchResult {
        caller.require_auth();

        if items.len() > MAX_BATCH_SIZE {
            panic!("batch exceeds limit of 20");
        }

        let count = items.len();
        let mut results: Vec<BatchItemResult> = Vec::new(&env);
        let mut succeeded: u32 = 0;
        let mut failed: u32 = 0;

        let mut meta_map = Self::load_meta(&env);
        let mut user_map = Self::load_user_positions(&env);
        let now = env.ledger().timestamp();

        for item in items.iter() {
            let ok = if let Some(mut meta) = meta_map.get(item.position_id) {
                if meta.owner != item.owner
                    || meta.status == PositionStatus::Closed
                    || meta.status == PositionStatus::Liquidating
                {
                    false
                } else if let Some(pos_list) = user_map.get(item.owner.clone()) {
                    let mut new_list: Vec<SyntheticPosition> = Vec::new(&env);
                    let mut changed = false;
                    for mut p in pos_list.iter() {
                        if p.asset_id == meta.asset_id && p.owner == meta.owner {
                            let current_col = p.collateral_deposits
                                .get(item.collateral_token.clone())
                                .unwrap_or(0);
                            if item.delta < 0 {
                                let remove = (-item.delta) as u64;
                                if remove > current_col {
                                    // Would underflow — skip
                                } else {
                                    p.collateral_deposits.set(item.collateral_token.clone(), current_col - remove);
                                    p.last_updated = now;
                                    changed = true;
                                }
                            } else {
                                let add = item.delta as u64;
                                p.collateral_deposits.set(item.collateral_token.clone(), current_col + add);
                                p.last_updated = now;
                                changed = true;
                            }
                        }
                        new_list.push_back(p);
                    }
                    if changed {
                        user_map.set(item.owner.clone(), new_list);
                        meta.last_updated = now;
                        meta_map.set(item.position_id, meta);
                    }
                    changed
                } else {
                    false
                }
            } else {
                false
            };

            let error_code = if ok { Symbol::short("OK") } else { Symbol::short("ERR") };
            results.push_back(BatchItemResult { position_id: item.position_id, success: ok, error_code });
            if ok { succeeded += 1; } else { failed += 1; }
        }

        env.storage().instance().set(&POSITION_META, &meta_map);
        env.storage().instance().set(&USER_POSITIONS, &user_map);

        let batch_result = BatchResult { total: count, succeeded, failed, results };
        env.events().publish((Symbol::short("BATCH_COL"),), (succeeded, failed));
        batch_result
    }

    // =========================================================================
    // Issue #212 — Position risk scoring
    // =========================================================================

    /// Compute and persist the risk score for `position_id`.
    ///
    /// Callers supply live market inputs; the contract stores the result and
    /// returns the full breakdown.
    ///
    /// # Arguments
    /// * `position_id`      — position to score
    /// * `current_ratio`    — current collateral ratio in basis points
    /// * `volatility_30d`   — 30-day annualised volatility in basis points
    ///                        (e.g. 3000 = 30 %)
    /// * `position_age_days`— days the position has been open
    /// * `market_score`     — external market-condition score 0-10 000
    ///                        (0 = perfect conditions, 10 000 = worst)
    pub fn calculate_risk_score(
        env: Env,
        position_id: u64,
        current_ratio: u32,
        volatility_30d: u32,
        position_age_days: u32,
        market_score: u32,
    ) -> RiskScoreBreakdown {
        let breakdown = Self::compute_risk_score_internal(
            current_ratio,
            volatility_30d,
            position_age_days,
            market_score,
        );

        // Persist back into position metadata
        let mut meta_map = Self::load_meta(&env);
        if let Some(mut meta) = meta_map.get(position_id) {
            meta.risk_score = breakdown.total_score;
            meta.last_updated = env.ledger().timestamp();

            // Upgrade status based on new score
            if meta.status != PositionStatus::Closed && meta.status != PositionStatus::Liquidating {
                meta.status = if breakdown.total_score >= 9500 {
                    PositionStatus::Frozen
                } else if breakdown.total_score >= 8000 {
                    PositionStatus::Warning
                } else {
                    PositionStatus::Active
                };
            }
            meta_map.set(position_id, meta);
            env.storage().instance().set(&POSITION_META, &meta_map);
        }

        env.events().publish(
            (Symbol::short("RISK_SCORED"),),
            (position_id, breakdown.total_score),
        );

        breakdown
    }

    /// Return the stored risk score for a position without re-computing it.
    pub fn get_risk_score(env: Env, position_id: u64) -> u32 {
        let meta_map = Self::load_meta(&env);
        meta_map
            .get(position_id)
            .map(|m| m.risk_score)
            .unwrap_or(10_000) // unknown → maximum risk
    }

    // =========================================================================
    // Issue #213 — Alert system
    // =========================================================================

    /// Raise an alert for a position.
    ///
    /// Deduplication: if the same `(position_id, alert_type)` pair was raised
    /// within `ALERT_DEDUP_WINDOW` seconds, the call is a no-op and returns 0.
    /// Otherwise the new alert ID is returned.
    pub fn raise_alert(
        env: Env,
        position_id: u64,
        alert_type: AlertType,
        severity: AlertSeverity,
        message: Symbol,
    ) -> u64 {
        let now = env.ledger().timestamp();

        // Deduplication key: combine position_id and alert_type discriminant
        let dedup_key: u64 = position_id * 10
            + match alert_type {
                AlertType::LowCollateral    => 0,
                AlertType::PriceVolatility  => 1,
                AlertType::LiquidationRisk  => 2,
                AlertType::OracleFailure    => 3,
            };

        let mut dedup_map: Map<u64, u64> = env
            .storage()
            .instance()
            .get(&ALERT_LAST_TS)
            .unwrap_or_else(|| Map::new(&env));

        if let Some(last_ts) = dedup_map.get(dedup_key) {
            if now - last_ts < ALERT_DEDUP_WINDOW {
                return 0; // duplicate — silently ignore
            }
        }

        // Record dedup timestamp
        dedup_map.set(dedup_key, now);
        env.storage().instance().set(&ALERT_LAST_TS, &dedup_map);

        // Create alert
        let alert_id = env.ledger().seq_num() as u64;
        let alert = PositionAlert {
            alert_id,
            position_id,
            alert_type: alert_type.clone(),
            severity: severity.clone(),
            message,
            timestamp: now,
            acknowledged: false,
        };

        let mut alerts_map = Self::load_alerts(&env);
        let mut pos_alerts = alerts_map
            .get(position_id)
            .unwrap_or_else(|| Vec::new(&env));
        pos_alerts.push_back(alert);
        alerts_map.set(position_id, pos_alerts);
        env.storage().instance().set(&ALERTS, &alerts_map);

        env.events().publish(
            (Symbol::short("ALERT_RAISED"),),
            (alert_id, position_id),
        );
        alert_id
    }

    /// Acknowledge an alert so it no longer appears as outstanding.
    ///
    /// Only the owner of the position the alert belongs to may acknowledge it.
    pub fn acknowledge_alert(env: Env, caller: Address, position_id: u64, alert_id: u64) {
        caller.require_auth();

        // Verify caller owns the position
        let meta_map = Self::load_meta(&env);
        let meta = meta_map.get(position_id)
            .unwrap_or_else(|| panic!("position not found"));
        if meta.owner != caller {
            panic!("not position owner");
        }

        let mut alerts_map = Self::load_alerts(&env);
        let pos_alerts = alerts_map
            .get(position_id)
            .unwrap_or_else(|| panic!("no alerts for position"));

        let mut updated: Vec<PositionAlert> = Vec::new(&env);
        for mut a in pos_alerts.iter() {
            if a.alert_id == alert_id {
                a.acknowledged = true;
            }
            updated.push_back(a);
        }
        alerts_map.set(position_id, updated);
        env.storage().instance().set(&ALERTS, &alerts_map);

        env.events().publish(
            (Symbol::short("ALERT_ACK"),),
            (alert_id, position_id),
        );
    }

    /// Return all alerts for a position (history, including acknowledged ones).
    pub fn get_position_alerts(env: Env, position_id: u64) -> Vec<PositionAlert> {
        let alerts_map = Self::load_alerts(&env);
        alerts_map
            .get(position_id)
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Return all un-acknowledged alerts across all positions owned by `user`.
    pub fn get_user_alerts(env: Env, user: Address) -> Vec<PositionAlert> {
        let meta_map   = Self::load_meta(&env);
        let alerts_map = Self::load_alerts(&env);
        let mut out: Vec<PositionAlert> = Vec::new(&env);

        for meta in meta_map.values() {
            if meta.owner != user {
                continue;
            }
            if let Some(pos_alerts) = alerts_map.get(meta.position_id) {
                for a in pos_alerts.iter() {
                    if !a.acknowledged {
                        out.push_back(a);
                    }
                }
            }
        }
        out
    }

    /// Trigger the standard monitoring pass: evaluate every active position and
    /// raise alerts automatically based on risk score thresholds.
    pub fn monitor_positions(
        env: Env,
        volatility_30d: u32,
        market_score: u32,
    ) {
        let now = env.ledger().timestamp();
        let mut meta_map = Self::load_meta(&env);
        let user_map     = Self::load_user_positions(&env);

        for mut meta in meta_map.values() {
            if meta.status == PositionStatus::Closed {
                continue;
            }
            if now.saturating_sub(meta.last_updated) < MONITORING_INTERVAL {
                continue;
            }

            // Find the underlying position to get current ratio
            let current_ratio = if let Some(pos_list) = user_map.get(meta.owner.clone()) {
                pos_list
                    .iter()
                    .find(|p| p.asset_id == meta.asset_id)
                    .map(|p| p.collateral_ratio)
                    .unwrap_or(15000)
            } else {
                15000
            };

            let age_days = ((now - meta.created_at) / 86400) as u32;
            let breakdown = Self::compute_risk_score_internal(
                current_ratio, volatility_30d, age_days, market_score,
            );

            meta.risk_score   = breakdown.total_score;
            meta.last_updated = now;

            // Raise alerts based on thresholds
            if breakdown.total_score >= 9500 {
                meta.status = PositionStatus::Frozen;
                Self::raise_alert(
                    env.clone(), meta.position_id,
                    AlertType::LiquidationRisk, AlertSeverity::Emergency,
                    Symbol::short("NEAR_LIQ"),
                );
            } else if breakdown.total_score >= 8000 {
                meta.status = PositionStatus::Warning;
                Self::raise_alert(
                    env.clone(), meta.position_id,
                    AlertType::LiquidationRisk, AlertSeverity::Critical,
                    Symbol::short("LIQ_RISK"),
                );
            } else if breakdown.total_score >= 6000 {
                Self::raise_alert(
                    env.clone(), meta.position_id,
                    AlertType::LowCollateral, AlertSeverity::Warning,
                    Symbol::short("LOW_COL"),
                );
            } else {
                meta.status = PositionStatus::Active;
            }

            if volatility_30d >= 5000 {
                Self::raise_alert(
                    env.clone(), meta.position_id,
                    AlertType::PriceVolatility, AlertSeverity::Warning,
                    Symbol::short("HIGH_VOL"),
                );
            }

            meta_map.set(meta.position_id, meta);
        }

        env.storage().instance().set(&POSITION_META, &meta_map);
    }

    // =========================================================================
    // Issue #214 — Position analytics / PnL
    // =========================================================================

    /// Update analytics for `position_id` using the latest mark price.
    ///
    /// Should be called at least once per day to accumulate daily-return
    /// samples for the Sharpe ratio.
    ///
    /// # Arguments
    /// * `current_price`   — latest oracle mark price (8 decimals)
    pub fn update_analytics(env: Env, position_id: u64, current_price: u64) {
        let now = env.ledger().timestamp();
        let mut anl_map = Self::load_analytics(&env);

        let mut anl = anl_map.get(position_id)
            .unwrap_or_else(|| panic!("analytics record not found"));

        // PnL = (current_price - entry_price) * synthetic_amount / 1e8
        let price_delta: i64 = current_price as i64 - anl.entry_price as i64;
        anl.pnl = (price_delta * anl.synthetic_amount as i64) / 100_000_000;

        // ROI in basis points = pnl / initial_collateral * 10000
        if anl.initial_collateral > 0 {
            anl.roi_bps = ((anl.pnl * 10_000) / anl.initial_collateral as i64) as i32;
        }

        // Max drawdown: track the worst (most negative) roi_bps seen
        if anl.roi_bps < 0 {
            let drawdown = (-anl.roi_bps) as u32;
            if drawdown > anl.max_drawdown_bps {
                anl.max_drawdown_bps = drawdown;
            }
        }

        // Daily return sample (in basis points relative to previous price)
        if anl.current_price > 0 && anl.current_price != current_price {
            let daily_ret_bps: i64 =
                ((current_price as i64 - anl.current_price as i64) * 10_000)
                / anl.current_price as i64;
            anl.daily_returns_sum      += daily_ret_bps;
            anl.daily_returns_sq_sum   += (daily_ret_bps * daily_ret_bps) as u64;
            anl.daily_return_samples   += 1;

            // Sharpe ratio = mean_return / std_dev  (scaled by 10 000)
            // mean = sum / n
            // variance = sum_sq/n - mean^2
            // std_dev = sqrt(variance)   — integer-approximated via Newton's method
            if anl.daily_return_samples >= 2 {
                let n = anl.daily_return_samples as i64;
                let mean = anl.daily_returns_sum / n;
                let mean_sq = mean * mean;
                let sq_mean = (anl.daily_returns_sq_sum as i64) / n;
                let variance = sq_mean - mean_sq;
                if variance > 0 {
                    let std_dev = Self::isqrt(variance as u64) as i64;
                    if std_dev > 0 {
                        // Scale mean by 10 000 to keep precision
                        anl.sharpe_ratio_scaled = ((mean * 10_000) / std_dev) as i32;
                    }
                }
            }
        }

        anl.current_price = current_price;
        anl.days_held      = ((now - anl.last_updated) / 86400) as u32;
        anl.last_updated   = now;

        anl_map.set(position_id, anl);
        env.storage().instance().set(&ANALYTICS, &anl_map);

        env.events().publish(
            (Symbol::short("ANL_UPDATED"),),
            (position_id, current_price),
        );
    }

    /// Return the full analytics snapshot for a single position.
    pub fn get_position_analytics(env: Env, position_id: u64) -> PositionAnalytics {
        let anl_map = Self::load_analytics(&env);
        anl_map.get(position_id)
            .unwrap_or_else(|| panic!("analytics record not found"))
    }

    /// Return analytics snapshots for **all** positions owned by `user`.
    /// Satisfies the "exportable per position" acceptance criterion.
    pub fn export_user_analytics(env: Env, user: Address) -> Vec<PositionAnalytics> {
        let meta_map = Self::load_meta(&env);
        let anl_map  = Self::load_analytics(&env);
        let mut out: Vec<PositionAnalytics> = Vec::new(&env);

        for meta in meta_map.values() {
            if meta.owner == user {
                if let Some(anl) = anl_map.get(meta.position_id) {
                    out.push_back(anl);
                }
            }
        }
        out
    }

    // =========================================================================
    // Internal helpers
    // =========================================================================

    /// Core risk-score computation (issue #212).
    ///
    /// Returns a `RiskScoreBreakdown` with weighted components.
    ///
    /// | Factor           | Weight |
    /// |------------------|--------|
    /// | Collateral ratio | 50 %   |
    /// | Asset volatility | 30 %   |
    /// | Position age     | 10 %   |
    /// | Market cond.     | 10 %   |
    ///
    /// Every component is a score 0-10 000 where **higher = riskier**.
    fn compute_risk_score_internal(
        collateral_ratio: u32,   // basis points
        volatility_30d: u32,     // basis points (e.g. 3000 = 30%)
        position_age_days: u32,
        market_score: u32,       // 0-10000
    ) -> RiskScoreBreakdown {
        // ── Collateral component ─────────────────────────────────────────────
        // ratio >= 300 % → 0 risk; ratio <= 110 % → 10 000 (max risk)
        let collateral_component: u32 = if collateral_ratio >= 30000 {
            0
        } else if collateral_ratio <= 11000 {
            10_000
        } else {
            // Linear interpolation between 11 000 and 30 000 bp
            let range = 30000u32 - 11000;
            let dist  = 30000u32.saturating_sub(collateral_ratio);
            (dist * 10_000) / range
        };

        // ── Volatility component ─────────────────────────────────────────────
        // 0 % volatility → 0; >= 100 % → 10 000
        let volatility_component: u32 = volatility_30d.min(10_000);

        // ── Age component ────────────────────────────────────────────────────
        // Older positions have accumulated more risk exposure.
        // 0 days → 0; >= 365 days → 10 000
        let age_component: u32 = ((position_age_days as u64 * 10_000) / 365).min(10_000) as u32;

        // ── Market component ─────────────────────────────────────────────────
        // Directly passed in (already 0-10 000)
        let market_component: u32 = market_score.min(10_000);

        // ── Weighted sum ─────────────────────────────────────────────────────
        let total_score: u32 = (
            (collateral_component as u64 * WEIGHT_COLLATERAL as u64
                + volatility_component as u64 * WEIGHT_VOLATILITY as u64
                + age_component as u64 * WEIGHT_AGE as u64
                + market_component as u64 * WEIGHT_MARKET as u64)
            / 10_000
        ) as u32;

        RiskScoreBreakdown {
            total_score: total_score.min(10_000),
            collateral_component,
            volatility_component,
            age_component,
            market_component,
        }
    }

    /// Integer square-root via Newton's method (no floating point).
    fn isqrt(n: u64) -> u64 {
        if n == 0 {
            return 0;
        }
        let mut x = n;
        let mut y = (x + 1) / 2;
        while y < x {
            x = y;
            y = (x + n / x) / 2;
        }
        x
    }

    // ── Storage loaders ───────────────────────────────────────────────────────

    fn load_user_positions(env: &Env) -> Map<Address, Vec<SyntheticPosition>> {
        env.storage()
            .instance()
            .get(&USER_POSITIONS)
            .unwrap_or_else(|| Map::new(env))
    }

    fn load_meta(env: &Env) -> Map<u64, PositionMetadata> {
        env.storage()
            .instance()
            .get(&POSITION_META)
            .unwrap_or_else(|| Map::new(env))
    }

    fn load_alerts(env: &Env) -> Map<u64, Vec<PositionAlert>> {
        env.storage()
            .instance()
            .get(&ALERTS)
            .unwrap_or_else(|| Map::new(env))
    }

    fn load_analytics(env: &Env) -> Map<u64, PositionAnalytics> {
        env.storage()
            .instance()
            .get(&ANALYTICS)
            .unwrap_or_else(|| Map::new(env))
    }

    // ── Auth ──────────────────────────────────────────────────────────────────

    fn require_admin(env: &Env) {
        let admin: Address = env.storage().instance().get(&ADMIN).unwrap_optimized();
        admin.require_auth();
    }
}
