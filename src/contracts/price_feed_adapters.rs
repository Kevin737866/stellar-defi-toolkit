//! Price Feed Source Adapters for Different Asset Categories
//!
//! Provides specialized adapters for different types of price feed sources
//! and asset categories, with category-specific validation and processing.
//!
//! ## Access Control
//! - **Admin**: `register_adapter`, `activate_adapter`, `deactivate_adapter`,
//!   `update_adapter_settings`, `update_category_config` — gated by a broken
//!   `require_admin()` (compares the contract's own address, not the caller). See
//!   `docs/ACCESS_CONTROL_MATRIX.md`.
//! - **User**: read-only (adapter/category config lookups, `validate_price`).
use soroban_sdk::{contracttype, Address, Env, Symbol};

use soroban_sdk::{contract, contractimpl, Address, Env, Symbol, Vec, Map, unwrap::UnwrapOptimized};
use crate::types::asset::{
    StellarAssetId, AssetCategory, AssetPrice, PriceSource, PriceSourceType,
    AggregationMethod, AlertSeverity, PriceDeviationAlert,
};

// ─── Constants ───────────────────────────────────────────────────────────────

/// Maximum price age for different categories
const MAX_PRICE_AGE_CRYPTO: u64 = 300;      // 5 minutes for crypto
const MAX_PRICE_AGE_STABLECOIN: u64 = 60;  // 1 minute for stablecoins
const MAX_PRICE_AGE_RWA: u64 = 3600;        // 1 hour for real-world assets
const MAX_PRICE_AGE_FOREX: u64 = 60;         // 1 minute for forex

/// Confidence thresholds for different categories
const MIN_CONFIDENCE_CRYPTO: u32 = 7000;    // 70% for crypto
const MIN_CONFIDENCE_STABLECOIN: u32 = 9000; // 90% for stablecoins
const MIN_CONFIDENCE_RWA: u32 = 6000;        // 60% for RWA
const MIN_CONFIDENCE_FOREX: u32 = 8500;      // 85% for forex

// ─── Storage Keys ─────────────────────────────────────────────────────────────

const ADMIN: Symbol = Symbol::short("ADMIN");
const ADAPTERS: Symbol = Symbol::short("ADAPTERS");
const CATEGORY_CONFIGS: Symbol = Symbol::short("CAT_CONFIG");

// ─── Category-Specific Configuration ───────────────────────────────────────────

#[derive(Clone, Debug)]
#[contracttype]
pub struct CategoryConfig {
    /// Asset category
    pub category: AssetCategory,
    /// Maximum price age (seconds)
    pub max_price_age: u64,
    /// Minimum confidence threshold (basis points)
    pub min_confidence: u32,
    /// Preferred aggregation method
    pub preferred_aggregation: AggregationMethod,
    /// Minimum number of sources required
    pub min_sources: u32,
    /// Circuit breaker threshold (basis points)
    pub circuit_breaker_threshold: u32,
    /// Whether to use TWAP by default
    pub use_twap: bool,
    /// Default TWAP period (seconds)
    pub twap_period: u64,
}

/// Price feed adapter configuration
#[derive(Clone, Debug)]
#[contracttype]
pub struct AdapterConfig {
    /// Adapter address
    pub address: Address,
    /// Adapter name
    pub name: Symbol,
    /// Adapter type
    pub adapter_type: PriceSourceType,
    /// Supported categories
    pub supported_categories: Vec<AssetCategory>,
    /// Adapter-specific settings
    pub settings: Map<Symbol, Symbol>,
    /// Whether adapter is active
    pub active: bool,
}

// ─── Price Feed Adapters Contract ─────────────────────────────────────────────

/// Price feed source adapters contract
#[contract]
pub struct PriceFeedAdaptersContract;

