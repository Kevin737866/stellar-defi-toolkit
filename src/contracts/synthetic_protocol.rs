//! Synthetic Asset Protocol for Stellar DeFi Toolkit
//!
//! Enables creation of synthetic versions of real-world assets (stocks, commodities, crypto)
//! on Stellar using over-collateralized positions and Soroban smart contracts.
//!
//! ## Features
//! - Multi-asset synthetic token support
//! - Over-collateralized minting with dynamic ratios
//! - Automated liquidation mechanisms
//! - Multi-oracle price feeds
//! - Fee distribution to stakers
//! - Governance for asset listing
//! - Comprehensive risk management
//!
//! ## Access Control
//! - **Admin**: `list_asset`, `pause`, `unpause`, `update_risk_params` — gated by a
//!   broken `require_admin()` (compares the contract's own address, not the caller).
//!   See `docs/ACCESS_CONTROL_MATRIX.md`.
//! - **Keeper**: `liquidate_position`, `update_oracle_price` — no auth check;
//!   `update_oracle_price` only checks whitelist membership, not caller identity.
//! - **User**: `mint_synthetic`, `burn_synthetic`, `stake`, `unstake` — none call
//!   `require_auth()` on the acting `user` address.

use soroban_sdk::{contract, contractimpl, Address, Env, Symbol, Vec, Map, unwrap::UnwrapOptimized};
use crate::types::synthetic::{
    DebtPool, DebtPoolEvent, DebtPoolKey, DebtShare, DebtSnapshot, FeeDistribution,
    LiquidationEvent, OraclePrice, ProtocolStats, RiskParameters, StakingPosition,
    SyntheticAsset, SyntheticPosition, DEBT_INDEX_SCALE,
};

// ─── Constants ───────────────────────────────────────────────────────────────

/// Minimum collateral ratio (150%)
const MIN_COLLATERAL_RATIO: u32 = 15000;
/// Maximum collateral ratio (1000%)
const MAX_COLLATERAL_RATIO: u32 = 100000;
/// Default minting fee (0.5%)
/// Liquidation penalty (10%)
const LIQUIDATION_PENALTY_BPS: u32 = 1000;
/// Minimum oracle confidence (80%)
const MIN_ORACLE_CONFIDENCE: u32 = 8000;
/// Staking reward rate (10% APY)
const STAKING_REWARD_RATE_BPS: u32 = 1000;

// ─── Storage Keys ─────────────────────────────────────────────────────────────

const ADMIN: Symbol = Symbol::short("ADMIN");
const PAUSED: Symbol = Symbol::short("PAUSED");
const ASSETS: Symbol = Symbol::short("ASSETS");
const POSITIONS: Symbol = Symbol::short("POSITIONS");
const ORACLES: Symbol = Symbol::short("ORACLES");
const RISK_PARAMS: Symbol = Symbol::short("RISKPARAM");
const FEE_DIST: Symbol = Symbol::short("FEE_DIST");
const STAKING: Symbol = Symbol::short("STAKING");
const PROTOCOL_STATS: Symbol = Symbol::short("PSTATS");
const NEXT_ASSET_ID: Symbol = Symbol::short("NEXTASSET");
const NEXT_PROPOSAL_ID: Symbol = Symbol::short("NEXTPROP");

// ─── Synthetic Protocol Contract ───────────────────────────────────────────

/// Main synthetic asset protocol contract
#[contract]
pub struct SyntheticProtocolContract;

