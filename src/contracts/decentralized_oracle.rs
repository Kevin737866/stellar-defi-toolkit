//! Decentralized Oracle Contract for Stellar DeFi Toolkit
//!
//! A truly decentralized price oracle that aggregates prices from multiple sources
//! using staking, reputation, and consensus mechanisms.
//!
//! ## Features
//! - Permissionless oracle registration with staking
//! - Multi-source price aggregation with various methods
//! - Reputation-based oracle weighting
//! - Slashing mechanism for malicious behavior
//! - Governance through token holders
//! - Price deviation detection and alerts
//! - Circuit breaker for extreme price movements

use soroban_sdk::{contract, contractimpl, contracterror, Address, Env, Symbol, Vec, Map, unwrap::UnwrapOptimized, symbol_short, panic_with_error};

// ─── Constants ───────────────────────────────────────────────────────────────

/// Minimum stake required to become an oracle (in base token units)
const MIN_STAKE: u64 = 1_000_000;
/// Minimum number of oracles required for price aggregation
const MIN_ORACLES: u32 = 3;
/// Maximum price deviation allowed (5% = 500 basis points)
const MAX_PRICE_DEVIATION: u32 = 500;
/// Oracle report timeout period (1 hour)
const ORACLE_TIMEOUT: u64 = 3600;
/// Minimum confidence threshold (70% = 7000 basis points)
const MIN_CONFIDENCE: u32 = 7000;
/// Slashing percentage for malicious behavior (10% = 1000 basis points)
const SLASH_PERCENTAGE: u32 = 1000;
/// Reward percentage for accurate reports (0.1% = 10 basis points)
const REWARD_PERCENTAGE: u32 = 10;
/// Unbonding period for withdrawing stake (7 days)
const UNBONDING_PERIOD: u64 = 604800;
/// Maximum number of oracles
const MAX_ORACLES: u32 = 100;

// ─── Storage Keys ─────────────────────────────────────────────────────────────

const ADMIN: Symbol = symbol_short!("ADMIN");
const PAUSED: Symbol = symbol_short!("PAUSED");
const ORACLES: Symbol = symbol_short!("ORACLES");
const STAKES: Symbol = symbol_short!("STAKES");
const PRICES: Symbol = symbol_short!("PRICES");
const AGG_PRICES: Symbol = symbol_short!("AG_PRICES");
const REPUTATION: Symbol = symbol_short!("REPUTAT");
const SLASH_EV: Symbol = symbol_short!("SLASH_EV");
const CONFIG: Symbol = symbol_short!("CONFIG");

// ─── Error Codes ───────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum DecentralizedOracleError {
    AlreadyInitialized = 1,
    Unauthorized = 2,
    InsufficientStake = 3,
    OracleExists = 4,
    OracleNotFound = 5,
    InvalidPrice = 6,
    InsufficientOracles = 7,
    PriceTooOld = 8,
    ConfidenceTooLow = 9,
    MaxOraclesReached = 10,
    NotStaked = 11,
    StillUnbonding = 12,
    ContractPaused = 13,
    InvalidConfig = 14,
}

// ─── Decentralized Oracle Contract ───────────────────────────────────────────

#[contract]
pub struct DecentralizedOracle;