#[contractimpl]
impl PriceFeedAdaptersContract {
    /// Initialize the price feed adapters contract
    ///
    /// # Arguments
    /// * `admin` - Admin address for governance
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&ADMIN) {
            panic!("Already initialized");
        }

        env.storage().instance().set(&ADMIN, &admin);

        // Initialize adapters storage
        let adapters: Map<Address, AdapterConfig> = Map::new(&env);
        env.storage().instance().set(&ADAPTERS, &adapters);

        // Initialize default category configurations
        let category_configs = Self::initialize_default_configs(&env);
        env.storage().instance().set(&CATEGORY_CONFIGS, &category_configs);

        env.events().publish(
            Symbol::short("ADAPTERS_INITIALIZED"),
            admin,
        );
    }

    /// Register a price feed adapter
    ///
    /// # Arguments
    /// * `adapter_address` - Adapter address
    /// * `name` - Adapter name
    /// * `adapter_type` - Type of adapter
    /// * `supported_categories` - Categories this adapter supports
    pub fn register_adapter(
        env: Env,
        adapter_address: Address,
        name: Symbol,
        adapter_type: PriceSourceType,
        supported_categories: Vec<AssetCategory>,
    ) {
        Self::require_admin(&env);

        let mut adapters = Self::get_adapters(&env);
        
        if adapters.contains_key(&adapter_address) {
            panic!("Adapter already registered");
        }

        let config = AdapterConfig {
            address: adapter_address.clone(),
            name,
            adapter_type,
            supported_categories,
            settings: Map::new(&env),
            active: true,
        };

        adapters.set(adapter_address, config);
        env.storage().instance().set(&ADAPTERS, &adapters);

        env.events().publish(
            Symbol::short("ADAPTER_REGISTERED"),
            adapter_address,
        );
    }

    /// Update adapter settings
    ///
    /// # Arguments
    /// * `adapter_address` - Adapter address
    /// * `settings` - New settings
    pub fn update_adapter_settings(
        env: Env,
        adapter_address: Address,
        settings: Map<Symbol, Symbol>,
    ) {
        Self::require_admin(&env);

        let mut adapters = Self::get_adapters(&env);
        
        let mut config = adapters.get(adapter_address.clone())
            .unwrap_or_else(|| panic!("Adapter not registered"));
        
        config.settings = settings;
        adapters.set(adapter_address, config);
        env.storage().instance().set(&ADAPTERS, &adapters);

        env.events().publish(
            Symbol::short("ADAPTER_SETTINGS_UPDATED"),
            adapter_address,
        );
    }

    /// Activate adapter
    ///
    /// # Arguments
    /// * `adapter_address` - Adapter address
    pub fn activate_adapter(env: Env, adapter_address: Address) {
        Self::require_admin(&env);

        let mut adapters = Self::get_adapters(&env);
        
        let mut config = adapters.get(adapter_address.clone())
            .unwrap_or_else(|| panic!("Adapter not registered"));
        
        config.active = true;
        adapters.set(adapter_address, config);
        env.storage().instance().set(&ADAPTERS, &adapters);

        env.events().publish(
            Symbol::short("ADAPTER_ACTIVATED"),
            adapter_address,
        );
    }

    /// Deactivate adapter
    ///
    /// # Arguments
    /// * `adapter_address` - Adapter address
    pub fn deactivate_adapter(env: Env, adapter_address: Address) {
        Self::require_admin(&env);

        let mut adapters = Self::get_adapters(&env);
        
        let mut config = adapters.get(adapter_address.clone())
            .unwrap_or_else(|| panic!("Adapter not registered"));
        
        config.active = false;
        adapters.set(adapter_address, config);
        env.storage().instance().set(&ADAPTERS, &adapters);

        env.events().publish(
            Symbol::short("ADAPTER_DEACTIVATED"),
            adapter_address,
        );
    }

    /// Update category configuration
    ///
    /// # Arguments
    /// * `category` - Asset category
    /// * `config` - New configuration
    pub fn update_category_config(env: Env, category: AssetCategory, config: CategoryConfig) {
        Self::require_admin(&env);

        let mut category_configs = Self::get_category_configs(&env);
        category_configs.set(category, config);
        env.storage().instance().set(&CATEGORY_CONFIGS, &category_configs);

        env.events().publish(
            Symbol::short("CATEGORY_CONFIG_UPDATED"),
            category,
        );
    }

    /// Get category configuration
    ///
    /// # Arguments
    /// * `category` - Asset category
    pub fn get_category_config(env: Env, category: AssetCategory) -> CategoryConfig {
        Self::get_category_configs(&env).get(category)
            .unwrap_or_else(|| panic!("Category config not found"))
    }

    /// Get adapter configuration
    ///
    /// # Arguments
    /// * `adapter_address` - Adapter address
    pub fn get_adapter_config(env: Env, adapter_address: Address) -> AdapterConfig {
        Self::get_adapters(&env).get(adapter_address)
            .unwrap_or_else(|| panic!("Adapter not found"))
    }

    /// Get adapters for a category
    ///
    /// # Arguments
    /// * `category` - Asset category
    pub fn get_adapters_for_category(env: Env, category: AssetCategory) -> Vec<AdapterConfig> {
        let adapters = Self::get_adapters(&env);
        let mut category_adapters = Vec::new(&env);
        
        for config in adapters.values() {
            if config.active {
                for supported_category in config.supported_categories.iter() {
                    if *supported_category == category {
                        category_adapters.push_back(config);
                        break;
                    }
                }
            }
        }
        
        category_adapters
    }

    /// Validate price for category
    ///
    /// # Arguments
    /// * `category` - Asset category
    /// * `price` - Price to validate
    pub fn validate_price(env: Env, category: AssetCategory, price: AssetPrice) -> bool {
        let config = Self::get_category_configs(&env).get(category)
            .unwrap_or_else(|| return false);

        let current_time = env.ledger().timestamp();
        
        // Check price age
        if current_time - price.timestamp > config.max_price_age {
            return false;
        }

        // Check confidence
        if price.confidence < config.min_confidence {
            return false;
        }

        true
    }

    /// Get recommended aggregation method for category
    ///
    /// # Arguments
    /// * `category` - Asset category
    pub fn get_recommended_aggregation(env: Env, category: AssetCategory) -> AggregationMethod {
        let config = Self::get_category_configs(&env).get(category)
            .unwrap_or_else(|| AggregationMethod::WeightedAverage);
        
        config.preferred_aggregation
    }

    /// Get all registered adapters
    pub fn get_all_adapters(env: Env) -> Vec<AdapterConfig> {
        let adapters = Self::get_adapters(&env);
        let mut all_adapters = Vec::new(&env);
        
        for config in adapters.values() {
            all_adapters.push_back(config);
        }
        
        all_adapters
    }

    /// Get all category configurations
    pub fn get_all_category_configs(env: Env) -> Map<AssetCategory, CategoryConfig> {
        Self::get_category_configs(&env)
    }

    // ─── Internal Helpers ─────────────────────────────────────────────────────

    fn initialize_default_configs(env: &Env) -> Map<AssetCategory, CategoryConfig> {
        let mut configs = Map::new(env);

        // Cryptocurrency configuration
        configs.set(
            AssetCategory::Cryptocurrency,
            CategoryConfig {
                category: AssetCategory::Cryptocurrency,
                max_price_age: MAX_PRICE_AGE_CRYPTO,
                min_confidence: MIN_CONFIDENCE_CRYPTO,
                preferred_aggregation: AggregationMethod::WeightedAverage,
                min_sources: 3,
                circuit_breaker_threshold: 1000,
                use_twap: true,
                twap_period: 300,
            },
        );

        // Stablecoin configuration
        configs.set(
            AssetCategory::Stablecoin,
            CategoryConfig {
                category: AssetCategory::Stablecoin,
                max_price_age: MAX_PRICE_AGE_STABLECOIN,
                min_confidence: MIN_CONFIDENCE_STABLECOIN,
                preferred_aggregation: AggregationMethod::Median,
                min_sources: 2,
                circuit_breaker_threshold: 100,
                use_twap: false,
                twap_period: 60,
            },
        );

        // Real-world asset configuration
        configs.set(
            AssetCategory::RealWorldAsset,
            CategoryConfig {
                category: AssetCategory::RealWorldAsset,
                max_price_age: MAX_PRICE_AGE_RWA,
                min_confidence: MIN_CONFIDENCE_RWA,
                preferred_aggregation: AggregationMethod::ConfidenceWeighted,
                min_sources: 2,
                circuit_breaker_threshold: 500,
                use_twap: false,
                twap_period: 3600,
            },
        );

        // Forex configuration
        configs.set(
            AssetCategory::Forex,
            CategoryConfig {
                category: AssetCategory::Forex,
                max_price_age: MAX_PRICE_AGE_FOREX,
                min_confidence: MIN_CONFIDENCE_FOREX,
                preferred_aggregation: AggregationMethod::TimeWeightedAverage,
                min_sources: 3,
                circuit_breaker_threshold: 200,
                use_twap: true,
                twap_period: 60,
            },
        );

        // Native XLM configuration
        configs.set(
            AssetCategory::Native,
            CategoryConfig {
                category: AssetCategory::Native,
                max_price_age: MAX_PRICE_AGE_CRYPTO,
                min_confidence: MIN_CONFIDENCE_CRYPTO,
                preferred_aggregation: AggregationMethod::WeightedAverage,
                min_sources: 3,
                circuit_breaker_threshold: 1000,
                use_twap: true,
                twap_period: 300,
            },
        );

        // DeFi token configuration
        configs.set(
            AssetCategory::DeFiToken,
            CategoryConfig {
                category: AssetCategory::DeFiToken,
                max_price_age: MAX_PRICE_AGE_CRYPTO,
                min_confidence: MIN_CONFIDENCE_CRYPTO,
                preferred_aggregation: AggregationMethod::WeightedAverage,
                min_sources: 3,
                circuit_breaker_threshold: 1500,
                use_twap: true,
                twap_period: 300,
            },
        );

        // Wrapped asset configuration
        configs.set(
            AssetCategory::Wrapped,
            CategoryConfig {
                category: AssetCategory::Wrapped,
                max_price_age: MAX_PRICE_AGE_CRYPTO,
                min_confidence: MIN_CONFIDENCE_CRYPTO,
                preferred_aggregation: AggregationMethod::WeightedAverage,
                min_sources: 3,
                circuit_breaker_threshold: 1000,
                use_twap: true,
                twap_period: 300,
            },
        );

        configs
    }

    fn get_adapters(env: &Env) -> Map<Address, AdapterConfig> {
        env.storage().instance().get(&ADAPTERS).unwrap()
    }

    fn get_category_configs(env: &Env) -> Map<AssetCategory, CategoryConfig> {
        env.storage().instance().get(&CATEGORY_CONFIGS).unwrap()
    }

    fn get_admin(env: &Env) -> Address {
        env.storage().instance().get(&ADMIN).unwrap_optimized()
    }

    fn require_admin(env: &Env) {
        let admin = Self::get_admin(env);
        if env.current_contract_address() != admin {
            panic!("Not authorized");
        }
    }
}

