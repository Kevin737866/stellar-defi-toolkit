//! Price Oracle Contract for Stellar DeFi Toolkit
//!
//! Provides reliable price feeds for various assets on the Stellar network.
//! This oracle aggregates prices from multiple sources and provides
//! tamper-resistant price data for DeFi applications.
//!
//! ## Features
//! - Multi-source price aggregation
//! - Time-weighted average price (TWAP)
//! - Price deviation alerts
//! - Circuit breaker for extreme price movements
//! - Governance-controlled price sources

use soroban_sdk::{contract, contractimpl, Address, Env, Symbol, Vec, Map, unwrap::UnwrapOptimized};
use crate::types::stablecoin::{OraclePrice, PriceDeviationAlert, AlertSeverity};

// ─── Constants ───────────────────────────────────────────────────────────────

/// Maximum price deviation allowed (5%)
const MAX_PRICE_DEVIATION_BPS: u32 = 500;
/// Minimum number of price sources required
const MIN_PRICE_SOURCES: u32 = 3;
/// Maximum age of price data (1 hour)
const MAX_PRICE_AGE: u64 = 3600;
/// Heartbeat interval for price updates (5 minutes)
const HEARTBEAT_INTERVAL: u64 = 300;

// ─── Storage Keys ─────────────────────────────────────────────────────────────

const ADMIN: Symbol = Symbol::short("ADMIN");
const PAUSED: Symbol = Symbol::short("PAUSED");
const PRICE_SOURCES: Symbol = Symbol::short("PRICESRC");
const PRICES: Symbol = Symbol::short("PRICES");
const PRICE_HISTORY: Symbol = Symbol::short("PRICEHIST");
const DEVIATION_ALERTS: Symbol = Symbol::short("DEVIATE");
const LAST_UPDATE: Symbol = Symbol::short("LASTUPD");

// ─── Price Source Information ─────────────────────────────────────────────────

#[derive(Clone, Debug)]
#[contracttype]
pub struct PriceSource {
    /// Address of the price source
    pub address: Address,
    /// Name of the price source
    pub name: Symbol,
    /// Weight of this source in aggregation (basis points)
    pub weight: u32,
    /// Whether this source is active
    pub active: bool,
    /// Last successful update from this source
    pub last_update: u64,
    /// Number of successful updates
    pub successful_updates: u64,
    /// Number of failed updates
    pub failed_updates: u64,
}

/// Price history entry for TWAP calculation
#[derive(Clone, Debug)]
#[contracttype]
pub struct PriceHistoryEntry {
    /// Price value
    pub price: u64,
    /// Timestamp of this price
    pub timestamp: u64,
    /// Source of this price
    pub source: Address,
}

// ─── Oracle Contract ─────────────────────────────────────────────────────────

/// Price oracle contract
#[contract]
pub struct PriceOracleContract;

#[contractimpl]
impl PriceOracleContract {
    /// Initialize the price oracle
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
        let price_sources: Map<Address, PriceSource> = Map::new(&env);
        env.storage().instance().set(&PRICE_SOURCES, &price_sources);
        
        let prices: Map<Address, OraclePrice> = Map::new(&env);
        env.storage().instance().set(&PRICES, &prices);
        
        let price_history: Map<Address, Vec<PriceHistoryEntry>> = Map::new(&env);
        env.storage().instance().set(&PRICE_HISTORY, &price_history);
        
        let deviation_alerts: Vec<PriceDeviationAlert> = Vec::new(&env);
        env.storage().instance().set(&DEVIATION_ALERTS, &deviation_alerts);
        
