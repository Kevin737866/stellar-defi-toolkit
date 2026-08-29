//! Price Oracle Implementation for Stellar DeFi Toolkit
//!
//! Provides two implementations:
//! 1. `PriceOracle`: A proper, production-ready Soroban smart contract using `#[contract]` and `#[contractimpl]`.
//! 2. `PriceOracleSim`: A simulated, standard Rust version of the price oracle for backward compatibility.
//!
//! ## Access Control
//! - **`PriceOracle`** (Soroban contract): `set_price` is correctly gated — **Admin**
//!   only, enforced via `caller.require_auth()` plus a stored-admin equality check.
//!   This is one of the few functions in the whole codebase with fully correct
//!   enforcement; see `docs/ACCESS_CONTROL_MATRIX.md`.
//! - **`PriceOracleSim`** (internal simulation, used by `lending.rs`): `set_price`,
//!   `set_price_at`, `set_sanity_config` are gated by plain `String` equality against
//!   the stored admin — no cryptographic auth exists in this struct.
//! - **User**: read-only (`get_price`, `get_price_at`, `admin`).

use soroban_sdk::{contracttype, Env, Symbol};

use std::collections::BTreeMap;
use soroban_sdk::{contract, contractimpl, contracterror, Address, Env, Map, String as SorobanString, Symbol};
use crate::types::{ProtocolError, OracleSanityConfig};

// ─── Soroban Price Oracle Contract ───────────────────────────────────────────

/// Error codes specific to the Price Oracle contract.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum OracleError {
    AlreadyInitialized = 1,
    Unauthorized = 2,
    InvalidAmount = 3,
    MissingPrice = 4,
}

/// Price Oracle contract implementing standard price feed functionality.
#[contract]
pub struct PriceOracle;

#[contractimpl]
impl PriceOracle {
    /// Initialize the price oracle with an admin Address.
    ///
    /// # Arguments
    /// * `admin` - Governance administrator address.
    pub fn initialize(env: Env, admin: Address) -> Result<(), OracleError> {
        let admin_key = Symbol::new(&env, "admin");
        if env.storage().instance().has(&admin_key) {
            return Err(OracleError::AlreadyInitialized);
        }
        env.storage().instance().set(&admin_key, &admin);

        let prices_key = Symbol::new(&env, "prices");
        let prices: Map<SorobanString, i128> = Map::new(&env);
        env.storage().instance().set(&prices_key, &prices);
        Ok(())
    }

    /// Retrieve the administrator address.
    pub fn admin(env: Env) -> Address {
        let admin_key = Symbol::new(&env, "admin");
        env.storage()
            .instance()
            .get(&admin_key)
            .unwrap_or_else(|| panic!("not initialized"))
    }

    /// Set a price feed for an asset (admin only).
    ///
    /// # Arguments
    /// * `caller` - The calling administrator address.
    /// * `asset` - The asset symbol / key.
    /// * `price` - The new asset price (must be positive).
    pub fn set_price(
        env: Env,
        caller: Address,
        asset: SorobanString,
        price: i128,
    ) -> Result<(), OracleError> {
        caller.require_auth();

        let admin = Self::admin(env.clone());
        if caller != admin {
            return Err(OracleError::Unauthorized);
        }
        if price <= 0 {
            return Err(OracleError::InvalidAmount);
        }

        let prices_key = Symbol::new(&env, "prices");
        let mut prices: Map<SorobanString, i128> = env
            .storage()
            .instance()
            .get(&prices_key)
            .unwrap_or_else(|| Map::new(&env));

        prices.set(asset, price);
        env.storage().instance().set(&prices_key, &prices);
        Ok(())
    }

    /// Retrieve the current price of an asset.
    ///
    /// # Arguments
    /// * `asset` - The asset symbol / key.
    pub fn get_price(env: Env, asset: SorobanString) -> Result<i128, OracleError> {
        let prices_key = Symbol::new(&env, "prices");
        let prices: Map<SorobanString, i128> = env
            .storage()
            .instance()
            .get(&prices_key)
            .unwrap_or_else(|| Map::new(&env));

        prices
            .get(asset)
            .ok_or(OracleError::MissingPrice)
    }
}