// ─── Pure-Rust Failover Engine + Health Metrics ───────────────────────────────
//
// Compiled only for native targets (tests, off-chain tooling).
// Provides:
//   • AdapterHealth  – per-adapter performance counters (latency, success-rate,
//                      price deviation, uptime)
//   • HealthMonitor  – aggregates health across all adapters and auto-demotes
//                      underperforming ones
//   • FailoverEngine – ordered adapter priority list with automatic failover,
//                      health-check gate, manual override, and event log

#[cfg(not(target_family = "wasm"))]
pub mod failover {
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    // ── Constants ─────────────────────────────────────────────────────────────

    /// Below this success-rate (bps) an adapter is auto-demoted.
    pub const DEMOTION_THRESHOLD_BPS: u32 = 8_000; // 80 %
    /// P95 / P99 bucket count (rolling window length).
    pub const LATENCY_WINDOW: usize = 100;
    /// Deviation from consensus that counts as "bad" (bps).
    pub const MAX_ACCEPTABLE_DEVIATION_BPS: u32 = 200; // 2 %

    // ── Health metrics ────────────────────────────────────────────────────────

    /// Snapshot of one adapter's performance.
    #[derive(Clone, Debug)]
    pub struct AdapterHealth {
        /// Adapter identifier (matches the `name` used at registration).
        pub adapter_id: String,
        /// Total requests sent to this adapter.
        pub total_requests: u64,
        /// Requests that returned a usable price.
        pub successful_requests: u64,
        /// Success rate in basis points (0–10 000).
        pub success_rate_bps: u32,
        /// Rolling response-time samples (milliseconds), oldest-first.
        pub latency_samples_ms: std::collections::VecDeque<u64>,
        /// Average response time (ms) over the rolling window.
        pub avg_latency_ms: u64,
        /// 95th-percentile response time (ms).
        pub p95_latency_ms: u64,
        /// 99th-percentile response time (ms).
        pub p99_latency_ms: u64,
        /// Cumulative price deviation from consensus (bps, sum).
        pub total_deviation_bps: u64,
        /// Average deviation from consensus (bps).
        pub avg_deviation_bps: u32,
        /// Uptime percentage in basis points (0–10 000).
        pub uptime_bps: u32,
        /// Total seconds this adapter has been monitored.
        pub monitored_secs: u64,
        /// Total seconds the adapter was responding successfully.
        pub healthy_secs: u64,
        /// Whether this adapter is currently considered healthy.
        pub is_healthy: bool,
        /// Timestamp (Unix secs) of the last successful response.
        pub last_success_ts: u64,
    }

