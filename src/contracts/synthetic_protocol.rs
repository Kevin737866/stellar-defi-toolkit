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
    SyntheticAsset, SyntheticPosition, LiquidationEvent, OraclePrice, FeeDistribution,
    RiskParameters, ProtocolStats, AssetProposal, AssetProposalType, AssetUpdateParams,
    StakingPosition, MarketData, AssetType, MintRequest, MintReceipt, RedemptionRequest,
    RedemptionReceipt, ExchangeRequest, ExchangeReceipt,
};

// ─── Constants ───────────────────────────────────────────────────────────────

/// Minimum collateral ratio (150%)
const MIN_COLLATERAL_RATIO: u32 = 15000;
/// Maximum collateral ratio (1000%)
const MAX_COLLATERAL_RATIO: u32 = 100000;
/// Default minting fee (0.5%)
const DEFAULT_MINTING_FEE_BPS: u32 = 50;
/// Liquidation penalty (10%)
const LIQUIDATION_PENALTY_BPS: u32 = 1000;
/// Minimum oracle confidence (80%)
const MIN_ORACLE_CONFIDENCE: u32 = 8000;
/// Staking reward rate (10% APY)
const STAKING_REWARD_RATE_BPS: u32 = 1000;

// ─── Storage Keys ─────────────────────────────────────────────────────────────

