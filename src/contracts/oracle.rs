//! Price Oracle Implementation for Stellar DeFi Toolkit
//!
//! Provides two implementations:
//! 1. `PriceOracle`: A proper, production-ready Soroban smart contract using `#[contract]` and `#[contractimpl]`.
//! 2. `PriceOracleSim`: A simulated, standard Rust version of the price oracle for backward compatibility.

use std::collections::BTreeMap;
use soroban_sdk::{contract, contractimpl, contracterror, Address, Env, Map, String as SorobanString, Symbol};
use crate::types::ProtocolError;

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

/// Simulated Price Oracle struct for backward compatibility with standard Rust simulations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriceOracleSim {
    admin: String,
    prices: BTreeMap<String, i128>,
}

impl PriceOracleSim {
    /// Create a new price oracle simulator.
    pub fn new(admin: impl Into<String>) -> Self {
        Self {
            admin: admin.into(),
            prices: BTreeMap::new(),
        }
    }

    /// Retrieve the admin username/string.
    pub fn admin(&self) -> &str {
        &self.admin
    }

    /// Set a price feed for an asset (admin only).
    pub fn set_price(
        &mut self,
        caller: &str,
        asset: impl Into<String>,
        price: i128,
    ) -> Result<(), ProtocolError> {
        if caller != self.admin {
            return Err(ProtocolError::Unauthorized);
        }
        if price <= 0 {
            return Err(ProtocolError::InvalidAmount);
        }
        self.prices.insert(asset.into(), price);
        Ok(())
    }

    /// Retrieve the current price of an asset.
    pub fn get_price(&self, asset: &str) -> Result<i128, ProtocolError> {
        self.prices
            .get(asset)
            .copied()
            .ok_or_else(|| ProtocolError::MissingPrice(asset.to_string()))
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