    impl AdapterHealth {
        fn new(adapter_id: &str) -> Self {
            Self {
                adapter_id: adapter_id.to_string(),
                total_requests: 0,
                successful_requests: 0,
                success_rate_bps: 10_000,
                latency_samples_ms: std::collections::VecDeque::new(),
                avg_latency_ms: 0,
                p95_latency_ms: 0,
                p99_latency_ms: 0,
                total_deviation_bps: 0,
                avg_deviation_bps: 0,
                uptime_bps: 10_000,
                monitored_secs: 0,
                healthy_secs: 0,
                is_healthy: true,
                last_success_ts: 0,
            }
        }

        /// Record one request outcome.
        ///
        /// * `latency_ms`    – measured round-trip time
        /// * `success`       – did the adapter return a usable price?
        /// * `deviation_bps` – abs deviation from consensus price (0 if unknown)
        /// * `now_ts`        – current Unix timestamp (seconds)
        pub fn record(
            &mut self,
            latency_ms: u64,
            success: bool,
            deviation_bps: u32,
            now_ts: u64,
        ) {
            self.total_requests += 1;
            if success {
                self.successful_requests += 1;
                self.last_success_ts = now_ts;
                self.total_deviation_bps += deviation_bps as u64;
            }

            // Latency window
            self.latency_samples_ms.push_back(latency_ms);
            while self.latency_samples_ms.len() > LATENCY_WINDOW {
                self.latency_samples_ms.pop_front();
            }
            self.recompute_latency_stats();

            // Success rate
            self.success_rate_bps = if self.total_requests == 0 {
                10_000
            } else {
                ((self.successful_requests * 10_000) / self.total_requests) as u32
            };

            // Avg deviation
            self.avg_deviation_bps = if self.successful_requests == 0 {
                0
            } else {
                (self.total_deviation_bps / self.successful_requests) as u32
            };

            // Health flag
            self.is_healthy = self.success_rate_bps >= DEMOTION_THRESHOLD_BPS
                && self.avg_deviation_bps <= MAX_ACCEPTABLE_DEVIATION_BPS;
        }

