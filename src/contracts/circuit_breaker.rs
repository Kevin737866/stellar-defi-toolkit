//! Circuit Breaker Module for Price Volatility Protection
//!
//! Provides automatic circuit breaker functionality to halt operations
//! when price volatility exceeds safe thresholds. This protects the protocol
//! from extreme market conditions and potential manipulation.
//!
//! ## Features
//! - Automatic triggering on extreme price movements
//! - Configurable thresholds and cooldown periods
//! - Per-asset circuit breaker status
//! - Rate limiting on price updates
//! - Cascading protection across dependent contracts

use soroban_sdk::{contract, contractimpl, Address, Env, Symbol, Map, Vec};

// ─── Constants ───────────────────────────────────────────────────────────────

/// Circuit breaker triggers at 10% single-update deviation
const CIRCUIT_BREAKER_THRESHOLD_BPS: u32 = 1000;
/// Circuit breaker triggers after 3 consecutive deviations > 5%
const CONSECUTIVE_DEVIATION_THRESHOLD: u32 = 3;
/// Minimum deviation to count as consecutive (5%)
const MIN_CONSECUTIVE_DEVIATION_BPS: u32 = 500;
/// Cooldown period after circuit breaker trips (30 minutes)
const CIRCUIT_BREAKER_COOLDOWN: u64 = 1800;
/// Minimum time between price updates (5 minutes)
const MIN_UPDATE_INTERVAL: u64 = 300;
/// Maximum price change per update during recovery (2%)
const RECOVERY_MAX_CHANGE_BPS: u32 = 200;
/// Warning threshold for monitoring (3%)
const WARNING_THRESHOLD_BPS: u32 = 300;
/// Maximum trip history entries to keep
const MAX_TRIP_HISTORY: u32 = 100;
/// Recovery mode duration (1 hour)
const RECOVERY_MODE_DURATION: u64 = 3600;

// ─── Storage Keys ─────────────────────────────────────────────────────────────

const ADMIN: Symbol = Symbol::short("ADMIN");
const CIRCUIT_BREAKER_STATUS: Symbol = Symbol::short("CB_STATUS");
const CONSECUTIVE_DEVIATIONS: Symbol = Symbol::short("CONSEC");
const LAST_PRICE_UPDATE: Symbol = Symbol::short("LAST_UPD");
const CIRCUIT_BREAKER_CONFIG: Symbol = Symbol::short("CB_CONFIG");
const TRIP_HISTORY: Symbol = Symbol::short("TRIP_HIST");
const WARNING_ALERTS: Symbol = Symbol::short("WARNINGS");
const GLOBAL_PAUSE: Symbol = Symbol::short("GLB_PAUSE");

// ─── Data Structures ─────────────────────────────────────────────────────────

/// Circuit breaker status for an asset
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum CircuitBreakerStatus {
    /// Normal operation
    Active,
    /// Circuit breaker tripped - operations halted
    Tripped,
    /// Recovery mode - limited operations allowed
    Recovery,
}

/// Warning alert for monitoring
#[derive(Clone, Debug)]
#[contracttype]
pub struct WarningAlert {
    /// Asset address
    pub asset_address: Address,
    /// Current price
    pub current_price: u64,
    /// Previous price
    pub previous_price: u64,
    /// Deviation in basis points
    pub deviation_bps: u32,
    /// Consecutive deviation count
    pub consecutive_count: u32,
    /// Timestamp
    pub timestamp: u64,
    /// Alert level
    pub level: AlertLevel,
}

/// Alert severity levels
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum AlertLevel {
    Info,
    Warning,
    Critical,
}