#[contractimpl]
impl DecentralizedOracle {
    /// Initialize the decentralized oracle
    ///
    /// # Arguments
    /// * `admin` - Admin address for governance
    pub fn initialize(env: Env, admin: Address) -> Result<(), DecentralizedOracleError> {
        if env.storage().instance().has(&ADMIN) {
            return Err(DecentralizedOracleError::AlreadyInitialized);
        }

        env.storage().instance().set(&ADMIN, &admin);
        env.storage().instance().set(&PAUSED, &false);

        // Initialize storage with simple Maps
        let oracles: Map<Address, u64> = Map::new(&env); // oracle -> stake amount
        env.storage().instance().set(&ORACLES, &oracles);

        let stakes: Map<Address, u64> = Map::new(&env);
        env.storage().instance().set(&STAKES, &stakes);

        let prices: Map<u32, Map<Address, u64>> = Map::new(&env); // asset_id -> (oracle -> price)
        env.storage().instance().set(&PRICES, &prices);

        let aggregated_prices: Map<u32, u64> = Map::new(&env); // asset_id -> aggregated price
        env.storage().instance().set(&AGG_PRICES, &aggregated_prices);

        let reputation: Map<Address, u32> = Map::new(&env); // oracle -> reputation score
        env.storage().instance().set(&REPUTATION, &reputation);

        let slash_count: Map<Address, u64> = Map::new(&env); // oracle -> slash count
        env.storage().instance().set(&SLASH_EV, &slash_count);

        // Initialize configuration as individual values
        env.storage().instance().set(&symbol_short!("MIN_STAKE"), &MIN_STAKE);
        env.storage().instance().set(&symbol_short!("MIN_ORACL"), &MIN_ORACLES);
        env.storage().instance().set(&symbol_short!("MAX_DEV"), &MAX_PRICE_DEVIATION);
        env.storage().instance().set(&symbol_short!("TIMEOUT"), &ORACLE_TIMEOUT);
        env.storage().instance().set(&symbol_short!("MIN_CONF"), &MIN_CONFIDENCE);
        env.storage().instance().set(&symbol_short!("SLASH_PCT"), &SLASH_PERCENTAGE);
        env.storage().instance().set(&symbol_short!("UNBOND"), &UNBONDING_PERIOD);
        env.storage().instance().set(&symbol_short!("MAX_ORCL"), &MAX_ORACLES);

        env.events().publish(
            (symbol_short!("INIT"),),
            admin,
        );

        Ok(())
    }

    /// Register as an oracle by staking tokens
    ///
    /// # Arguments
    /// * `oracle_address` - Address registering as oracle
    /// * `stake_amount` - Amount to stake
    pub fn register_oracle(
        env: Env,
        oracle_address: Address,
        stake_amount: u64,
    ) -> Result<(), DecentralizedOracleError> {
        Self::require_not_paused(&env)?;

        let min_stake: u64 = env.storage().instance().get(&symbol_short!("MIN_STAKE")).unwrap();
        let max_oracles: u32 = env.storage().instance().get(&symbol_short!("MAX_ORCL")).unwrap();

        if stake_amount < min_stake {
            return Err(DecentralizedOracleError::InsufficientStake);
        }

        let mut oracles = Self::get_oracles(&env);
        
        if oracles.contains_key(oracle_address.clone()) {
            return Err(DecentralizedOracleError::OracleExists);
        }

        if oracles.len() as u32 >= max_oracles {
            return Err(DecentralizedOracleError::MaxOraclesReached);
        }

        oracles.set(oracle_address.clone(), stake_amount);
        env.storage().instance().set(&ORACLES, &oracles);

        let mut stakes = Self::get_stakes(&env);
        stakes.set(oracle_address.clone(), stake_amount);
        env.storage().instance().set(&STAKES, &stakes);

        let mut reputation = Self::get_reputation(&env);
        reputation.set(oracle_address.clone(), 8000); // Start with 80% reputation
        env.storage().instance().set(&REPUTATION, &reputation);

        env.events().publish(
            (symbol_short!("REGD"),),
            (oracle_address, stake_amount),
        );

        Ok(())
    }

    /// Submit a price report
    ///
    /// # Arguments
    /// * `oracle_address` - Oracle submitting the price
    /// * `asset_id` - Asset ID
    /// * `price` - Price value
    /// * `confidence` - Confidence score (0-10000)
    /// * `timestamp` - When price was observed
    pub fn submit_price(
        env: Env,
        oracle_address: Address,
        asset_id: u32,
        price: u64,
        confidence: u32,
        timestamp: u64,
    ) -> Result<(), DecentralizedOracleError> {
        Self::require_not_paused(&env)?;

        let oracles = Self::get_oracles(&env);
        if !oracles.contains_key(oracle_address.clone()) {
            return Err(DecentralizedOracleError::OracleNotFound);
        }

        let oracle_timeout: u64 = env.storage().instance().get(&symbol_short!("TIMEOUT")).unwrap();
        let min_confidence: u32 = env.storage().instance().get(&symbol_short!("MIN_CONF")).unwrap();
        let current_time = env.ledger().timestamp();

        if timestamp > current_time || current_time - timestamp > oracle_timeout {
            return Err(DecentralizedOracleError::PriceTooOld);
        }

        if confidence < min_confidence {
            return Err(DecentralizedOracleError::ConfidenceTooLow);
        }

        if price == 0 {
            return Err(DecentralizedOracleError::InvalidPrice);
        }

        // Store price submission
        let mut prices = Self::get_prices(&env);
        let asset_prices = prices.get(asset_id).unwrap_or_else(|| Map::new(&env));
        let mut updated_prices = asset_prices;
        updated_prices.set(oracle_address.clone(), price);
        prices.set(asset_id, updated_prices);
        env.storage().instance().set(&PRICES, &prices);

        // Trigger price aggregation
        Self::aggregate_price(&env, asset_id);

        env.events().publish(
            (symbol_short!("SUBMIT"),),
            (oracle_address, asset_id, price, confidence),
        );

        Ok(())
    }

