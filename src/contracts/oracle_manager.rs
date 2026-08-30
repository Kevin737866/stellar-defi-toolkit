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
//!
//! ## Access Control
//! - **Admin**: `register_oracle`, `deactivate_oracle`, `update_oracle_weight`,
//!   `update_aggregation_params` — gated by a broken `require_admin()` (compares the
//!   contract's own address, not the caller). See `docs/ACCESS_CONTROL_MATRIX.md`.
//! - **Keeper**: `submit_price` — intended for a registered oracle, but has no
//!   `require_auth()` on the `oracle_address` parameter.
//! - **User**: read-only (aggregated price/oracle info/alert lookups).
use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol, Vec};
use crate::contracts::price_feed_adapters::OracleAdapter;

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol, Vec};
use crate::contracts::decentralized_oracle::OracleNode;

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol};
use crate::contracts::oracle::{PriceData, check_staleness};

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol, Vec};

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
const HEARTBEAT_CONFIGS: Symbol = Symbol::short("HB_CONF");
const HEARTBEAT_STATUS: Symbol = Symbol::short("HB_STAT");
const HEARTBEAT_ALERTS: Symbol = Symbol::short("HB_ALERT");

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

/// Heartbeat configuration per oracle node
#[derive(Clone, Debug)]
#[contracttype]
pub struct HeartbeatConfig {
    /// Oracle address
    pub oracle_address: Address,
    /// Expected heartbeat interval in seconds
    pub interval_seconds: u64,
    /// Maximum tolerated missed heartbeats before alert
    pub max_missed: u32,
    /// Whether heartbeat monitoring is enabled for this oracle
    pub enabled: bool,
}

/// Heartbeat status tracked per oracle node
#[derive(Clone, Debug)]
#[contracttype]
pub struct HeartbeatStatus {
    /// Oracle address
    pub oracle_address: Address,
    /// Timestamp of last successful heartbeat
    pub last_heartbeat: u64,
    /// Number of consecutive missed heartbeats
    pub missed_count: u32,
    /// Total missed heartbeats over lifetime
    pub total_missed: u32,
    /// Whether the oracle is currently flagged for missed heartbeats
    pub flagged: bool,
}

/// Alert emitted when an oracle misses heartbeats
#[derive(Clone, Debug)]
#[contracttype]
pub struct HeartbeatAlert {
    /// Oracle address that missed the heartbeat
    pub oracle_address: Address,
    /// Number of consecutive misses at time of alert
    pub consecutive_misses: u32,
    /// Alert timestamp
    pub timestamp: u64,
    /// Alert severity
    pub severity: AlertSeverity,
    /// Whether reputation was auto-downgraded
    pub reputation_downgraded: bool,
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

    // ─── Heartbeat Monitoring ─────────────────────────────────────────────────

    /// Configure heartbeat monitoring for an oracle node
    ///
    /// # Arguments
    /// * `oracle_address` - Oracle to configure heartbeat for
    /// * `interval_seconds` - Expected heartbeat interval
    /// * `max_missed` - Maximum tolerated misses before alert
    pub fn set_heartbeat_config(
        env: Env,
        oracle_address: Address,
        interval_seconds: u64,
        max_missed: u32,
    ) {
        Self::require_admin(&env);

        if interval_seconds == 0 {
            panic!("Heartbeat interval must be positive");
        }
n        let config = HeartbeatConfig {
            oracle_address: oracle_address.clone(),
            interval_seconds,
            max_missed,
            enabled: true,
        };

        let mut configs: Map<Address, HeartbeatConfig> = env
            .storage()
            .instance()
            .get(&HEARTBEAT_CONFIGS)
            .unwrap_or_else(|| Map::new(&env));
        configs.set(oracle_address.clone(), config);
        env.storage().instance().set(&HEARTBEAT_CONFIGS, &configs);

        // Initialize heartbeat status if not present
        let mut statuses: Map<Address, HeartbeatStatus> = env
            .storage()
            .instance()
            .get(&HEARTBEAT_STATUS)
            .unwrap_or_else(|| Map::new(&env));
        if !statuses.contains_key(&oracle_address) {
            statuses.set(
                oracle_address.clone(),
                HeartbeatStatus {
                    oracle_address: oracle_address.clone(),
                    last_heartbeat: env.ledger().timestamp(),
                    missed_count: 0,
                    total_missed: 0,
                    flagged: false,
                },
            );
            env.storage().instance().set(&HEARTBEAT_STATUS, &statuses);
        }

        env.events().publish(
            Symbol::short("HB_CONFIG_SET"),
            (oracle_address, interval_seconds, max_missed),
        );
    }

