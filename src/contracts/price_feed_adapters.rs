//! Price Feed Source Adapters for Different Asset Categories
//!
//! Provides specialized adapters for different types of price feed sources
//! and asset categories, with category-specific validation and processing.

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
