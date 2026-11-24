//! Tests for issue #172: stability fee auction for governance participation.
//!
//! Covers bid submission, winning rate installation, treasury funding,
//! min/max bounds, auction events and the governance interface.

use soroban_sdk::testutils::{Address as _, Events as _, Ledger};
use soroban_sdk::{Address, Env, String, Symbol, TryFromVal, Val};
use stellar_defi_toolkit::contracts::fee_auction_governance::{
    FeeAuctionGovernanceContract, FeeAuctionGovernanceContractClient, StabilityFeeAuctionGovernance,
};
use stellar_defi_toolkit::contracts::stablecoin::{StablecoinContract, StablecoinContractClient};

const MIN_FEE_BPS: u32 = 100;
const MAX_FEE_BPS: u32 = 1000;
const AUCTION_DURATION: u64 = 100;
const FEE_PERIOD: u64 = 200;
const MINIMUM_BID: u64 = 50;

fn setup(env: &Env) -> (Address, StablecoinContractClient<'_>, Address, Address) {
    let admin = Address::generate(env);
    let treasury = Address::generate(env);
    let oracle = Address::generate(env);
    let contract_id = env.register_contract(None, StablecoinContract);
    let client = StablecoinContractClient::new(env, &contract_id);

    client.initialize(
        &admin,
        &String::from_str(env, "Stable USD"),
        &String::from_str(env, "SUSD"),
        &oracle,
    );
    client.configure_fee_auction(
        &admin,
        &treasury,
        &MIN_FEE_BPS,
        &MAX_FEE_BPS,
        &AUCTION_DURATION,
        &FEE_PERIOD,
        &MINIMUM_BID,
    );

    (admin, client, treasury, oracle)
}

fn open_auction(env: &Env, client: &StablecoinContractClient<'_>) -> u64 {
    let start = env.ledger().timestamp();
    let auction_id = client.start_fee_auction();
    assert_eq!(auction_id, 1);
    let auction = client.get_active_fee_auction().unwrap();
    assert_eq!(auction.auction_id, auction_id);
    assert_eq!(auction.end_time, start + AUCTION_DURATION);
    assert!(auction.bidder.is_none());
    auction_id
}

fn has_event(env: &Env, name: &str) -> bool {
    let expected = Symbol::new(env, name);
    env.events().all().iter().any(|(_, topics, _)| {
        topics.iter().any(|topic: Val| {
            Symbol::try_from_val(env, &topic)
                .map(|topic: Symbol| topic == expected)
                .unwrap_or(false)
        })
    })
}

#[test]
fn winning_bid_sets_rate_and_funds_treasury() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, client, _treasury, _oracle) = setup(&env);
    let bidder = Address::generate(&env);
    let auction_id = open_auction(&env, &client);

    client.bid_stability_fee(&bidder, &400, &1_000);
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + AUCTION_DURATION);

    let fee = client.settle_fee_auction();
    assert_eq!(fee, 400);
    assert_eq!(client.get_current_stability_fee(), 400);
    assert_eq!(client.get_treasury_fee_proceeds(), 1_000);
    assert_eq!(
        client.get_stability_fee_period_end(),
        env.ledger().timestamp() + FEE_PERIOD
    );
    assert!(client.get_active_fee_auction().is_none());

    let settled = client.get_fee_auction(&auction_id);
    assert!(settled.settled);
    assert_eq!(settled.bid_amount, 1_000);
    assert_eq!(settled.bidder, Some(bidder));
}

#[test]
fn fee_bounds_are_enforced_on_every_bid() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, client, _treasury, _oracle) = setup(&env);
    let bidder = Address::generate(&env);
    open_auction(&env, &client);

    assert!(client
        .try_bid_stability_fee(&bidder, &(MIN_FEE_BPS - 1), &1_000)
        .is_err());
    assert!(client
        .try_bid_stability_fee(&bidder, &(MAX_FEE_BPS + 1), &1_000)
        .is_err());
    assert!(client
        .try_bid_stability_fee(&bidder, &500, &(MINIMUM_BID - 1))
        .is_err());
    assert!(client.try_bid_stability_fee(&bidder, &500, &1_000).is_ok());
}

#[test]
fn later_bid_must_exceed_current_bid() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, client, _treasury, _oracle) = setup(&env);
    let first = Address::generate(&env);
    let second = Address::generate(&env);
    open_auction(&env, &client);

    client.bid_stability_fee(&first, &500, &1_000);
    assert!(client.try_bid_stability_fee(&second, &600, &1_000).is_err());
    assert!(client.try_bid_stability_fee(&second, &600, &999).is_err());
    client.bid_stability_fee(&second, &700, &2_000);

    let auction = client.get_active_fee_auction().unwrap();
    assert_eq!(auction.bidder, Some(second));
    assert_eq!(auction.fee_bps, 700);
    assert_eq!(auction.bid_amount, 2_000);
}

