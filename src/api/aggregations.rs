use crate::utils::StellarClient;
use crate::api::types::{AccountStats, NetworkStats, AssetVolume, Portfolio, PositionSummary, RiskSummary, YieldSummary};
use anyhow::Result;
use chrono::{Utc, Duration};

pub struct Aggregator {
    pub client: StellarClient,
}

impl Aggregator {
    pub fn new(client: StellarClient) -> Self {
        Self { client }
    }

    pub async fn get_account_stats(&self, address: &str) -> Result<AccountStats> {
        let txs = self.client.get_transactions(Some(address.to_string()), 50).await?;
        let ops = self.client.get_operations(None, 50).await?;
        
        let mut total_xlm = 0.0;
        for op in &ops {
            if op.source_account == address && op.operation_type == "payment" {
                // In a real implementation, we would check the amount
                total_xlm += 10.0; // Mocked amount increment
            }
        }

        Ok(AccountStats {
            transaction_count: txs.len() as i32,
            operation_count: ops.len() as i32,
            total_volume_xlm: format!("{:.2}", total_xlm),
            last_active: txs.first().map(|tx| tx.created_at).unwrap_or(Utc::now()),
        })
    }

    pub async fn get_network_stats(&self) -> Result<NetworkStats> {
        self.client.get_network_stats().await
    }

    pub async fn get_asset_volume(&self, asset_code: &str, timeframe: &str) -> Result<AssetVolume> {
        // Mocking volume calculation based on timeframe
        let multiplier = match timeframe {
            "24h" => 1.0,
            "7d" => 7.0,
            "30d" => 30.0,
            _ => 1.0,
        };

        Ok(AssetVolume {
            asset_code: asset_code.to_string(),
            volume: format!("{:.2}", 50000.0 * multiplier),
            transaction_count: (1200.0 * multiplier) as i32,
            timeframe: timeframe.to_string(),
        })
    }

    pub async fn aggregate_portfolio(&self, _address: &str) -> Result<Portfolio> {
        let positions = vec![
            PositionSummary {
                module: "lending".to_string(),
                asset: "USDC".to_string(),
                amount: "5000.00".to_string(),
                value_usd: "5000.00".to_string(),
                apy: Some(5.2),
            },
            PositionSummary {
                module: "staking".to_string(),
                asset: "XLM".to_string(),
                amount: "10000.00".to_string(),
                value_usd: "12000.00".to_string(),
                apy: Some(8.5),
            },
            PositionSummary {
                module: "liquidity".to_string(),
                asset: "XLM/USDC".to_string(),
                amount: "2500.00".to_string(),
                value_usd: "2600.00".to_string(),
                apy: Some(12.3),
            },
            PositionSummary {
                module: "vault".to_string(),
                asset: "yXLM".to_string(),
                amount: "3000.00".to_string(),
                value_usd: "3100.00".to_string(),
                apy: Some(6.7),
            },
            PositionSummary {
                module: "synthetic".to_string(),
                asset: "sBTC".to_string(),
                amount: "0.50".to_string(),
                value_usd: "15000.00".to_string(),
                apy: None,
            },
        ];

        let total: f64 = positions.iter().filter_map(|p| p.value_usd.parse::<f64>().ok()).sum();
        let apy_values: Vec<f64> = positions.iter().filter_map(|p| p.apy).collect();
        let avg_apy = if apy_values.is_empty() {
            0.0
        } else {
            apy_values.iter().sum::<f64>() / apy_values.len() as f64
        };

        let net_worth_by_asset: Vec<f64> = positions.iter()
            .filter_map(|p| p.value_usd.parse::<f64>().ok())
            .collect();
        let max_asset = net_worth_by_asset.iter().cloned().fold(0.0, f64::max);
        let concentration = if total > 0.0 { max_asset / total } else { 0.0 };

        Ok(Portfolio {
            total_net_worth: format!("{:.2}", total),
            positions,
            risk_summary: RiskSummary {
                health_factor: 1.8,
                liquidation_risk: "low".to_string(),
                concentration_risk: concentration,
            },
            yield_summary: YieldSummary {
                earned_ytd: "1250.45".to_string(),
                projected_annual: format!("{:.2}", total * avg_apy / 100.0),
                average_apy: avg_apy,
            },
            last_updated: Utc::now(),
        })
    }

    pub async fn get_daily_stats(&self) -> Result<serde_json::Value> {
        let now = Utc::now();
        let _yesterday = now - Duration::days(1);
        
        // Fetch last 100 transactions to estimate daily count
        let txs = self.client.get_transactions(None, 100).await?;
        
        Ok(serde_json::json!({
            "daily_transaction_count": txs.len() * 10, // Scaled for mock
            "daily_payment_volume_xlm": "150230.45",
            "active_accounts_24h": 450,
            "top_accounts_by_balance": [
                {"address": "GBBD47ZO...7Z7O", "balance": "1000000.00"},
                {"address": "GDEFGH...2345", "balance": "850000.00"}
            ]
        }))
    }
}
