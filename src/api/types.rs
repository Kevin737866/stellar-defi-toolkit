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
