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

    // ── Issue #211: Batch operations ─────────────────────────────────────────

    /// Simulate a batch-close result for up to 20 positions.
    ///
    /// `position_ids` and `owners` must be parallel arrays of equal length.
    async fn batch_close_preview(
        &self,
        position_ids: Vec<String>,
        owners: Vec<String>,
    ) -> Result<crate::api::types::ApiBatchResult> {
        if position_ids.len() != owners.len() {
            return Err(async_graphql::Error::new("position_ids and owners must have the same length"));
        }
        if position_ids.len() > 20 {
            return Err(async_graphql::Error::new("batch exceeds limit of 20"));
        }
        let total = position_ids.len() as i32;
        let results: Vec<crate::api::types::ApiBatchItemResult> = position_ids
            .iter()
            .map(|pid| crate::api::types::ApiBatchItemResult {
                position_id: pid.clone(),
                success: true,
                error_code: "OK".to_string(),
            })
            .collect();
        Ok(crate::api::types::ApiBatchResult {
            total,
            succeeded: total,
            failed: 0,
            results,
        })
    }

    // ── Issue #212: Risk scoring ──────────────────────────────────────────────

    /// Calculate a composite risk score for a position.
    ///
    /// All inputs are in basis points unless noted.
    /// Returns the full weighted breakdown (0-10 000, higher = riskier).
    async fn position_risk_score(
        &self,
        collateral_ratio: i32,
        volatility_30d: i32,
        position_age_days: i32,
        market_score: i32,
    ) -> Result<crate::api::types::ApiRiskScoreBreakdown> {
        // Weight constants (must sum to 10 000)
        const W_COL: i64 = 5_000;
        const W_VOL: i64 = 3_000;
        const W_AGE: i64 = 1_000;
        const W_MKT: i64 = 1_000;

        let collateral_component = if collateral_ratio >= 30000 {
            0i32
        } else if collateral_ratio <= 11000 {
            10_000
        } else {
            let range = 30000 - 11000;
            let dist  = 30000 - collateral_ratio;
            (dist * 10_000) / range
        };

        let volatility_component = volatility_30d.min(10_000).max(0);

        let age_component = ((position_age_days as i64 * 10_000) / 365).min(10_000) as i32;

        let market_component = market_score.min(10_000).max(0);

        let total_score = ((collateral_component as i64 * W_COL
            + volatility_component as i64 * W_VOL
            + age_component as i64 * W_AGE
            + market_component as i64 * W_MKT)
            / 10_000) as i32;

        Ok(crate::api::types::ApiRiskScoreBreakdown {
            total_score: total_score.min(10_000),
            collateral_component,
            volatility_component,
            age_component,
            market_component,
        })
    }

    // ── Issue #213: Alerts ────────────────────────────────────────────────────

    /// Return mock outstanding alerts for a position.
    /// In production this reads from the on-chain contract state.
    async fn position_alerts(
        &self,
        position_id: String,
    ) -> Result<Vec<crate::api::types::ApiPositionAlert>> {
        // Mock response — wire to real contract client when available
        Ok(vec![
            crate::api::types::ApiPositionAlert {
                alert_id: "1".to_string(),
                position_id: position_id.clone(),
                alert_type: "LowCollateral".to_string(),
                severity: "Warning".to_string(),
                message: "Collateral ratio approaching minimum threshold".to_string(),
                timestamp: 0,
                acknowledged: false,
            },
        ])
    }

    // ── Issue #214: Analytics ─────────────────────────────────────────────────

    /// Return PnL, ROI, max-drawdown, and Sharpe ratio for a single position.
    ///
    /// `entry_price`, `current_price`, `synthetic_amount`, and
    /// `initial_collateral` are all in 8-decimal fixed-point integers (as strings).
    async fn position_analytics(
        &self,
        position_id: String,
        owner: String,
        asset_id: i32,
        entry_price: String,
        current_price: String,
        synthetic_amount: String,
        initial_collateral: String,
        days_held: i32,
        daily_returns_bps: Vec<i32>,
    ) -> Result<crate::api::types::ApiPositionAnalytics> {
        let entry: i64  = entry_price.parse().map_err(|_| async_graphql::Error::new("invalid entry_price"))?;
        let current: i64 = current_price.parse().map_err(|_| async_graphql::Error::new("invalid current_price"))?;
        let synth: i64  = synthetic_amount.parse().map_err(|_| async_graphql::Error::new("invalid synthetic_amount"))?;
        let init_col: i64 = initial_collateral.parse().map_err(|_| async_graphql::Error::new("invalid initial_collateral"))?;

        // PnL = (current - entry) * synthetic_amount / 1e8
        let pnl = ((current - entry) * synth) / 100_000_000;

        // ROI in basis points
        let roi_bps = if init_col > 0 { ((pnl * 10_000) / init_col) as i32 } else { 0 };

        // Max drawdown
        let mut max_drawdown_bps = 0i32;
        let mut running_max: i64 = entry;
        let mut price_sim: i64 = entry;
        for ret in &daily_returns_bps {
            price_sim += (price_sim * *ret as i64) / 10_000;
            if price_sim > running_max { running_max = price_sim; }
            if running_max > 0 {
                let dd = (((running_max - price_sim) * 10_000) / running_max) as i32;
                if dd > max_drawdown_bps { max_drawdown_bps = dd; }
            }
        }

        // Sharpe ratio = mean_daily_return / std_dev  (scaled by 10 000)
        let sharpe_ratio_scaled = if daily_returns_bps.len() >= 2 {
            let n = daily_returns_bps.len() as i64;
            let sum: i64 = daily_returns_bps.iter().map(|&r| r as i64).sum();
            let mean = sum / n;
            let variance: i64 = daily_returns_bps.iter()
                .map(|&r| { let d = r as i64 - mean; d * d })
                .sum::<i64>() / n;
            let std_dev = (variance as f64).sqrt() as i64;
            if std_dev > 0 { ((mean * 10_000) / std_dev) as i32 } else { 0 }
        } else {
            0
        };

        Ok(crate::api::types::ApiPositionAnalytics {
            position_id,
            owner,
            asset_id,
            pnl: pnl.to_string(),
            roi_bps,
            max_drawdown_bps,
            sharpe_ratio_scaled,
            entry_price,
            current_price,
            synthetic_amount,
            initial_collateral,
            days_held,
            last_updated: 0,
        })
    }
}
