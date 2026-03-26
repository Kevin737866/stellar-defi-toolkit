use async_graphql::{Object, Result, Context};
use crate::api::types::{Ledger, Transaction, Operation, Account, NetworkStats, AccountStats, AssetVolume};
use crate::utils::StellarClient;
use crate::api::aggregations::Aggregator;

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

    /// Get daily aggregated statistics
    async fn daily_stats(&self, ctx: &Context<'_>) -> Result<serde_json::Value> {
        let client = ctx.data::<StellarClient>()?;
        let aggregator = Aggregator::new(client.clone());
        Ok(aggregator.get_daily_stats().await?)
    }
}
