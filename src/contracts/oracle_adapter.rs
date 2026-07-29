// Issue #230: Custom oracle adapter interface
// Issue #229: Centralized exchange price adapter
//! Oracle adapter interface, registry, and CEX adapters.
//!
//! Defines a standardized adapter interface that allows any oracle provider
//! to plug into the system, with adapter registration, discovery, failover,
//! and health monitoring.

use soroban_sdk::{Address, Env, Symbol, Vec, Map, BytesN};
use crate::types::asset::{AssetPrice, PriceSource, PriceSourceType};

// ── Issue #230: OracleAdapter trait ─────────────────────────────────────────

/// Adapter priority level (lower = higher priority).
pub const ADAPTER_PRIORITY_HIGH: u32 = 1;
pub const ADAPTER_PRIORITY_MEDIUM: u32 = 5;
pub const ADAPTER_PRIORITY_LOW: u32 = 10;

/// Oracle adapter health status.
#[derive(Clone, Debug, PartialEq)]
pub enum AdapterHealth {
    Healthy,
    Degraded,
    Offline,
}

/// A registered oracle adapter.
#[derive(Clone, Debug)]
pub struct RegisteredAdapter {
    pub id: Symbol,
    pub name: String,
    pub priority: u32,
    pub is_active: bool,
    pub health: AdapterHealth,
    pub last_price_timestamp: u64,
    pub failover_count: u32,
}

/// Issue #230: Oracle adapter trait — standardized interface for any oracle provider.
/// Each adapter implements get_price, get_twap, and is_stale.
pub trait OracleAdapter {
    /// Get the current price for an asset.
    fn get_price(&self, env: &Env, asset_id: &Symbol) -> Option<AssetPrice>;

    /// Get the time-weighted average price over a period.
    fn get_twap(&self, env: &Env, asset_id: &Symbol, period_ledgers: u32) -> Option<AssetPrice>;

    /// Check if the adapter's price data is stale.
    fn is_stale(&self, env: &Env, max_age_ledgers: u32) -> bool;

    /// Get the adapter's name.
    fn name(&self) -> &str;

    /// Health check — returns the adapter's health status.
    fn health_check(&self, env: &Env) -> AdapterHealth;
}

// ── Issue #230: Adapter registry ─────────────────────────────────────────────

/// Registry for oracle adapters — supports discovery, priority ordering, and failover.
pub struct AdapterRegistry {
    adapters: Map<Symbol, RegisteredAdapter>,
    priority_order: Vec<Symbol>,
}

impl AdapterRegistry {
    pub fn new(env: &Env) -> Self {
        Self {
            adapters: Map::new(env),
            priority_order: Vec::new(env),
        }
    }

    /// Register a new adapter. New adapters can be added without contract upgrade.
    pub fn register(&mut self, adapter: RegisteredAdapter) {
        let id = adapter.id.clone();
        self.adapters.set(id.clone(), adapter);

        // Insert into priority order (sorted by priority, ascending)
        let mut new_order = Vec::new(&self.adapters.env());
        let mut inserted = false;
        for existing_id in self.priority_order.iter() {
            let existing = self.adapters.get(existing_id.clone());
            if !inserted {
                if let Some(ref existing_adapter) = existing {
                    if existing_adapter.priority > self.adapters.get(id.clone()).unwrap().priority {
                        new_order.push_back(id.clone());
                        inserted = true;
                    }
                }
            }
            new_order.push_back(existing_id);
        }
        if !inserted {
            new_order.push_back(id);
        }
        self.priority_order = new_order;
    }

    /// Unregister an adapter.
    pub fn unregister(&mut self, id: Symbol) {
        self.adapters.remove(id);
        self.priority_order = self.priority_order.iter().filter(|x| *x != id).collect();
    }

    /// Get all adapters in priority order.
    pub fn get_ordered_adapters(&self) -> Vec<Symbol> {
        self.priority_order.clone()
    }

    /// Find the first healthy adapter for failover.
    pub fn find_healthy(&self, env: &Env) -> Option<Symbol> {
        for id in self.priority_order.iter() {
            if let Some(adapter) = self.adapters.get(id) {
                if adapter.is_active && adapter.health != AdapterHealth::Offline {
                    return Some(id);
                }
            }
        }
        None
    }

    /// Update adapter health status (health monitoring).
    pub fn update_health(&mut self, id: Symbol, health: AdapterHealth) {
        if let Some(mut adapter) = self.adapters.get(id) {
            let was_healthy = adapter.health == AdapterHealth::Healthy;
            adapter.health = health;
            if was_healthy && health != AdapterHealth::Healthy {
                adapter.failover_count += 1;
            }
            self.adapters.set(id, adapter);
        }
    }
}

// ── Issue #229: CEX price adapters ──────────────────────────────────────────

/// Configuration for a centralized exchange price adapter.
#[derive(Clone, Debug)]
pub struct CexAdapterConfig {
    pub exchange_name: String,
    pub api_base_url: String,
    pub min_volume_threshold: u64,
    pub rate_limit_ms: u64,
    pub priority: u32,
}

/// Default configs for major exchanges.
pub fn binance_config() -> CexAdapterConfig {
    CexAdapterConfig {
        exchange_name: "Binance".to_string(),
        api_base_url: "https://api.binance.com/api/v3/ticker/24hr".to_string(),
        min_volume_threshold: 10_000,
        rate_limit_ms: 100,
        priority: ADAPTER_PRIORITY_HIGH,
    }
}

