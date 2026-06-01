//! Multi-Asset Price Oracle for Stellar DeFi Toolkit
//!
//! Enhanced price oracle that supports a wide range of Stellar assets
//! with type-specific configurations and price feed routing.

use soroban_sdk::{contract, contractimpl, Address, Env, Symbol, Vec, Map, unwrap::UnwrapOptimized};
use crate::types::asset::{
    StellarAssetId, AssetCategory, AssetPrice, PriceSource, PriceSourceType,
    AssetMetadata, PriceFeedConfig, AggregationMethod, PriceDeviationAlert,
    AlertSeverity, AssetStats,
};

// ─── Constants ───────────────────────────────────────────────────────────────

/// Default minimum sources for aggregation
const DEFAULT_MIN_SOURCES: u32 = 3;
/// Default maximum price age (1 hour)
const DEFAULT_MAX_PRICE_AGE: u64 = 3600;
/// Default circuit breaker threshold (10%)
const DEFAULT_CIRCUIT_BREAKER: u32 = 1000;
/// Default TWAP period (5 minutes)
const DEFAULT_TWAP_PERIOD: u64 = 300;

// ─── Storage Keys ─────────────────────────────────────────────────────────────

const ADMIN: Symbol = Symbol::short("ADMIN");
const PAUSED: Symbol = Symbol::short("PAUSED");
const ASSET_REGISTRY: Symbol = Symbol::short("ASSET_REG");
const PRICE_FEEDS: Symbol = Symbol::short("PRICE_FEED");
const PRICE_HISTORY: Symbol = Symbol::short("PRICE_HIST");
const DEVIATION_ALERTS: Symbol = Symbol::short("DEV_ALERT");
const AGGREGATION_CACHE: Symbol = Symbol::short("AGG_CACHE");

// ─── Multi-Asset Oracle Contract ───────────────────────────────────────────────

/// Multi-asset price oracle contract
#[contract]
pub struct MultiAssetOracleContract;

#[contractimpl]
impl MultiAssetOracleContract {
    /// Initialize the multi-asset oracle
    ///
    /// # Arguments
    /// * `admin` - Admin address for governance
    /// * `asset_registry_address` - Address of the asset registry contract
    pub fn initialize(env: Env, admin: Address, asset_registry_address: Address) {
        if env.storage().instance().has(&ADMIN) {
            panic!("Already initialized");
        }

        env.storage().instance().set(&ADMIN, &admin);
        env.storage().instance().set(&PAUSED, &false);
        env.storage().instance().set(&ASSET_REGISTRY, &asset_registry_address);

        // Initialize empty storage
        let price_feeds: Map<StellarAssetId, Vec<AssetPrice>> = Map::new(&env);
        env.storage().instance().set(&PRICE_FEEDS, &price_feeds);

        let price_history: Map<StellarAssetId, Vec<AssetPrice>> = Map::new(&env);
        env.storage().instance().set(&PRICE_HISTORY, &price_history);

        let deviation_alerts: Vec<PriceDeviationAlert> = Vec::new(&env);
        env.storage().instance().set(&DEVIATION_ALERTS, &deviation_alerts);

        let aggregation_cache: Map<StellarAssetId, AssetPrice> = Map::new(&env);
        env.storage().instance().set(&AGGREGATION_CACHE, &aggregation_cache);

        env.events().publish(
            Symbol::short("ORACLE_INITIALIZED"),
            (admin, asset_registry_address),
        );
    }

    /// Submit a price for an asset
    ///
    /// # Arguments
    /// * `asset_id` - Asset identifier
    /// * `price` - Price data from a source
    pub fn submit_price(env: Env, asset_id: StellarAssetId, price: AssetPrice) {
        Self::require_not_paused(&env);

        // Verify asset is registered and active
        let asset_registry_address = Self::get_asset_registry_address(&env);
        // In production, this would call the asset registry to verify
        // For now, we'll proceed with the submission

        let mut price_feeds = Self::get_price_feeds(&env);
        let asset_prices = price_feeds.get(asset_id.clone()).unwrap_or_else(|| Vec::new(&env));
        let mut updated_prices = asset_prices;
        
        updated_prices.push_back(price.clone());
        
        // Keep only last 20 price submissions per asset
        if updated_prices.len() > 20 {
            updated_prices.pop_front();
        }
        
        price_feeds.set(asset_id.clone(), updated_prices);
        env.storage().instance().set(&PRICE_FEEDS, &price_feeds);

        // Add to price history
        let mut price_history = Self::get_price_history(&env);
        let history = price_history.get(asset_id.clone()).unwrap_or_else(|| Vec::new(&env));
        let mut updated_history = history;
        
        updated_history.push_back(price.clone());
        
        // Keep only last 100 history entries
        if updated_history.len() > 100 {
            updated_history.pop_front();
        }
        
        price_history.set(asset_id.clone(), updated_history);
        env.storage().instance().set(&PRICE_HISTORY, &price_history);

        // Trigger price aggregation
        Self::aggregate_price(&env, asset_id.clone());

        env.events().publish(
            Symbol::short("PRICE_SUBMITTED"),
            (asset_id, price.price),
        );
    }

