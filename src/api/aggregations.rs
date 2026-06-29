use crate::utils::StellarClient;
use crate::api::types::{AccountStats, NetworkStats, AssetVolume};
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