pub fn coinbase_config() -> CexAdapterConfig {
    CexAdapterConfig {
        exchange_name: "Coinbase".to_string(),
        api_base_url: "https://api.exchange.coinbase.com/products".to_string(),
        min_volume_threshold: 5_000,
        rate_limit_ms: 150,
        priority: ADAPTER_PRIORITY_MEDIUM,
    }
}

pub fn kraken_config() -> CexAdapterConfig {
    CexAdapterConfig {
        exchange_name: "Kraken".to_string(),
        api_base_url: "https://api.kraken.com/0/public/Ticker".to_string(),
        min_volume_threshold: 3_000,
        rate_limit_ms: 200,
        priority: ADAPTER_PRIORITY_LOW,
    }
}

/// Centralized exchange price adapter.
/// Fetches prices from major exchanges and computes volume-weighted average.
pub struct CexPriceAdapter {
    pub config: CexAdapterConfig,
    pub last_request_time: u64,
    pub last_price: Option<AssetPrice>,
    pub consecutive_errors: u32,
}

impl CexPriceAdapter {
    pub fn new(config: CexAdapterConfig) -> Self {
        Self {
            config,
            last_request_time: 0,
            last_price: None,
            consecutive_errors: 0,
        }
    }

    /// Compute volume-weighted average price across multiple exchange responses.
    /// Filters out responses below the minimum volume threshold.
    pub fn compute_vwap(&self, prices: Vec<(u64, u64)>) -> Option<u64> {
        // prices: Vec of (price, volume) pairs
        let mut total_volume: u64 = 0;
        let mut weighted_sum: u128 = 0;

        for (price, volume) in prices.iter() {
            if *volume >= self.config.min_volume_threshold {
                total_volume = total_volume.saturating_add(*volume);
                weighted_sum = weighted_sum.saturating_add(
                    (*price as u128).saturating_mul(*volume as u128)
                );
            }
        }

        if total_volume == 0 {
            return None;
        }

        Some((weighted_sum / total_volume as u128) as u64)
    }

    /// Handle rate limiting with exponential backoff.
    pub fn should_throttle(&self, current_time: u64) -> bool {
        current_time < self.last_request_time + self.config.rate_limit_ms
    }

    /// Get backoff delay based on consecutive error count.
    pub fn backoff_delay(&self) -> u64 {
        // Exponential backoff: 100ms * 2^errors, capped at 10s
        let delay = 100u64.saturating_mul(2u64.saturating_pow(self.consecutive_errors));
        delay.min(10_000)
    }

    /// Record a successful request.
    pub fn record_success(&mut self, timestamp: u64, price: AssetPrice) {
        self.last_request_time = timestamp;
        self.last_price = Some(price);
        self.consecutive_errors = 0;
    }

    /// Record a failed request (for backoff tracking).
    pub fn record_error(&mut self, timestamp: u64) {
        self.last_request_time = timestamp;
        self.consecutive_errors = self.consecutive_errors.saturating_add(1);
    }
}

impl OracleAdapter for CexPriceAdapter {
    fn get_price(&self, _env: &Env, _asset_id: &Symbol) -> Option<AssetPrice> {
        self.last_price.clone()
    }

    fn get_twap(&self, _env: &Env, _asset_id: &Symbol, _period_ledgers: u32) -> Option<AssetPrice> {
        // TWAP would require historical price storage — return last known price
        self.last_price.clone()
    }

    fn is_stale(&self, env: &Env, max_age_ledgers: u32) -> bool {
        if let Some(ref price) = self.last_price {
            let current_ledger = env.ledger().sequence() as u64;
            current_ledger.saturating_sub(price.timestamp) > max_age_ledgers as u64
        } else {
            true
        }
    }

    fn name(&self) -> &str {
        &self.config.exchange_name
    }

    fn health_check(&self, _env: &Env) -> AdapterHealth {
        if self.consecutive_errors >= 5 {
            AdapterHealth::Offline
        } else if self.consecutive_errors > 0 {
            AdapterHealth::Degraded
        } else {
            AdapterHealth::Healthy
        }
    }
}

/// Multi-exchange aggregator: combines prices from Binance, Coinbase, and Kraken.
pub struct MultiExchangeAggregator {
    pub adapters: Vec<CexPriceAdapter>,
}

impl MultiExchangeAggregator {
    pub fn new() -> Self {
        Self {
            adapters: Vec::new(),
        }
    }

    /// Create with default adapters for all 3 major exchanges.
    pub fn with_defaults() -> Self {
        let mut adapters = Vec::new();
        adapters.push_back(CexPriceAdapter::new(binance_config()));
        adapters.push_back(CexPriceAdapter::new(coinbase_config()));
        adapters.push_back(CexPriceAdapter::new(kraken_config()));
        Self { adapters }
    }

    /// Get the best (highest-priority healthy) adapter's price.
    pub fn get_best_price(&self, env: &Env, asset_id: &Symbol) -> Option<AssetPrice> {
        for adapter in self.adapters.iter() {
            if adapter.health_check(env) != AdapterHealth::Offline {
                if let Some(price) = adapter.get_price(env, asset_id) {
                    return Some(price);
                }
            }
        }
        None
    }
}