    /// Record a heartbeat from an oracle (typically called when it submits a price)
    pub fn record_heartbeat(env: Env, oracle_address: Address) {
        let mut statuses: Map<Address, HeartbeatStatus> = env
            .storage()
            .instance()
            .get(&HEARTBEAT_STATUS)
            .unwrap_or_else(|| Map::new(&env));

        let mut status = statuses.get(oracle_address.clone())
            .unwrap_or(HeartbeatStatus {
                oracle_address: oracle_address.clone(),
                last_heartbeat: 0,
                missed_count: 0,
                total_missed: 0,
                flagged: false,
            });

        let was_flagged = status.flagged;
        status.last_heartbeat = env.ledger().timestamp();
        status.missed_count = 0;
        status.flagged = false;
        statuses.set(oracle_address.clone(), status);
        env.storage().instance().set(&HEARTBEAT_STATUS, &statuses);

        if was_flagged {
            env.events().publish(
                Symbol::short("HB_RECOVERED"),
                oracle_address,
            );
        }
    }

    /// Check heartbeats for all configured oracles and emit alerts
    /// for any that have missed their expected intervals.
    ///
    /// This should be called periodically (e.g., by a keeper bot).
    pub fn check_heartbeats(env: Env) -> Vec<HeartbeatAlert> {
        let configs: Map<Address, HeartbeatConfig> = env
            .storage()
            .instance()
            .get(&HEARTBEAT_CONFIGS)
            .unwrap_or_else(|| Map::new(&env));
        let mut statuses: Map<Address, HeartbeatStatus> = env
            .storage()
            .instance()
            .get(&HEARTBEAT_STATUS)
            .unwrap_or_else(|| Map::new(&env));
        let mut oracles = Self::get_oracles(&env);

        let current_time = env.ledger().timestamp();
        let mut alerts = Vec::new(&env);
        let mut rep_updates: Vec<(Address, u32)> = Vec::new(&env);
n        for config in configs.values() {
            if !config.enabled {
                continue;
            }
n            let mut status = statuses.get(config.oracle_address.clone())
                .unwrap_or(HeartbeatStatus {
                    oracle_address: config.oracle_address.clone(),
                    last_heartbeat: 0,
                    missed_count: 0,
                    total_missed: 0,
                    flagged: false,
                });

            let elapsed = current_time.saturating_sub(status.last_heartbeat);
            let expected_misses = (elapsed / config.interval_seconds) as u32;
            if expected_misses > 0 {
                let new_missed = expected_misses.saturating_sub(status.missed_count);
                status.missed_count = expected_misses;
                status.total_missed = status.total_missed.saturating_add(new_missed);

                // Determine severity based on consecutive misses
                let severity = if status.missed_count >= config.max_missed * 2 {
                    AlertSeverity::Critical
                } else if status.missed_count >= config.max_missed {
                    AlertSeverity::High
                } else if status.missed_count >= config.max_missed / 2 {
                    AlertSeverity::Medium
                } else {
                    AlertSeverity::Low
                };

                // Auto-downgrade reputation on consecutive misses
                let mut reputation_downgraded = false;
                let mut oracle_info = oracles.get(config.oracle_address.clone());
                if let Some(ref mut info) = oracle_info {
                    if status.missed_count >= config.max_missed {
                        let downgrade = 200u32 * (status.missed_count / config.max_missed).min(10);
                        info.reputation = info.reputation.saturating_sub(downgrade);
                        reputation_downgraded = true;
                        rep_updates.push((config.oracle_address.clone(), info.reputation));
                    }
                }
                if let Some(info) = oracle_info {
                    oracles.set(config.oracle_address.clone(), info);
                }

                status.flagged = status.missed_count >= config.max_missed;

                let alert = HeartbeatAlert {
                    oracle_address: config.oracle_address.clone(),
                    consecutive_misses: status.missed_count,
                    timestamp: current_time,
                    severity,
                    reputation_downgraded,
                };
                alerts.push_back(alert);
n                env.events().publish(
                    Symbol::short("HB_MISSED"),
                    (
                        config.oracle_address.clone(),
                        status.missed_count,
                        reputation_downgraded,
                    ),
                );
            }

            statuses.set(config.oracle_address.clone(), status);
        }

        env.storage().instance().set(&HEARTBEAT_STATUS, &statuses);
        env.storage().instance().set(&ORACLES, &oracles);

        alerts
    }

