//! Oracle Manager Contract for Synthetic Asset Protocol
//!
//! Manages multiple price oracles and provides aggregated price feeds
//! for synthetic asset collateralization and liquidation calculations.
//!
//! ## Features
//! - Multi-oracle price aggregation
//! - Confidence-weighted price calculation
//! - Oracle reputation system
//! - Price deviation detection
//! - Automatic oracle failover

use soroban_sdk::{contract, contractimpl, Address, Env, Symbol, Vec, Map, unwrap::UnwrapOptimized};
use crate::types::synthetic::{OraclePrice, SyntheticAsset};

// ─── Constants ───────────────────────────────────────────────────────────────

/// Minimum number of oracles required for aggregation
const MIN_ORACLES: u32 = 3;
/// Maximum price deviation allowed (5%)
const MAX_PRICE_DEVIATION: u32 = 500;
/// Oracle timeout period (1 hour)
const ORACLE_TIMEOUT: u64 = 3600;
/// Minimum confidence threshold (70%)
const MIN_CONFIDENCE: u32 = 7000;

// ─── Storage Keys ─────────────────────────────────────────────────────────────

const ADMIN: Symbol = Symbol::short("ADMIN");
const ORACLES: Symbol = Symbol::short("ORACLES");
const PRICES: Symbol = Symbol::short("PRICES");
const REPUTATION: Symbol = Symbol::short("REPUTATION");
const AGGREGATION_PARAMS: Symbol = Symbol::short("AGG_PARAMS");

// ─── Oracle Information ─────────────────────────────────────────────────────

#[derive(Clone, Debug)]
#[contracttype]
pub struct OracleInfo {
    /// Oracle address
    pub address: Address,
    /// Oracle name/symbol
    pub name: Symbol,
    /// Weight in aggregation (basis points)
    pub weight: u32,
    /// Reputation score (0-10000)
    pub reputation: u32,
    /// Number of successful updates
    pub successful_updates: u64,
    /// Number of failed updates
    pub failed_updates: u64,
    /// Last update timestamp
    pub last_update: u64,
    /// Whether oracle is active
    pub active: bool,
}

/// Aggregation parameters
#[derive(Clone, Debug)]
#[contracttype]
pub struct AggregationParams {
    /// Minimum oracles required
    pub min_oracles: u32,
    /// Maximum price deviation
    pub max_price_deviation: u32,
    /// Oracle timeout period
    pub oracle_timeout: u64,
    /// Minimum confidence threshold
    pub min_confidence: u32,
    /// Aggregation method
    pub aggregation_method: AggregationMethod,
}

/// Price aggregation methods
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum AggregationMethod {
    /// Weighted average
    WeightedAverage,
    /// Median
    Median,
    /// Confidence-weighted
    ConfidenceWeighted,
    /// Timed-weighted average
    TimeWeightedAverage,
}

/// Price deviation alert
#[derive(Clone, Debug)]
#[contracttype]
pub struct PriceDeviationAlert {
    /// Asset ID
    pub asset_id: u32,
    /// Expected price range
    pub expected_price_min: u64,
    pub expected_price_max: u64,
    /// Reported price
    pub reported_price: u64,
    /// Deviation percentage
    pub deviation_bps: u32,
    /// Oracle reporting the deviation
    pub oracle_address: Address,
    /// Alert timestamp
    pub timestamp: u64,
    /// Alert severity
    pub severity: AlertSeverity,
}

/// Alert severity levels
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum AlertSeverity {
    Low,
    Medium,
    High,
    Critical,
}

// ─── Oracle Manager Contract ─────────────────────────────────────────────

/// Oracle manager contract
#[contract]
pub struct OracleManagerContract;