#[contractimpl]
impl SyntheticProtocolContract {
    /// Initialize the synthetic protocol
    /// 
    /// # Arguments
    /// * `admin` - Admin address for governance
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&ADMIN) {
            panic!("Already initialized");
        }

        env.storage().instance().set(&ADMIN, &admin);
        env.storage().instance().set(&PAUSED, &false);
        env.storage().instance().set(&NEXT_ASSET_ID, &1u32);
        env.storage().instance().set(&NEXT_PROPOSAL_ID, &1u64);

        // Initialize storage
        let assets: Map<u32, SyntheticAsset> = Map::new(&env);
        env.storage().instance().set(&ASSETS, &assets);

        let positions: Map<Address, Vec<SyntheticPosition>> = Map::new(&env);
        env.storage().instance().set(&POSITIONS, &positions);

        let oracles: Map<u32, Vec<Address>> = Map::new(&env);
        env.storage().instance().set(&ORACLES, &oracles);

        // Initialize risk parameters
        let risk_params = RiskParameters {
            global_min_ratio: MIN_COLLATERAL_RATIO,
            max_debt_per_user: 1_000_000_000_000, // $1M max per user
            max_total_debt: 100_000_000_000_000, // $100M max total
            liquidation_threshold: 12000, // 120%
            emergency_pause_threshold: 5000, // 50%
            min_oracle_confidence: MIN_ORACLE_CONFIDENCE,
        };
        env.storage().instance().set(&RISK_PARAMS, &risk_params);

        // Initialize fee distribution
        let fee_dist = FeeDistribution {
            total_fees: 0,
            fees_to_stakers: 0,
            fees_to_treasury: 0,
            last_distribution: env.ledger().timestamp(),
            staking_share_price: 1_000_000, // Start at 1.0
        };
        env.storage().instance().set(&FEE_DIST, &fee_dist);

        // Initialize protocol stats
        let stats = ProtocolStats {
            total_value_locked: 0,
            total_synthetic_supply: 0,
            active_positions: 0,
            avg_collateral_ratio: 0,
            total_fees_collected: 0,
            daily_liquidations: 0,
            health_score: 10000, // Perfect health
        };
        env.storage().instance().set(&PROTOCOL_STATS, &stats);

        let staking: Map<Address, StakingPosition> = Map::new(&env);
        env.storage().instance().set(&STAKING, &staking);

        // Shared debt pool (#156). Starts empty and is extended by every mint,
        // burn, and price update.
        env.storage().persistent().set(
            &DebtPoolKey::Pool,
            &DebtPool {
                total_debt: 0,
                settled_debt: 0,
                total_synth_supply: 0,
                debt_index: DEBT_INDEX_SCALE,
                active_positions: 0,
                last_updated: env.ledger().timestamp(),
            },
        );

        env.events().publish(
            (Symbol::new(&env, "SYNTH_INIT"),),
            admin,
        );
    }

    /// List a new synthetic asset (admin only)
    /// 
    /// # Arguments
    /// * `asset` - Synthetic asset configuration
    /// Returns the id assigned to the new asset.
    pub fn list_asset(env: Env, admin: Address, asset: SyntheticAsset) -> u32 {
        Self::require_admin(&env, &admin);
        Self::require_not_paused(&env);

        // Validate asset parameters
        if asset.min_collateral_ratio < MIN_COLLATERAL_RATIO {
            panic!("Minimum collateral ratio too low");
        }
        if asset.max_collateral_ratio > MAX_COLLATERAL_RATIO {
            panic!("Maximum collateral ratio too high");
        }
        if asset.minting_fee_bps > 1000 {
            panic!("Minting fee too high"); // Max 10%
        }

        let asset_id = env.storage().instance().get(&NEXT_ASSET_ID).unwrap();
        let next_id = asset_id + 1;
        env.storage().instance().set(&NEXT_ASSET_ID, &next_id);

        let mut assets = Self::get_assets(&env);
        assets.set(asset_id, asset.clone());
        env.storage().instance().set(&ASSETS, &assets);

        // Initialize oracle list for this asset
        let mut oracles = Self::get_oracles(&env);
        let mut oracle_list = Vec::new(&env);
        oracle_list.push_back(asset.oracle_address);
        oracles.set(asset_id, oracle_list);
        env.storage().instance().set(&ORACLES, &oracles);

        env.events().publish(
            (Symbol::new(&env, "ASSET_LISTED"),),
            (asset_id, asset.symbol),
        );

        asset_id
    }

    /// Mint synthetic tokens
    /// 
    /// # Arguments
    /// * `user` - User address
    /// * `asset_id` - Asset to mint
    /// * `collateral_token` - Collateral token address
    /// * `collateral_amount` - Amount of collateral to deposit
    /// * `synthetic_amount` - Amount of synthetic tokens to mint
    pub fn mint_synthetic(
        env: Env,
        user: Address,
        asset_id: u32,
        collateral_token: Address,
        collateral_amount: u64,
        synthetic_amount: u64,
    ) {
        Self::require_not_paused(&env);

        // Get asset info
        let asset = Self::get_asset(env.clone(), asset_id);
        if !asset.active {
            panic!("Asset is not active");
        }

        // Get current price
        let price = Self::get_asset_price(env.clone(), asset_id);
        if price.confidence < MIN_ORACLE_CONFIDENCE {
            panic!("Oracle confidence too low");
        }

        // Calculate collateral value
        let collateral_value = Self::calculate_usd_value(collateral_amount, &price);
        let required_collateral = (synthetic_amount * asset.min_collateral_ratio as u64) / 10000;

        if collateral_value < required_collateral {
            panic!("Insufficient collateral");
        }

        // Check user limits
        let risk_params = Self::get_risk_params(&env);
        let current_debt = Self::get_user_debt(&env, user.clone());
        if current_debt + synthetic_amount > risk_params.max_debt_per_user {
            panic!("Exceeds maximum debt per user");
        }

        // Calculate and deduct minting fee
        let minting_fee = (synthetic_amount * asset.minting_fee_bps as u64) / 10000;
        let mint_amount = synthetic_amount - minting_fee;

        // Update or create user position
        let mut positions = Self::get_positions(&env);
        let user_positions = positions.get(user.clone()).unwrap_or_else(|| Vec::new(&env));
        let mut updated_positions = user_positions;

        // Find existing position for this asset
        let mut position_found = false;
        for i in 0..updated_positions.len() {
            let mut pos = updated_positions.get(i).unwrap();
            if pos.asset_id == asset_id {
                // Update existing position
                pos.synthetic_amount += mint_amount;
                pos.collateral_deposits.set(collateral_token.clone(), 
                    pos.collateral_deposits.get(collateral_token.clone()).unwrap_or(0) + collateral_amount);
                pos.debt_amount += synthetic_amount;
                pos.collateral_ratio = Self::calculate_collateral_ratio(&env, &pos);
                pos.last_updated = env.ledger().timestamp();
                position_found = true;
            }
            updated_positions.set(i, pos);
        }

        if !position_found {
            // Create new position
            let mut collateral_deposits = Map::new(&env);
            collateral_deposits.set(collateral_token.clone(), collateral_amount);
            
            let now = env.ledger().timestamp();
            let mut new_position = SyntheticPosition {
                owner: user.clone(),
                asset_id,
                synthetic_amount: mint_amount,
                collateral_deposits,
                debt_amount: synthetic_amount,
                collateral_ratio: 0,
                created_at: now,
                last_updated: now,
                liquidating: false,
            };
            new_position.collateral_ratio = Self::calculate_collateral_ratio(&env, &new_position);
            updated_positions.push_back(new_position);
        }

        positions.set(user.clone(), updated_positions);
        env.storage().instance().set(&POSITIONS, &positions);

        // Update asset totals
        let mut assets = Self::get_assets(&env);
        let mut asset_info = assets.get(asset_id).unwrap();
        asset_info.total_supply += mint_amount;
        asset_info.total_collateral += collateral_amount;
        assets.set(asset_id, asset_info);
        env.storage().instance().set(&ASSETS, &assets);

        // Track the new debt in the shared pool
        Self::increase_debt(&env, user.clone(), asset_id, synthetic_amount);

        // Update protocol stats
        Self::update_protocol_stats(&env, mint_amount, collateral_value, 0);

        // Distribute minting fee
        Self::distribute_fees(&env, minting_fee);

        env.events().publish(
            (Symbol::new(&env, "SYNTH_MINTED"),),
            (user, asset_id, mint_amount, collateral_amount),
        );
    }

    /// Burn synthetic tokens and release collateral
    /// 
    /// # Arguments
    /// * `user` - User address
    /// * `asset_id` - Asset to burn
    /// * `synthetic_amount` - Amount to burn
    /// * `collateral_token` - Collateral token to withdraw
    /// * `collateral_amount` - Amount of collateral to withdraw
    pub fn burn_synthetic(
        env: Env,
        user: Address,
        asset_id: u32,
        synthetic_amount: u64,
        collateral_token: Address,
        collateral_amount: u64,
    ) {
        Self::require_not_paused(&env);

        let mut positions = Self::get_positions(&env);
        let mut user_positions = positions.get(user.clone())
            .unwrap_or_else(|| panic!("No positions found for user"));

        // Find and update position
        let mut position_found = false;
        for i in 0..user_positions.len() {
            let mut pos = user_positions.get(i).unwrap();
            if pos.asset_id == asset_id {
                if pos.synthetic_amount < synthetic_amount {
                    panic!("Insufficient synthetic balance");
                }
                
                let collateral_balance = pos.collateral_deposits.get(collateral_token.clone()).unwrap_or(0);
                if collateral_balance < collateral_amount {
                    panic!("Insufficient collateral balance");
                }

                // Update position
                pos.synthetic_amount -= synthetic_amount;
                pos.debt_amount -= synthetic_amount;
                
                if pos.synthetic_amount == 0 {
                    pos.collateral_deposits.remove(collateral_token.clone());
                } else {
                    pos.collateral_deposits.set(collateral_token.clone(), collateral_balance - collateral_amount);
                }
                
                pos.collateral_ratio = Self::calculate_collateral_ratio(&env, &pos);
                pos.last_updated = env.ledger().timestamp();
                position_found = true;
            }
            user_positions.set(i, pos);
        }

        if !position_found {
            panic!("Position not found for asset");
        }

        positions.set(user.clone(), user_positions);
        env.storage().instance().set(&POSITIONS, &positions);

        // Update asset totals
        let mut assets = Self::get_assets(&env);
        let mut asset_info = assets.get(asset_id).unwrap();
        asset_info.total_supply -= synthetic_amount;
        asset_info.total_collateral -= collateral_amount;
        assets.set(asset_id, asset_info);
        env.storage().instance().set(&ASSETS, &assets);

        // Remove the repaid debt from the shared pool
        Self::decrease_debt(&env, user.clone(), asset_id, synthetic_amount);

        // Update protocol stats
        Self::update_protocol_stats(&env, 0, 0, synthetic_amount);

        env.events().publish(
            (Symbol::new(&env, "SYNTH_BURNED"),),
            (user.clone(), asset_id, synthetic_amount, collateral_amount),
        );
    }

    /// Liquidate an under-collateralized position
    /// 
    /// # Arguments
    /// * `liquidator` - Address performing liquidation
    /// * `user` - User being liquidated
    /// * `asset_id` - Asset being liquidated
    pub fn liquidate_position(
        env: Env,
        liquidator: Address,
        user: Address,
        asset_id: u32,
    ) {
        Self::require_not_paused(&env);

        let risk_params = Self::get_risk_params(&env);
        let mut positions = Self::get_positions(&env);
        let user_positions = positions.get(user.clone())
            .unwrap_or_else(|| panic!("No positions found for user"));

        // Find position to liquidate
        let mut target_position: Option<SyntheticPosition> = None;
        let mut position_index = 0;
        
        for i in 0..user_positions.len() {
            let pos = user_positions.get(i).unwrap();
            if pos.asset_id == asset_id {
                if pos.liquidating {
                    panic!("Position already being liquidated");
                }
                target_position = Some(pos.clone());
                position_index = i;
                break;
            }
        }

        let mut position = target_position.unwrap();
        
        // Check if liquidation is justified
        let current_ratio = Self::calculate_collateral_ratio(&env, &position);
        if current_ratio >= risk_params.liquidation_threshold {
            panic!("Position is not under-collateralized");
        }

        // Mark as liquidating
        position.liquidating = true;
        let mut updated_positions = user_positions;
        updated_positions.set(position_index, position.clone());
        positions.set(user.clone(), updated_positions);
        env.storage().instance().set(&POSITIONS, &positions);

        // Calculate liquidation
        let price = Self::get_asset_price(env.clone(), asset_id);
        let debt_value = Self::calculate_usd_value(position.debt_amount, &price);
        let liquidation_penalty = (debt_value * LIQUIDATION_PENALTY_BPS as u64) / 10000;
        
        // Distribute collateral
        let total_collateral_value = Self::get_position_collateral_value(&env, &position);
        let liquidator_share = (total_collateral_value * 9000) / 10000; // 90% to liquidator
        let _user_share = total_collateral_value - liquidator_share;

        // Create liquidation event
        let event_id = env.ledger().timestamp();
        let _liquidation_event = LiquidationEvent {
            event_id,
            position_owner: user.clone(),
            asset_id,
            liquidator: liquidator.clone(),
            synthetic_amount_burned: position.synthetic_amount,
            collateral_to_liquidator: Map::new(&env), // Would be populated with actual distribution
            collateral_returned: Map::new(&env), // Would be populated with actual distribution
            penalty_bps: LIQUIDATION_PENALTY_BPS,
            timestamp: env.ledger().timestamp(),
        };

        // In production, handle actual collateral transfers
        // For now, just emit events

        // Update asset totals
        let mut assets = Self::get_assets(&env);
        let mut asset_info = assets.get(asset_id).unwrap();
        asset_info.total_supply -= position.synthetic_amount;
        asset_info.total_collateral -= position.debt_amount; // Simplified
        assets.set(asset_id, asset_info);
        env.storage().instance().set(&ASSETS, &assets);

        // Remove liquidated position
        let mut final_positions = Self::get_positions(&env).get(user.clone()).unwrap();
        for i in 0..final_positions.len() {
            let pos = final_positions.get(i).unwrap();
            if pos.asset_id == asset_id {
                final_positions.remove(i);
                break;
            }
        }
        positions.set(user.clone(), final_positions);
        env.storage().instance().set(&POSITIONS, &positions);

        // The liquidated position no longer contributes debt to the pool
        Self::decrease_debt(&env, user.clone(), asset_id, position.debt_amount);

        // Update protocol stats
        Self::update_protocol_stats(&env, 0, 0, position.synthetic_amount);

        // Distribute liquidation fee
        Self::distribute_fees(&env, liquidation_penalty);

        env.events().publish(
            (Symbol::new(&env, "POS_LIQUIDATED"),),
            (user.clone(), asset_id, liquidator, position.synthetic_amount),
        );
    }

    /// Update oracle price for an asset
    /// 
    /// # Arguments
    /// * `oracle_address` - Oracle providing the price
    /// * `asset_id` - Asset ID
    /// * `price` - New price in USD
    /// * `confidence` - Price confidence (0-10000)
    pub fn update_oracle_price(
        env: Env,
        oracle_address: Address,
        asset_id: u32,
        price: u64,
        confidence: u32,
    ) {
        // Verify oracle is authorized for this asset
        let oracles = Self::get_oracles(&env);
        let asset_oracles = oracles.get(asset_id)
            .unwrap_or_else(|| panic!("Asset not found"));

        let oracle_authorized = asset_oracles.iter().any(|addr| addr == oracle_address);
        if !oracle_authorized {
            panic!("Oracle not authorized for this asset");
        }

        if confidence < MIN_ORACLE_CONFIDENCE {
            panic!("Confidence too low");
        }

        let price_data = OraclePrice {
            asset_id,
            price,
            decimals: 6, // Standard 6 decimals
            confidence,
            timestamp: env.ledger().timestamp(),
            source_address: oracle_address,
        };
        Self::save_price(&env, &price_data);

        // Revalue every position on this asset so pool shares stay accurate
        Self::revalue_asset_debt(&env, asset_id);

        env.events().publish(
            (Symbol::new(&env, "PRICE_UPDATED"),),
            (asset_id, price, confidence),
        );
    }

    /// Stake tokens for fee sharing
    /// 
    /// # Arguments
    /// * `user` - User staking
    /// * `amount` - Amount to stake
    pub fn stake(env: Env, user: Address, amount: u64) {
        Self::require_not_paused(&env);

        if amount == 0 {
            panic!("Amount must be greater than 0");
        }

        let mut staking = Self::get_staking(&env);
        let mut staking_pos = staking.get(user.clone()).unwrap_or_else(|| StakingPosition {
            staker: user.clone(),
            staked_amount: 0,
            reward_index: 0,
            staked_at: env.ledger().timestamp(),
            last_claim: 0,
            total_rewards_claimed: 0,
        });

        staking_pos.staked_amount += amount;
        staking.set(user.clone(), staking_pos);
        env.storage().instance().set(&STAKING, &staking);

        env.events().publish(
            (Symbol::new(&env, "STAKED"),),
            (user.clone(), amount),
        );
    }

    /// Unstake tokens
    /// 
    /// # Arguments
    /// * `user` - User unstaking
    /// * `amount` - Amount to unstake
    pub fn unstake(env: Env, user: Address, amount: u64) {
        Self::require_not_paused(&env);

        let mut staking = Self::get_staking(&env);
        let mut staking_pos = staking.get(user.clone())
            .unwrap_or_else(|| panic!("No staking position found"));

        if staking_pos.staked_amount < amount {
            panic!("Insufficient staked balance");
        }

        // Calculate rewards
        let current_rewards = Self::calculate_staking_rewards(&env, &staking_pos);
        staking_pos.staked_amount -= amount;
        staking_pos.total_rewards_claimed += current_rewards;
        staking_pos.last_claim = env.ledger().timestamp();

        staking.set(user.clone(), staking_pos);
        env.storage().instance().set(&STAKING, &staking);

        env.events().publish(
            (Symbol::new(&env, "UNSTAKED"),),
            (user.clone(), amount, current_rewards),
        );
    }

    /// Get asset information
    pub fn get_asset(env: Env, asset_id: u32) -> SyntheticAsset {
        Self::get_assets(&env).get(asset_id)
            .unwrap_or_else(|| panic!("Asset not found"))
    }

    /// Get user position
    pub fn get_user_position(env: Env, user: Address, asset_id: u32) -> SyntheticPosition {
        let positions = Self::get_positions(&env).get(user.clone())
            .unwrap_or_else(|| Vec::new(&env));
        
        for pos in positions.iter() {
            if pos.asset_id == asset_id {
                return pos;
            }
        }
        
        panic!("Position not found");
    }

    /// Get current asset price
    /// Latest price for an asset, falling back to the seeded quote until an
    /// authorized oracle reports one.
    pub fn get_asset_price(env: Env, asset_id: u32) -> OraclePrice {
        Self::load_price(&env, asset_id).unwrap_or(OraclePrice {
            asset_id,
            price: 100_000_000, // $100.00 with 6 decimals
            decimals: 6,
            confidence: 9500, // 95% confidence
            timestamp: env.ledger().timestamp(),
            source_address: env.current_contract_address(),
        })
    }

    fn load_price(env: &Env, asset_id: u32) -> Option<OraclePrice> {
        env.storage()
            .persistent()
            .get(&DebtPoolKey::Price(asset_id))
    }

    fn save_price(env: &Env, price: &OraclePrice) {
        env.storage()
            .persistent()
            .set(&DebtPoolKey::Price(price.asset_id), price);
    }

    /// Get protocol statistics
    pub fn get_protocol_stats(env: Env) -> ProtocolStats {
        Self::load_protocol_stats(&env)
    }

    // ─── Shared debt pool (#156) ─────────────────────────────────────────────

    /// Current state of the global debt pool.
    pub fn get_debt_pool(env: Env) -> DebtPool {
        Self::load_debt_pool(&env)
    }

    /// Debt share owned by `owner` for `asset_id`, if the position has debt.
    pub fn get_debt_share(env: Env, owner: Address, asset_id: u32) -> Option<DebtShare> {
        Self::load_debt_share(&env, asset_id, owner)
    }

    /// Fraction of the global pool owned by `owner` for `asset_id`, in basis
    /// points. Returns `0` when the position carries no debt.
    pub fn debt_share_bps(env: Env, owner: Address, asset_id: u32) -> u32 {
        let pool = Self::load_debt_pool(&env);
        if pool.total_debt == 0 {
            return 0;
        }
        let base_debt = match Self::load_debt_share(&env, asset_id, owner) {
            Some(share) => share.base_debt,
            None => return 0,
        };
        (base_debt.saturating_mul(10_000) / pool.total_debt) as u32
    }

    /// Take a snapshot of the pool for global settlement.
    ///
    /// The snapshot freezes `settled_debt` at the current total so settlement
    /// can proceed against a fixed number while trading continues.
    pub fn take_debt_snapshot(env: Env, admin: Address) -> DebtSnapshot {
        Self::require_admin(&env, &admin);

        let mut pool = Self::load_debt_pool(&env);
        let snapshot = DebtSnapshot {
            id: Self::next_snapshot_id(&env),
            total_debt: pool.total_debt,
            debt_index: pool.debt_index,
            total_synth_supply: pool.total_synth_supply,
            timestamp: env.ledger().timestamp(),
        };
        pool.settled_debt = pool.total_debt;
        pool.last_updated = env.ledger().timestamp();
        Self::save_debt_pool(&env, &pool);

        env.events().publish(
            (Symbol::new(&env, "DEBT_POOL"),),
            DebtPoolEvent::DebtSnapshotTaken(snapshot.id, snapshot.total_debt),
        );
        snapshot
    }

    /// Get all listed assets
    pub fn get_listed_assets(env: Env) -> Vec<SyntheticAsset> {
        let assets = Self::get_assets(&env);
        let mut listed_assets = Vec::new(&env);
        
        for asset in assets.values() {
            if asset.active {
                listed_assets.push_back(asset);
            }
        }
        
        listed_assets
    }

    // ─── Admin Functions ─────────────────────────────────────────────────────

    /// Pause the protocol (admin only)
    pub fn pause(env: Env, admin: Address) {
        Self::require_admin(&env, &admin);
        env.storage().instance().set(&PAUSED, &true);
        env.events().publish(
            (Symbol::new(&env, "PAUSED"),), true);
    }

    /// Unpause the protocol (admin only)
    pub fn unpause(env: Env, admin: Address) {
        Self::require_admin(&env, &admin);
        env.storage().instance().set(&PAUSED, &false);
        env.events().publish(
            (Symbol::new(&env, "PAUSED"),), false);
    }

    /// Update risk parameters (admin only)
    pub fn update_risk_params(env: Env, admin: Address, new_params: RiskParameters) {
        Self::require_admin(&env, &admin);
        
        if new_params.global_min_ratio < MIN_COLLATERAL_RATIO {
            panic!("Invalid minimum ratio");
        }
        if new_params.min_oracle_confidence < 5000 {
            panic!("Invalid confidence threshold");
        }
        
        env.storage().instance().set(&RISK_PARAMS, &new_params);
        env.events().publish(
            (Symbol::new(&env, "RISK_UPDATED"),), ());
    }

    // ─── Internal Helpers ─────────────────────────────────────────────────────

    fn calculate_collateral_ratio(env: &Env, position: &SyntheticPosition) -> u32 {
        let total_collateral_value = Self::get_position_collateral_value(env, position);
        if position.debt_amount == 0 {
            return 100000; // Infinite ratio when no debt
        }
        ((total_collateral_value * 10000) / position.debt_amount) as u32
    }

    fn calculate_usd_value(amount: u64, price: &OraclePrice) -> u64 {
        (amount * price.price) / (10_u64.pow(price.decimals))
    }

    fn get_position_collateral_value(_env: &Env, position: &SyntheticPosition) -> u64 {
        let mut total_value = 0u64;
        for (_collateral_token, amount) in position.collateral_deposits.iter() {
            // In production, get price for each collateral token
            // For now, assume 1:1 with USD
            total_value += amount;
        }
        total_value
    }

    fn get_user_debt(env: &Env, user: Address) -> u64 {
        let positions = Self::get_positions(env).get(user)
            .unwrap_or_else(|| Vec::new(env));
        let mut total_debt = 0u64;
        for pos in positions.iter() {
            total_debt += pos.debt_amount;
        }
        total_debt
    }

    fn calculate_staking_rewards(env: &Env, staking_pos: &StakingPosition) -> u64 {
        let time_elapsed = env.ledger().timestamp() - staking_pos.staked_at;
        let rewards = (staking_pos.staked_amount * STAKING_REWARD_RATE_BPS as u64 * time_elapsed) 
            / (10000 * 365 * 24 * 3600);
        rewards
    }

    fn update_protocol_stats(env: &Env, minted: u64, collateral_added: u64, burned: u64) {
        let mut stats = Self::load_protocol_stats(env);
        stats.total_synthetic_supply = stats
            .total_synthetic_supply
            .saturating_add(minted)
            .saturating_sub(burned);
        stats.total_value_locked = stats.total_value_locked.saturating_add(collateral_added);
        
        // Recalculate average collateral ratio
        let positions = Self::get_positions(env);
        let mut total_ratio = 0u32;
        let mut position_count = 0u32;
        
        for user_positions in positions.values() {
            for pos in user_positions.iter() {
                total_ratio += Self::calculate_collateral_ratio(env, &pos);
                position_count += 1;
            }
        }
        
        if position_count > 0 {
            stats.avg_collateral_ratio = total_ratio / position_count;
        }
        
        env.storage().instance().set(&PROTOCOL_STATS, &stats);
    }

    fn distribute_fees(env: &Env, fee_amount: u64) {
        let mut fee_dist = Self::get_fee_distribution(env);
        let staking_share = (fee_amount * 8000) / 10000; // 80% to stakers
        let treasury_share = fee_amount - staking_share;
        
        fee_dist.total_fees += fee_amount;
        fee_dist.fees_to_stakers += staking_share;
        fee_dist.fees_to_treasury += treasury_share;
        
        env.storage().instance().set(&FEE_DIST, &fee_dist);
    }

    fn require_admin(env: &Env, admin: &Address) {
        admin.require_auth();
        let stored: Address = env.storage().instance().get(&ADMIN).unwrap();
        if &stored != admin {
            panic!("Not authorized");
        }
    }

    fn require_not_paused(env: &Env) {
        let paused = env.storage().instance().get(&PAUSED).unwrap();
        if paused {
            panic!("Protocol is paused");
        }
    }

    // Storage getters
    fn get_assets(env: &Env) -> Map<u32, SyntheticAsset> {
        env.storage().instance().get(&ASSETS).unwrap()
    }

    fn get_positions(env: &Env) -> Map<Address, Vec<SyntheticPosition>> {
        env.storage().instance().get(&POSITIONS).unwrap()
    }

    fn get_oracles(env: &Env) -> Map<u32, Vec<Address>> {
        env.storage().instance().get(&ORACLES).unwrap()
    }

    fn get_risk_params(env: &Env) -> RiskParameters {
        env.storage().instance().get(&RISK_PARAMS).unwrap()
    }

    fn get_fee_distribution(env: &Env) -> FeeDistribution {
        env.storage().instance().get(&FEE_DIST).unwrap()
    }

    fn load_protocol_stats(env: &Env) -> ProtocolStats {
        env.storage().instance().get(&PROTOCOL_STATS).unwrap()
    }

    fn get_staking(env: &Env) -> Map<Address, StakingPosition> {
        env.storage().instance().get(&STAKING).unwrap()
    }

    // ─── Shared debt pool (#156) ─────────────────────────────────────────────

    fn load_debt_pool(env: &Env) -> DebtPool {
        env.storage()
            .persistent()
            .get(&DebtPoolKey::Pool)
            .unwrap_or(DebtPool {
                total_debt: 0,
                settled_debt: 0,
                total_synth_supply: 0,
                debt_index: DEBT_INDEX_SCALE,
                active_positions: 0,
                last_updated: env.ledger().timestamp(),
            })
    }

    fn save_debt_pool(env: &Env, pool: &DebtPool) {
        env.storage().persistent().set(&DebtPoolKey::Pool, pool);
    }

    /// Base-currency value of `synthetic_amount` units of an asset.
    fn base_currency_value(amount: u64, price: &OraclePrice) -> u64 {
        Self::calculate_usd_value(amount, price)
    }

    /// Record debt added to the pool and refresh the owner's share.
    fn increase_debt(env: &Env, owner: Address, asset_id: u32, synthetic_amount: u64) {
        let price = Self::get_asset_price(env.clone(), asset_id);
        let added = Self::base_currency_value(synthetic_amount, &price);
        let mut pool = Self::load_debt_pool(env);

        let existing = Self::load_debt_share(env, asset_id, owner.clone());
        let was_active = existing.is_some();
        let previous = existing.map(|share| share.base_debt).unwrap_or(0);
        let base_debt = previous + added;

        pool.total_debt += added;
        pool.total_synth_supply += synthetic_amount;
        if !was_active {
            pool.active_positions += 1;
        }
        pool.last_updated = env.ledger().timestamp();
        // The index tracks debt per synth unit so shares can be recomputed
        // after a burn or revaluation.
        pool.debt_index = Self::index_for(&pool);

        Self::store_debt_share(env, &mut pool, owner.clone(), asset_id, base_debt);
        Self::save_debt_pool(env, &pool);
        env.events().publish(
            (Symbol::new(&env, "DEBT_POOL"),),
            DebtPoolEvent::DebtIncreased(owner, asset_id, added, pool.total_debt),
        );
    }

    /// Record debt repaid from the pool and refresh the owner's share.
    fn decrease_debt(env: &Env, owner: Address, asset_id: u32, synthetic_amount: u64) {
        let price = Self::get_asset_price(env.clone(), asset_id);
        let removed = Self::base_currency_value(synthetic_amount, &price);
        let mut pool = Self::load_debt_pool(env);

        let existing = Self::load_debt_share(env, asset_id, owner.clone());
        let had_share = existing.is_some();
        let previous = existing.map(|share| share.base_debt).unwrap_or(0);
        let base_debt = previous.saturating_sub(removed);

        pool.total_debt = pool.total_debt.saturating_sub(removed);
        pool.total_synth_supply = pool.total_synth_supply.saturating_sub(synthetic_amount);
        if base_debt == 0 && had_share {
            pool.active_positions = pool.active_positions.saturating_sub(1);
            env.storage()
                .persistent()
                .remove(&DebtPoolKey::Share(asset_id, owner.clone()));
        } else {
            Self::store_debt_share(env, &mut pool, owner.clone(), asset_id, base_debt);
        }
        pool.last_updated = env.ledger().timestamp();
        pool.debt_index = Self::index_for(&pool);

        Self::save_debt_pool(env, &pool);
        env.events().publish(
            (Symbol::new(&env, "DEBT_POOL"),),
            DebtPoolEvent::DebtDecreased(owner, asset_id, removed, pool.total_debt),
        );
    }

    /// Revalue every position holding `asset_id` after a price update.
    fn revalue_asset_debt(env: &Env, asset_id: u32) {
        let mut pool = Self::load_debt_pool(env);
        let old_total = pool.total_debt;
        if old_total == 0 {
            return;
        }

        let quote = Self::get_asset_price(env.clone(), asset_id);

        let positions = Self::get_positions(env);
        let mut new_total = 0u64;
        let mut updates: Vec<(Address, u64)> = Vec::new(env);

        for (owner, user_positions) in positions.iter() {
            let mut owner_debt = 0u64;
            for position in user_positions.iter() {
                if position.asset_id == asset_id && position.debt_amount > 0 {
                    owner_debt += Self::base_currency_value(position.debt_amount, &quote);
                }
            }
            if owner_debt > 0 {
                new_total += owner_debt;
                updates.push_back((owner, owner_debt));
            }
        }

        // Positions on other assets keep the value recorded at their last sync.
        let other_assets = Self::other_assets_debt(env, asset_id);
        new_total += other_assets;

        pool.total_debt = new_total;
        pool.debt_index = Self::index_for(&pool);
        pool.last_updated = env.ledger().timestamp();
        Self::save_debt_pool(env, &pool);

        for (owner, base_debt) in updates.iter() {
            Self::store_debt_share(env, &mut pool, owner, asset_id, base_debt);
        }
        Self::save_debt_pool(env, &pool);

        env.events().publish(
            (Symbol::new(&env, "DEBT_POOL"),),
            DebtPoolEvent::DebtRevalued(old_total, new_total),
        );
    }

    /// Total debt held by positions on assets other than `asset_id`.
    fn other_assets_debt(env: &Env, asset_id: u32) -> u64 {
        let positions = Self::get_positions(env);
        let mut total = 0u64;
        for (_owner, user_positions) in positions.iter() {
            for position in user_positions.iter() {
                if position.asset_id != asset_id && position.debt_amount > 0 {
                    let price = Self::get_asset_price(env.clone(), position.asset_id);
                    total += Self::base_currency_value(position.debt_amount, &price);
                }
            }
        }
        total
    }

    fn index_for(pool: &DebtPool) -> u64 {
        if pool.total_synth_supply == 0 {
            return DEBT_INDEX_SCALE;
        }
        pool.total_debt.saturating_mul(DEBT_INDEX_SCALE) / pool.total_synth_supply
    }

    fn load_debt_share(env: &Env, asset_id: u32, owner: Address) -> Option<DebtShare> {
        env.storage()
            .persistent()
            .get(&DebtPoolKey::Share(asset_id, owner))
    }

    fn store_debt_share(
        env: &Env,
        pool: &mut DebtPool,
        owner: Address,
        asset_id: u32,
        base_debt: u64,
    ) {
        let share_bps = if pool.total_debt == 0 {
            0
        } else {
            (base_debt.saturating_mul(10_000) / pool.total_debt) as u32
        };
        let share = DebtShare {
            owner: owner.clone(),
            asset_id,
            base_debt,
            debt_index_entry: pool.debt_index,
            share_bps,
            updated_at: env.ledger().timestamp(),
        };
        env.storage()
            .persistent()
            .set(&DebtPoolKey::Share(asset_id, owner), &share);
    }

    fn next_snapshot_id(env: &Env) -> u64 {
        let id: u64 = env
            .storage()
            .persistent()
            .get(&DebtPoolKey::SnapshotCount)
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&DebtPoolKey::SnapshotCount, &(id + 1));
        id + 1
    }
}