// ─── Price Oracle Simulation ──────────────────────────────────────────────────

/// A timestamped price entry stored inside `PriceOracleSim`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriceEntry {
    /// The price value (WAD-scaled, i.e. 1e9 = $1.00).
    pub price: i128,
    /// Unix timestamp (seconds) when this price was recorded.
    pub timestamp: u64,
}

/// Simulated Price Oracle struct for backward compatibility with standard Rust simulations.
///
/// Extends the basic price map with:
/// - **Staleness detection** — prices older than `sanity.max_price_age_secs` are rejected.
/// - **Circuit-breaker** — price updates that deviate more than
///   `sanity.max_price_deviation_bps` from the last accepted price are rejected.
/// - **Range checks** — prices outside `[min_price, max_price]` are rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriceOracleSim {
    admin: String,
    prices: BTreeMap<String, PriceEntry>,
    sanity: OracleSanityConfig,
}

impl PriceOracleSim {
    /// Create a new price oracle simulator with default sanity configuration.
    pub fn new(admin: impl Into<String>) -> Self {
        Self {
            admin: admin.into(),
            prices: BTreeMap::new(),
            sanity: OracleSanityConfig::default(),
        }
    }

    /// Create a new price oracle simulator with a custom sanity configuration.
    pub fn with_sanity(admin: impl Into<String>, sanity: OracleSanityConfig) -> Self {
        Self {
            admin: admin.into(),
            prices: BTreeMap::new(),
            sanity,
        }
    }

    /// Retrieve the admin username/string.
    pub fn admin(&self) -> &str {
        &self.admin
    }

    /// Replace the sanity configuration (admin only).
    pub fn set_sanity_config(
        &mut self,
        caller: &str,
        sanity: OracleSanityConfig,
    ) -> Result<(), ProtocolError> {
        if caller != self.admin {
            return Err(ProtocolError::Unauthorized);
        }
        self.sanity = sanity;
        Ok(())
    }

    /// Set a price feed for an asset (admin only).
    ///
    /// `timestamp` is the Unix time (seconds) at which the price was observed.
    /// Pass the current time so that staleness checks work correctly.
    ///
    /// # Sanity checks performed
    /// 1. Caller must be the admin.
    /// 2. Price must be ≥ `sanity.min_price` (default: 1).
    /// 3. Price must be ≤ `sanity.max_price` when a maximum is configured.
    /// 4. If a previous price exists and `max_price_deviation_bps > 0`, the new
    ///    price must not deviate more than that threshold from the last accepted
    ///    price (circuit-breaker).
    pub fn set_price(
        &mut self,
        caller: &str,
        asset: impl Into<String>,
        price: i128,
    ) -> Result<(), ProtocolError> {
        // Delegate to the timestamped variant with timestamp = 0 (no staleness
        // check on the *incoming* price — only on reads).
        self.set_price_at(caller, asset, price, 0)
    }