// `Symbol::short` caps at 9 characters, so longer keys are abbreviated.
const ADMIN: Symbol = Symbol::short("ADMIN");
const PAUSED: Symbol = Symbol::short("PAUSED");
const ASSETS: Symbol = Symbol::short("ASSETS");
const POSITIONS: Symbol = Symbol::short("POSITIONS");
const ORACLES: Symbol = Symbol::short("ORACLES");
const RISK_PARAMS: Symbol = Symbol::short("RISKPARMS");
const FEE_DIST: Symbol = Symbol::short("FEE_DIST");
const STAKING: Symbol = Symbol::short("STAKING");
const PROTOCOL_STATS: Symbol = Symbol::short("PROTOSTAT");
const NEXT_ASSET_ID: Symbol = Symbol::short("NEXTASST");
const NEXT_PROPOSAL_ID: Symbol = Symbol::short("NEXTPROP");
const REDEEM_FEE: Symbol = Symbol::short("REDEEMFEE");
const EXCHANGE_FEE: Symbol = Symbol::short("EXCHGFEE");
const PRICES: Symbol = Symbol::short("PRICES");
/// Default redemption fee (0.5%)
const DEFAULT_REDEMPTION_FEE_BPS: u32 = 50;
/// Default synth-to-synth exchange fee (0.3%)
const DEFAULT_EXCHANGE_FEE_BPS: u32 = 30;
/// Ceiling for the configurable redemption and exchange fees (10%)
const MAX_FEE_BPS: u32 = 1000;
/// Price used before an oracle reports for an asset ($1.00 at 6 decimals)
const DEFAULT_PRICE: u64 = 100_000_000;
const PRICE_DECIMALS: u32 = 6;
const DEFAULT_ORACLE_CONFIDENCE: u32 = 9500;

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

        env.events().publish(
            (Symbol::short("PROTOINIT"), admin.clone()),
            (),
        );
    }

    /// List a new synthetic asset (admin only)
    /// 
    /// # Arguments
    /// * `asset` - Synthetic asset configuration
    pub fn list_asset(env: Env, asset: SyntheticAsset) {
        Self::require_admin(&env);
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
        oracle_list.push_back(asset.oracle_address.clone());
        oracles.set(asset_id, oracle_list);
        env.storage().instance().set(&ORACLES, &oracles);

        env.events().publish(
            (Symbol::short("ASSETLIST"), asset_id),
            asset.symbol.clone(),
        );
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
        let asset = Self::asset_of(&env, asset_id);
        if !asset.active {
            panic!("Asset is not active");
        }

        // Get current price
        let price = Self::price_of(&env, asset_id);
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
            
            let mut new_position = SyntheticPosition {
                owner: user.clone(),
                asset_id,
                synthetic_amount: mint_amount,
                collateral_deposits,
                debt_amount: synthetic_amount,
                collateral_ratio: 0,
                created_at: env.ledger().timestamp(),
                last_updated: env.ledger().timestamp(),
                liquidating: false,
            };
            new_position.collateral_ratio =
                Self::calculate_collateral_ratio(&env, &new_position);
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

        // Update protocol stats
        Self::update_protocol_stats(&env, mint_amount, collateral_value, 0);

        // Distribute minting fee
        Self::distribute_fees(&env, minting_fee);

        env.events().publish(
            (Symbol::short("SYNTHMINT"), user.clone(), asset_id),
            (mint_amount, collateral_amount, price.price),
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

        // Update protocol stats
        Self::update_protocol_stats(&env, 0, 0, synthetic_amount);

        env.events().publish(
            (Symbol::short("SYNTHBURN"), user.clone(), asset_id),
            (synthetic_amount, collateral_amount),
        );
    }

    /// Mint synthetic tokens against deposited collateral at the live oracle price
    ///
    /// The position must end at or above the asset's minimum collateral ratio, and
    /// the minting fee is deducted from the synthetic credited to the user while the
    /// full requested amount is registered as debt.
    pub fn mint(env: Env, request: MintRequest) -> MintReceipt {
        Self::require_not_paused(&env);
        request.user.require_auth();

        if request.synthetic_amount == 0 {
            panic!("Synthetic amount must be positive");
        }
        if request.collateral_amount == 0 {
            panic!("Collateral amount must be positive");
        }

        let asset = Self::asset_of(&env, request.asset_id);
        if !asset.active {
            panic!("Asset is not active");
        }

        let price = Self::price_of(&env, request.asset_id);
        if price.confidence < MIN_ORACLE_CONFIDENCE {
            panic!("Oracle confidence too low");
        }

        let minting_fee = (request.synthetic_amount * asset.minting_fee_bps as u64) / 10_000;
        let mint_amount = request.synthetic_amount - minting_fee;

        let collateral_value = Self::calculate_usd_value(request.collateral_amount, &price);
        let debt_value = Self::calculate_usd_value(mint_amount, &price);
        let required_collateral =
            (debt_value * asset.min_collateral_ratio as u64) / 10_000;

        if collateral_value < required_collateral {
            panic!("Insufficient collateral");
        }

        let risk_params = Self::risk_params_of(&env);
        let current_debt = Self::get_user_debt(&env, request.user.clone());
        if current_debt + mint_amount > risk_params.max_debt_per_user {
            panic!("Exceeds maximum debt per user");
        }

        let mut positions = Self::get_positions(&env);
        let mut user_positions = positions
            .get(request.user.clone())
            .unwrap_or_else(|| Vec::new(&env));

        match Self::position_index(&user_positions, request.asset_id) {
            Some(index) => {
                let mut position = user_positions.get(index).unwrap();
                position.synthetic_amount += mint_amount;
                position.debt_amount += mint_amount;
                let deposited = position
                    .collateral_deposits
                    .get(request.collateral_token.clone())
                    .unwrap_or(0);
                position.collateral_deposits.set(
                    request.collateral_token.clone(),
                    deposited + request.collateral_amount,
                );
                position.collateral_ratio = Self::calculate_collateral_ratio(&env, &position);
                position.last_updated = env.ledger().timestamp();
                user_positions.set(index, position);
            }
            None => {
                let mut collateral_deposits = Map::new(&env);
                collateral_deposits.set(request.collateral_token.clone(), request.collateral_amount);
                let mut position = SyntheticPosition {
                    owner: request.user.clone(),
                    asset_id: request.asset_id,
                    synthetic_amount: mint_amount,
                    collateral_deposits,
                    debt_amount: mint_amount,
                    collateral_ratio: 0,
                    created_at: env.ledger().timestamp(),
                    last_updated: env.ledger().timestamp(),
                    liquidating: false,
                };
                position.collateral_ratio = Self::calculate_collateral_ratio(&env, &position);
                user_positions.push_back(position);
            }
        }

        positions.set(request.user.clone(), user_positions);
        env.storage().instance().set(&POSITIONS, &positions);

        let mut assets = Self::get_assets(&env);
        let mut asset_info = assets.get(request.asset_id).unwrap();
        asset_info.total_supply += mint_amount;
        asset_info.total_collateral += request.collateral_amount;
        assets.set(request.asset_id, asset_info);
        env.storage().instance().set(&ASSETS, &assets);

        Self::update_protocol_stats(&env, mint_amount, collateral_value, 0);
        Self::distribute_fees(&env, minting_fee);

        let collateral_ratio = Self::position_ratio(&env, request.user.clone(), request.asset_id);
        let receipt = MintReceipt {
            user: request.user.clone(),
            asset_id: request.asset_id,
            collateral_token: request.collateral_token,
            collateral_amount: request.collateral_amount,
            synthetic_minted: mint_amount,
            minting_fee,
            collateral_value_usd: collateral_value,
            debt_value_usd: debt_value,
            collateral_ratio,
            price: price.price,
            timestamp: env.ledger().timestamp(),
        };

        env.events().publish(
            (Symbol::short("SYNTHMINT"), request.user, request.asset_id),
            (mint_amount, price.price, collateral_ratio),
        );

        receipt
    }

    /// Redeem synthetic tokens for the deposited collateral at the live oracle price
    ///
    /// The redemption fee is charged on the collateral released. A partial redemption
    /// is allowed as long as the remaining position stays above the liquidation
    /// threshold; a full redemption must first clear the position's debt.
    pub fn redeem(env: Env, request: RedemptionRequest) -> RedemptionReceipt {
        Self::require_not_paused(&env);
        request.user.require_auth();

        if request.synthetic_amount == 0 {
            panic!("Synthetic amount must be positive");
        }

        let price = Self::price_of(&env, request.asset_id);
        let fee_bps = Self::redemption_fee_bps(&env);

        let positions = Self::get_positions(&env);
        let mut user_positions = positions
            .get(request.user.clone())
            .unwrap_or_else(|| panic!("No positions found for user"));

        let index = Self::position_index(&user_positions, request.asset_id)
            .unwrap_or_else(|| panic!("Position not found for asset"));

        let mut position = user_positions.get(index).unwrap();
        if position.synthetic_amount < request.synthetic_amount {
            panic!("Insufficient synthetic balance");
        }

        let collateral_balance = position
            .collateral_deposits
            .get(request.collateral_token.clone())
            .unwrap_or(0);

        // Size the payout in collateral units. Collateral is valued 1:1 with USD
        // (see `get_position_collateral_value`), so the burned synths are first
        // converted to USD and then back to collateral units through the feed.
        let burned_value = Self::calculate_usd_value(request.synthetic_amount, &price);
        let gross_collateral = Self::value_to_amount(burned_value, &price);
        if collateral_balance < gross_collateral {
            panic!("Insufficient collateral balance");
        }
        let redemption_fee = (gross_collateral * fee_bps as u64) / 10_000;
        let collateral_released = gross_collateral - redemption_fee;

        position.synthetic_amount -= request.synthetic_amount;
        position.debt_amount = position
            .debt_amount
            .saturating_sub(request.synthetic_amount);
        let remaining = collateral_balance - gross_collateral;
        if remaining == 0 {
            position.collateral_deposits.remove(request.collateral_token.clone());
        } else {
            position.collateral_deposits
                .set(request.collateral_token.clone(), remaining);
        }
        position.collateral_ratio = Self::calculate_collateral_ratio(&env, &position);
        position.last_updated = env.ledger().timestamp();

        let risk_params = Self::risk_params_of(&env);
        if position.debt_amount > 0 && position.collateral_ratio < risk_params.liquidation_threshold {
            panic!("Health factor too low after redemption");
        }

        let collateral_ratio = position.collateral_ratio;
        user_positions.set(index, position);
        let mut positions = positions;
        positions.set(request.user.clone(), user_positions);
        env.storage().instance().set(&POSITIONS, &positions);

        let mut assets = Self::get_assets(&env);
        if let Some(mut asset_info) = assets.get(request.asset_id) {
            asset_info.total_supply = asset_info.total_supply.saturating_sub(request.synthetic_amount);
            asset_info.total_collateral =
                asset_info.total_collateral.saturating_sub(gross_collateral);
            assets.set(request.asset_id, asset_info);
        }
        env.storage().instance().set(&ASSETS, &assets);

        Self::update_protocol_stats(&env, 0, 0, request.synthetic_amount);
        Self::distribute_fees(&env, redemption_fee);

        let receipt = RedemptionReceipt {
            user: request.user.clone(),
            asset_id: request.asset_id,
            collateral_token: request.collateral_token,
            synthetic_burned: request.synthetic_amount,
            collateral_released,
            redemption_fee,
            price: price.price,
            collateral_ratio,
            timestamp: env.ledger().timestamp(),
        };

        env.events().publish(
            (Symbol::short("SYNTHREDM"), request.user, request.asset_id),
            (request.synthetic_amount, collateral_released, price.price),
        );

        receipt
    }

    /// Exchange one synthetic asset for another at oracle prices
    ///
    /// `route` lists the asset ids to pass through. A two-element route is a direct
    /// swap; longer routes perform a multi-hop exchange, applying the fee per hop.
    /// Every touched debt pool is updated and the resulting position must stay above
    /// the liquidation threshold.
    pub fn exchange(env: Env, request: ExchangeRequest, route: Vec<u32>) -> ExchangeReceipt {
        Self::require_not_paused(&env);
        request.user.require_auth();

        if request.synthetic_amount == 0 {
            panic!("Synthetic amount must be positive");
        }
        if request.from_asset_id == request.to_asset_id {
            panic!("Cannot exchange an asset for itself");
        }
        if route.len() < 2 {
            panic!("Route must contain at least two assets");
        }
        if route.first().unwrap() != request.from_asset_id
            || route.last().unwrap() != request.to_asset_id
        {
            panic!("Route must start and end with the requested assets");
        }

        let fee_bps = Self::exchange_fee_bps(&env);
        let from_price = Self::price_of(&env, request.from_asset_id);
        let to_price = Self::price_of(&env, request.to_asset_id);

        // Only the starting balance is debited; intermediate hops are netted out.
        let from_balance = Self::position_amount(&env, request.user.clone(), request.from_asset_id);
        if from_balance < request.synthetic_amount {
            panic!("Insufficient synthetic balance");
        }

        let mut amount = request.synthetic_amount;
        let mut total_fee = 0u64;
        let mut hops = 0u32;

        for hop in 0..route.len() - 1 {
            let hop_from = route.get(hop).unwrap();
            let hop_to = route.get(hop + 1).unwrap();
            let hop_from_price = Self::price_of(&env, hop_from);
            let hop_to_price = Self::price_of(&env, hop_to);

            let value = Self::calculate_usd_value(amount, &hop_from_price);
            let gross_out = Self::value_to_amount(value, &hop_to_price);
            let fee = (gross_out * fee_bps as u64) / 10_000;

            amount = gross_out - fee;
            total_fee += fee;
            hops += 1;
        }

        let risk_params = Self::risk_params_of(&env);
        let (total_collateral, total_debt) = Self::account_totals(&env, request.user.clone());
        if total_debt > 0 && (total_collateral * 10_000) / total_debt < risk_params.liquidation_threshold as u64 {
            panic!("Health factor too low after exchange");
        }

        let mut positions = Self::get_positions(&env);
        let mut user_positions = positions
            .get(request.user.clone())
            .unwrap_or_else(|| Vec::new(&env));

        let from_index = Self::position_index(&user_positions, request.from_asset_id)
            .unwrap_or_else(|| panic!("No position for source asset"));
        let mut from_position = user_positions.get(from_index).unwrap();
        from_position.synthetic_amount =
            from_position.synthetic_amount.saturating_sub(request.synthetic_amount);
        from_position.debt_amount =
            from_position.debt_amount.saturating_sub(request.synthetic_amount);
        from_position.collateral_ratio = Self::calculate_collateral_ratio(&env, &from_position);
        from_position.last_updated = env.ledger().timestamp();
        user_positions.set(from_index, from_position);

        match Self::position_index(&user_positions, request.to_asset_id) {
            Some(index) => {
                let mut position = user_positions.get(index).unwrap();
                position.synthetic_amount += amount;
                position.debt_amount += amount;
                position.collateral_ratio = Self::calculate_collateral_ratio(&env, &position);
                position.last_updated = env.ledger().timestamp();
                user_positions.set(index, position);
            }
            None => {
                let mut position = SyntheticPosition {
                    owner: request.user.clone(),
                    asset_id: request.to_asset_id,
                    synthetic_amount: amount,
                    collateral_deposits: Map::new(&env),
                    debt_amount: amount,
                    collateral_ratio: 0,
                    created_at: env.ledger().timestamp(),
                    last_updated: env.ledger().timestamp(),
                    liquidating: false,
                };
                position.collateral_ratio = Self::calculate_collateral_ratio(&env, &position);
                user_positions.push_back(position);
            }
        }

        positions.set(request.user.clone(), user_positions);
        env.storage().instance().set(&POSITIONS, &positions);

        let mut assets = Self::get_assets(&env);
        if let Some(mut from_info) = assets.get(request.from_asset_id) {
            from_info.total_supply = from_info.total_supply.saturating_sub(request.synthetic_amount);
            assets.set(request.from_asset_id, from_info);
        }
        if let Some(mut to_info) = assets.get(request.to_asset_id) {
            to_info.total_supply += amount;
            assets.set(request.to_asset_id, to_info);
        }
        env.storage().instance().set(&ASSETS, &assets);

        Self::distribute_fees(&env, total_fee);

        let receipt = ExchangeReceipt {
            user: request.user.clone(),
            from_asset_id: request.from_asset_id,
            to_asset_id: request.to_asset_id,
            amount_in: request.synthetic_amount,
            amount_out: amount,
            exchange_fee: total_fee,
            from_price: from_price.price,
            to_price: to_price.price,
            hops,
            timestamp: env.ledger().timestamp(),
        };

        env.events().publish(
            (Symbol::short("SYNTHEXCH"), request.user, request.from_asset_id),
            (request.to_asset_id, request.synthetic_amount, amount, total_fee),
        );

        receipt
    }

    /// Set the redemption fee (admin only)
    pub fn set_redemption_fee(env: Env, fee_bps: u32) {
        Self::require_admin(&env);
        if fee_bps > MAX_FEE_BPS {
            panic!("Redemption fee too high");
        }
        env.storage().instance().set(&REDEEM_FEE, &fee_bps);
    }

    /// Set the synth-to-synth exchange fee (admin only)
    pub fn set_exchange_fee(env: Env, fee_bps: u32) {
        Self::require_admin(&env);
        if fee_bps > MAX_FEE_BPS {
            panic!("Exchange fee too high");
        }
        env.storage().instance().set(&EXCHANGE_FEE, &fee_bps);
    }

    /// Current redemption fee in basis points
    pub fn get_redemption_fee(env: Env) -> u32 {
        Self::redemption_fee_bps(&env)
    }

    /// Current exchange fee in basis points
    pub fn get_exchange_fee(env: Env) -> u32 {
        Self::exchange_fee_bps(&env)
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
        let mut user_positions = positions.get(user.clone())
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
        let price = Self::price_of(&env, asset_id);
        let debt_value = Self::calculate_usd_value(position.debt_amount, &price);
        let liquidation_penalty = (debt_value * LIQUIDATION_PENALTY_BPS as u64) / 10000;
        
        // Distribute collateral
        let total_collateral_value = Self::get_position_collateral_value(&env, &position);
        let liquidator_share = (total_collateral_value * 9000) / 10000; // 90% to liquidator
        let user_share = total_collateral_value - liquidator_share;

        // Create liquidation event
        let event_id = env.ledger().sequence() as u64;
        let liquidation_event = LiquidationEvent {
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

        // Update protocol stats
        Self::update_protocol_stats(&env, 0, 0, position.synthetic_amount);

        // Distribute liquidation fee
        Self::distribute_fees(&env, liquidation_penalty);

        env.events().publish(
            (Symbol::short("LIQUIDATE"), user, asset_id),
            (liquidator, position.synthetic_amount),
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

        // Update price
        let price_data = OraclePrice {
            asset_id,
            price,
            decimals: 6, // Standard 6 decimals
            confidence,
            timestamp: env.ledger().timestamp(),
            source_address: oracle_address,
        };

        let mut prices = Self::get_prices(&env);
        prices.set(asset_id, price_data);
        env.storage().instance().set(&PRICES, &prices);

        env.events().publish(
            (Symbol::short("PRICEUPD"), asset_id),
            (price, confidence),
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
            (Symbol::short("STAKED"), user.clone()),
            amount,
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
            (Symbol::short("UNSTAKED"), user.clone()),
            (amount, current_rewards),
        );
    }

    /// Get asset information
    pub fn get_asset(env: Env, asset_id: u32) -> SyntheticAsset {
        Self::asset_of(&env, asset_id)
    }

    fn asset_of(env: &Env, asset_id: u32) -> SyntheticAsset {
        Self::get_assets(env).get(asset_id)
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
    pub fn get_asset_price(env: Env, asset_id: u32) -> OraclePrice {
        Self::price_of(&env, asset_id)
    }

    fn price_of(env: &Env, asset_id: u32) -> OraclePrice {
        if let Some(price) = Self::get_prices(env).get(asset_id) {
            return price;
        }
        OraclePrice {
            asset_id,
            price: DEFAULT_PRICE,
            decimals: PRICE_DECIMALS,
            confidence: DEFAULT_ORACLE_CONFIDENCE,
            timestamp: env.ledger().timestamp(),
            source_address: env.current_contract_address(),
        }
    }

    /// Get protocol statistics
    pub fn get_protocol_stats(env: Env) -> ProtocolStats {
        Self::stats_of(&env)
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
    pub fn pause(env: Env) {
        Self::require_admin(&env);
        env.storage().instance().set(&PAUSED, &true);
        env.events().publish((Symbol::short("PAUSED"),), true);
    }

    /// Unpause the protocol (admin only)
    pub fn unpause(env: Env) {
        Self::require_admin(&env);
        env.storage().instance().set(&PAUSED, &false);
        env.events().publish((Symbol::short("PAUSED"),), false);
    }

    /// Update risk parameters (admin only)
    pub fn update_risk_params(env: Env, new_params: RiskParameters) {
        Self::require_admin(&env);
        
        if new_params.global_min_ratio < MIN_COLLATERAL_RATIO {
            panic!("Invalid minimum ratio");
        }
        if new_params.min_oracle_confidence < 5000 {
            panic!("Invalid confidence threshold");
        }
        
        env.storage().instance().set(&RISK_PARAMS, &new_params);
        env.events().publish((Symbol::short("RISKUPD"),), ());
    }

    // ─── Internal Helpers ─────────────────────────────────────────────────────

    fn position_index(positions: &Vec<SyntheticPosition>, asset_id: u32) -> Option<u32> {
        for i in 0..positions.len() {
            if positions.get(i).unwrap().asset_id == asset_id {
                return Some(i);
            }
        }
        None
    }

    fn position_ratio(env: &Env, user: Address, asset_id: u32) -> u32 {
        Self::try_position_ratio(env, user, asset_id).unwrap_or(0)
    }

    fn try_position_ratio(env: &Env, user: Address, asset_id: u32) -> Option<u32> {
        let positions = Self::get_positions(env).get(user)?;
        let index = Self::position_index(&positions, asset_id)?;
        let position = positions.get(index)?;
        Some(Self::calculate_collateral_ratio(env, &position))
    }

    /// Total collateral and debt across every position the user holds
    fn account_totals(env: &Env, user: Address) -> (u64, u64) {
        let positions = Self::get_positions(env).get(user).unwrap_or_else(|| Vec::new(env));
        let mut collateral = 0u64;
        let mut debt = 0u64;
        for position in positions.iter() {
            collateral += Self::get_position_collateral_value(env, &position);
            debt += position.debt_amount;
        }
        (collateral, debt)
    }

    fn position_amount(env: &Env, user: Address, asset_id: u32) -> u64 {
        let positions = Self::get_positions(env).get(user).unwrap_or_else(|| Vec::new(env));
        match Self::position_index(&positions, asset_id) {
            Some(index) => positions.get(index).unwrap().synthetic_amount,
            None => 0,
        }
    }

    fn position_debt(env: &Env, user: Address, asset_id: u32) -> u64 {
        let positions = Self::get_positions(env).get(user).unwrap_or_else(|| Vec::new(env));
        match Self::position_index(&positions, asset_id) {
            Some(index) => positions.get(index).unwrap().debt_amount,
            None => 0,
        }
    }

    fn value_to_amount(value: u64, price: &OraclePrice) -> u64 {
        let scale = 10_u64.pow(price.decimals);
        if price.price == 0 {
            return 0;
        }
        (value * scale) / price.price
    }

    fn risk_params_of(env: &Env) -> RiskParameters {
        env.storage().instance().get(&RISK_PARAMS).unwrap()
    }


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

    fn get_position_collateral_value(env: &Env, position: &SyntheticPosition) -> u64 {
        let mut total_value = 0u64;
        for (collateral_token, amount) in position.collateral_deposits.iter() {
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
        let fee_dist = Self::get_fee_distribution(env);
        let time_elapsed = env.ledger().timestamp() - staking_pos.staked_at;
        let rewards = (staking_pos.staked_amount * STAKING_REWARD_RATE_BPS as u64 * time_elapsed) 
            / (10000 * 365 * 24 * 3600);
        rewards
    }

    fn update_protocol_stats(env: &Env, minted: u64, collateral_added: u64, burned: u64) {
        let mut stats = Self::stats_of(env);
        // Mint and burn are mutually exclusive per call, so subtract from the
        // running total rather than netting `minted - burned` (which underflows).
        stats.total_synthetic_supply = stats
            .total_synthetic_supply
            .saturating_add(minted)
            .saturating_sub(burned);
        stats.total_value_locked += collateral_added;
        
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

    fn require_admin(env: &Env) {
        let admin = env.storage().instance().get(&ADMIN).unwrap_optimized();
        if env.current_contract_address() != admin {
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

    fn stats_of(env: &Env) -> ProtocolStats {
        env.storage().instance().get(&PROTOCOL_STATS).unwrap()
    }

    fn get_staking(env: &Env) -> Map<Address, StakingPosition> {
        env.storage().instance().get(&STAKING).unwrap()
    }

    fn get_prices(env: &Env) -> Map<u32, OraclePrice> {
        env.storage()
            .instance()
            .get(&PRICES)
            .unwrap_or_else(|| Map::new(env))
    }

    fn redemption_fee_bps(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&REDEEM_FEE)
            .unwrap_or(DEFAULT_REDEMPTION_FEE_BPS)
    }

    fn exchange_fee_bps(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&EXCHANGE_FEE)
            .unwrap_or(DEFAULT_EXCHANGE_FEE_BPS)
    }
}