/// Circuit breaker configuration
#[derive(Clone, Debug)]
#[contracttype]
pub struct CircuitBreakerConfig {
    /// Single-update deviation threshold (basis points)
    pub single_deviation_threshold: u32,
    /// Number of consecutive deviations to trigger
    pub consecutive_deviation_count: u32,
    /// Minimum deviation for consecutive count (basis points)
    pub min_consecutive_deviation: u32,
    /// Cooldown period in seconds
    pub cooldown_period: u64,
    /// Minimum interval between updates in seconds
    pub min_update_interval: u64,
    /// Maximum price change during recovery (basis points)
    pub recovery_max_change: u32,
    /// Whether circuit breaker is enabled
    pub enabled: bool,
}

/// Circuit breaker state for an asset
#[derive(Clone, Debug)]
#[contracttype]
pub struct CircuitBreakerState {
    /// Current status
    pub status: CircuitBreakerStatus,
    /// Number of consecutive deviations
    pub consecutive_deviations: u32,
    /// Timestamp when circuit breaker was tripped
    pub tripped_at: u64,
    /// Last price before trip
    pub last_safe_price: u64,
    /// Timestamp of last price update
    pub last_update: u64,
    /// Recovery mode start time
    pub recovery_started_at: u64,
    /// Number of times this asset has tripped
    pub trip_count: u32,
}

/// Circuit breaker trip event
#[derive(Clone, Debug)]
#[contracttype]
pub struct CircuitBreakerTrip {
    /// Asset address
    pub asset_address: Address,
    /// Price that triggered the breaker
    pub trigger_price: u64,
    /// Previous price
    pub previous_price: u64,
    /// Deviation in basis points
    pub deviation_bps: u32,
    /// Timestamp of trip
    pub timestamp: u64,
    /// Reason for trip
    pub reason: Symbol,
}

// ─── Circuit Breaker Contract ─────────────────────────────────────────────────

/// Circuit breaker contract for price volatility protection
#[contract]
pub struct CircuitBreakerContract;