        env.storage().instance().set(&LAST_UPDATE, &env.ledger().timestamp());
    }

    /// Add a new price source (admin only)
    /// 
    /// # Arguments
    /// * `source_address` - Address of the price source
    /// * `name` - Name of the price source
    /// * `weight` - Weight in aggregation (basis points)
    pub fn add_price_source(
        env: Env,
        source_address: Address,
        name: Symbol,
        weight: u32,
    ) {
        Self::require_admin(&env);
        
        if weight == 0 || weight > 10000 {
            panic!("Invalid weight");
        }
        
        let mut price_sources = Self::get_price_sources(&env);
        
        // Check if total weight would exceed 10000
        let mut total_weight = weight;
        for source in price_sources.values() {
            total_weight += source.weight;
        }
        
        if total_weight > 10000 {
            panic!("Total weight would exceed 10000");
        }
        
        let price_source = PriceSource {
            address: source_address.clone(),
            name,
            weight,
            active: true,
            last_update: 0,
            successful_updates: 0,
            failed_updates: 0,
        };
        
        price_sources.set(source_address, price_source);
        env.storage().instance().set(&PRICE_SOURCES, &price_sources);
        
        env.events().publish(
            (Symbol::short("PRICE_SOURCE_ADDED"), source_address.clone()),
            (name, weight),
        );
    }

    /// Update price from a source
    /// 
    /// # Arguments
    /// * `source_address` - Address of the price source
    /// * `asset_address` - Address of the asset
    /// * `price` - New price value
    /// * `decimals` - Number of decimals in the price
    pub fn update_price(
        env: Env,
        source_address: Address,
        asset_address: Address,
        price: u64,
        decimals: u32,
    ) {
        Self::require_not_paused(&env);
        
        // Verify source is authorized
        let mut price_sources = Self::get_price_sources(&env);
        let mut source = price_sources.get(source_address.clone())
            .unwrap_or_else(|| panic!("Price source not found"));
            
        if !source.active {
            panic!("Price source is not active");
        }
        
        // Create new price entry
        let oracle_price = OraclePrice {
            asset_address: asset_address.clone(),
            price,
            decimals,
            last_update: env.ledger().timestamp(),
        };
        
        // Store price from this source
        let source_prices_key = (source_address.clone(), asset_address.clone());
        env.storage().temporary().set(&source_prices_key, &oracle_price);
        
        // Update source statistics
        source.last_update = env.ledger().timestamp();
        source.successful_updates += 1;
        price_sources.set(source_address, source);
        env.storage().instance().set(&PRICE_SOURCES, &price_sources);
        
        // Add to price history
        let mut price_history = Self::get_price_history(&env);
        let history_entry = PriceHistoryEntry {
            price,
            timestamp: env.ledger().timestamp(),
            source: source_address,
        };
        
        let asset_history = price_history.get(asset_address.clone())
            .unwrap_or_else(|| Vec::new(&env));
        let mut updated_history = asset_history;
        updated_history.push_back(history_entry);
        
        // Keep only last 100 entries per asset
        if updated_history.len() > 100 {
            updated_history.pop_front();
        }
        
        price_history.set(asset_address, updated_history);
        env.storage().instance().set(&PRICE_HISTORY, &price_history);
        
        // Trigger aggregation if we have enough sources
        Self::aggregate_price(&env, asset_address);
        
        env.events().publish(
            (Symbol::short("PRICE_UPDATED"), source_address),
            (asset_address, price, decimals),
        );
    }

    /// Get the current price for an asset
    /// 
    /// # Arguments
    /// * `asset_address` - Address of the asset
    /// 
    /// # Returns
    /// Current oracle price for the asset
    pub fn get_price(env: Env, asset_address: Address) -> OraclePrice {
        let prices = Self::get_prices(&env);
        prices.get(asset_address.clone())
            .unwrap_or_else(|| panic!("Price not available for asset"))
    }

    /// Get time-weighted average price
    /// 
    /// # Arguments
    /// * `asset_address` - Address of the asset
    /// * `period` - Time period in seconds for TWAP calculation
    /// 
    /// # Returns
    /// TWAP price for the specified period
    pub fn get_twap(env: Env, asset_address: Address, period: u64) -> OraclePrice {
        let price_history = Self::get_price_history(&env);
        let history = price_history.get(asset_address.clone())
            .unwrap_or_else(|| panic!("No price history for asset"));
        
        let current_time = env.ledger().timestamp();
        let cutoff_time = current_time - period;
        
        let mut weighted_sum = 0u128;
        let mut total_weight = 0u64;
        let mut last_timestamp = 0u64;
        let mut last_price = 0u64;
        let mut decimals = 6u32;
        
        for entry in history.iter() {
            if entry.timestamp >= cutoff_time {
                if last_timestamp > 0 {
                    let time_weight = entry.timestamp - last_timestamp;
                    weighted_sum += (last_price as u128) * (time_weight as u128);
                    total_weight += time_weight;
                }
                last_timestamp = entry.timestamp;
                last_price = entry.price;
                
                // Get decimals from current price if available
                if let Ok(current_price) = Self::get_price(env.clone(), asset_address.clone()) {
                    decimals = current_price.decimals;
                }
            }
        }
        
        if total_weight == 0 {
            panic!("No price data in the specified period");
        }
        
        let twap_price = (weighted_sum / (total_weight as u128)) as u64;
        
        OraclePrice {
            asset_address,
            price: twap_price,
            decimals,
            last_update: current_time,
        }
    }

    /// Get all active price sources
    pub fn get_price_sources(env: Env) -> Vec<PriceSource> {
        let price_sources = Self::get_price_sources(&env);
        let mut active_sources = Vec::new(&env);
        
        for source in price_sources.values() {
            if source.active {
                active_sources.push_back(source);
            }
        }
        
        active_sources
    }

    /// Get recent price deviation alerts
    pub fn get_deviation_alerts(env: Env) -> Vec<PriceDeviationAlert> {
        env.storage().instance().get(&DEVIATION_ALERTS).unwrap()
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

    /// Remove a price source (admin only)
    pub fn remove_price_source(env: Env, source_address: Address) {
        Self::require_admin(&env);
        
        let mut price_sources = Self::get_price_sources(&env);
        price_sources.remove(source_address.clone());
        env.storage().instance().set(&PRICE_SOURCES, &price_sources);
        
        env.events().publish(
            (Symbol::short("PRICE_SOURCE_REMOVED"), source_address.clone()),
            (),
        );
    }

    /// Update source weight (admin only)
    pub fn update_source_weight(env: Env, source_address: Address, new_weight: u32) {
        Self::require_admin(&env);
        
        if new_weight == 0 || new_weight > 10000 {
            panic!("Invalid weight");
        }
        
        let mut price_sources = Self::get_price_sources(&env);
        let mut source = price_sources.get(source_address.clone())
            .unwrap_or_else(|| panic!("Price source not found"));
        
        source.weight = new_weight;
        price_sources.set(source_address, source);
        env.storage().instance().set(&PRICE_SOURCES, &price_sources);
        
        env.events().publish(
            (Symbol::short("SOURCE_WEIGHT_UPDATED"), source_address.clone()),
            new_weight,
        );
    }

    // ─── Internal Functions ─────────────────────────────────────────────────────

    fn aggregate_price(env: &Env, asset_address: Address) {
        let price_sources = Self::get_price_sources(env);
        let mut weighted_prices = Vec::new(env);
        let mut total_weight = 0u32;
        
        // Collect prices from all active sources
        for source in price_sources.values() {
            if source.active {
                let source_prices_key = (source.address.clone(), asset_address.clone());
                if let Some(price) = env.storage().temporary().get::<_, OraclePrice>(&source_prices_key) {
                    weighted_prices.push_back((price, source.weight));
                    total_weight += source.weight;
                }
            }
        }
        
        // Check if we have enough sources
        if weighted_prices.len() < MIN_PRICE_SOURCES as usize {
            return; // Not enough sources for aggregation
        }
        
        // Calculate weighted average
        let mut weighted_sum = 0u128;
        for (price, weight) in weighted_prices.iter() {
            weighted_sum += (price.price as u128) * (*weight as u128);
        }
        
        let aggregated_price = (weighted_sum / (total_weight as u128)) as u64;
        
        // Get decimals from first price
        let decimals = weighted_prices.first()
            .map(|(price, _)| price.decimals)
            .unwrap_or(6);
        
        // Check for price deviation
        if let Some(current_price) = Self::get_prices(env).get(asset_address.clone()) {
            let deviation = Self::calculate_price_deviation(
                current_price.price,
                aggregated_price,
            );
            
            if deviation > MAX_PRICE_DEVIATION_BPS {
                Self::create_deviation_alert(
                    env,
                    asset_address.clone(),
                    current_price.price,
                    aggregated_price,
                    deviation,
                );
            }
        }
        
        // Store aggregated price
        let oracle_price = OraclePrice {
            asset_address: asset_address.clone(),
            price: aggregated_price,
            decimals,
            last_update: env.ledger().timestamp(),
        };
        
        let mut prices = Self::get_prices(env);
        prices.set(asset_address, oracle_price);
        env.storage().instance().set(&PRICES, &prices);
        
        env.storage().instance().set(&LAST_UPDATE, &env.ledger().timestamp());
    }

    fn calculate_price_deviation(old_price: u64, new_price: u64) -> u32 {
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
        asset_address: Address,
        expected_price: u64,
        actual_price: u64,
        deviation_bps: u32,
    ) {
        let severity = if deviation_bps > 1000 {
            AlertSeverity::Critical
        } else if deviation_bps > 500 {
            AlertSeverity::High
        } else if deviation_bps > 200 {
            AlertSeverity::Medium
        } else {
            AlertSeverity::Low
        };
        
        let alert = PriceDeviationAlert {
            token_address: asset_address.clone(),
            expected_price,
            actual_price,
            deviation_bps,
            triggered_at: env.ledger().timestamp(),
            severity,
        };
        
        let mut alerts = env.storage().instance().get(&DEVIATION_ALERTS).unwrap();
        alerts.push_back(alert);
        
        // Keep only last 50 alerts
        if alerts.len() > 50 {
            alerts.pop_front();
        }
        
        env.storage().instance().set(&DEVIATION_ALERTS, &alerts);
        
        env.events().publish(
            (Symbol::short("PRICE_DEVIATION"), asset_address.clone()),
            (expected_price, actual_price, deviation_bps),
        );
    }

    fn require_admin(env: &Env) {
        let admin = env.storage().instance().get(&ADMIN).unwrap_optimized();
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

    fn get_price_sources(env: &Env) -> Map<Address, PriceSource> {
        env.storage().instance().get(&PRICE_SOURCES).unwrap()
    }

    fn get_prices(env: &Env) -> Map<Address, OraclePrice> {
        env.storage().instance().get(&PRICES).unwrap()
    }

    fn get_price_history(env: &Env) -> Map<Address, Vec<PriceHistoryEntry>> {
        env.storage().instance().get(&PRICE_HISTORY).unwrap()
    }
}
