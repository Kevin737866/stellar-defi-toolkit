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
//!
//! ## Access Control
//! - **Admin**: `slash_oracle`, `update_config`, `pause`, `unpause` — `require_admin()`
//!   compares the passed-in admin argument to the stored admin, but never calls
//!   `require_auth()`, so it is not cryptographically enforced. See
//!   `docs/ACCESS_CONTROL_MATRIX.md`.
//! - **Keeper**: `register_oracle`, `submit_price`, `request_unbond`, `withdraw_stake` —
//!   no auth check on the `oracle_address` parameter; any caller can act on behalf of
//!   any registered oracle.
//! - **User**: read-only (price/stake/reputation lookups).
use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol};

use soroban_sdk::{contract, contractimpl, contracterror, contracttype, Address, Env, Symbol, Vec, Map, unwrap::UnwrapOptimized, symbol_short, panic_with_error};

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
/// Dispute review period (48 hours)
const DISPUTE_REVIEW_PERIOD: u64 = 172800;
/// Dispute bond amount
const DISPUTE_BOND: u64 = 500_000;

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
const DISPUTES: Symbol = symbol_short!("DISPUTES");
const DISP_CNT: Symbol = symbol_short!("DISP_CNT");

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
    DisputeNotFound = 15,
    DisputeNotPending = 16,
    DisputeReviewActive = 17,
    InvalidBond = 18,
}