        /// Update uptime counters.  Call once per monitoring tick.
        pub fn tick(&mut self, elapsed_secs: u64, was_healthy: bool) {
            self.monitored_secs += elapsed_secs;
            if was_healthy {
                self.healthy_secs += elapsed_secs;
            }
            self.uptime_bps = if self.monitored_secs == 0 {
                10_000
            } else {
                ((self.healthy_secs * 10_000) / self.monitored_secs) as u32
            };
        }

        fn recompute_latency_stats(&mut self) {
            if self.latency_samples_ms.is_empty() {
                self.avg_latency_ms = 0;
                self.p95_latency_ms = 0;
                self.p99_latency_ms = 0;
                return;
            }
            let mut sorted: Vec<u64> =
                self.latency_samples_ms.iter().copied().collect();
            sorted.sort_unstable();
            let n = sorted.len();
            let sum: u64 = sorted.iter().sum();
            self.avg_latency_ms = sum / n as u64;
            self.p95_latency_ms = sorted[(n * 95 / 100).min(n - 1)];
            self.p99_latency_ms = sorted[(n * 99 / 100).min(n - 1)];
        }
    }

    // ── Health Monitor ────────────────────────────────────────────────────────

    /// Aggregates health across all registered adapters and enforces
    /// auto-demotion of underperformers.
    pub struct HealthMonitor {
        health: HashMap<String, AdapterHealth>,
    }