#[test]
fn settlement_requires_expiry_and_a_bid() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, client, _treasury, _oracle) = setup(&env);
    let bidder = Address::generate(&env);
    open_auction(&env, &client);

    assert!(client.try_settle_fee_auction().is_err());
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + AUCTION_DURATION);
    assert!(client.try_settle_fee_auction().is_err());

    // A second auction can only start after the first is resolved.
    assert!(client.try_start_fee_auction().is_err());
    client.cancel_fee_auction(&_admin);
    assert_eq!(client.start_fee_auction(), 2);
    client.bid_stability_fee(&bidder, &250, &500);
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + AUCTION_DURATION);
    assert_eq!(client.settle_fee_auction(), 250);
}

#[test]
fn non_admin_cannot_reconfigure_auction() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, client, treasury, _oracle) = setup(&env);
    let intruder = Address::generate(&env);

    assert!(client
        .try_configure_fee_auction(
            &intruder,
            &treasury,
            &MAX_FEE_BPS,
            &(MAX_FEE_BPS + 1),
            &AUCTION_DURATION,
            &FEE_PERIOD,
            &MINIMUM_BID,
        )
        .is_err());
    assert_eq!(client.get_fee_auction_config().max_fee_bps, MAX_FEE_BPS);
}

#[test]
fn auction_events_are_published() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, client, _treasury, _oracle) = setup(&env);
    let bidder = Address::generate(&env);

    assert!(has_event(&env, "FEE_AUCTION_CONFIGURED"));
    open_auction(&env, &client);
    assert!(has_event(&env, "FEE_AUCTION_STARTED"));
    client.bid_stability_fee(&bidder, &500, &1_000);
    assert!(has_event(&env, "FEE_AUCTION_BID"));
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + AUCTION_DURATION);
    client.settle_fee_auction();
    assert!(has_event(&env, "FEE_AUCTION_SETTLED"));
    assert!(has_event(&env, "STABILITY_FEE_UPDATED"));
}

#[test]
fn governance_controls_auction_and_reads_fee() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, stablecoin_client, treasury, _oracle) = setup(&env);

    let gov_id = env.register_contract(None, FeeAuctionGovernanceContract);
    let gov = FeeAuctionGovernanceContractClient::new(&env, &gov_id);
    gov.initialize(&admin, &stablecoin_client.address);

    gov.configure_auction(
        &admin,
        &treasury,
        &MIN_FEE_BPS,
        &MAX_FEE_BPS,
        &AUCTION_DURATION,
        &FEE_PERIOD,
        &MINIMUM_BID,
    );
    assert_eq!(gov.get_auction_bounds(), (MIN_FEE_BPS, MAX_FEE_BPS));
    assert_eq!(gov.get_current_fee(), 500);

    let auction_id = gov.start_auction(&admin);
    assert_eq!(auction_id, 1);
    let bidder = Address::generate(&env);
    stablecoin_client.bid_stability_fee(&bidder, &750, &2_000);
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + AUCTION_DURATION);

    assert_eq!(gov.settle_auction(&admin), 750);
    assert_eq!(gov.get_current_fee(), 750);
    assert_eq!(stablecoin_client.get_treasury_fee_proceeds(), 2_000);
}

#[test]
fn trait_interface_exposes_auction_controls() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, client, _treasury, _oracle) = setup(&env);

    let contract_id = client.address.clone();
    assert_eq!(
        env.as_contract(&contract_id, || {
            StablecoinContract::fee_auction_bounds(&env)
        }),
        (MIN_FEE_BPS, MAX_FEE_BPS)
    );
    assert_eq!(
        env.as_contract(&contract_id, || {
            StablecoinContract::current_stability_fee(&env)
        }),
        500
    );
    assert_eq!(
        env.as_contract(&contract_id, || {
            <StablecoinContract as StabilityFeeAuctionGovernance>::start_fee_auction(&env)
        }),
        1
    );

    let bidder = Address::generate(&env);
    client.bid_stability_fee(&bidder, &900, &300);
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + AUCTION_DURATION);
    assert_eq!(
        env.as_contract(&contract_id, || {
            <StablecoinContract as StabilityFeeAuctionGovernance>::settle_fee_auction(&env)
        }),
        900
    );
    assert_eq!(
        env.as_contract(&contract_id, || {
            StablecoinContract::current_stability_fee(&env)
        }),
        900
    );
}
