use async_graphql::{Object, Result, Context};
use crate::api::types::{Ledger, Transaction, Operation, Account, NetworkStats, AccountStats, AssetVolume, Portfolio};
use crate::utils::StellarClient;
use crate::api::aggregations::Aggregator;
use std::sync::{Arc, RwLock};
use std::collections::HashMap;
use chrono::{DateTime, Utc, Duration};

pub struct PortfolioCache {
    cache: Arc<RwLock<HashMap<String, (DateTime<Utc>, Portfolio)>>>,
    ttl_seconds: i64,
}

impl PortfolioCache {
    pub fn new(ttl_seconds: i64) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            ttl_seconds,
        }
    }

    pub fn get(&self, key: &str) -> Option<Portfolio> {
        if let Ok(cache) = self.cache.read() {
            if let Some((timestamp, portfolio)) = cache.get(key) {
                if Utc::now() - *timestamp < Duration::seconds(self.ttl_seconds) {
                    return Some(portfolio.clone());
                }
            }
        }
        None
    }

    pub fn set(&self, key: String, portfolio: Portfolio) {
        if let Ok(mut cache) = self.cache.write() {
            cache.insert(key, (Utc::now(), portfolio));
        }
    }
}

pub struct QueryRoot;

#[Object]
impl QueryRoot {
    /// Get details for a single ledger by sequence
    async fn ledger(&self, ctx: &Context<'_>, sequence: i32) -> Result<Ledger> {
        let client = ctx.data::<StellarClient>()?;
        Ok(client.get_ledger(sequence).await?)
    }

    /// Get a list of recent ledgers (paginated)
    async fn ledgers(&self, ctx: &Context<'_>, limit: Option<i32>, cursor: Option<String>) -> Result<Vec<Ledger>> {
        let client = ctx.data::<StellarClient>()?;
        let limit = limit.unwrap_or(10);
        Ok(client.get_ledgers(limit, cursor).await?)
    }

    /// Get recent transactions, optionally filtered by account address
    async fn transactions(&self, ctx: &Context<'_>, address: Option<String>, limit: Option<i32>) -> Result<Vec<Transaction>> {
        let client = ctx.data::<StellarClient>()?;
        let limit = limit.unwrap_or(10);
        Ok(client.get_transactions(address, limit).await?)
    }

    /// Get recent operations, optionally filtered by type
    async fn operations(&self, ctx: &Context<'_>, operation_type: Option<String>, limit: Option<i32>) -> Result<Vec<Operation>> {
        let client = ctx.data::<StellarClient>()?;
        let limit = limit.unwrap_or(10);
        Ok(client.get_operations(operation_type, limit).await?)
    }

    /// Get detailed statistics for an account
    async fn account_stats(&self, ctx: &Context<'_>, address: String) -> Result<AccountStats> {
        let client = ctx.data::<StellarClient>()?;
        let aggregator = Aggregator::new(client.clone());
        Ok(aggregator.get_account_stats(&address).await?)
    }

    /// Get global network metrics
    async fn network_stats(&self, ctx: &Context<'_>) -> Result<NetworkStats> {
        let client = ctx.data::<StellarClient>()?;
        let aggregator = Aggregator::new(client.clone());
        Ok(aggregator.get_network_stats().await?)
    }

    /// Get DEX volume for an asset
    async fn asset_volume(&self, ctx: &Context<'_>, asset_code: String, timeframe: String) -> Result<AssetVolume> {
        let client = ctx.data::<StellarClient>()?;
        let aggregator = Aggregator::new(client.clone());
        Ok(aggregator.get_asset_volume(&asset_code, &timeframe).await?)
    }

    /// Get account details including balances
    async fn account(&self, ctx: &Context<'_>, address: String) -> Result<Account> {
        let client = ctx.data::<StellarClient>()?;
        Ok(client.get_account_details(&address).await?)
    }

    /// Get aggregated portfolio for a user address, with caching
    async fn portfolio(&self, ctx: &Context<'_>, address: String) -> Result<Portfolio> {
        let cache = ctx.data::<PortfolioCache>()?;
        if let Some(cached) = cache.get(&address) {
            return Ok(cached);
        }
        let client = ctx.data::<StellarClient>()?;
        let aggregator = Aggregator::new(client.clone());
        let portfolio = aggregator.aggregate_portfolio(&address).await?;
        cache.set(address, portfolio.clone());
        Ok(portfolio)
    }

    /// Get daily aggregated statistics
    async fn daily_stats(&self, ctx: &Context<'_>) -> Result<serde_json::Value> {
        let client = ctx.data::<StellarClient>()?;
        let aggregator = Aggregator::new(client.clone());
        Ok(aggregator.get_daily_stats().await?)
    }
}