    /// Get aggregated price for an asset
    ///
    /// # Arguments
    /// * `asset_id` - Asset identifier
    pub fn get_price(env: Env, asset_id: StellarAssetId) -> AssetPrice {
        // Check cache first
        let cache = Self::get_aggregation_cache(&env);
        if let Some(cached_price) = cache.get(asset_id.clone()) {
            let current_time = env.ledger().timestamp();
            // Cache is valid for 1 minute
            if current_time - cached_price.timestamp < 60 {
                return cached_price;
            }
        }

        // Aggregate fresh price
        Self::aggregate_price(&env, asset_id.clone());
        
        let cache = Self::get_aggregation_cache(&env);
        cache.get(asset_id)
            .unwrap_or_else(|| panic!("Price not available"))
    }

    /// Get price with TWAP
    ///
    /// # Arguments
    /// * `asset_id` - Asset identifier
    /// * `period` - TWAP period in seconds
    pub fn get_twap_price(env: Env, asset_id: StellarAssetId, period: u64) -> AssetPrice {
        let price_history = Self::get_price_history(&env);
        let history = price_history.get(asset_id.clone())
            .unwrap_or_else(|| panic!("No price history for asset"));

        let current_time = env.ledger().timestamp();
        let cutoff_time = current_time - period;

        let mut weighted_sum = 0u128;
        let mut total_weight = 0u64;
        let mut last_timestamp = 0u64;
        let mut last_price = 0u64;
        let mut decimals = 6u32;
        let mut confidence = 0u32;

        for entry in history.iter() {
            if entry.timestamp >= cutoff_time {
                if last_timestamp > 0 {
                    let time_weight = entry.timestamp - last_timestamp;
                    weighted_sum += (last_price as u128) * (time_weight as u128);
                    total_weight += time_weight;
                }
                last_timestamp = entry.timestamp;
                last_price = entry.price;
                decimals = entry.decimals;
                confidence = entry.confidence;
            }
        }

        if total_weight == 0 {
            panic!("No price data in the specified period");
        }

        let twap_price = (weighted_sum / (total_weight as u128)) as u64;

        AssetPrice {
            asset_id,
            price: twap_price,
            decimals,
            confidence,
            timestamp: current_time,
            source: Address::generate(&env),
            price_change_24h: 0,
            high_24h: twap_price,
            low_24h: twap_price,
            volume_24h: 0,
        }
    }

    /// Get prices for multiple assets
    ///
    /// # Arguments
    /// * `asset_ids` - List of asset identifiers
    pub fn get_batch_prices(env: Env, asset_ids: Vec<StellarAssetId>) -> Map<StellarAssetId, AssetPrice> {
        let mut prices = Map::new(&env);
        
        for asset_id in asset_ids.iter() {
            let price = Self::get_price(env.clone(), asset_id.clone());
            prices.set(asset_id.clone(), price);
        }
        
        prices
    }