    impl HealthMonitor {
        pub fn new() -> Self {
            Self { health: HashMap::new() }
        }

        /// Ensure an adapter is tracked; no-op if already present.
        pub fn register(&mut self, adapter_id: &str) {
            self.health
                .entry(adapter_id.to_string())
                .or_insert_with(|| AdapterHealth::new(adapter_id));
        }

        /// Record a request outcome for `adapter_id`.
        pub fn record(
            &mut self,
            adapter_id: &str,
            latency_ms: u64,
            success: bool,
            deviation_bps: u32,
            now_ts: u64,
        ) {
            self.register(adapter_id);
            self.health
                .get_mut(adapter_id)
                .unwrap()
                .record(latency_ms, success, deviation_bps, now_ts);
        }

        /// Advance the uptime clock for all adapters.
        pub fn tick_all(&mut self, elapsed_secs: u64) {
            for health in self.health.values_mut() {
                let was_healthy = health.is_healthy;
                health.tick(elapsed_secs, was_healthy);
            }
        }

        /// Get a snapshot of one adapter's health.
        pub fn get(&self, adapter_id: &str) -> Option<&AdapterHealth> {
            self.health.get(adapter_id)
        }

        /// Return ids of all adapters currently marked unhealthy.
        pub fn unhealthy_adapters(&self) -> Vec<String> {
            self.health
                .values()
                .filter(|h| !h.is_healthy)
                .map(|h| h.adapter_id.clone())
                .collect()
        }

        /// Return all health snapshots sorted by success_rate descending.
        pub fn ranked(&self) -> Vec<AdapterHealth> {
            let mut v: Vec<AdapterHealth> = self.health.values().cloned().collect();
            v.sort_by(|a, b| b.success_rate_bps.cmp(&a.success_rate_bps));
            v
        }
    }

    impl Default for HealthMonitor {
        fn default() -> Self { Self::new() }
    }

    // ── Failover types ────────────────────────────────────────────────────────

    /// Priority entry for one adapter in the failover list.
    #[derive(Clone, Debug)]
    pub struct AdapterPriority {
        /// Adapter identifier.
        pub adapter_id: String,
        /// Lower value = tried first (0 = highest priority).
        pub priority: u32,
        /// Whether this adapter is currently enabled in the failover chain.
        pub enabled: bool,
    }