    /// Set a price feed with an explicit observation timestamp.
    ///
    /// Prefer this over `set_price` when you want staleness checks on reads to
    /// work correctly.
    pub fn set_price_at(
        &mut self,
        caller: &str,
        asset: impl Into<String>,
        price: i128,
        timestamp: u64,
    ) -> Result<(), ProtocolError> {
        if caller != self.admin {
            return Err(ProtocolError::Unauthorized);
        }

        let asset: String = asset.into();

        // ── Sanity check 1: price must be within the configured range ──────
        if price < self.sanity.min_price {
            return Err(ProtocolError::OracleSanityCheckFailed(
                asset.clone(),
                format!(
                    "price {} is below minimum {}",
                    price, self.sanity.min_price
                ),
            ));
        }
        if self.sanity.max_price > 0 && price > self.sanity.max_price {
            return Err(ProtocolError::OracleSanityCheckFailed(
                asset.clone(),
                format!(
                    "price {} exceeds maximum {}",
                    price, self.sanity.max_price
                ),
            ));
        }

        // ── Sanity check 2: circuit-breaker — max deviation from last price ─
        if self.sanity.max_price_deviation_bps > 0 {
            if let Some(prev) = self.prices.get(&asset) {
                let deviation_bps = Self::price_deviation_bps(prev.price, price);
                if deviation_bps > u64::from(self.sanity.max_price_deviation_bps) {
                    return Err(ProtocolError::OracleSanityCheckFailed(
                        asset.clone(),
                        format!(
                            "price deviation {}bps exceeds circuit-breaker threshold {}bps",
                            deviation_bps, self.sanity.max_price_deviation_bps
                        ),
                    ));
                }
            }
        }

        self.prices.insert(asset, PriceEntry { price, timestamp });
        Ok(())
    }

    /// Retrieve the current price of an asset.
    ///
    /// Returns `ProtocolError::OraclePriceStale` when the stored price is older
    /// than `sanity.max_price_age_secs` and a non-zero `now` is provided.
    ///
    /// Pass `now = 0` to skip the staleness check (useful in unit tests that
    /// don't track time).
    pub fn get_price_at(&self, asset: &str, now: u64) -> Result<i128, ProtocolError> {
        let entry = self
            .prices
            .get(asset)
            .ok_or_else(|| ProtocolError::MissingPrice(asset.to_string()))?;

        // ── Staleness check ────────────────────────────────────────────────
        if now > 0 && self.sanity.max_price_age_secs > 0 && entry.timestamp > 0 {
            let age = now.saturating_sub(entry.timestamp);
            if age > self.sanity.max_price_age_secs {
                return Err(ProtocolError::OraclePriceStale(asset.to_string()));
            }
        }

        Ok(entry.price)
    }

    /// Retrieve the current price of an asset (no staleness check).
    ///
    /// This is the backward-compatible variant used by `LendingProtocol` internally.
    pub fn get_price(&self, asset: &str) -> Result<i128, ProtocolError> {
        self.get_price_at(asset, 0)
    }

    // ── Internal helpers ───────────────────────────────────────────────────

    /// Compute the absolute deviation between two prices in basis points.
    fn price_deviation_bps(old_price: i128, new_price: i128) -> u64 {
        if old_price == 0 {
            return 0;
        }
        let diff = (new_price - old_price).unsigned_abs();
        // deviation_bps = |new - old| * 10_000 / old
        (diff as u128)
            .saturating_mul(10_000)
            .checked_div(old_price.unsigned_abs() as u128)
            .unwrap_or(u64::MAX as u128) as u64
    }
}

// ─── Mock Oracle for Deterministic Testing ───────────────────────────────────

/// A programmable mock oracle for deterministic testing of protocol modules.
///
/// Wraps [`PriceOracleSim`] with a virtual clock, so tests can script price
/// behavior (and staleness) precisely without depending on wall-clock time or
/// a `soroban_sdk::Env`. `MockOracle` derefs to `PriceOracleSim`, so it can be
/// passed anywhere a `&PriceOracleSim` is expected (e.g. `LendingProtocol`
/// methods), making it compatible with every contract that consumes the sim
/// oracle today.
///
/// # Scenario helpers
/// - [`MockOracle::simulate_trend`] — linear price movement over N steps.
/// - [`MockOracle::simulate_spike`] — a temporary jump that reverts.
/// - [`MockOracle::simulate_crash`] — a rapid drop over N steps (e.g. "50% in
///   1 hour" — pass `drop_bps = 5000`, `steps` small enough that `duration /
///   steps` matches the desired granularity).
/// - [`MockOracle::simulate_staleness`] — advance the virtual clock without
///   updating the price, so the next `get_price` sees a stale read.
///
/// All scenario helpers go through [`PriceOracleSim::set_price_at`], so the
/// oracle's configured sanity checks (min/max price, deviation circuit
/// breaker) still apply — construct with [`MockOracle::with_sanity`] and a
/// permissive `OracleSanityConfig` (e.g. `max_price_deviation_bps: 0`) if a
/// scenario needs to push through a single large jump.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MockOracle {
    inner: PriceOracleSim,
    now: u64,
}