// ─── Structs ─────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DisputeStatus {
    Pending,
    ResolvedValid,
    ResolvedInvalid,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct DisputeRecord {
    pub oracle: Address,
    pub asset_id: u32,
    pub challenger: Address,
    pub bond_amount: u64,
    pub start_time: u64,
    pub status: DisputeStatus,
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

        let disputes: Map<u64, DisputeRecord> = Map::new(&env);
        env.storage().instance().set(&DISPUTES, &disputes);
        env.storage().instance().set(&DISP_CNT, &0u64);

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

    /// Open a dispute against a specific oracle's price report
    pub fn open_dispute(
        env: Env,
        challenger: Address,
        oracle: Address,
        asset_id: u32,
        bond_amount: u64,
    ) -> Result<u64, DecentralizedOracleError> {
        Self::require_not_paused(&env)?;
        challenger.require_auth();

        if bond_amount != DISPUTE_BOND {
            return Err(DecentralizedOracleError::InvalidBond);
        }

        let mut disputes = Self::get_disputes(&env);
        let dispute_id: u64 = env.storage().instance().get(&DISP_CNT).unwrap_or(0);
        let current_time = env.ledger().timestamp();

        let record = DisputeRecord {
            oracle: oracle.clone(),
            asset_id,
            challenger: challenger.clone(),
            bond_amount,
            start_time: current_time,
            status: DisputeStatus::Pending,
        };

        disputes.set(dispute_id, record);
        env.storage().instance().set(&DISPUTES, &disputes);
        env.storage().instance().set(&DISP_CNT, &(dispute_id + 1));

        env.events().publish(
            (symbol_short!("DISPUTE"), symbol_short!("OPENED")),
            (dispute_id, challenger, oracle, asset_id),
        );

        Ok(dispute_id)
    }

    /// Resolve an open dispute
    pub fn resolve_dispute(
        env: Env,
        admin: Address,
        dispute_id: u64,
        is_valid: bool,
    ) -> Result<(), DecentralizedOracleError> {
        Self::require_admin(&env, admin.clone());

        let mut disputes = Self::get_disputes(&env);
        let mut record = disputes.get(dispute_id).ok_or(DecentralizedOracleError::DisputeNotFound)?;

        if record.status != DisputeStatus::Pending {
            return Err(DecentralizedOracleError::DisputeNotPending);
        }

        let current_time = env.ledger().timestamp();
        if current_time - record.start_time < DISPUTE_REVIEW_PERIOD {
            return Err(DecentralizedOracleError::DisputeReviewActive);
        }

        if is_valid {
            record.status = DisputeStatus::ResolvedValid;
            // The challenger was right. We slash the oracle and reward the challenger.
            // (Bond is effectively returned to challenger off-chain or by token transfer,
            // here we handle the internal logic for slashing the oracle)
            
            let mut stakes = Self::get_stakes(&env);
            if let Some(oracle_stake) = stakes.get(record.oracle.clone()) {
                // Slash the oracle by the slash percentage
                let slash_pct: u32 = env.storage().instance().get(&symbol_short!("SLASH_PCT")).unwrap();
                let slash_amount = (oracle_stake as u128 * slash_pct as u128 / 10000) as u64;
                let new_stake = oracle_stake.saturating_sub(slash_amount);
                stakes.set(record.oracle.clone(), new_stake);
                env.storage().instance().set(&STAKES, &stakes);

                // Increment slash count
                let mut slash_counts = Self::get_slash_count(&env);
                let current_slash = slash_counts.get(record.oracle.clone()).unwrap_or(0);
                slash_counts.set(record.oracle.clone(), current_slash + 1);
                env.storage().instance().set(&SLASH_EV, &slash_counts);
            }
        } else {
            record.status = DisputeStatus::ResolvedInvalid;
            // The challenger was wrong. Bond is forfeited to oracle.
            let mut stakes = Self::get_stakes(&env);
            if let Some(oracle_stake) = stakes.get(record.oracle.clone()) {
                stakes.set(record.oracle.clone(), oracle_stake.saturating_add(record.bond_amount));
                env.storage().instance().set(&STAKES, &stakes);
            }
        }

        disputes.set(dispute_id, record.clone());
        env.storage().instance().set(&DISPUTES, &disputes);

        env.events().publish(
            (symbol_short!("DISPUTE"), symbol_short!("RESOLVED")),
            (dispute_id, is_valid),
        );

        Ok(())
    }

    fn get_disputes(env: &Env) -> Map<u64, DisputeRecord> {
        env.storage().instance().get(&DISPUTES).unwrap_or_else(|| Map::new(env))
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


#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OracleNode {
    pub operator: Address,
    pub staked_balance: i128,
    pub active: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SlashingEvidence {
    pub node: Address,
    pub reported_price: i128,
    pub consensus_price: i128,
    pub deviation_bps: u32,
    pub severity: u32, // 1: Minor, 2: Moderate, 3: Severe
    pub challenge_deadline: u64,
    pub executed: bool,
}

#[contracttype]
pub enum DataKey {
    Node(Address),
    SlashingRecord(u64),
    SlashingCount,
    ConsensusDeviationThreshold, // basis points, e.g., 1500 = 15%
}

#[contract]
pub struct OracleSlashingContract;

#[contractimpl]
impl OracleSlashingContract {
    pub fn initialize(env: Env, admin: Address) {
        admin.require_auth();
        env.storage().instance().set(&DataKey::ConsensusDeviationThreshold, &1500u32);
        env.storage().instance().set(&DataKey::SlashingCount, &0u64);
        env.events().publish((Symbol::new(&env, "Initialized"),), admin);
    }

    pub fn report_bad_data(
        env: Env,
        challenger: Address,
        node: Address,
        reported_price: i128,
        consensus_price: i128,
        challenge_duration: u64,
    ) -> u64 {
        challenger.require_auth();

        let threshold: u32 = env.storage().instance().get(&DataKey::ConsensusDeviationThreshold).unwrap_or(1500);
        
        let diff = if reported_price > consensus_price {
            reported_price - consensus_price
        } else {
            consensus_price - reported_price
        };

        let deviation_bps = ((diff * 10000) / consensus_price) as u32;
        if deviation_bps <= threshold {
            panic!("Deviation is within acceptable consensus limits");
        }

        let severity = if deviation_bps > 5000 {
            3 // Severe (>50%)
        } else if deviation_bps > 3000 {
            2 // Moderate (>30%)
        } else {
            1 // Minor (>15%)
        };

        let count: u64 = env.storage().instance().get(&DataKey::SlashingCount).unwrap_or(0);
        let slashing_id = count + 1;
        let current_time = env.ledger().timestamp();
        let challenge_deadline = current_time + challenge_duration;

        let evidence = SlashingEvidence {
            node: node.clone(),
            reported_price,
            consensus_price,
            deviation_bps,
            severity,
            challenge_deadline,
            executed: false,
        };

        env.storage().instance().set(&DataKey::SlashingCount, &slashing_id);
        env.storage().persistent().set(&DataKey::SlashingRecord(slashing_id), &evidence);

        env.events().publish(
            (Symbol::new(&env, "SlashingInitiated"), slashing_id),
            (node, deviation_bps, severity),
        );

        slashing_id
    }

    pub fn execute_slashing(env: Env, admin: Address, slashing_id: u64) {
        admin.require_auth();

        let key = DataKey::SlashingRecord(slashing_id);
        let mut evidence: SlashingEvidence = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic!("Slashing record not found"));

        if evidence.executed {
            panic!("Slashing already executed");
        }

        if env.ledger().timestamp() < evidence.challenge_deadline {
            panic!("Challenge period has not yet elapsed");
        }

        evidence.executed = true;
        env.storage().persistent().set(&key, &evidence);

        env.events().publish(
            (Symbol::new(&env, "SlashingExecuted"), slashing_id),
            (evidence.node, evidence.severity),
        );
    }
}