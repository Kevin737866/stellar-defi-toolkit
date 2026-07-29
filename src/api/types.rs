use async_graphql::{SimpleObject, ID};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(SimpleObject, Serialize, Deserialize, Clone, Debug)]
pub struct Ledger {
    pub sequence: i32,
    pub hash: String,
    pub close_time: DateTime<Utc>,
    pub transaction_count: i32,
    pub operation_count: i32,
}

#[derive(SimpleObject, Serialize, Deserialize, Clone, Debug)]
pub struct Transaction {
    pub id: ID,
    pub hash: String,
    pub ledger_sequence: i32,
    pub source_account: String,
    pub fee_paid: String,
    pub operation_count: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(SimpleObject, Serialize, Deserialize, Clone, Debug)]
pub struct Operation {
    pub id: ID,
    pub transaction_id: String,
    pub source_account: String,
    pub operation_type: String,
    pub created_at: DateTime<Utc>,
}

#[derive(SimpleObject, Serialize, Deserialize, Clone, Debug)]
pub struct Account {
    pub id: ID,
    pub sequence: String,
    pub subentry_count: i32,
    pub balances: Vec<Balance>,
}

#[derive(SimpleObject, Serialize, Deserialize, Clone, Debug)]
pub struct Balance {
    pub asset_type: String,
    pub asset_code: Option<String>,
    pub asset_issuer: Option<String>,
    pub balance: String,
}

#[derive(SimpleObject, Serialize, Deserialize, Clone, Debug)]
pub struct NetworkStats {
    pub tps: f64,
    pub total_accounts: i64,
    pub total_transactions: i64,
    pub ledger_count: i32,
}

#[derive(SimpleObject, Serialize, Deserialize, Clone, Debug)]
pub struct AccountStats {
    pub transaction_count: i32,
    pub operation_count: i32,
    pub total_volume_xlm: String,
    pub last_active: DateTime<Utc>,
}

#[derive(SimpleObject, Serialize, Deserialize, Clone, Debug)]
pub struct AssetVolume {
    pub asset_code: String,
    pub volume: String,
    pub transaction_count: i32,
    pub timeframe: String,
}

#[derive(SimpleObject, Serialize, Deserialize, Clone, Debug)]
pub struct Portfolio {
    pub total_net_worth: String,
    pub positions: Vec<PositionSummary>,
    pub risk_summary: RiskSummary,
    pub yield_summary: YieldSummary,
    pub last_updated: DateTime<Utc>,
}

#[derive(SimpleObject, Serialize, Deserialize, Clone, Debug)]
pub struct PositionSummary {
    pub module: String,
    pub asset: String,
    pub amount: String,
    pub value_usd: String,
    pub apy: Option<f64>,
}

#[derive(SimpleObject, Serialize, Deserialize, Clone, Debug)]
pub struct RiskSummary {
    pub health_factor: f64,
    pub liquidation_risk: String,
    pub concentration_risk: f64,
}

#[derive(SimpleObject, Serialize, Deserialize, Clone, Debug)]
pub struct YieldSummary {
    pub earned_ytd: String,
    pub projected_annual: String,
    pub average_apy: f64,
}

// ─── Position Manager API types (issues #211-#214) ────────────────────────────

#[derive(SimpleObject, Serialize, Deserialize, Clone, Debug)]
pub struct ApiBatchItemResult {
    pub position_id: String,
    pub success: bool,
    pub error_code: String,
}

#[derive(SimpleObject, Serialize, Deserialize, Clone, Debug)]
pub struct ApiBatchResult {
    pub total: i32,
    pub succeeded: i32,
    pub failed: i32,
    pub results: Vec<ApiBatchItemResult>,
}

#[derive(SimpleObject, Serialize, Deserialize, Clone, Debug)]
pub struct ApiRiskScoreBreakdown {
    /// Composite risk score 0–10 000 (higher = riskier)
    pub total_score: i32,
    /// Collateral ratio component 0–10 000 (weight 50 %)
    pub collateral_component: i32,
    /// Volatility component 0–10 000 (weight 30 %)
    pub volatility_component: i32,
    /// Age component 0–10 000 (weight 10 %)
    pub age_component: i32,
    /// Market condition component 0–10 000 (weight 10 %)
    pub market_component: i32,
}

#[derive(SimpleObject, Serialize, Deserialize, Clone, Debug)]
pub struct ApiPositionAlert {
    pub alert_id: String,
    pub position_id: String,
    pub alert_type: String,
    pub severity: String,
    pub message: String,
    pub timestamp: i64,
    pub acknowledged: bool,
}

#[derive(SimpleObject, Serialize, Deserialize, Clone, Debug)]
pub struct ApiPositionAnalytics {
    pub position_id: String,
    pub owner: String,
    pub asset_id: i32,
    /// Profit / loss in USD (8 decimals, negative = loss)
    pub pnl: String,
    /// ROI in basis points
    pub roi_bps: i32,
    /// Maximum drawdown in basis points
    pub max_drawdown_bps: i32,
    /// Sharpe ratio scaled by 10 000 (e.g. 15 000 = 1.5)
    pub sharpe_ratio_scaled: i32,
    pub entry_price: String,
    pub current_price: String,
    pub synthetic_amount: String,
    pub initial_collateral: String,
    pub days_held: i32,
    pub last_updated: i64,
}