impl MockOracle {
    /// Create a new mock oracle with default sanity configuration and a
    /// virtual clock starting at `0`.
    pub fn new(admin: impl Into<String>) -> Self {
        Self {
            inner: PriceOracleSim::new(admin),
            now: 0,
        }
    }

    /// Create a new mock oracle with a custom sanity configuration.
    pub fn with_sanity(admin: impl Into<String>, sanity: OracleSanityConfig) -> Self {
        Self {
            inner: PriceOracleSim::with_sanity(admin, sanity),
            now: 0,
        }
    }

    /// The oracle's current virtual time (seconds).
    pub fn now(&self) -> u64 {
        self.now
    }

    /// Move the virtual clock forward by `secs` seconds.
    pub fn advance_time(&mut self, secs: u64) {
        self.now = self.now.saturating_add(secs);
    }

    /// Set the virtual clock to an absolute timestamp.
    pub fn set_time(&mut self, timestamp: u64) {
        self.now = timestamp;
    }

    /// Set a price for `asset`, timestamped at the oracle's current virtual
    /// time.
    pub fn set_price(
        &mut self,
        caller: &str,
        asset: impl Into<String>,
        price: i128,
    ) -> Result<(), ProtocolError> {
        let now = self.now;
        self.inner.set_price_at(caller, asset, price, now)
    }

    /// Set a price for `asset` at an explicit timestamp, without moving the
    /// virtual clock.
    pub fn set_price_at(
        &mut self,
        caller: &str,
        asset: impl Into<String>,
        price: i128,
        timestamp: u64,
    ) -> Result<(), ProtocolError> {
        self.inner.set_price_at(caller, asset, price, timestamp)
    }

    /// Read the current price of `asset`, staleness-checked against the
    /// oracle's current virtual time.
    pub fn get_price(&self, asset: &str) -> Result<i128, ProtocolError> {
        self.inner.get_price_at(asset, self.now)
    }

    /// Simulate a linear price trend from `start_price` to `end_price` over
    /// `steps` updates, each `step_duration_secs` apart. Advances the virtual
    /// clock as it goes; leaves it at the timestamp of the final update.
    pub fn simulate_trend(
        &mut self,
        caller: &str,
        asset: &str,
        start_price: i128,
        end_price: i128,
        steps: u32,
        step_duration_secs: u64,
    ) -> Result<(), ProtocolError> {
        assert!(steps > 0, "simulate_trend requires at least one step");
        for step in 0..=steps {
            let price = start_price
                + (end_price - start_price) * step as i128 / steps as i128;
            self.set_price(caller, asset, price)?;
            if step < steps {
                self.advance_time(step_duration_secs);
            }
        }
        Ok(())
    }

    /// Simulate a temporary price spike: set `base_price`, jump to
    /// `spike_price` one second later, then revert to `base_price` after
    /// `spike_duration_secs`.
    pub fn simulate_spike(
        &mut self,
        caller: &str,
        asset: &str,
        base_price: i128,
        spike_price: i128,
        spike_duration_secs: u64,
    ) -> Result<(), ProtocolError> {
        self.set_price(caller, asset, base_price)?;
        self.advance_time(1);
        self.set_price(caller, asset, spike_price)?;
        self.advance_time(spike_duration_secs);
        self.set_price(caller, asset, base_price)
    }

