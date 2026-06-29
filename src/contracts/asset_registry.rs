//! Asset Registry Contract for Stellar DeFi Toolkit
//!
//! Manages registration and configuration of a wide range of Stellar assets
//! for price feed support. This contract serves as a central registry for
//! asset metadata, price feed configurations, and asset whitelisting.

use soroban_sdk::{contract, contractimpl, Address, Env, Symbol, Vec, Map, unwrap::UnwrapOptimized};
use crate::types::asset::{
    StellarAssetId, AssetCategory, AssetMetadata, PriceFeedConfig, AssetPrice,
    PriceSource, PriceSourceType, AssetRegistryEntry, WhitelistEntry, CrossChainAsset,
    AggregationMethod, AlertSeverity, PriceDeviationAlert, AssetStats,
};

// ─── Constants ───────────────────────────────────────────────────────────────

/// Maximum number of assets that can be registered
const MAX_ASSETS: u32 = 1000;
/// Maximum number of price sources per asset
const MAX_SOURCES_PER_ASSET: u32 = 10;
/// Maximum price history entries per asset
const MAX_PRICE_HISTORY: u32 = 100;
/// Default minimum update interval (5 minutes)
const DEFAULT_MIN_UPDATE_INTERVAL: u64 = 300;
/// Default maximum price deviation (5%)
const DEFAULT_MAX_DEVIATION: u32 = 500;
/// Default minimum confidence (70%)
const DEFAULT_MIN_CONFIDENCE: u32 = 7000;

// ─── Storage Keys ─────────────────────────────────────────────────────────────

const ADMIN: Symbol = Symbol::short("ADMIN");
const PAUSED: Symbol = Symbol::short("PAUSED");
const ASSET_REGISTRY: Symbol = Symbol::short("ASSET_REG");
const PRICE_SOURCES: Symbol = Symbol::short("PRICE_SRC");
const WHITELIST: Symbol = Symbol::short("WHITELIST");
const CROSS_CHAIN_ASSETS: Symbol = Symbol::short("XCHAIN");
const DEVIATION_ALERTS: Symbol = Symbol::short("DEV_ALERT");
const ASSET_STATS: Symbol = Symbol::short("ASSET_STATS");

// ─── Asset Registry Contract ───────────────────────────────────────────────────

/// Asset registry contract for managing Stellar assets
#[contract]
pub struct AssetRegistryContract;