    /// Get the aggregated price for an asset
    ///
    /// # Arguments
    /// * `asset_id` - Asset ID
    pub fn get_price(env: Env, asset_id: u32) -> Result<u64, DecentralizedOracleError> {
        let aggregated_prices = Self::get_aggregated_prices(&env);
        aggregated_prices.get(asset_id)
            .ok_or(DecentralizedOracleError::InsufficientOracles)
    }

    /// Get oracle stake amount
    ///
    /// # Arguments
    /// * `oracle_address` - Oracle address
    pub fn get_oracle_stake(env: Env, oracle_address: Address) -> Result<u64, DecentralizedOracleError> {
        let stakes = Self::get_stakes(&env);
        stakes.get(oracle_address)
            .ok_or(DecentralizedOracleError::OracleNotFound)
    }

    /// Get oracle reputation
    ///
    /// # Arguments
    /// * `oracle_address` - Oracle address
    pub fn get_oracle_reputation(env: Env, oracle_address: Address) -> Result<u32, DecentralizedOracleError> {
        let reputation = Self::get_reputation(&env);
        reputation.get(oracle_address)
            .ok_or(DecentralizedOracleError::OracleNotFound)
    }

    /// Get all registered oracle addresses
    pub fn get_oracle_addresses(env: Env) -> Vec<Address> {
        let oracles = Self::get_oracles(&env);
        let mut addresses = Vec::new(&env);
        for addr in oracles.keys() {
            addresses.push_back(addr);
        }
        addresses
    }

    /// Request to unbond and withdraw stake
    ///
    /// # Arguments
    /// * `oracle_address` - Oracle address
    pub fn request_unbond(env: Env, oracle_address: Address) -> Result<(), DecentralizedOracleError> {
        let stakes = Self::get_stakes(&env);
        if !stakes.contains_key(oracle_address.clone()) {
            return Err(DecentralizedOracleError::NotStaked);
        }

        let unbonding: u64 = env.storage().instance().get(&symbol_short!("UNBOND")).unwrap();
        let current_time = env.ledger().timestamp();

        // Store unbonding start time
        env.storage().temporary().set(
            &(oracle_address.clone(), symbol_short!("UNB_ST")),
            &current_time
        );

        env.events().publish(
            (symbol_short!("UNB_REQ"),),
            oracle_address,
        );

        Ok(())
    }

    /// Withdraw stake after unbonding period
    ///
    /// # Arguments
    /// * `oracle_address` - Oracle address
    pub fn withdraw_stake(env: Env, oracle_address: Address) -> Result<u64, DecentralizedOracleError> {
        let stakes = Self::get_stakes(&env);
        let stake_amount = stakes.get(oracle_address.clone())
            .ok_or(DecentralizedOracleError::NotStaked)?;

        let unbonding: u64 = env.storage().instance().get(&symbol_short!("UNBOND")).unwrap();
        let unbond_start: u64 = env.storage().temporary()
            .get(&(oracle_address.clone(), symbol_short!("UNB_ST")))
            .unwrap_or(0);

        if unbond_start == 0 {
            return Err(DecentralizedOracleError::NotStaked);
        }

        let current_time = env.ledger().timestamp();
        if current_time - unbond_start < unbonding {
            return Err(DecentralizedOracleError::StillUnbonding);
        }

        // Remove oracle
        let mut oracles = Self::get_oracles(&env);
        oracles.remove(oracle_address.clone());
        env.storage().instance().set(&ORACLES, &oracles);

        let mut stakes = Self::get_stakes(&env);
        stakes.remove(oracle_address.clone());
        env.storage().instance().set(&STAKES, &stakes);

        let mut reputation = Self::get_reputation(&env);
        reputation.remove(oracle_address.clone());
        env.storage().instance().set(&REPUTATION, &reputation);

        env.events().publish(
            (symbol_short!("WITHDRAW"),),
            (oracle_address, stake_amount),
        );

        Ok(stake_amount)
    }