    /// Simulate a market crash: a rapid decline from `start_price` by
    /// `drop_bps` basis points (e.g. `5000` = 50%), spread over `steps`
    /// updates across `total_duration_secs`.
    pub fn simulate_crash(
        &mut self,
        caller: &str,
        asset: &str,
        start_price: i128,
        drop_bps: u32,
        steps: u32,
        total_duration_secs: u64,
    ) -> Result<(), ProtocolError> {
        assert!(steps > 0, "simulate_crash requires at least one step");
        let drop_bps = drop_bps.min(10_000) as i128;
        let end_price = start_price - (start_price * drop_bps / 10_000);
        self.simulate_trend(
            caller,
            asset,
            start_price,
            end_price,
            steps,
            total_duration_secs / steps as u64,
        )
    }

    /// Simulate oracle staleness: advance the virtual clock by
    /// `extra_secs` without issuing a new price update, so the next
    /// `get_price` call observes the last-set price as stale (assuming
    /// `sanity.max_price_age_secs` is exceeded).
    pub fn simulate_staleness(&mut self, extra_secs: u64) {
        self.advance_time(extra_secs);
    }
}

impl std::ops::Deref for MockOracle {
    type Target = PriceOracleSim;

    fn deref(&self) -> &PriceOracleSim {
        &self.inner
    }
}

impl std::ops::DerefMut for MockOracle {
    fn deref_mut(&mut self) -> &mut PriceOracleSim {
        &mut self.inner
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{Env, Address, String as SorobanString};
    use soroban_sdk::testutils::Address as _;

    fn setup_test(env: &Env) -> (PriceOracleClient<'static>, Address) {
        let contract_id = env.register_contract(None, PriceOracle);
        let client = PriceOracleClient::new(env, &contract_id);
        let admin = Address::generate(env);
        client.initialize(&admin);
        (client, admin)
    }

    #[test]
    fn test_initialization() {
        let env = Env::default();
        let (client, admin) = setup_test(&env);
        assert_eq!(client.admin(), admin);
    }

    #[test]
    fn test_set_and_get_price() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin) = setup_test(&env);

        let asset = SorobanString::from_str(&env, "XLM");
        