    /// Get heartbeat status for a specific oracle
    pub fn get_heartbeat_status(env: Env, oracle_address: Address) -> Option<HeartbeatStatus> {
        let statuses: Map<Address, HeartbeatStatus> = env
            .storage()
            .instance()
            .get(&HEARTBEAT_STATUS)
            .unwrap_or_else(|| Map::new(&env));
        statuses.get(oracle_address)
    }

    /// Get heartbeat configuration for a specific oracle
    pub fn get_heartbeat_config(env: Env, oracle_address: Address) -> Option<HeartbeatConfig> {
        let configs: Map<Address, HeartbeatConfig> = env
            .storage()
            .instance()
            .get(&HEARTBEAT_CONFIGS)
            .unwrap_or_else(|| Map::new(&env));
        configs.get(oracle_address)
    }

    /// Get all oracle heartbeat statuses
    pub fn get_all_heartbeat_statuses(env: Env) -> Vec<HeartbeatStatus> {
        let statuses: Map<Address, HeartbeatStatus> = env
            .storage()
            .instance()
            .get(&HEARTBEAT_STATUS)
            .unwrap_or_else(|| Map::new(&env));
        let mut result = Vec::new(&env);
        for status in statuses.values() {
            result.push_back(status);
        }
        result
    }

    /// Disable heartbeat monitoring for an oracle (admin only)
    pub fn disable_heartbeat(env: Env, oracle_address: Address) {
        Self::require_admin(&env);
        let mut configs: Map<Address, HeartbeatConfig> = env
            .storage()
            .instance()
            .get(&HEARTBEAT_CONFIGS)
            .unwrap_or_else(|| Map::new(&env));
        if let Some(mut config) = configs.get(oracle_address.clone()) {
            config.enabled = false;
            configs.set(oracle_address, config);
            env.storage().instance().set(&HEARTBEAT_CONFIGS, &configs);
        }
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

// ─── Pure-Rust Failover Coordination + Health Integration ────────────────────
//
// Compiled only for native targets (tests, off-chain tooling).
// Re-exports and extends the failover / health primitives from
// `price_feed_adapters::failover` with oracle-manager–specific coordination:
//
//   • OracleFailoverCoordinator – wraps FailoverEngine with oracle-aware
//     consensus logic (deviation gating, minimum-oracle quorum, event routing)
//   • ConsensusResult           – structured result of a coordinated price fetch
//   • CoordinatorConfig         – tunable parameters

#[cfg(not(target_family = "wasm"))]
pub mod oracle_failover {
    use crate::contracts::price_feed_adapters::failover::{
        AdapterFetchResult, FailoverEngine, FailoverEvent, FailoverReason,
        HealthMonitor, DEMOTION_THRESHOLD_BPS, MAX_ACCEPTABLE_DEVIATION_BPS,
    };

    // ── Configuration ─────────────────────────────────────────────────────────

    /// Tunable parameters for the coordinator.
    #[derive(Clone, Debug)]
    pub struct CoordinatorConfig {
        /// Minimum number of oracle responses required to form a consensus.
        pub min_quorum: usize,
        /// Maximum allowed deviation (bps) from the median before a response
        /// is excluded from consensus.
        pub max_deviation_bps: u32,
        /// If `true`, exclude unhealthy oracles even if quorum would be lost.
        pub strict_health_gate: bool,
        /// Milliseconds before an oracle attempt is considered timed-out.
        pub timeout_ms: u64,
    }

    impl Default for CoordinatorConfig {
        fn default() -> Self {
            Self {
                min_quorum: 2,
                max_deviation_bps: MAX_ACCEPTABLE_DEVIATION_BPS,
                strict_health_gate: false,
                timeout_ms: 3_000,
            }
        }
    }

    // ── Result types ──────────────────────────────────────────────────────────

    /// Outcome of one oracle's contribution to a consensus round.
    #[derive(Clone, Debug)]
    pub struct OracleContribution {
        pub oracle_id: String,
        /// Price returned by this oracle (`None` if it failed or was excluded).
        pub price: Option<u64>,
        /// Whether it was included in the final consensus calculation.
        pub included: bool,
        /// Deviation from the consensus median (bps); 0 when not included.
        pub deviation_bps: u32,
        /// Round-trip latency (ms).
        pub latency_ms: u64,
    }

    /// The aggregated result of a coordinated multi-oracle price fetch.
    #[derive(Clone, Debug)]
    pub struct ConsensusResult {
        /// Asset that was priced.
        pub asset_id: String,
        /// Median consensus price (`None` if quorum was not met).
        pub consensus_price: Option<u64>,
        /// Individual oracle contributions.
        pub contributions: Vec<OracleContribution>,
        /// How many oracles contributed valid prices.
        pub quorum_reached: bool,
        /// Number of oracles that responded successfully.
        pub responses: usize,
        /// Number of oracles that were excluded (deviation too high).
        pub excluded: usize,
        /// Unix timestamp of this consensus round.
        pub timestamp: u64,
    }

    /// Error returned when coordination fails entirely.
    #[derive(Clone, Debug)]
    pub enum CoordinatorError {
        /// Fewer than `min_quorum` oracles responded successfully.
        QuorumNotMet { responses: usize, required: usize },
        /// No oracles are registered.
        NoOracles,
    }

    impl std::fmt::Display for CoordinatorError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                CoordinatorError::QuorumNotMet { responses, required } => write!(
                    f,
                    "quorum not met: got {} responses, need {}",
                    responses, required
                ),
                CoordinatorError::NoOracles => write!(f, "no oracles registered"),
            }
        }
    }

    impl std::error::Error for CoordinatorError {}

    // ── Coordinator ───────────────────────────────────────────────────────────

    /// Coordinates multi-oracle price fetching with failover and health gating.
    ///
    /// Unlike `FailoverEngine` (which stops at the first success), the
    /// coordinator collects responses from **all** healthy oracles in priority
    /// order and derives a median consensus price, excluding outliers.
    pub struct OracleFailoverCoordinator {
        pub engine: FailoverEngine,
        config: CoordinatorConfig,
        /// Accumulated coordination events (separate from engine events).
        events: Vec<CoordinationEvent>,
    }

    /// High-level coordination event for monitoring.
    #[derive(Clone, Debug)]
    pub struct CoordinationEvent {
        pub asset_id: String,
        pub event_type: CoordinationEventType,
        pub timestamp: u64,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum CoordinationEventType {
        ConsensusReached,
        QuorumFailed,
        OracleExcluded { oracle_id: String, deviation_bps: u32 },
        OracleFailedOver { from: String, to: String },
        ManualOverrideApplied { oracle_id: String },
    }

    impl OracleFailoverCoordinator {
        pub fn new() -> Self {
            Self::with_config(CoordinatorConfig::default())
        }

        pub fn with_config(config: CoordinatorConfig) -> Self {
            Self {
                engine: FailoverEngine::new(),
                config,
                events: Vec::new(),
            }
        }

        /// Register an oracle with the given priority (lower = higher priority).
        pub fn register_oracle(&mut self, oracle_id: &str, priority: u32) {
            self.engine.register(oracle_id, priority);
        }

        /// Enable / disable an oracle in the failover chain.
        pub fn set_oracle_enabled(&mut self, oracle_id: &str, enabled: bool) {
            self.engine.set_enabled(oracle_id, enabled);
        }

        /// Set a manual override — only this oracle will be used until cleared.
        pub fn set_manual_override(&mut self, oracle_id: Option<&str>) {
            self.engine.set_manual_override(oracle_id);
            if let Some(id) = oracle_id {
                self.events.push(CoordinationEvent {
                    asset_id: String::new(),
                    event_type: CoordinationEventType::ManualOverrideApplied {
                        oracle_id: id.to_string(),
                    },
                    timestamp: unix_now(),
                });
            }
        }

        /// Run a full consensus round for `asset_id`.
        ///
        /// `fetch` is called once per healthy oracle in priority order.
        /// The closure receives `(oracle_id, asset_id)` and returns
        /// `(Ok(price) | Err(msg), latency_ms)`.
        ///
        /// Returns `ConsensusResult` on success or `CoordinatorError` if
        /// quorum cannot be met.
        pub fn fetch_consensus<F>(
            &mut self,
            asset_id: &str,
            now_ts: u64,
            mut fetch: F,
        ) -> Result<ConsensusResult, CoordinatorError>
        where
            F: FnMut(&str, &str) -> (AdapterFetchResult, u64),
        {
            let oracle_ids: Vec<(String, u32, bool)> = self
                .engine
                .adapters()
                .iter()
                .map(|a| (a.adapter_id.clone(), a.priority, a.enabled))
                .collect();

            if oracle_ids.is_empty() {
                return Err(CoordinatorError::NoOracles);
            }

            // Manual override — single-oracle path
            if let Some(override_id) = self.engine.manual_override().map(str::to_string) {
                let (result, latency) = fetch(&override_id, asset_id);
                let success = result.is_ok();
                self.engine.health.record(&override_id, latency, success, 0, now_ts);
                let price = result.ok();
                let contributions = vec![OracleContribution {
                    oracle_id: override_id.clone(),
                    price,
                    included: price.is_some(),
                    deviation_bps: 0,
                    latency_ms: latency,
                }];
                let quorum = price.is_some();
                let ev_type = if quorum {
                    CoordinationEventType::ConsensusReached
                } else {
                    CoordinationEventType::QuorumFailed
                };
                self.events.push(CoordinationEvent {
                    asset_id: asset_id.to_string(),
                    event_type: ev_type,
                    timestamp: now_ts,
                });
                return if quorum {
                    Ok(ConsensusResult {
                        asset_id: asset_id.to_string(),
                        consensus_price: price,
                        contributions,
                        quorum_reached: true,
                        responses: 1,
                        excluded: 0,
                        timestamp: now_ts,
                    })
                } else {
                    Err(CoordinatorError::QuorumNotMet { responses: 0, required: 1 })
                };
            }

            // Multi-oracle path: collect all responses
            let mut contributions: Vec<OracleContribution> = Vec::new();
            let mut valid_prices: Vec<u64> = Vec::new();

            for (oracle_id, _priority, enabled) in &oracle_ids {
                if !enabled {
                    contributions.push(OracleContribution {
                        oracle_id: oracle_id.clone(),
                        price: None,
                        included: false,
                        deviation_bps: 0,
                        latency_ms: 0,
                    });
                    continue;
                }

                // Health gate
                let is_healthy = self
                    .engine
                    .health
                    .get(oracle_id)
                    .map(|h| h.is_healthy)
                    .unwrap_or(true);

                if !is_healthy && self.config.strict_health_gate {
                    contributions.push(OracleContribution {
                        oracle_id: oracle_id.clone(),
                        price: None,
                        included: false,
                        deviation_bps: 0,
                        latency_ms: 0,
                    });
                    continue;
                }

                let (result, latency) = fetch(oracle_id, asset_id);
                let success = result.is_ok();
                self.engine.health.record(oracle_id, latency, success, 0, now_ts);

                let price = result.ok();
                if let Some(p) = price {
                    valid_prices.push(p);
                }
                contributions.push(OracleContribution {
                    oracle_id: oracle_id.clone(),
                    price,
                    included: price.is_some(), // updated below after deviation check
                    deviation_bps: 0,
                    latency_ms: latency,
                });
            }

            let responses = valid_prices.len();
            if responses < self.config.min_quorum {
                self.events.push(CoordinationEvent {
                    asset_id: asset_id.to_string(),
                    event_type: CoordinationEventType::QuorumFailed,
                    timestamp: now_ts,
                });
                return Err(CoordinatorError::QuorumNotMet {
                    responses,
                    required: self.config.min_quorum,
                });
            }

            // Compute median of valid prices
            let mut sorted = valid_prices.clone();
            sorted.sort_unstable();
            let median = sorted[sorted.len() / 2];

            // Deviation filtering: exclude outliers
            let mut included_prices: Vec<u64> = Vec::new();
            let mut excluded = 0usize;

            for contrib in contributions.iter_mut() {
                if let Some(p) = contrib.price {
                    let dev = abs_deviation_bps(p, median);
                    contrib.deviation_bps = dev;
                    if dev <= self.config.max_deviation_bps {
                        contrib.included = true;
                        included_prices.push(p);
                    } else {
                        contrib.included = false;
                        excluded += 1;
                        self.events.push(CoordinationEvent {
                            asset_id: asset_id.to_string(),
                            event_type: CoordinationEventType::OracleExcluded {
                                oracle_id: contrib.oracle_id.clone(),
                                deviation_bps: dev,
                            },
                            timestamp: now_ts,
                        });
                        // Update health with bad deviation
                        self.engine.health.record(
                            &contrib.oracle_id,
                            contrib.latency_ms,
                            true,
                            dev,
                            now_ts,
                        );
                    }
                }
            }

            // Final consensus = median of included prices
            let consensus_price = if included_prices.is_empty() {
                None
            } else {
                let mut sp = included_prices.clone();
                sp.sort_unstable();
                Some(sp[sp.len() / 2])
            };

            let quorum_reached =
                included_prices.len() >= self.config.min_quorum;

            self.events.push(CoordinationEvent {
                asset_id: asset_id.to_string(),
                event_type: if quorum_reached {
                    CoordinationEventType::ConsensusReached
                } else {
                    CoordinationEventType::QuorumFailed
                },
                timestamp: now_ts,
            });

            if quorum_reached {
                Ok(ConsensusResult {
                    asset_id: asset_id.to_string(),
                    consensus_price,
                    contributions,
                    quorum_reached: true,
                    responses,
                    excluded,
                    timestamp: now_ts,
                })
            } else {
                Err(CoordinatorError::QuorumNotMet {
                    responses: included_prices.len(),
                    required: self.config.min_quorum,
                })
            }
        }

        /// Drain accumulated coordination events.
        pub fn drain_events(&mut self) -> Vec<CoordinationEvent> {
            let engine_evs = self.engine.drain_events();
            // Convert engine events into coordination events for unified log
            let mut coord_evs: Vec<CoordinationEvent> = engine_evs
                .into_iter()
                .map(|ev| CoordinationEvent {
                    asset_id: ev.asset_id,
                    event_type: if ev.succeeded {
                        CoordinationEventType::ConsensusReached
                    } else {
                        CoordinationEventType::QuorumFailed
                    },
                    timestamp: ev.timestamp,
                })
                .collect();
            coord_evs.extend(std::mem::take(&mut self.events));
            coord_evs
        }

        /// Read-only view of the health monitor.
        pub fn health(&self) -> &HealthMonitor {
            &self.engine.health
        }

        /// Current coordinator configuration.
        pub fn config(&self) -> &CoordinatorConfig {
            &self.config
        }
    }

    impl Default for OracleFailoverCoordinator {
        fn default() -> Self { Self::new() }
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    fn abs_deviation_bps(a: u64, b: u64) -> u32 {
        if b == 0 { return 0; }
        let diff = if a >= b { a - b } else { b - a };
        ((diff as u128 * 10_000) / b as u128) as u32
    }

    fn unix_now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
}


#[contracttype]
pub enum DataKey {
    PrimaryOracle,
    SecondaryOracle,
    StalenessThreshold,
    LastGoodPrice(Symbol),
    CircuitBreakerTripped,
}

#[contract]
pub struct OracleManagerContract;

#[contractimpl]
impl OracleManagerContract {
    pub fn get_price(env: Env, asset: Symbol, threshold: u64) -> PriceData {
        let primary_price_opt: Option<PriceData> = env.storage().persistent().get(&DataKey::PrimaryOracle);
        
        if let Some(primary) = primary_price_opt {
            if !check_staleness(&env, primary.timestamp, threshold) {
                env.storage().persistent().set(&DataKey::LastGoodPrice(asset.clone()), &primary);
                return primary;
            }
        }

        // Primary failed or stale; try secondary oracle
        let secondary_price_opt: Option<PriceData> = env.storage().persistent().get(&DataKey::SecondaryOracle);
        
        if let Some(secondary) = secondary_price_opt {
            if !check_staleness(&env, secondary.timestamp, threshold) {
                env.events().publish((Symbol::new(&env, "OracleFallbackTriggered"), asset.clone()), ());
                let degraded_price = PriceData {
                    price: secondary.price,
                    timestamp: secondary.timestamp,
                    is_degraded: true,
                };
                env.storage().persistent().set(&DataKey::LastGoodPrice(asset.clone()), &degraded_price);
                return degraded_price;
            }
        }

        // Secondary failed or stale; check for last known good price or trip circuit breaker
        let last_good_opt: Option<PriceData> = env.storage().persistent().get(&DataKey::LastGoodPrice(asset.clone()));
        if let Some(mut last_good) = last_good_opt {
            env.events().publish((Symbol::new(&env, "StalenessCircuitBreakerTriggered"), asset), ());
            last_good.is_degraded = true;
            return last_good;
        }

        panic!("All oracle sources stale and no last known good price available");
    }
}


#[contracttype]
pub enum DataKey {
    Oracle(Address),
    PauseThreshold,
}

#[contract]
pub struct OracleReputationManagerContract;

#[contractimpl]
impl OracleReputationManagerContract {
    pub fn record_missed_update(env: Env, operator: Address) {
        let key = DataKey::Oracle(operator.clone());
        let mut node: OracleNode = env.storage().persistent().get(&key).unwrap_or_else(|| panic!("Oracle not registered"));

        node.missed_updates += 1;
        node.reputation_score = node.reputation_score.saturating_sub(150);

        let pause_threshold: u32 = env.storage().instance().get(&DataKey::PauseThreshold).unwrap_or(3000);
        if node.reputation_score < pause_threshold {
            node.active = false;
            env.events().publish((Symbol::new(&env, "OraclePaused"), operator.clone()), node.reputation_score);
        }

        env.storage().persistent().set(&key, &node);
    }

    pub fn get_oracle_weight(env: Env, operator: Address) -> u32 {
        let key = DataKey::Oracle(operator);
        let node: OracleNode = env.storage().persistent().get(&key).unwrap_or_else(|| panic!("Oracle not registered"));
        if !node.active {
            return 0;
        }
        node.reputation_score
    }
}


#[contracttype]
pub enum DataKey {
    AdapterPriorityList(Symbol),
    ManualOverride(Symbol),
}

#[contract]
pub struct OracleFailoverManagerContract;

#[contractimpl]
impl OracleFailoverManagerContract {
    pub fn set_oracle_priority(env: Env, asset: Symbol, adapters: Vec<OracleAdapter>) {
        env.storage().persistent().set(&DataKey::AdapterPriorityList(asset), &adapters);
        env.events().publish((Symbol::new(&env, "PriorityListUpdated"),), ());
    }

    pub fn set_manual_override(env: Env, asset: Symbol, source_id: Address) {
        env.storage().persistent().set(&DataKey::ManualOverride(asset.clone()), &source_id);
        env.events().publish((Symbol::new(&env, "ManualOverrideSet"), asset), source_id);
    }

    pub fn get_active_oracle(env: Env, asset: Symbol) -> Address {
        if let Some(override_source) = env.storage().persistent().get::<_, Address>(&DataKey::ManualOverride(asset.clone())) {
            return override_source;
        }

        let adapters: Vec<OracleAdapter> = env
            .storage()
            .persistent()
            .get(&DataKey::AdapterPriorityList(asset.clone()))
            .unwrap_or_else(|| panic!("No oracle priority list configured for asset"));

        let mut sorted_adapters: Vec<OracleAdapter> = adapters;
        
        for adapter in sorted_adapters.iter() {
            if adapter.is_healthy {
                return adapter.source_id;
            }
        }

        panic!("All prioritized oracle adapters are unhealthy; failover failed");
    }
}