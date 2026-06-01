//! Price Oracle Implementation for Stellar DeFi Toolkit
//!
//! Provides two implementations:
//! 1. `PriceOracle`: A proper, production-ready Soroban smart contract using `#[contract]` and `#[contractimpl]`.
//! 2. `PriceOracleSim`: A simulated, standard Rust version of the price oracle for backward compatibility.

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
}
