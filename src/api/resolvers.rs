use async_graphql::{Object, Result, Context};
use crate::api::types::{Ledger, Transaction, Operation, Account, NetworkStats, AccountStats, AssetVolume, Portfolio};
use crate::utils::StellarClient;
use crate::api::aggregations::Aggregator;
use crate::contracts::price_history::{PriceHistoryManager, PriceQueryResult};

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

    /// Query historical prices with time range and granularity
    async fn historical_prices(
        &self,
        ctx: &Context<'_>,
        asset_id: String,
        start_time: u64,
        end_time: u64,
        granularity: String,
        limit: Option<i32>,
        offset: Option<i32>,
    ) -> Result<PriceQueryResult> {
        let manager = ctx.data::<PriceHistoryManager>()?;
        let limit = limit.map(|l| l as usize);
        let offset = offset.map(|o| o as usize);
        manager
            .query_historical_prices(&asset_id, start_time, end_time, &granularity, limit, offset)
            .map_err(|e| async_graphql::Error::new(format!("{}", e)))
    }
}
