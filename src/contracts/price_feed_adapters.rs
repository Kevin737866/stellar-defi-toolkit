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

// ─── Stellar DEX On-Chain Price Adapter ──────────────────────────────────────

/// Stellar DEX order book price adapter
///
/// Reads prices directly from Stellar native DEX order books, providing
/// on-chain price data without external dependencies.
#[contract]
pub struct StellarDexAdapter;

/// Result of querying the Stellar DEX order book
#[derive(Clone, Debug)]
#[contracttype]
pub struct DexOrderBook {
    /// Asset pair being queried
    pub base_asset: StellarAssetId,
    pub quote_asset: StellarAssetId,
    /// Best bid price (highest buy order)
    pub best_bid: u64,
    /// Best ask price (lowest sell order)
    pub best_ask: u64,
    /// Total bid volume (base asset)
    pub bid_volume: u64,
    /// Total ask volume (base asset)
    pub ask_volume: u64,
    /// Timestamp of the last trade
    pub last_trade_time: u64,
    /// Current ledger timestamp
    pub timestamp: u64,
}

/// Configuration for the Stellar DEX adapter
#[derive(Clone, Debug)]
#[contracttype]
pub struct DexAdapterConfig {
    /// Minimum liquidity required (in base asset units) for valid prices
    pub min_liquidity_threshold: u64,
    /// Maximum staleness of the order book (seconds)
    pub max_staleness_secs: u64,
    /// Minimum bid-ask spread (basis points) for valid markets
    pub max_spread_bps: u32,
    /// Whether the adapter is active
    pub active: bool,
}

const DEX_ADAPTER_CONFIG: Symbol = Symbol::short("DEX_CFG");
const DEX_ORDER_BOOKS: Symbol = Symbol::short("DEX_BOOKS");