        client.set_price(&admin, &asset, &15000000);
        assert_eq!(client.get_price(&asset), 15000000);
    }

    #[test]
    fn test_unauthorized_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin) = setup_test(&env);

        let asset = SorobanString::from_str(&env, "XLM");
        let attacker = Address::generate(&env);

        let result = client.try_set_price(&attacker, &asset, &15000000);
        assert_eq!(result, Err(Ok(OracleError::Unauthorized)));
    }

    #[test]
    fn test_invalid_amount_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin) = setup_test(&env);

        let asset = SorobanString::from_str(&env, "XLM");

        let result = client.try_set_price(&admin, &asset, &0);
        assert_eq!(result, Err(Ok(OracleError::InvalidAmount)));

        let result = client.try_set_price(&admin, &asset, &-5);
        assert_eq!(result, Err(Ok(OracleError::InvalidAmount)));
    }

    #[test]
    fn test_missing_price_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin) = setup_test(&env);

        let asset = SorobanString::from_str(&env, "BTC");

        let result = client.try_get_price(&asset);
        assert_eq!(result, Err(Ok(OracleError::MissingPrice)));
    }

    // ── MockOracle ──────────────────────────────────────────────────────────

    #[test]
    fn mock_oracle_sets_and_reads_price_for_any_asset() {
        let mut oracle = MockOracle::new("admin");
        oracle.set_price("admin", "XLM", 100).unwrap();
        oracle.set_price("admin", "BTC", 60_000).unwrap();

        assert_eq!(oracle.get_price("XLM").unwrap(), 100);
        assert_eq!(oracle.get_price("BTC").unwrap(), 60_000);
    }

    #[test]
    fn mock_oracle_rejects_non_admin_price_updates() {
        let mut oracle = MockOracle::new("admin");
        let err = oracle.set_price("attacker", "XLM", 100).unwrap_err();
        assert_eq!(err, ProtocolError::Unauthorized);
    }

    #[test]
    fn mock_oracle_simulates_linear_trend() {
        let mut oracle = MockOracle::new("admin");
        oracle
            .simulate_trend("admin", "XLM", 100, 200, 4, 3_600)
            .unwrap();

        // Trend should have landed exactly on the end price.
        assert_eq!(oracle.get_price("XLM").unwrap(), 200);
        // 4 steps of 1 hour each => clock advanced 4 hours.
        assert_eq!(oracle.now(), 4 * 3_600);
    }

    #[test]
    fn mock_oracle_simulates_spike_and_reversion() {
        let mut sanity = OracleSanityConfig::default();
        sanity.max_price_deviation_bps = 0; // disable circuit-breaker for this scenario
        let mut oracle = MockOracle::with_sanity("admin", sanity);

        oracle
            .simulate_spike("admin", "XLM", 100, 500, 60)
            .unwrap();

        // Spike reverted back to the base price.
        assert_eq!(oracle.get_price("XLM").unwrap(), 100);
    }

    #[test]
    fn mock_oracle_simulates_50_percent_crash_over_one_hour() {
        let mut sanity = OracleSanityConfig::default();
        sanity.max_price_deviation_bps = 0; // gradual steps still individually large
        let mut oracle = MockOracle::with_sanity("admin", sanity);

        oracle
            .simulate_crash("admin", "XLM", 1_000, 5_000, 6, 3_600)
            .unwrap();

        assert_eq!(oracle.get_price("XLM").unwrap(), 500);
        assert_eq!(oracle.now(), 3_600);
    }

    #[test]
    fn mock_oracle_simulates_staleness() {
        let mut sanity = OracleSanityConfig::default();
        sanity.max_price_age_secs = 3_600;
        let mut oracle = MockOracle::with_sanity("admin", sanity);

        oracle.set_price("admin", "XLM", 100).unwrap();
        assert_eq!(oracle.get_price("XLM").unwrap(), 100);

        oracle.simulate_staleness(3_601);
        let err = oracle.get_price("XLM").unwrap_err();
        assert_eq!(err, ProtocolError::OraclePriceStale("XLM".to_string()));
    }

    #[test]
    fn mock_oracle_derefs_to_price_oracle_sim_for_protocol_use() {
        use crate::contracts::lending::LendingProtocol;
        use crate::types::{InterestRateModel, ReserveConfig};

        let mut protocol = LendingProtocol::new(
            vec!["admin".to_string()],
            1,
            "treasury",
            InterestRateModel::default(),
        );
        protocol
            .register_asset(
                "admin",
                ReserveConfig {
                    asset: "XLM".to_string(),
                    decimals: 7,
                    collateral_factor_bps: 8_000,
                    liquidation_threshold_bps: 8_500,
                    liquidation_bonus_bps: 1_000,
                    reserve_factor_bps: 1_000,
                    flash_loan_fee_bps: 9,
                    borrow_enabled: true,
                    deposit_enabled: true,
                    flash_loan_enabled: true,
                    supply_cap: 0,
                    borrow_cap: 0,
                    interest_rate_model: None,
                },
                0,
            )
            .unwrap();

        let mut oracle = MockOracle::new("admin");
        oracle.set_price("admin", "XLM", crate::utils::WAD).unwrap();

        protocol.deposit("alice", "XLM", 1_000_000, 0).unwrap();
        // `&MockOracle` derefs to `&PriceOracleSim`, satisfying LendingProtocol's
        // oracle parameter directly.
        let position = protocol.position("alice", &oracle).unwrap();
        assert!(position.collateral_value > 0);
    }
}


#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriceData {
    pub price: i128,
    pub timestamp: u64,
    pub is_degraded: bool,
}

pub fn check_staleness(env: &Env, timestamp: u64, threshold_seconds: u64) -> bool {
    let current_time = env.ledger().timestamp();
    current_time.saturating_sub(timestamp) > threshold_seconds
}