#[contractimpl]
impl AssetRegistryContract {
    /// Initialize the asset registry
    ///
    /// # Arguments
    /// * `admin` - Admin address for governance
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&ADMIN) {
            panic!("Already initialized");
        }

        env.storage().instance().set(&ADMIN, &admin);
        env.storage().instance().set(&PAUSED, &false);

        // Initialize empty storage
        let asset_registry: Map<StellarAssetId, AssetRegistryEntry> = Map::new(&env);
        env.storage().instance().set(&ASSET_REGISTRY, &asset_registry);

        let price_sources: Map<Address, PriceSource> = Map::new(&env);
        env.storage().instance().set(&PRICE_SOURCES, &price_sources);

        let whitelist: Map<StellarAssetId, WhitelistEntry> = Map::new(&env);
        env.storage().instance().set(&WHITELIST, &whitelist);

        let cross_chain_assets: Map<StellarAssetId, CrossChainAsset> = Map::new(&env);
        env.storage().instance().set(&CROSS_CHAIN_ASSETS, &cross_chain_assets);

        let deviation_alerts: Vec<PriceDeviationAlert> = Vec::new(&env);
        env.storage().instance().set(&DEVIATION_ALERTS, &deviation_alerts);

        let asset_stats: Map<StellarAssetId, AssetStats> = Map::new(&env);
        env.storage().instance().set(&ASSET_STATS, &asset_stats);

        env.events().publish(
            Symbol::short("REGISTRY_INITIALIZED"),
            admin,
        );
    }

    /// Register a new asset
    ///
    /// # Arguments
    /// * `asset_id` - Unique asset identifier
    /// * `symbol` - Asset symbol
    /// * `name` - Asset name
    /// * `category` - Asset category
    /// * `decimals` - Number of decimals
    pub fn register_asset(
        env: Env,
        asset_id: StellarAssetId,
        symbol: Symbol,
        name: Symbol,
        category: AssetCategory,
        decimals: u32,
    ) {
        Self::require_admin(&env);
        Self::require_not_paused(&env);

        let mut asset_registry = Self::get_asset_registry(&env);
        
        if asset_registry.contains_key(&asset_id) {
            panic!("Asset already registered");
        }

        let asset_count = asset_registry.len();
        if asset_count >= MAX_ASSETS {
            panic!("Maximum asset limit reached");
        }

        // Create asset metadata
        let metadata = AssetMetadata {
            asset_id: asset_id.clone(),
            symbol,
            name,
            category,
            decimals,
            active: true,
            min_update_interval: DEFAULT_MIN_UPDATE_INTERVAL,
            max_price_deviation: DEFAULT_MAX_DEVIATION,
            min_confidence: DEFAULT_MIN_CONFIDENCE,
            approved_sources: Vec::new(&env),
            registered_at: env.ledger().timestamp(),
            last_price_update: 0,
            custom_metadata: Map::new(&env),
        };

        // Create default price feed configuration
        let price_config = PriceFeedConfig {
            asset_id: asset_id.clone(),
            aggregation_method: AggregationMethod::WeightedAverage,
            min_sources: 3,
            max_price_age: 3600,
            circuit_breaker_threshold: 1000,
            use_twap: false,
            twap_period: 300,
            heartbeat_interval: 300,
        };

        // Create registry entry
        let entry = AssetRegistryEntry {
            metadata,
            price_config,
            current_price: None,
            price_history: Vec::new(&env),
        };

        asset_registry.set(asset_id.clone(), entry);
        env.storage().instance().set(&ASSET_REGISTRY, &asset_registry);

        // Initialize asset stats
        let stats = AssetStats {
            asset_id: asset_id.clone(),
            total_updates: 0,
            avg_update_interval: 0,
            deviation_alerts: 0,
            current_confidence: 0,
            avg_confidence: 0,
            last_update: 0,
        };
        
        let mut asset_stats = Self::get_asset_stats(&env);
        asset_stats.set(asset_id, stats);
        env.storage().instance().set(&ASSET_STATS, &asset_stats);

        env.events().publish(
            Symbol::short("ASSET_REGISTERED"),
            (symbol, category),
        );
    }

    /// Update asset metadata
    ///
    /// # Arguments
    /// * `asset_id` - Asset identifier
    /// * `metadata` - New metadata
    pub fn update_asset_metadata(env: Env, asset_id: StellarAssetId, metadata: AssetMetadata) {
        Self::require_admin(&env);
        Self::require_not_paused(&env);

        let mut asset_registry = Self::get_asset_registry(&env);
        
        let mut entry = asset_registry.get(asset_id.clone())
            .unwrap_or_else(|| panic!("Asset not registered"));
        
        entry.metadata = metadata;
        asset_registry.set(asset_id, entry);
        env.storage().instance().set(&ASSET_REGISTRY, &asset_registry);

        env.events().publish(
            Symbol::short("METADATA_UPDATED"),
            asset_id,
        );
    }

    /// Update price feed configuration for an asset
    ///
    /// # Arguments
    /// * `asset_id` - Asset identifier
    /// * `config` - New price feed configuration
    pub fn update_price_config(env: Env, asset_id: StellarAssetId, config: PriceFeedConfig) {
        Self::require_admin(&env);
        Self::require_not_paused(&env);

        let mut asset_registry = Self::get_asset_registry(&env);
        
        let mut entry = asset_registry.get(asset_id.clone())
            .unwrap_or_else(|| panic!("Asset not registered"));
        
        entry.price_config = config;
        asset_registry.set(asset_id, entry);
        env.storage().instance().set(&ASSET_REGISTRY, &asset_registry);

        env.events().publish(
            Symbol::short("PRICE_CONFIG_UPDATED"),
            asset_id,
        );
    }

    /// Add a price source for an asset
    ///
    /// # Arguments
    /// * `asset_id` - Asset identifier
    /// * `source_address` - Price source address
    pub fn add_price_source(env: Env, asset_id: StellarAssetId, source_address: Address) {
        Self::require_admin(&env);
        Self::require_not_paused(&env);

        let mut asset_registry = Self::get_asset_registry(&env);
        
        let mut entry = asset_registry.get(asset_id.clone())
            .unwrap_or_else(|| panic!("Asset not registered"));
        
        if entry.metadata.approved_sources.len() >= MAX_SOURCES_PER_ASSET {
            panic!("Maximum price sources reached");
        }

        // Check if source is already approved
        for addr in entry.metadata.approved_sources.iter() {
            if addr == source_address {
                panic!("Source already approved");
            }
        }

        entry.metadata.approved_sources.push_back(source_address);
        asset_registry.set(asset_id, entry);
        env.storage().instance().set(&ASSET_REGISTRY, &asset_registry);

        env.events().publish(
            Symbol::short("PRICE_SOURCE_ADDED"),
            (asset_id, source_address),
        );
    }

    /// Remove a price source from an asset
    ///
    /// # Arguments
    /// * `asset_id` - Asset identifier
    /// * `source_address` - Price source address
    pub fn remove_price_source(env: Env, asset_id: StellarAssetId, source_address: Address) {
        Self::require_admin(&env);
        Self::require_not_paused(&env);

        let mut asset_registry = Self::get_asset_registry(&env);
        
        let mut entry = asset_registry.get(asset_id.clone())
            .unwrap_or_else(|| panic!("Asset not registered"));
        
        let mut found = false;
        let mut new_sources = Vec::new(&env);
        
        for addr in entry.metadata.approved_sources.iter() {
            if addr == source_address {
                found = true;
            } else {
                new_sources.push_back(addr);
            }
        }

        if !found {
            panic!("Source not found");
        }

        entry.metadata.approved_sources = new_sources;
        asset_registry.set(asset_id, entry);
        env.storage().instance().set(&ASSET_REGISTRY, &asset_registry);

        env.events().publish(
            Symbol::short("PRICE_SOURCE_REMOVED"),
            (asset_id, source_address),
        );
    }

    /// Register a price source
    ///
    /// # Arguments
    /// * `source_address` - Source address
    /// * `name` - Source name
    /// * `source_type` - Type of price source
    /// * `weight` - Weight in aggregation
    /// * `supported_categories` - Categories this source supports
    pub fn register_price_source(
        env: Env,
        source_address: Address,
        name: Symbol,
        source_type: PriceSourceType,
        weight: u32,
        supported_categories: Vec<AssetCategory>,
    ) {
        Self::require_admin(&env);
        Self::require_not_paused(&env);

        let mut price_sources = Self::get_price_sources(&env);
        
        if price_sources.contains_key(&source_address) {
            panic!("Price source already registered");
        }

        if weight == 0 || weight > 10000 {
            panic!("Invalid weight");
        }

        let price_source = PriceSource {
            address: source_address.clone(),
            name,
            source_type,
            weight,
            reputation: 8000,
            active: true,
            supported_categories,
            last_update: 0,
            successful_updates: 0,
            failed_updates: 0,
        };

        price_sources.set(source_address, price_source);
        env.storage().instance().set(&PRICE_SOURCES, &price_sources);

        env.events().publish(
            Symbol::short("PRICE_SOURCE_REGISTERED"),
            (source_address, name),
        );
    }

    /// Update asset price
    ///
    /// # Arguments
    /// * `asset_id` - Asset identifier
    /// * `price` - New price data
    pub fn update_price(env: Env, asset_id: StellarAssetId, price: AssetPrice) {
        Self::require_not_paused(&env);

        let mut asset_registry = Self::get_asset_registry(&env);
        
        let mut entry = asset_registry.get(asset_id.clone())
            .unwrap_or_else(|| panic!("Asset not registered"));
        
        if !entry.metadata.active {
            panic!("Asset is not active");
        }

        // Check update interval
        let current_time = env.ledger().timestamp();
        if entry.metadata.last_price_update > 0 {
            let elapsed = current_time - entry.metadata.last_price_update;
            if elapsed < entry.metadata.min_update_interval {
                panic!("Update interval not met");
            }
        }

        // Update price history
        if let Some(current_price) = entry.current_price.clone() {
            entry.price_history.push_back(current_price);
            
            // Keep only last N entries
            if entry.price_history.len() > MAX_PRICE_HISTORY {
                entry.price_history.pop_front();
            }
        }

        // Update current price
        entry.current_price = Some(price.clone());
        entry.metadata.last_price_update = current_time;
        
        asset_registry.set(asset_id.clone(), entry);
        env.storage().instance().set(&ASSET_REGISTRY, &asset_registry);

        // Update asset stats
        Self::update_asset_stats(&env, asset_id.clone(), &price);

        env.events().publish(
            Symbol::short("PRICE_UPDATED"),
            (asset_id, price.price),
        );
    }

    /// Get asset information
    ///
    /// # Arguments
    /// * `asset_id` - Asset identifier
    pub fn get_asset(env: Env, asset_id: StellarAssetId) -> AssetRegistryEntry {
        Self::get_asset_registry(&env).get(asset_id)
            .unwrap_or_else(|| panic!("Asset not registered"))
    }

    /// Get all registered assets
    pub fn get_all_assets(env: Env) -> Vec<AssetRegistryEntry> {
        let asset_registry = Self::get_asset_registry(&env);
        let mut assets = Vec::new(&env);
        
        for entry in asset_registry.values() {
            assets.push_back(entry);
        }
        
        assets
    }

    /// Get assets by category
    ///
    /// # Arguments
    /// * `category` - Asset category
    pub fn get_assets_by_category(env: Env, category: AssetCategory) -> Vec<AssetRegistryEntry> {
        let asset_registry = Self::get_asset_registry(&env);
        let mut assets = Vec::new(&env);
        
        for entry in asset_registry.values() {
            if entry.metadata.category == category {
                assets.push_back(entry);
            }
        }
        
        assets
    }

    /// Get asset price
    ///
    /// # Arguments
    /// * `asset_id` - Asset identifier
    pub fn get_price(env: Env, asset_id: StellarAssetId) -> AssetPrice {
        let entry = Self::get_asset_registry(&env).get(asset_id)
            .unwrap_or_else(|| panic!("Asset not registered"));
        
        entry.current_price.unwrap_or_else(|| panic!("No price available"))
    }

    /// Get asset price history
    ///
    /// # Arguments
    /// * `asset_id` - Asset identifier
    pub fn get_price_history(env: Env, asset_id: StellarAssetId) -> Vec<AssetPrice> {
        let entry = Self::get_asset_registry(&env).get(asset_id)
            .unwrap_or_else(|| panic!("Asset not registered"));
        
        entry.price_history
    }

    /// Add asset to whitelist
    ///
    /// # Arguments
    /// * `asset_id` - Asset identifier
    /// * `reason` - Reason for whitelisting
    pub fn whitelist_asset(env: Env, asset_id: StellarAssetId, reason: Symbol) {
        Self::require_admin(&env);
        Self::require_not_paused(&env);

        let mut whitelist = Self::get_whitelist(&env);
        
        if whitelist.contains_key(&asset_id) {
            panic!("Asset already whitelisted");
        }

        let entry = WhitelistEntry {
            asset_id: asset_id.clone(),
            added_by: Self::get_admin(&env),
            reason,
            added_at: env.ledger().timestamp(),
            active: true,
        };

        whitelist.set(asset_id, entry);
        env.storage().instance().set(&WHITELIST, &whitelist);

        env.events().publish(
            Symbol::short("ASSET_WHITELISTED"),
            asset_id,
        );
    }

    /// Remove asset from whitelist
    ///
    /// # Arguments
    /// * `asset_id` - Asset identifier
    pub fn remove_from_whitelist(env: Env, asset_id: StellarAssetId) {
        Self::require_admin(&env);

        let mut whitelist = Self::get_whitelist(&env);
        
        if !whitelist.contains_key(&asset_id) {
            panic!("Asset not whitelisted");
        }

        whitelist.remove(asset_id.clone());
        env.storage().instance().set(&WHITELIST, &whitelist);

        env.events().publish(
            Symbol::short("ASSET_REMOVED_FROM_WHITELIST"),
            asset_id,
        );
    }

    /// Check if asset is whitelisted
    ///
    /// # Arguments
    /// * `asset_id` - Asset identifier
    pub fn is_whitelisted(env: Env, asset_id: StellarAssetId) -> bool {
        let whitelist = Self::get_whitelist(&env);
        
        match whitelist.get(asset_id) {
            Some(entry) => entry.active,
            None => false,
        }
    }

    /// Register cross-chain asset
    ///
    /// # Arguments
    /// * `cross_chain_asset` - Cross-chain asset information
    pub fn register_cross_chain_asset(env: Env, cross_chain_asset: CrossChainAsset) {
        Self::require_admin(&env);
        Self::require_not_paused(&env);

        let mut cross_chain_assets = Self::get_cross_chain_assets(&env);
        
        let stellar_asset = cross_chain_asset.stellar_asset.clone();
        cross_chain_assets.set(stellar_asset, cross_chain_asset);
        env.storage().instance().set(&CROSS_CHAIN_ASSETS, &cross_chain_assets);

        env.events().publish(
            Symbol::short("XCHAIN_ASSET_REGISTERED"),
            stellar_asset,
        );
    }

    /// Get cross-chain asset information
    ///
    /// # Arguments
    /// * `stellar_asset` - Stellar asset identifier
    pub fn get_cross_chain_asset(env: Env, stellar_asset: StellarAssetId) -> CrossChainAsset {
        Self::get_cross_chain_assets(&env).get(stellar_asset)
            .unwrap_or_else(|| panic!("Cross-chain asset not found"))
    }

    /// Activate asset
    ///
    /// # Arguments
    /// * `asset_id` - Asset identifier
    pub fn activate_asset(env: Env, asset_id: StellarAssetId) {
        Self::require_admin(&env);

        let mut asset_registry = Self::get_asset_registry(&env);
        
        let mut entry = asset_registry.get(asset_id.clone())
            .unwrap_or_else(|| panic!("Asset not registered"));
        
        entry.metadata.active = true;
        asset_registry.set(asset_id, entry);
        env.storage().instance().set(&ASSET_REGISTRY, &asset_registry);

        env.events().publish(
            Symbol::short("ASSET_ACTIVATED"),
            asset_id,
        );
    }

    /// Deactivate asset
    ///
    /// # Arguments
    /// * `asset_id` - Asset identifier
    pub fn deactivate_asset(env: Env, asset_id: StellarAssetId) {
        Self::require_admin(&env);

        let mut asset_registry = Self::get_asset_registry(&env);
        
        let mut entry = asset_registry.get(asset_id.clone())
            .unwrap_or_else(|| panic!("Asset not registered"));
        
        entry.metadata.active = false;
        asset_registry.set(asset_id, entry);
        env.storage().instance().set(&ASSET_REGISTRY, &asset_registry);

        env.events().publish(
            Symbol::short("ASSET_DEACTIVATED"),
            asset_id,
        );
    }

    /// Pause the registry (admin only)
    pub fn pause(env: Env) {
        Self::require_admin(&env);
        env.storage().instance().set(&PAUSED, &true);
        env.events().publish(Symbol::short("REGISTRY_PAUSED"), true);
    }

    /// Unpause the registry (admin only)
    pub fn unpause(env: Env) {
        Self::require_admin(&env);
        env.storage().instance().set(&PAUSED, &false);
        env.events().publish(Symbol::short("REGISTRY_PAUSED"), false);
    }

    /// Get asset statistics
    ///
    /// # Arguments
    /// * `asset_id` - Asset identifier
    pub fn get_asset_stats(env: Env, asset_id: StellarAssetId) -> AssetStats {
        Self::get_asset_stats(&env).get(asset_id)
            .unwrap_or_else(|| panic!("Asset stats not found"))
    }

    // ─── Internal Helpers ─────────────────────────────────────────────────────

    fn update_asset_stats(env: &Env, asset_id: StellarAssetId, price: &AssetPrice) {
        let mut asset_stats = Self::get_asset_stats(env);
        
        let mut stats = asset_stats.get(asset_id.clone())
            .unwrap_or_else(|| AssetStats {
                asset_id: asset_id.clone(),
                total_updates: 0,
                avg_update_interval: 0,
                deviation_alerts: 0,
                current_confidence: 0,
                avg_confidence: 0,
                last_update: 0,
            });

        let current_time = env.ledger().timestamp();
        
        // Update total updates
        stats.total_updates += 1;
        
        // Update average interval
        if stats.last_update > 0 {
            let interval = current_time - stats.last_update;
            stats.avg_update_interval = (stats.avg_update_interval * (stats.total_updates - 1) + interval) / stats.total_updates;
        }
        
        // Update confidence
        stats.current_confidence = price.confidence;
        stats.avg_confidence = (stats.avg_confidence * (stats.total_updates - 1) + price.confidence) / stats.total_updates;
        
        // Update last update
        stats.last_update = current_time;

        asset_stats.set(asset_id, stats);
        env.storage().instance().set(&ASSET_STATS, &asset_stats);
    }

    fn get_asset_registry(env: &Env) -> Map<StellarAssetId, AssetRegistryEntry> {
        env.storage().instance().get(&ASSET_REGISTRY).unwrap()
    }

    fn get_price_sources(env: &Env) -> Map<Address, PriceSource> {
        env.storage().instance().get(&PRICE_SOURCES).unwrap()
    }

    fn get_whitelist(env: &Env) -> Map<StellarAssetId, WhitelistEntry> {
        env.storage().instance().get(&WHITELIST).unwrap()
    }

    fn get_cross_chain_assets(env: &Env) -> Map<StellarAssetId, CrossChainAsset> {
        env.storage().instance().get(&CROSS_CHAIN_ASSETS).unwrap()
    }

    fn get_asset_stats(env: &Env) -> Map<StellarAssetId, AssetStats> {
        env.storage().instance().get(&ASSET_STATS).unwrap()
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
            panic!("Registry is paused");
        }
    }
}