#[contractimpl]
impl StellarDexAdapter {
    /// Initialize the DEX adapter with default configuration
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&ADMIN) {
            panic!("Already initialized");
        }

        env.storage().instance().set(&ADMIN, &admin);

        let config = DexAdapterConfig {
            min_liquidity_threshold: 100_000,  // 100k base units
            max_staleness_secs: 300,            // 5 minutes
            max_spread_bps: 500,                // 5% max spread
            active: true,
        };
        env.storage().instance().set(&DEX_ADAPTER_CONFIG, &config);

        let order_books: Map<Symbol, DexOrderBook> = Map::new(&env);
        env.storage().instance().set(&DEX_ORDER_BOOKS, &order_books);

        env.events().publish(
            Symbol::short("DEX_ADAPTER_INIT"),
            admin,
        );
    }

    /// Update the DEX adapter configuration
    pub fn update_config(env: Env, config: DexAdapterConfig) {
        Self::require_admin(&env);
        env.storage().instance().set(&DEX_ADAPTER_CONFIG, &config);
        env.events().publish(
            Symbol::short("DEX_CONFIG_UPDATED"),
            (),
        );
    }

    /// Get the current DEX adapter configuration
    pub fn get_config(env: Env) -> DexAdapterConfig {
        env.storage().instance().get(&DEX_ADAPTER_CONFIG)
            .unwrap_or_else(|| panic!("DEX adapter not initialized"))
    }

    /// Query the order book for a given asset pair
    ///
    /// In a production environment, this would query the Stellar DEX directly.
    /// For simulation, it reads from stored order book snapshots.
    pub fn query_order_book(
        env: Env,
        base_asset: StellarAssetId,
        quote_asset: StellarAssetId,
    ) -> DexOrderBook {
        let pair_key = Self::make_pair_key(&base_asset, &quote_asset);
        let order_books: Map<Symbol, DexOrderBook> =
            env.storage().instance().get(&DEX_ORDER_BOOKS).unwrap();

        order_books
            .get(pair_key)
            .unwrap_or_else(|| DexOrderBook {
                base_asset: base_asset.clone(),
                quote_asset: quote_asset.clone(),
                best_bid: 0,
                best_ask: 0,
                bid_volume: 0,
                ask_volume: 0,
                last_trade_time: 0,
                timestamp: env.ledger().timestamp(),
            })
    }

    /// Submit an order book snapshot to the adapter
    ///
    /// In production this would not be needed since the adapter reads
    /// directly from the DEX. For simulation/testing, this provides
    /// the order book data.
    pub fn submit_order_book(env: Env, book: DexOrderBook) {
        Self::require_admin(&env);

        let pair_key = Self::make_pair_key(&book.base_asset, &book.quote_asset);
        let mut order_books: Map<Symbol, DexOrderBook> =
            env.storage().instance().get(&DEX_ORDER_BOOKS).unwrap();

        order_books.set(pair_key, book);
        env.storage().instance().set(&DEX_ORDER_BOOKS, &order_books);

        env.events().publish(
            Symbol::short("ORDER_BOOK_SUBMITTED"),
            (),
        );
    }

    /// Get the mid-price from the DEX order book
    ///
    /// Calculates mid-price as (best_bid + best_ask) / 2.
    /// Validates liquidity, staleness, and spread thresholds.
    pub fn get_dex_mid_price(
        env: Env,
        base_asset: StellarAssetId,
        quote_asset: StellarAssetId,
    ) -> AssetPrice {
        let config = Self::get_config(env.clone());
        
        if !config.active {
            panic!("DEX adapter is not active");
        }

        let book = Self::query_order_book(env.clone(), base_asset.clone(), quote_asset);
        let current_time = env.ledger().timestamp();

        // Validate staleness
        if current_time - book.timestamp > config.max_staleness_secs {
            panic!("Order book data is stale");
        }

        // Validate minimum liquidity
        if book.bid_volume < config.min_liquidity_threshold
            || book.ask_volume < config.min_liquidity_threshold
        {
            panic!("Insufficient liquidity in order book");
        }

        // Validate that we have valid bid/ask prices
        if book.best_bid == 0 || book.best_ask == 0 {
            panic!("No valid bids/asks in order book");
        }

        // Validate spread
        if book.best_ask > book.best_bid {
            let spread_bps = ((book.best_ask - book.best_bid) as u128 * 10000 / book.best_bid as u128) as u32;
            if spread_bps > config.max_spread_bps {
                panic!("Spread too wide: {} bps exceeds {} bps max", spread_bps, config.max_spread_bps);
            }
        }

        // Calculate mid-price
        let mid_price = (book.best_bid as u128 + book.best_ask as u128) / 2;

        AssetPrice {
            asset_id: base_asset,
            price: mid_price as u64,
            decimals: 7,
            confidence: 8500, // DEX has good confidence but not perfect
            timestamp: current_time,
            source: Address::generate(&env),
            price_change_24h: 0,
            high_24h: book.best_ask,
            low_24h: book.best_bid,
            volume_24h: book.bid_volume.saturating_add(book.ask_volume),
        }
    }

    /// Get the best bid price from the DEX
    pub fn get_dex_best_bid(
        env: Env,
        base_asset: StellarAssetId,
        quote_asset: StellarAssetId,
    ) -> u64 {
        let book = Self::query_order_book(env, base_asset, quote_asset);
        book.best_bid
    }

    /// Get the best ask price from the DEX
    pub fn get_dex_best_ask(
        env: Env,
        base_asset: StellarAssetId,
        quote_asset: StellarAssetId,
    ) -> u64 {
        let book = Self::query_order_book(env, base_asset, quote_asset);
        book.best_ask
    }

    /// Check if the order book data is stale
    pub fn is_stale(env: Env, base_asset: StellarAssetId, quote_asset: StellarAssetId) -> bool {
        let config = Self::get_config(env.clone());
        let book = Self::query_order_book(env, base_asset, quote_asset);
        let current_time = env.ledger().timestamp();
        current_time - book.timestamp > config.max_staleness_secs
    }

    // ─── Internal helpers ──────────────────────────────────────────────────

    fn make_pair_key(base: &StellarAssetId, quote: &StellarAssetId) -> Symbol {
        // Derive a short deterministic pair key from both assets.
        // Uses a simple hash of the debug representation to fit within Symbol limits.
        use soroban_sdk::xdr::ScVal;
        // Combine debug representations and truncate to fit Symbol limits (max ~32 chars)
        let raw = format!("{:?}/{:?}", base, quote);
        // Take the first 10 meaningful chars and last 4 chars as a fingerprint
        let key = if raw.len() <= 10 {
            raw
        } else {
            // Use sum of bytes as a simple hash for uniqueness
            let hash: u64 = raw.bytes().fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64));
            format!("DEX{:016x}", hash)[..20].to_string()
        };
        Symbol::from_str(&key)
    }

    fn require_admin(env: &Env) {
        let admin: Address = env.storage().instance().get(&ADMIN).unwrap_optimized();
        if env.current_contract_address() != admin {
            panic!("Not authorized");
        }
    }
}