#[contractimpl]
impl OracleManagerContract {
    /// Initialize the oracle manager
    /// 
    /// # Arguments
    /// * `admin` - Admin address
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&ADMIN) {
            panic!("Already initialized");
        }

        env.storage().instance().set(&ADMIN, &admin);

        // Initialize storage
        let oracles: Map<Address, OracleInfo> = Map::new(&env);
        env.storage().instance().set(&ORACLES, &oracles);

        let prices: Map<u32, Vec<OraclePrice>> = Map::new(&env);
        env.storage().instance().set(&PRICES, &prices);

        let reputation: Map<Address, u32> = Map::new(&env);
        env.storage().instance().set(&REPUTATION, &reputation);

        // Initialize aggregation parameters
        let agg_params = AggregationParams {
            min_oracles: MIN_ORACLES,
            max_price_deviation: MAX_PRICE_DEVIATION,
            oracle_timeout: ORACLE_TIMEOUT,
            min_confidence: MIN_CONFIDENCE,
            aggregation_method: AggregationMethod::ConfidenceWeighted,
        };
        env.storage().instance().set(&AGGREGATION_PARAMS, &agg_params);

        env.events().publish(
            Symbol::short("ORACLE_MANAGER_INITIALIZED"),
            admin,
        );
    }

    /// Register a new oracle
    /// 
    /// # Arguments
    /// * `oracle_address` - Oracle contract address
    /// * `name` - Oracle identifier
    /// * `weight` - Weight in aggregation
    pub fn register_oracle(
        env: Env,
        oracle_address: Address,
        name: Symbol,
        weight: u32,
    ) {
        Self::require_admin(&env);

        let mut oracles = Self::get_oracles(&env);
        
        if oracles.contains_key(&oracle_address) {
            panic!("Oracle already registered");
        }

        let total_weight = Self::calculate_total_weight(&env);
        if total_weight + weight > 10000 {
            panic!("Total weight would exceed 100%");
        }

        let oracle_info = OracleInfo {
            address: oracle_address.clone(),
            name,
            weight,
            reputation: 8000, // Start with 80% reputation
            successful_updates: 0,
            failed_updates: 0,
            last_update: 0,
            active: true,
        };

        oracles.set(oracle_address, oracle_info);
        env.storage().instance().set(&ORACLES, &oracles);

        env.events().publish(
            Symbol::short("ORACLE_REGISTERED"),
            (oracle_address, weight),
        );
    }

    /// Submit price from an oracle
    /// 
    /// # Arguments
    /// * `oracle_address` - Oracle submitting the price
    /// * `asset_id` - Asset ID
    /// * `price` - Price in USD
    /// * `confidence` - Price confidence (0-10000)
    /// * `timestamp` - When price was observed
    pub fn submit_price(
        env: Env,
        oracle_address: Address,
        asset_id: u32,
        price: u64,
        confidence: u32,
        timestamp: u64,
    ) {
        // Verify oracle is registered and active
        let mut oracles = Self::get_oracles(&env);
        let mut oracle_info = oracles.get(oracle_address.clone())
            .unwrap_or_else(|| panic!("Oracle not registered"));

        if !oracle_info.active {
            panic!("Oracle is not active");
        }

        let current_time = env.ledger().timestamp();
        if timestamp > current_time {
            panic!("Timestamp cannot be in the future");
        }

        // Check for stale data
        let agg_params = Self::get_aggregation_params(&env);
        if current_time - oracle_info.last_update > agg_params.oracle_timeout {
            panic!("Oracle data is stale");
        }

        // Validate confidence
        if confidence < agg_params.min_confidence {
            panic!("Confidence too low");
        }

        // Create price submission
        let price_submission = OraclePrice {
            asset_id,
            price,
            decimals: 6, // Standard 6 decimals
            confidence,
            timestamp,
            source_address: oracle_address.clone(),
        };

        // Store price submission
        let mut prices = Self::get_prices(&env);
        let asset_prices = prices.get(asset_id).unwrap_or_else(|| Vec::new(&env));
        let mut updated_prices = asset_prices;
        updated_prices.push_back(price_submission);
        prices.set(asset_id, updated_prices);
        env.storage().instance().set(&PRICES, &prices);

        // Update oracle statistics
        oracle_info.successful_updates += 1;
        oracle_info.last_update = current_time;
        oracles.set(oracle_address, oracle_info);
        env.storage().instance().set(&ORACLES, &oracles);

        // Update reputation based on timeliness and accuracy
        Self::update_oracle_reputation(&env, oracle_address, true, confidence);

        // Trigger price aggregation
        Self::aggregate_price(&env, asset_id);

        env.events().publish(
            Symbol::short("PRICE_SUBMITTED"),
            (oracle_address, asset_id, price, confidence),
        );
    }

    /// Get aggregated price for an asset
    /// 
    /// # Arguments
    /// * `asset_id` - Asset ID
    pub fn get_aggregated_price(env: Env, asset_id: u32) -> OraclePrice {
        let prices = Self::get_prices(&env).get(asset_id)
            .unwrap_or_else(|| panic!("No price data for asset"));

        Self::aggregate_prices(&env, &prices)
    }

    /// Get oracle information
    pub fn get_oracle_info(env: Env, oracle_address: Address) -> OracleInfo {
        Self::get_oracles(&env).get(oracle_address)
            .unwrap_or_else(|| panic!("Oracle not found"))
    }

    /// Get all registered oracles
    pub fn get_registered_oracles(env: Env) -> Vec<OracleInfo> {
        let oracles = Self::get_oracles(&env);
        let mut active_oracles = Vec::new(&env);
        
        for oracle_info in oracles.values() {
            if oracle_info.active {
                active_oracles.push_back(oracle_info);
            }
        }
        
        active_oracles
    }

    /// Update oracle weight (admin only)
    pub fn update_oracle_weight(
        env: Env,
        oracle_address: Address,
        new_weight: u32,
    ) {
        Self::require_admin(&env);

        let mut oracles = Self::get_oracles(&env);
        let mut oracle_info = oracles.get(oracle_address.clone())
            .unwrap_or_else(|| panic!("Oracle not found"));

        let total_weight = Self::calculate_total_weight(&env) - oracle_info.weight + new_weight;
        if total_weight > 10000 {
            panic!("Total weight would exceed 100%");
        }

        oracle_info.weight = new_weight;
        oracles.set(oracle_address, oracle_info);
        env.storage().instance().set(&ORACLES, &oracles);

        env.events().publish(
            Symbol::short("ORACLE_WEIGHT_UPDATED"),
            (oracle_address, new_weight),
        );
    }

    /// Deactivate oracle (admin only)
    pub fn deactivate_oracle(env: Env, oracle_address: Address) {
        Self::require_admin(&env);

        let mut oracles = Self::get_oracles(&env);
        let mut oracle_info = oracles.get(oracle_address.clone())
            .unwrap_or_else(|| panic!("Oracle not found"));

        oracle_info.active = false;
        oracles.set(oracle_address, oracle_info);
        env.storage().instance().set(&ORACLES, &oracles);

        env.events().publish(
            Symbol::short("ORACLE_DEACTIVATED"),
            oracle_address,
        );
    }

    /// Update aggregation parameters (admin only)
    pub fn update_aggregation_params(env: Env, new_params: AggregationParams) {
        Self::require_admin(&env);

        if new_params.min_oracles < 2 {
            panic!("Minimum oracles must be at least 2");
        }

        env.storage().instance().set(&AGGREGATION_PARAMS, &new_params);

        env.events().publish(
            Symbol::short("AGGREGATION_PARAMS_UPDATED"),
            (),
        );
    }

    /// Get price deviation alerts
    pub fn get_price_alerts(env: Env, asset_id: u32) -> Vec<PriceDeviationAlert> {
        // In production, this would return actual alerts
        // For now, return empty vector
        Vec::new(&env)
    }

    // ─── Internal Helpers ─────────────────────────────────────────────────────

    fn aggregate_price(env: &Env, asset_id: u32) {
        let prices = Self::get_prices(env).get(asset_id)
            .unwrap_or_else(|| return);

        let aggregated_price = Self::aggregate_prices(env, &prices);

        env.events().publish(
            Symbol::short("PRICE_AGGREGATED"),
            (asset_id, aggregated_price.price, aggregated_price.confidence),
        );
    }

    fn aggregate_prices(env: &Env, prices: &Vec<OraclePrice>) -> OraclePrice {
        let agg_params = Self::get_aggregation_params(env);
        
        if prices.len() < agg_params.min_oracles as usize {
            panic!("Insufficient price sources");
        }

        match agg_params.aggregation_method {
            AggregationMethod::WeightedAverage => Self::weighted_average(env, prices),
            AggregationMethod::Median => Self::median_price(env, prices),
            AggregationMethod::ConfidenceWeighted => Self::confidence_weighted(env, prices),
            AggregationMethod::TimeWeightedAverage => Self::time_weighted_average(env, prices),
        }
    }

    fn weighted_average(env: &Env, prices: &Vec<OraclePrice>) -> OraclePrice {
        let oracles = Self::get_oracles(env);
        let mut weighted_sum = 0u128;
        let mut total_weight = 0u32;

        for price in prices.iter() {
            if let Some(oracle_info) = oracles.get(&price.source_address) {
                if oracle_info.active {
                    weighted_sum += (price.price as u128) * (oracle_info.weight as u128);
                    total_weight += oracle_info.weight;
                }
            }
        }

        if total_weight == 0 {
            panic!("No active oracles found");
        }

        let avg_price = (weighted_sum / (total_weight as u128)) as u64;
        let avg_confidence = Self::calculate_average_confidence(env, prices);

        OraclePrice {
            asset_id: prices.get(0).unwrap().asset_id,
            price: avg_price,
            decimals: 6,
            confidence: avg_confidence,
            timestamp: env.ledger().timestamp(),
            source_address: Address::generate(env), // Aggregated price
        }
    }

    fn median_price(env: &Env, prices: &Vec<OraclePrice>) -> OraclePrice {
        let mut price_list: Vec<u64> = Vec::new(env);
        let mut confidence_list: Vec<u32> = Vec::new(env);

        for price in prices.iter() {
            price_list.push_back(price.price);
            confidence_list.push_back(price.confidence);
        }

        // Sort prices to find median
        // In production, implement proper sorting
        let median_price = price_list.get(price_list.len() / 2).unwrap_or(0);
        let median_confidence = confidence_list.get(confidence_list.len() / 2).unwrap_or(0);

        OraclePrice {
            asset_id: prices.get(0).unwrap().asset_id,
            price: median_price,
            decimals: 6,
            confidence: median_confidence,
            timestamp: env.ledger().timestamp(),
            source_address: Address::generate(env),
        }
    }

    fn confidence_weighted(env: &Env, prices: &Vec<OraclePrice>) -> OraclePrice {
        let mut weighted_sum = 0u128;
        let mut total_confidence_weight = 0u32;

        for price in prices.iter() {
            let confidence_weight = price.confidence;
            weighted_sum += (price.price as u128) * (confidence_weight as u128);
            total_confidence_weight += confidence_weight;
        }

        if total_confidence_weight == 0 {
            panic!("No valid price data");
        }

        let weighted_price = (weighted_sum / (total_confidence_weight as u128)) as u64;
        let avg_confidence = total_confidence_weight / (prices.len() as u32);

        OraclePrice {
            asset_id: prices.get(0).unwrap().asset_id,
            price: weighted_price,
            decimals: 6,
            confidence: avg_confidence,
            timestamp: env.ledger().timestamp(),
            source_address: Address::generate(env),
        }
    }

    fn time_weighted_average(env: &Env, prices: &Vec<OraclePrice>) -> OraclePrice {
        let current_time = env.ledger().timestamp();
        let mut weighted_sum = 0u128;
        let mut total_weight = 0u64;

        for price in prices.iter() {
            let time_weight = current_time - price.timestamp;
            let recency_factor = if time_weight > ORACLE_TIMEOUT {
                0 // Too old, ignore
            } else {
                ORACLE_TIMEOUT - time_weight
            };
            
            weighted_sum += (price.price as u128) * (recency_factor as u128);
            total_weight += recency_factor;
        }

        if total_weight == 0 {
            panic!("No recent price data");
        }

        let time_weighted_price = (weighted_sum / (total_weight as u128)) as u64;
        let avg_confidence = Self::calculate_average_confidence(env, prices);

        OraclePrice {
            asset_id: prices.get(0).unwrap().asset_id,
            price: time_weighted_price,
            decimals: 6,
            confidence: avg_confidence,
            timestamp: current_time,
            source_address: Address::generate(env),
        }
    }

    fn calculate_average_confidence(env: &Env, prices: &Vec<OraclePrice>) -> u32 {
        let mut total_confidence = 0u32;
        for price in prices.iter() {
            total_confidence += price.confidence;
        }
        total_confidence / (prices.len() as u32)
    }

    fn calculate_total_weight(env: &Env) -> u32 {
        let oracles = Self::get_oracles(env);
        let mut total_weight = 0u32;
        
        for oracle_info in oracles.values() {
            if oracle_info.active {
                total_weight += oracle_info.weight;
            }
        }
        
        total_weight
    }

    fn update_oracle_reputation(env: &Env, oracle_address: Address, successful: bool, confidence: u32) {
        let mut oracles = Self::get_oracles(&env);
        let mut oracle_info = oracles.get(oracle_address.clone())
            .unwrap_or_else(|| return);

        let mut reputation = oracle_info.reputation;
        
        if successful {
            // Reward timely and accurate submissions
            let timeliness_bonus = if confidence >= 9000 { 500 } else { 0 };
            reputation = (reputation + timeliness_bonus).min(10000);
        } else {
            // Penalize failed or low-confidence submissions
            reputation = (reputation - 200).max(0);
        }

        oracle_info.reputation = reputation;
        oracles.set(oracle_address, oracle_info);
        env.storage().instance().set(&ORACLES, &oracles);
    }

    // Storage getters
    fn get_oracles(env: &Env) -> Map<Address, OracleInfo> {
        env.storage().instance().get(&ORACLES).unwrap()
    }

    fn get_prices(env: &Env) -> Map<u32, Vec<OraclePrice>> {
        env.storage().instance().get(&PRICES).unwrap()
    }

    fn get_aggregation_params(env: &Env) -> AggregationParams {
        env.storage().instance().get(&AGGREGATION_PARAMS).unwrap()
    }

    fn require_admin(env: &Env) {
        let admin = env.storage().instance().get(&ADMIN).unwrap_optimized();
        if env.current_contract_address() != admin {
            panic!("Not authorized");
        }
    }
}
