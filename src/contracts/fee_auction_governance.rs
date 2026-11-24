//! Governance interface for the stablecoin stability fee auction.
//!
//! Governance token holders bid directly on `StablecoinContract`; this module
//! exposes the controls governance needs (bounds, opening and settling) and a
//! read-only view of the fee currently in force.

use soroban_sdk::{contract, contractimpl, Address, Env, Symbol};

use crate::contracts::stablecoin::{StablecoinContract, StablecoinContractClient};

// ─── Storage Keys ─────────────────────────────────────────────────────────────

const FEE_AUCTION_GOV_ADMIN: Symbol = Symbol::short("FAGADM");
const FEE_AUCTION_STABLECOIN: Symbol = Symbol::short("FAGSTB");

// ─── Interface ────────────────────────────────────────────────────────────────

/// Governance-facing view of the stablecoin's stability fee auction.
///
/// Implemented by `StablecoinContract` so governance code and clients can
/// drive auctions without duplicating the fee-bounds logic.
pub trait StabilityFeeAuctionGovernance {
    /// Fee currently in force, in basis points
    fn current_stability_fee(env: &Env) -> u32;
    /// Configured `(min, max)` fee bounds in basis points
    fn fee_auction_bounds(env: &Env) -> (u32, u32);
    /// Open an auction, returning its id
    fn start_fee_auction(env: &Env) -> u64;
    /// Settle an expired auction, returning the installed fee
    fn settle_fee_auction(env: &Env) -> u32;
}

impl StabilityFeeAuctionGovernance for StablecoinContract {
    fn current_stability_fee(env: &Env) -> u32 {
        StablecoinContract::get_current_stability_fee(env.clone())
    }

    fn fee_auction_bounds(env: &Env) -> (u32, u32) {
        let config = StablecoinContract::get_fee_auction_config(env.clone());
        (config.min_fee_bps, config.max_fee_bps)
    }

    fn start_fee_auction(env: &Env) -> u64 {
        StablecoinContract::start_fee_auction(env.clone())
    }

    fn settle_fee_auction(env: &Env) -> u32 {
        StablecoinContract::settle_fee_auction(env.clone())
    }
}

// ─── Governance Controller ───────────────────────────────────────────────────

/// Thin governance controller for the stablecoin's fee auction.
///
/// Token holders bid directly on the stablecoin contract; governance controls
/// the auction bounds and may open or settle auctions on the protocol's behalf.
#[contract]
pub struct FeeAuctionGovernanceContract;

#[contractimpl]
impl FeeAuctionGovernanceContract {
    /// Bind this controller to a stablecoin contract (admin only).
    pub fn initialize(env: Env, admin: Address, stablecoin: Address) {
        if env.storage().instance().has(&FEE_AUCTION_GOV_ADMIN) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&FEE_AUCTION_GOV_ADMIN, &admin);
        env.storage()
            .instance()
            .set(&FEE_AUCTION_STABLECOIN, &stablecoin);
        env.events().publish(
            (Symbol::new(&env, "FEE_AUCTION_GOV_INITIALIZED"), admin),
            stablecoin,
        );
    }

    /// Set the auction bounds and timing on the stablecoin (admin only).
    pub fn configure_auction(
        env: Env,
        admin: Address,
        treasury: Address,
        min_fee_bps: u32,
        max_fee_bps: u32,
        auction_duration: u64,
        fee_period: u64,
        minimum_bid: u64,
    ) {
        admin.require_auth();
        let expected: Address = env
            .storage()
            .instance()
            .get(&FEE_AUCTION_GOV_ADMIN)
            .unwrap();
        if admin != expected {
            panic!("Not authorized");
        }

        let stablecoin: Address = env
            .storage()
            .instance()
            .get(&FEE_AUCTION_STABLECOIN)
            .unwrap();
        StablecoinContractClient::new(&env, &stablecoin).configure_fee_auction(
            &admin,
            &treasury,
            &min_fee_bps,
            &max_fee_bps,
            &auction_duration,
            &fee_period,
            &minimum_bid,
        );

        env.events().publish(
            (Symbol::new(&env, "FEE_AUCTION_GOV_CONFIGURED"), admin),
            (treasury, min_fee_bps, max_fee_bps),
        );
    }

    /// Open an auction through governance.
    pub fn start_auction(env: Env, caller: Address) -> u64 {
        caller.require_auth();
        let stablecoin: Address = env
            .storage()
            .instance()
            .get(&FEE_AUCTION_STABLECOIN)
            .unwrap();
        let auction_id = StablecoinContractClient::new(&env, &stablecoin).start_fee_auction();
        env.events().publish(
            (Symbol::new(&env, "FEE_AUCTION_GOV_STARTED"), caller),
            auction_id,
        );
        auction_id
    }

    /// Settle an expired auction through governance.
    pub fn settle_auction(env: Env, caller: Address) -> u32 {
        caller.require_auth();
        let stablecoin: Address = env
            .storage()
            .instance()
            .get(&FEE_AUCTION_STABLECOIN)
            .unwrap();
        let fee_bps = StablecoinContractClient::new(&env, &stablecoin).settle_fee_auction();
        env.events().publish(
            (Symbol::new(&env, "FEE_AUCTION_GOV_SETTLED"), caller),
            fee_bps,
        );
        fee_bps
    }

    /// Read the fee currently in force.
    pub fn get_current_fee(env: Env) -> u32 {
        let stablecoin: Address = env
            .storage()
            .instance()
            .get(&FEE_AUCTION_STABLECOIN)
            .unwrap();
        StablecoinContractClient::new(&env, &stablecoin).get_current_stability_fee()
    }

    /// Read the auction bounds configured on the stablecoin.
    pub fn get_auction_bounds(env: Env) -> (u32, u32) {
        let stablecoin: Address = env
            .storage()
            .instance()
            .get(&FEE_AUCTION_STABLECOIN)
            .unwrap();
        let config = StablecoinContractClient::new(&env, &stablecoin).get_fee_auction_config();
        (config.min_fee_bps, config.max_fee_bps)
    }
}