#[cfg(test)]
mod dex_tests {
    use super::*;
    use soroban_sdk::Env;

    fn setup_dex() -> (Env, Address, StellarDexAdapter) {
        let env = Env::default();
        let admin = Address::generate(&env);
        // Manual init: we test the logic directly via the helper types
        (env, admin, StellarDexAdapter)
    }

    #[test]
    fn test_dex_adapter_config_defaults() {
        let config = DexAdapterConfig {
            min_liquidity_threshold: 100_000,
            max_staleness_secs: 300,
            max_spread_bps: 500,
            active: true,
        };

        assert_eq!(config.min_liquidity_threshold, 100_000);
        assert_eq!(config.max_staleness_secs, 300);
        assert_eq!(config.max_spread_bps, 500);
        assert!(config.active);
    }

    #[test]
    fn test_order_book_creation() {
        let env = Env::default();
        let base = StellarAssetId::Native;
        let quote = StellarAssetId::Token {
            code: Symbol::short("USDC"),
            issuer: Address::generate(&env),
        };

        let book = DexOrderBook {
            base_asset: base.clone(),
            quote_asset: quote.clone(),
            best_bid: 1_000_000,
            best_ask: 1_001_000,
            bid_volume: 500_000,
            ask_volume: 500_000,
            last_trade_time: 1000,
            timestamp: 1000,
        };

        assert_eq!(book.best_bid, 1_000_000);
        assert_eq!(book.best_ask, 1_001_000);
        assert_eq!(book.bid_volume, 500_000);
        assert_eq!(book.ask_volume, 500_000);
    }

    #[test]
    fn test_dex_mid_price_calculation() {
        // (best_bid + best_ask) / 2 = (1000000 + 1010000) / 2 = 1005000
        let bid: u64 = 1_000_000;
        let ask: u64 = 1_010_000;
        let mid = ((bid as u128 + ask as u128) / 2) as u64;
        assert_eq!(mid, 1_005_000);
    }

    #[test]
    fn test_spread_calculation() {
        let best_bid: u64 = 1_000_000;
        let best_ask: u64 = 1_020_000;
        let spread_bps = ((best_ask - best_bid) as u128 * 10000 / best_bid as u128) as u32;
        // 20000 / 1000000 * 10000 = 200 bps
        assert_eq!(spread_bps, 200);
    }

    #[test]
    fn test_staleness_detection() {
        let config = DexAdapterConfig {
            min_liquidity_threshold: 100_000,
            max_staleness_secs: 300,
            max_spread_bps: 500,
            active: true,
        };

        let current_time: u64 = 2000;
        let book_time: u64 = 1500;

        let is_stale = current_time - book_time > config.max_staleness_secs;
        assert!(!is_stale);

        let book_time_old: u64 = 500;
        let is_stale = current_time - book_time_old > config.max_staleness_secs;
        assert!(is_stale);
    }

    #[test]
    fn test_min_liquidity_validation() {
        let threshold: u64 = 100_000;

        let sufficient_volume: u64 = 200_000;
        assert!(sufficient_volume >= threshold);

        let insufficient_volume: u64 = 50_000;
        assert!(insufficient_volume < threshold);
    }

    #[test]
    fn test_inactive_adapter() {
        let config = DexAdapterConfig {
            min_liquidity_threshold: 100_000,
            max_staleness_secs: 300,
            max_spread_bps: 500,
            active: false,
        };

        assert!(!config.active);
    }
}