    /// Reason a failover attempt was skipped or failed.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum FailoverReason {
        /// Adapter is disabled.
        Disabled,
        /// Health-check gate rejected the adapter.
        UnhealthyAdapter,
        /// The adapter returned an error / timeout.
        AdapterError,
        /// Manual override is active — only the override adapter is used.
        ManualOverrideActive,
    }

    /// Event emitted each time the failover engine makes a decision.
    #[derive(Clone, Debug)]
    pub struct FailoverEvent {
        /// Asset being priced.
        pub asset_id: String,
        /// Adapter that was tried.
        pub adapter_id: String,
        /// Whether this attempt succeeded.
        pub succeeded: bool,
        /// Reason for skipping / failure (None on success).
        pub reason: Option<FailoverReason>,
        /// Unix timestamp of the event.
        pub timestamp: u64,
    }

    /// Result returned by a simulated adapter fetch.
    pub type AdapterFetchResult = Result<u64, String>;

    // ── Failover Engine ───────────────────────────────────────────────────────

    /// Manages an ordered adapter priority list and drives automatic failover.
    ///
    /// The engine is deliberately I/O-free: callers supply a closure that
    /// represents "fetch the price from this adapter".  This makes the engine
    /// fully unit-testable without network access.
    pub struct FailoverEngine {
        /// Priority-ordered adapter list (sorted ascending by `priority`).
        adapters: Vec<AdapterPriority>,
        /// Shared health monitor.
        pub health: HealthMonitor,
        /// Optional manual override: if `Some(id)`, only that adapter is tried.
        manual_override: Option<String>,
        /// Accumulated event log.
        events: Vec<FailoverEvent>,
    }

    impl FailoverEngine {
        pub fn new() -> Self {
            Self {
                adapters: Vec::new(),
                health: HealthMonitor::new(),
                manual_override: None,
                events: Vec::new(),
            }
        }

        /// Register an adapter with the given priority (lower = higher priority).
        pub fn register(&mut self, adapter_id: &str, priority: u32) {
            self.health.register(adapter_id);
            // Remove existing entry for the same id, then insert fresh.
            self.adapters.retain(|a| a.adapter_id != adapter_id);
            self.adapters.push(AdapterPriority {
                adapter_id: adapter_id.to_string(),
                priority,
                enabled: true,
            });
            self.adapters.sort_by_key(|a| a.priority);
        }

        /// Enable or disable an adapter in the failover chain.
        pub fn set_enabled(&mut self, adapter_id: &str, enabled: bool) {
            if let Some(a) = self.adapters.iter_mut().find(|a| a.adapter_id == adapter_id) {
                a.enabled = enabled;
            }
        }

        /// Update the priority of an existing adapter.
        pub fn set_priority(&mut self, adapter_id: &str, priority: u32) {
            if let Some(a) = self.adapters.iter_mut().find(|a| a.adapter_id == adapter_id) {
                a.priority = priority;
            }
            self.adapters.sort_by_key(|a| a.priority);
        }

        /// Set a manual override adapter.  Pass `None` to clear.
        pub fn set_manual_override(&mut self, adapter_id: Option<&str>) {
            self.manual_override = adapter_id.map(str::to_string);
        }

        /// Active manual override (if any).
        pub fn manual_override(&self) -> Option<&str> {
            self.manual_override.as_deref()
        }

        /// Run the failover chain for `asset_id`.
        ///
        /// `fetch` is called for each adapter in priority order until one
        /// succeeds.  The closure receives `(adapter_id, asset_id)` and returns
        /// `Ok(price)` or `Err(reason_string)`.
        ///
        /// Health metrics are updated for every attempt automatically.
        ///
        /// Returns `Ok(price)` from the first successful adapter, or
        /// `Err(Vec<FailoverEvent>)` if every adapter in the chain failed.
        pub fn fetch<F>(
            &mut self,
            asset_id: &str,
            now_ts: u64,
            mut fetch: F,
        ) -> Result<u64, Vec<FailoverEvent>>
        where
            F: FnMut(&str, &str) -> (AdapterFetchResult, u64 /* latency_ms */),
        {
            // Manual override path
            if let Some(ref override_id) = self.manual_override.clone() {
                let (result, latency) = fetch(override_id, asset_id);
                let success = result.is_ok();
                self.health.record(override_id, latency, success, 0, now_ts);
                let event = FailoverEvent {
                    asset_id: asset_id.to_string(),
                    adapter_id: override_id.clone(),
                    succeeded: success,
                    reason: if success { None } else { Some(FailoverReason::AdapterError) },
                    timestamp: now_ts,
                };
                self.events.push(event.clone());
                return match result {
                    Ok(price) => Ok(price),
                    Err(_) => Err(vec![event]),
                };
            }

            // Normal priority-ordered failover
            let candidates: Vec<AdapterPriority> = self.adapters.clone();
            let mut failed_events: Vec<FailoverEvent> = Vec::new();

            for adapter in &candidates {
                // Skip disabled adapters
                if !adapter.enabled {
                    let ev = FailoverEvent {
                        asset_id: asset_id.to_string(),
                        adapter_id: adapter.adapter_id.clone(),
                        succeeded: false,
                        reason: Some(FailoverReason::Disabled),
                        timestamp: now_ts,
                    };
                    self.events.push(ev.clone());
                    failed_events.push(ev);
                    continue;
                }

                // Health-check gate
                let is_healthy = self
                    .health
                    .get(&adapter.adapter_id)
                    .map(|h| h.is_healthy)
                    .unwrap_or(true);

                if !is_healthy {
                    let ev = FailoverEvent {
                        asset_id: asset_id.to_string(),
                        adapter_id: adapter.adapter_id.clone(),
                        succeeded: false,
                        reason: Some(FailoverReason::UnhealthyAdapter),
                        timestamp: now_ts,
                    };
                    self.events.push(ev.clone());
                    failed_events.push(ev);
                    continue;
                }

                // Attempt fetch
                let (result, latency) = fetch(&adapter.adapter_id, asset_id);
                let success = result.is_ok();
                self.health.record(&adapter.adapter_id, latency, success, 0, now_ts);

                let ev = FailoverEvent {
                    asset_id: asset_id.to_string(),
                    adapter_id: adapter.adapter_id.clone(),
                    succeeded: success,
                    reason: if success { None } else { Some(FailoverReason::AdapterError) },
                    timestamp: now_ts,
                };
                self.events.push(ev.clone());

                if success {
                    // Auto-demote adapters that fall below threshold
                    self.auto_demote_unhealthy();
                    return Ok(result.unwrap());
                }

                failed_events.push(ev);
            }

            Err(failed_events)
        }

        /// Drain and return all accumulated failover events.
        pub fn drain_events(&mut self) -> Vec<FailoverEvent> {
            std::mem::take(&mut self.events)
        }

        /// Read-only view of pending events.
        pub fn events(&self) -> &[FailoverEvent] {
            &self.events
        }

        /// Current priority list (sorted ascending by priority value).
        pub fn adapters(&self) -> &[AdapterPriority] {
            &self.adapters
        }

        // ── Private helpers ───────────────────────────────────────────────────

        /// Disable adapters whose health has dropped below the demotion threshold.
        fn auto_demote_unhealthy(&mut self) {
            let unhealthy: Vec<String> = self.health.unhealthy_adapters();
            for id in &unhealthy {
                if let Some(a) = self.adapters.iter_mut().find(|a| &a.adapter_id == id) {
                    a.enabled = false;
                }
            }
        }
    }

    impl Default for FailoverEngine {
        fn default() -> Self { Self::new() }
    }
}


#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OracleAdapter {
    pub source_id: Address,
    pub priority: u32,
    pub is_healthy: bool,
    pub last_health_check: u64,
}

pub fn perform_health_check(env: &Env, adapter: &mut OracleAdapter, timeout_seconds: u64) -> bool {
    let current_time = env.ledger().timestamp();
    let timed_out = current_time.saturating_sub(adapter.last_health_check) > timeout_seconds;
    
    if timed_out {
        adapter.is_healthy = false;
    }
    adapter.is_healthy
}