#[contractimpl]
impl CircuitBreakerContract {
    /// Initialize the circuit breaker
    /// 
    /// # Arguments
    /// * `admin` - Admin address for governance
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&ADMIN) {
            panic!("Already initialized");
        }

        env.storage().instance().set(&ADMIN, &admin);
        env.storage().instance().set(&GLOBAL_PAUSE, &false);
        
        // Set default configuration
        let config = CircuitBreakerConfig {
            single_deviation_threshold: CIRCUIT_BREAKER_THRESHOLD_BPS,
            consecutive_deviation_count: CONSECUTIVE_DEVIATION_THRESHOLD,
            min_consecutive_deviation: MIN_CONSECUTIVE_DEVIATION_BPS,
            cooldown_period: CIRCUIT_BREAKER_COOLDOWN,
            min_update_interval: MIN_UPDATE_INTERVAL,
            recovery_max_change: RECOVERY_MAX_CHANGE_BPS,
            enabled: true,
        };
        env.storage().instance().set(&CIRCUIT_BREAKER_CONFIG, &config);
        
        // Initialize empty storage
        let cb_status: Map<Address, CircuitBreakerState> = Map::new(&env);
        env.storage().instance().set(&CIRCUIT_BREAKER_STATUS, &cb_status);
        
        let trip_history: Vec<CircuitBreakerTrip> = Vec::new(&env);
        env.storage().instance().set(&TRIP_HISTORY, &trip_history);
        
        let warning_alerts: Vec<WarningAlert> = Vec::new(&env);
        env.storage().instance().set(&WARNING_ALERTS, &warning_alerts);

        env.events().publish(
            Symbol::short("CB_INITIALIZED"),
            admin,
        );
    }

    /// Check if operations are allowed for an asset
    /// 
    /// # Arguments
    /// * `asset_address` - Address of the asset
    /// 
    /// # Returns
    /// True if operations are allowed, false if circuit breaker is tripped
    pub fn is_operational(env: Env, asset_address: Address) -> bool {
        // Check global pause first
        let global_pause: bool = env.storage().instance()
            .get(&GLOBAL_PAUSE)
            .unwrap_or(false);
        
        if global_pause {
            return false;
        }
        
        let config = Self::get_config(&env);
        if !config.enabled {
            return true;
        }

        let state = Self::get_or_create_state(&env, asset_address.clone());
        
        match state.status {
            CircuitBreakerStatus::Active => true,
            CircuitBreakerStatus::Tripped => {
                // Check if cooldown period has passed
                let current_time = env.ledger().timestamp();
                if current_time >= state.tripped_at + config.cooldown_period {
                    // Transition to recovery mode
                    Self::transition_to_recovery(&env, asset_address);
                    true
                } else {
                    false
                }
            },
            CircuitBreakerStatus::Recovery => {
                // Check if recovery period has completed
                let current_time = env.ledger().timestamp();
                if current_time >= state.recovery_started_at + RECOVERY_MODE_DURATION {
                    // Transition back to active
                    Self::transition_to_active(&env, asset_address);
                }
                true
            },
        }
    }

    /// Check and potentially trip circuit breaker on price update
    /// 
    /// # Arguments
    /// * `asset_address` - Address of the asset
    /// * `old_price` - Previous price
    /// * `new_price` - New price being set
    /// 
    /// # Returns
    /// True if price update is allowed, false if circuit breaker trips
    pub fn check_price_update(
        env: Env,
        asset_address: Address,
        old_price: u64,
        new_price: u64,
    ) -> bool {
        let config = Self::get_config(&env);
        if !config.enabled {
            return true;
        }

        let mut state = Self::get_or_create_state(&env, asset_address.clone());
        let current_time = env.ledger().timestamp();
        
        // Check rate limiting
        if state.last_update > 0 {
            let time_since_update = current_time - state.last_update;
            if time_since_update < config.min_update_interval {
                env.events().publish(
                    (Symbol::short("RATE_LIMITED"), asset_address.clone()),
                    time_since_update,
                );
                return false;
            }
        }
        
        // Calculate price deviation
        let deviation_bps = Self::calculate_deviation(old_price, new_price);
        
        // Check if in recovery mode
        if state.status == CircuitBreakerStatus::Recovery {
            if deviation_bps > config.recovery_max_change {
                env.events().publish(
                    (Symbol::short("RECOVERY_VIOLATION"), asset_address.clone()),
                    deviation_bps,
                );
                return false;
            }
        }
        
        // Check single-update threshold
        if deviation_bps >= config.single_deviation_threshold {
            Self::trip_circuit_breaker(
                &env,
                asset_address.clone(),
                old_price,
                new_price,
                deviation_bps,
                Symbol::short("SINGLE_DEV"),
            );
            return false;
        }
        
        // Check consecutive deviations
        if deviation_bps >= config.min_consecutive_deviation {
            state.consecutive_deviations += 1;
            
            // Create warning alert
            Self::create_warning_alert(
                &env,
                asset_address.clone(),
                new_price,
                old_price,
                deviation_bps,
                state.consecutive_deviations,
            );
            
            if state.consecutive_deviations >= config.consecutive_deviation_count {
                Self::trip_circuit_breaker(
                    &env,
                    asset_address.clone(),
                    old_price,
                    new_price,
                    deviation_bps,
                    Symbol::short("CONSEC_DEV"),
                );
                return false;
            }
        } else {
            // Reset consecutive counter on normal update
            state.consecutive_deviations = 0;
        }
        
        // Create info alert for moderate deviations
        if deviation_bps >= WARNING_THRESHOLD_BPS && deviation_bps < config.min_consecutive_deviation {
            Self::create_warning_alert(
                &env,
                asset_address.clone(),
                new_price,
                old_price,
                deviation_bps,
                state.consecutive_deviations,
            );
        }
        
        // Update state
        state.last_update = current_time;
        state.last_safe_price = new_price;
        Self::set_state(&env, asset_address, state);
        
        true
    }

    /// Get circuit breaker status for an asset
    /// 
    /// # Arguments
    /// * `asset_address` - Address of the asset
    /// 
    /// # Returns
    /// Current circuit breaker state
    pub fn get_status(env: Env, asset_address: Address) -> CircuitBreakerState {
        Self::get_or_create_state(&env, asset_address)
    }

    /// Manually trip circuit breaker (admin only)
    /// 
    /// # Arguments
    /// * `asset_address` - Address of the asset
    /// * `reason` - Reason for manual trip
    pub fn manual_trip(env: Env, asset_address: Address, reason: Symbol) {
        Self::require_admin(&env);
        
        let state = Self::get_or_create_state(&env, asset_address.clone());
        Self::trip_circuit_breaker(
            &env,
            asset_address,
            state.last_safe_price,
            state.last_safe_price,
            0,
            reason,
        );
    }

    /// Reset circuit breaker (admin only)
    /// 
    /// # Arguments
    /// * `asset_address` - Address of the asset
    pub fn reset(env: Env, asset_address: Address) {
        Self::require_admin(&env);
        
        let mut state = Self::get_or_create_state(&env, asset_address.clone());
        state.status = CircuitBreakerStatus::Active;
        state.consecutive_deviations = 0;
        state.tripped_at = 0;
        
        Self::set_state(&env, asset_address.clone(), state);
        
        env.events().publish(
            (Symbol::short("CB_RESET"), asset_address),
            (),
        );
    }

    /// Update circuit breaker configuration (admin only)
    /// 
    /// # Arguments
    /// * `config` - New configuration
    pub fn update_config(env: Env, config: CircuitBreakerConfig) {
        Self::require_admin(&env);
        
        env.storage().instance().set(&CIRCUIT_BREAKER_CONFIG, &config);
        
        env.events().publish(
            Symbol::short("CONFIG_UPDATED"),
            config.enabled,
        );
    }

    /// Get circuit breaker configuration
    pub fn get_config_public(env: Env) -> CircuitBreakerConfig {
        Self::get_config(&env)
    }

    /// Get trip history
    pub fn get_trip_history(env: Env) -> Vec<CircuitBreakerTrip> {
        env.storage().instance()
            .get(&TRIP_HISTORY)
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Get warning alerts for monitoring
    pub fn get_warning_alerts(env: Env) -> Vec<WarningAlert> {
        env.storage().instance()
            .get(&WARNING_ALERTS)
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Clear warning alerts (admin only)
    pub fn clear_warning_alerts(env: Env) {
        Self::require_admin(&env);
        
        let warning_alerts: Vec<WarningAlert> = Vec::new(&env);
        env.storage().instance().set(&WARNING_ALERTS, &warning_alerts);
        
        env.events().publish(
            Symbol::short("WARNINGS_CLEARED"),
            (),
        );
    }

    /// Enable or disable global pause (admin only)
    /// When globally paused, all assets are non-operational
    pub fn set_global_pause(env: Env, paused: bool) {
        Self::require_admin(&env);
        
        env.storage().instance().set(&GLOBAL_PAUSE, &paused);
        
        env.events().publish(
            Symbol::short("GLOBAL_PAUSE"),
            paused,
        );
    }

    /// Get global pause status
    pub fn is_globally_paused(env: Env) -> bool {
        env.storage().instance()
            .get(&GLOBAL_PAUSE)
            .unwrap_or(false)
    }

    /// Get statistics for an asset
    pub fn get_asset_statistics(env: Env, asset_address: Address) -> CircuitBreakerState {
        Self::get_or_create_state(&env, asset_address)
    }

    /// Get all assets with tripped circuit breakers
    pub fn get_tripped_assets(env: Env) -> Vec<Address> {
        let states: Map<Address, CircuitBreakerState> = env.storage().instance()
            .get(&CIRCUIT_BREAKER_STATUS)
            .unwrap_or_else(|| Map::new(&env));
        
        let mut tripped_assets = Vec::new(&env);
        
        for (address, state) in states.iter() {
            if state.status == CircuitBreakerStatus::Tripped {
                tripped_assets.push_back(address);
            }
        }
        
        tripped_assets
    }

    /// Get all assets in recovery mode
    pub fn get_recovery_assets(env: Env) -> Vec<Address> {
        let states: Map<Address, CircuitBreakerState> = env.storage().instance()
            .get(&CIRCUIT_BREAKER_STATUS)
            .unwrap_or_else(|| Map::new(&env));
        
        let mut recovery_assets = Vec::new(&env);
        
        for (address, state) in states.iter() {
            if state.status == CircuitBreakerStatus::Recovery {
                recovery_assets.push_back(address);
            }
        }
        
        recovery_assets
    }

    /// Force transition to recovery mode (admin only)
    /// Useful for manual intervention after investigation
    pub fn force_recovery(env: Env, asset_address: Address) {
        Self::require_admin(&env);
        
        Self::transition_to_recovery(&env, asset_address);
    }

    /// Get health score for an asset (0-100)
    /// Based on trip history, consecutive deviations, and time since last trip
    pub fn get_health_score(env: Env, asset_address: Address) -> u32 {
        let state = Self::get_or_create_state(&env, asset_address.clone());
        let current_time = env.ledger().timestamp();
        
        let mut score = 100u32;
        
        // Deduct points for consecutive deviations
        score = score.saturating_sub(state.consecutive_deviations * 10);
        
        // Deduct points for trip count
        score = score.saturating_sub(state.trip_count.min(5) * 15);
        
        // Deduct points if currently tripped
        if state.status == CircuitBreakerStatus::Tripped {
            score = score.saturating_sub(50);
        }
        
        // Deduct points if in recovery
        if state.status == CircuitBreakerStatus::Recovery {
            score = score.saturating_sub(20);
        }
        
        // Add points for time since last trip (recovery over time)
        if state.tripped_at > 0 {
            let time_since_trip = current_time.saturating_sub(state.tripped_at);
            let recovery_bonus = (time_since_trip / 3600).min(10) as u32; // Max 10 points for 10+ hours
            score = (score + recovery_bonus).min(100);
        }
        
        score
    }

    // ─── Internal Functions ─────────────────────────────────────────────────────

    fn trip_circuit_breaker(
        env: &Env,
        asset_address: Address,
        old_price: u64,
        new_price: u64,
        deviation_bps: u32,
        reason: Symbol,
    ) {
        let mut state = Self::get_or_create_state(env, asset_address.clone());
        state.status = CircuitBreakerStatus::Tripped;
        state.tripped_at = env.ledger().timestamp();
        state.consecutive_deviations = 0;
        state.trip_count += 1;
        
        Self::set_state(env, asset_address.clone(), state);
        
        // Record trip event
        let trip = CircuitBreakerTrip {
            asset_address: asset_address.clone(),
            trigger_price: new_price,
            previous_price: old_price,
            deviation_bps,
            timestamp: env.ledger().timestamp(),
            reason: reason.clone(),
        };
        
        let mut history = env.storage().instance()
            .get(&TRIP_HISTORY)
            .unwrap_or_else(|| Vec::new(env));
        history.push_back(trip);
        
        // Keep only last MAX_TRIP_HISTORY trips
        while history.len() > MAX_TRIP_HISTORY as usize {
            history.pop_front();
        }
        
        env.storage().instance().set(&TRIP_HISTORY, &history);
        
        env.events().publish(
            (Symbol::short("CB_TRIPPED"), asset_address),
            (old_price, new_price, deviation_bps, reason),
        );
    }

    fn transition_to_recovery(env: &Env, asset_address: Address) {
        let mut state = Self::get_or_create_state(env, asset_address.clone());
        state.status = CircuitBreakerStatus::Recovery;
        state.consecutive_deviations = 0;
        state.recovery_started_at = env.ledger().timestamp();
        
        Self::set_state(env, asset_address.clone(), state);
        
        env.events().publish(
            (Symbol::short("CB_RECOVERY"), asset_address),
            (),
        );
    }

    fn transition_to_active(env: &Env, asset_address: Address) {
        let mut state = Self::get_or_create_state(env, asset_address.clone());
        state.status = CircuitBreakerStatus::Active;
        state.consecutive_deviations = 0;
        state.recovery_started_at = 0;
        
        Self::set_state(env, asset_address.clone(), state);
        
        env.events().publish(
            (Symbol::short("CB_ACTIVE"), asset_address),
            (),
        );
    }

    fn create_warning_alert(
        env: &Env,
        asset_address: Address,
        current_price: u64,
        previous_price: u64,
        deviation_bps: u32,
        consecutive_count: u32,
    ) {
        let level = if deviation_bps >= MIN_CONSECUTIVE_DEVIATION_BPS {
            if consecutive_count >= 2 {
                AlertLevel::Critical
            } else {
                AlertLevel::Warning
            }
        } else if deviation_bps >= WARNING_THRESHOLD_BPS {
            AlertLevel::Warning
        } else {
            AlertLevel::Info
        };
        
        let alert = WarningAlert {
            asset_address: asset_address.clone(),
            current_price,
            previous_price,
            deviation_bps,
            consecutive_count,
            timestamp: env.ledger().timestamp(),
            level: level.clone(),
        };
        
        let mut alerts = env.storage().instance()
            .get(&WARNING_ALERTS)
            .unwrap_or_else(|| Vec::new(env));
        alerts.push_back(alert);
        
        // Keep only last 50 alerts
        while alerts.len() > 50 {
            alerts.pop_front();
        }
        
        env.storage().instance().set(&WARNING_ALERTS, &alerts);
        
        env.events().publish(
            (Symbol::short("CB_WARNING"), asset_address),
            (deviation_bps, consecutive_count, level),
        );
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

    fn get_or_create_state(env: &Env, asset_address: Address) -> CircuitBreakerState {
        let mut states: Map<Address, CircuitBreakerState> = env.storage().instance()
            .get(&CIRCUIT_BREAKER_STATUS)
            .unwrap_or_else(|| Map::new(env));
        
        states.get(asset_address.clone()).unwrap_or_else(|| {
            CircuitBreakerState {
                status: CircuitBreakerStatus::Active,
                consecutive_deviations: 0,
                tripped_at: 0,
                last_safe_price: 0,
                last_update: 0,
                recovery_started_at: 0,
                trip_count: 0,
            }
        })
    }

    fn set_state(env: &Env, asset_address: Address, state: CircuitBreakerState) {
        let mut states: Map<Address, CircuitBreakerState> = env.storage().instance()
            .get(&CIRCUIT_BREAKER_STATUS)
            .unwrap_or_else(|| Map::new(env));
        
        states.set(asset_address, state);
        env.storage().instance().set(&CIRCUIT_BREAKER_STATUS, &states);
    }

    fn get_config(env: &Env) -> CircuitBreakerConfig {
        env.storage().instance()
            .get(&CIRCUIT_BREAKER_CONFIG)
            .unwrap_or_else(|| CircuitBreakerConfig {
                single_deviation_threshold: CIRCUIT_BREAKER_THRESHOLD_BPS,
                consecutive_deviation_count: CONSECUTIVE_DEVIATION_THRESHOLD,
                min_consecutive_deviation: MIN_CONSECUTIVE_DEVIATION_BPS,
                cooldown_period: CIRCUIT_BREAKER_COOLDOWN,
                min_update_interval: MIN_UPDATE_INTERVAL,
                recovery_max_change: RECOVERY_MAX_CHANGE_BPS,
                enabled: true,
            })
    }

    fn require_admin(env: &Env) {
        let admin: Address = env.storage().instance()
            .get(&ADMIN)
            .unwrap_or_else(|| panic!("Not initialized"));
        admin.require_auth();
    }
}