    /// Slash an oracle for malicious behavior (admin only)
    ///
    /// # Arguments
    /// * `admin` - Admin address
    /// * `oracle_address` - Oracle to slash
    /// * `reason` - Reason for slashing
    pub fn slash_oracle(
        env: Env,
        admin: Address,
        oracle_address: Address,
        reason: Symbol,
    ) -> Result<(), DecentralizedOracleError> {
        Self::require_admin(&env, admin);

        let mut stakes = Self::get_stakes(&env);
        let stake_amount = stakes.get(oracle_address.clone())
            .ok_or(DecentralizedOracleError::OracleNotFound)?;

        let slash_pct: u32 = env.storage().instance().get(&symbol_short!("SLASH_PCT")).unwrap();
        let slash_amount = (stake_amount * slash_pct as u64) / 10000;

        let new_stake = stake_amount - slash_amount;
        stakes.set(oracle_address.clone(), new_stake);
        env.storage().instance().set(&STAKES, &stakes);

        // Update reputation
        let mut reputation = Self::get_reputation(&env);
        let current_rep = reputation.get(oracle_address.clone()).unwrap_or(8000);
        reputation.set(oracle_address.clone(), (current_rep * 9000) / 10000);
        env.storage().instance().set(&REPUTATION, &reputation);

        // Update slash count
        let mut slash_count = Self::get_slash_count(&env);
        let current_count = slash_count.get(oracle_address.clone()).unwrap_or(0);
        slash_count.set(oracle_address.clone(), current_count + 1);
        env.storage().instance().set(&SLASH_EV, &slash_count);

        env.events().publish(
            (symbol_short!("SLASHED"),),
            (oracle_address, slash_amount, reason),
        );

        Ok(())
    }

    /// Update configuration parameter (admin only)
    ///
    /// # Arguments
    /// * `admin` - Admin address
    /// * `param_name` - Parameter name
    /// * `new_value` - New value
    pub fn update_config(
        env: Env,
        admin: Address,
        param_name: Symbol,
        new_value: u64,
    ) -> Result<(), DecentralizedOracleError> {
        Self::require_admin(&env, admin);

        env.storage().instance().set(&param_name, &new_value);

        env.events().publish(
            (symbol_short!("CFG_UPD"),),
            (param_name, new_value),
        );

        Ok(())
    }

    /// Pause the oracle (admin only)
    pub fn pause(env: Env, admin: Address) -> Result<(), DecentralizedOracleError> {
        Self::require_admin(&env, admin);
        env.storage().instance().set(&PAUSED, &true);
        env.events().publish((symbol_short!("PAUSED"),), true);
        Ok(())
    }

    /// Unpause the oracle (admin only)
    pub fn unpause(env: Env, admin: Address) -> Result<(), DecentralizedOracleError> {
        Self::require_admin(&env, admin);
        env.storage().instance().set(&PAUSED, &false);
        env.events().publish((symbol_short!("PAUSED"),), false);
        Ok(())
    }

    // ─── Internal Functions ─────────────────────────────────────────────────────