    /// Get price history for an asset
    ///
    /// # Arguments
    /// * `asset_id` - Asset identifier
    pub fn get_price_history(env: Env, asset_id: StellarAssetId) -> Vec<AssetPrice> {
        Self::get_price_history(&env).get(asset_id)
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Get price deviation alerts
    pub fn get_deviation_alerts(env: Env) -> Vec<PriceDeviationAlert> {
        env.storage().instance().get(&DEVIATION_ALERTS).unwrap()
    }

    /// Clear deviation alerts (admin only)
    pub fn clear_deviation_alerts(env: Env) {
        Self::require_admin(&env);
        
        let alerts: Vec<PriceDeviationAlert> = Vec::new(&env);
        env.storage().instance().set(&DEVIATION_ALERTS, &alerts);
        
        env.events().publish(
            Symbol::short("ALERTS_CLEARED"),
            (),
        );
    }

    /// Pause the oracle (admin only)
    pub fn pause(env: Env) {
        Self::require_admin(&env);
        env.storage().instance().set(&PAUSED, &true);
        env.events().publish(Symbol::short("ORACLE_PAUSED"), true);
    }

    /// Unpause the oracle (admin only)
    pub fn unpause(env: Env) {
        Self::require_admin(&env);
        env.storage().instance().set(&PAUSED, &false);
        env.events().publish(Symbol::short("ORACLE_PAUSED"), false);
    }

    /// Update asset registry address (admin only)
    ///
    /// # Arguments
    /// * `new_address` - New asset registry address
    pub fn update_asset_registry(env: Env, new_address: Address) {
        Self::require_admin(&env);
        env.storage().instance().set(&ASSET_REGISTRY, &new_address);
        
        env.events().publish(
            Symbol::short("REGISTRY_UPDATED"),
            new_address,
        );
    }

    // ─── Internal Helpers ─────────────────────────────────────────────────────

    fn aggregate_price(env: &Env, asset_id: StellarAssetId) {
        let price_feeds = Self::get_price_feeds(env);
        let prices = price_feeds.get(asset_id.clone())
            .unwrap_or_else(|| return);

        if prices.is_empty() {
            return;
        }

        // Use weighted average aggregation
        let mut weighted_sum = 0u128;
        let mut total_weight = 0u32;
        let mut total_confidence = 0u32;
        let mut decimals = 6u32;

        for price in prices.iter() {
            let weight = price.confidence;
            weighted_sum += (price.price as u128) * (weight as u128);
            total_weight += weight;
            total_confidence += price.confidence;
            decimals = price.decimals;
        }

        if total_weight == 0 {
            return;
        }

        let aggregated_price = (weighted_sum / (total_weight as u128)) as u64;
        let avg_confidence = total_confidence / (prices.len() as u32);

        // Check for price deviation
        let cache = Self::get_aggregation_cache(env);
        if let Some(cached_price) = cache.get(asset_id.clone()) {
            let deviation = Self::calculate_deviation(cached_price.price, aggregated_price);
            
            if deviation > DEFAULT_CIRCUIT_BREAKER {
                Self::create_deviation_alert(
                    env,
                    asset_id.clone(),
                    cached_price.price,
                    aggregated_price,
                    deviation,
                );
            }
        }

        // Create aggregated price
        let asset_price = AssetPrice {
            asset_id: asset_id.clone(),
            price: aggregated_price,
            decimals,
            confidence: avg_confidence,
            timestamp: env.ledger().timestamp(),
            source: Address::generate(env),
            price_change_24h: 0,
            high_24h: aggregated_price,
            low_24h: aggregated_price,
            volume_24h: 0,
        };

        // Update cache
        let mut aggregation_cache = Self::get_aggregation_cache(env);
        aggregation_cache.set(asset_id, asset_price);
        env.storage().instance().set(&AGGREGATION_CACHE, &aggregation_cache);
    }

    fn calculate_deviation(old_price: u64, new_price: u64) -> u32 {
        if old_price == 0 {
            return 0;
        }
        
        let diff = if new_price > old_price {
            new_price - old_price
        } else {
            old_price - new_price
        };
        
        ((diff as u128) * 10000 / (old_price as u128)) as u32
    }

    fn create_deviation_alert(
        env: &Env,
        asset_id: StellarAssetId,
        expected_price: u64,
        actual_price: u64,
        deviation_bps: u32,
    ) {
        let severity = if deviation_bps > 2000 {
            AlertSeverity::Critical
        } else if deviation_bps > 1000 {
            AlertSeverity::High
        } else if deviation_bps > 500 {
            AlertSeverity::Medium
        } else {
            AlertSeverity::Low
        };
        
        let alert = PriceDeviationAlert {
            asset_id: asset_id.clone(),
            expected_price,
            reported_price: actual_price,
            deviation_bps,
            source: Address::generate(env),
            severity,
            timestamp: env.ledger().timestamp(),
        };
        
        let mut alerts = env.storage().instance().get(&DEVIATION_ALERTS).unwrap();
        alerts.push_back(alert);
        
        // Keep only last 50 alerts
        if alerts.len() > 50 {
            alerts.pop_front();
        }
        
        env.storage().instance().set(&DEVIATION_ALERTS, &alerts);
        
        env.events().publish(
            (Symbol::short("PRICE_DEVIATION"), asset_id.clone()),
            (expected_price, actual_price, deviation_bps),
        );
    }

    fn get_price_feeds(env: &Env) -> Map<StellarAssetId, Vec<AssetPrice>> {
        env.storage().instance().get(&PRICE_FEEDS).unwrap()
    }

    fn get_price_history(env: &Env) -> Map<StellarAssetId, Vec<AssetPrice>> {
        env.storage().instance().get(&PRICE_HISTORY).unwrap()
    }

    fn get_aggregation_cache(env: &Env) -> Map<StellarAssetId, AssetPrice> {
        env.storage().instance().get(&AGGREGATION_CACHE).unwrap()
    }

    fn get_asset_registry_address(env: &Env) -> Address {
        env.storage().instance().get(&ASSET_REGISTRY).unwrap_optimized()
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

    fn require_not_paused(env: &Env) {
        let paused = env.storage().instance().get(&PAUSED).unwrap();
        if paused {
            panic!("Oracle is paused");
        }
    }
}