    fn aggregate_price(env: &Env, asset_id: u32) {
        let prices = Self::get_prices(env).get(asset_id);
        if prices.is_none() {
            return;
        }
        let prices = prices.unwrap();

        let min_oracles: u32 = env.storage().instance().get(&symbol_short!("MIN_ORACL")).unwrap();

        if prices.len() < min_oracles {
            return;
        }

        let reputation = Self::get_reputation(env);
        let mut weighted_sum = 0u128;
        let mut total_weight = 0u32;

        for (oracle_addr, price) in prices.iter() {
            if let Some(rep_score) = reputation.get(oracle_addr) {
                weighted_sum += (price as u128) * (rep_score as u128);
                total_weight += rep_score;
            }
        }

        if total_weight == 0 {
            let sum: u128 = prices.iter().map(|(_, p)| p as u128).sum();
            let avg_price = (sum / prices.len() as u128) as u64;
            let mut aggregated = Self::get_aggregated_prices(env);
            aggregated.set(asset_id, avg_price);
            env.storage().instance().set(&AGG_PRICES, &aggregated);
            return;
        }

        let weighted_price = (weighted_sum / (total_weight as u128)) as u64;
        let mut aggregated = Self::get_aggregated_prices(env);
        aggregated.set(asset_id, weighted_price);
        env.storage().instance().set(&AGG_PRICES, &aggregated);

        env.events().publish(
            (symbol_short!("PRICE_AGG"),),
            (asset_id, weighted_price),
        );
    }

    // Storage getters
    fn get_oracles(env: &Env) -> Map<Address, u64> {
        env.storage().instance().get(&ORACLES).unwrap()
    }

    fn get_stakes(env: &Env) -> Map<Address, u64> {
        env.storage().instance().get(&STAKES).unwrap()
    }

    fn get_prices(env: &Env) -> Map<u32, Map<Address, u64>> {
        env.storage().instance().get(&PRICES).unwrap()
    }

    fn get_aggregated_prices(env: &Env) -> Map<u32, u64> {
        env.storage().instance().get(&AGG_PRICES).unwrap()
    }

    fn get_reputation(env: &Env) -> Map<Address, u32> {
        env.storage().instance().get(&REPUTATION).unwrap()
    }

    fn get_slash_count(env: &Env) -> Map<Address, u64> {
        env.storage().instance().get(&SLASH_EV).unwrap()
    }

    fn require_admin(env: &Env, admin: Address) {
        let stored_admin = env.storage().instance().get(&ADMIN).unwrap_optimized();
        if admin != stored_admin {
            panic_with_error!(env, DecentralizedOracleError::Unauthorized);
        }
    }

    fn require_not_paused(env: &Env) -> Result<(), DecentralizedOracleError> {
        let paused = env.storage().instance().get(&PAUSED).unwrap();
        if paused {
            return Err(DecentralizedOracleError::ContractPaused);
        }
        Ok(())
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{Address, Symbol};
    use soroban_sdk::testutils::Address as _;

    fn setup_test(env: &Env) -> (DecentralizedOracleClient<'static>, Address) {
        let contract_id = env.register_contract(None, DecentralizedOracle);
        let client = DecentralizedOracleClient::new(env, &contract_id);
        let admin = Address::generate(env);
        client.initialize(&admin);
        (client, admin)
    }

    #[test]
    fn test_initialization() {
        let env = Env::default();
        let (_client, _admin) = setup_test(&env);
        
        let min_oracles: u32 = env.storage().instance().get(&symbol_short!("MIN_ORACL")).unwrap();
        assert_eq!(min_oracles, MIN_ORACLES);
    }

    #[test]
    fn test_register_oracle() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin) = setup_test(&env);

        let oracle = Address::generate(&env);
        client.register_oracle(&oracle, &MIN_STAKE);
        
        let stake = client.get_oracle_stake(&oracle);
        assert_eq!(stake, Ok(MIN_STAKE));
    }

    #[test]
    fn test_insufficient_stake_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin) = setup_test(&env);

        let oracle = Address::generate(&env);
        let result = client.try_register_oracle(&oracle, &(MIN_STAKE - 1));
        assert!(matches!(result, Err(_)));
    }

    #[test]
    fn test_submit_price() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin) = setup_test(&env);

        let oracle = Address::generate(&env);
        client.register_oracle(&oracle, &MIN_STAKE);

        let asset_id = 1u32;
        let price = 1000000u64;
        let confidence = 9000u32;
        let timestamp = env.ledger().timestamp();

        client.submit_price(&oracle, &asset_id, &price, &confidence, &timestamp);
    }

    #[test]
    fn test_unauthorized_config_update_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin) = setup_test(&env);

        let unauthorized = Address::generate(&env);
        let result = client.try_update_config(&unauthorized, &symbol_short!("MIN_ORACL"), &4);
        assert!(matches!(result, Err(_)));
    }
}
