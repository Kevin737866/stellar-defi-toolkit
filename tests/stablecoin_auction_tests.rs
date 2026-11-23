use soroban_sdk::{Address, Env, String as SorobanString, Symbol};
use stellar_defi_toolkit::contracts::stablecoin::{StablecoinContract, StablecoinContractClient};
use stellar_defi_toolkit::types::stablecoin::{AuctionKind, CollateralType};

fn initialize<'env>(env: &'env Env, admin: &Address) -> (Address, StablecoinContractClient<'env>) {
    let contract_id = env.register_contract(None, StablecoinContract);
    let client = StablecoinContractClient::new(env, &contract_id);
    client.initialize(
        admin,
        &SorobanString::from_str(env, "Stable USD"),
        &SorobanString::from_str(env, "SUSD"),
        &Address::generate(env),
    );
    (contract_id, client)
}

#[test]
fn fee_surplus_opens_and_settles_surplus_auction() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, StablecoinContract);
    let client = StablecoinContractClient::new(&env, &contract_id);
    client.initialize(
        &contract_id,
        &SorobanString::from_str(&env, "Stable USD"),
        &SorobanString::from_str(&env, "SUSD"),
        &Address::generate(&env),
    );

    let collateral = Address::generate(&env);
    client.add_collateral(
        &collateral,
        &CollateralType::Other(Symbol::short("COLL")),
        &15_000,
        &50_000,
    );

    for _ in 0..2 {
        client.mint(
            &Address::generate(&env),
            &collateral,
            &20_000_000_000,
            &10_000_000_000,
        );
    }

    assert_eq!(client.fee_surplus(), 100_000_000);
    let auction = client.get_surplus_auction().unwrap();
    assert_eq!(auction.kind, AuctionKind::Surplus);
    assert_eq!(auction.lot_amount, 100_000_000);
    assert_eq!(auction.start_price, 120_000_000);
    assert_eq!(auction.minimum_price, 80_000_000);
    assert_eq!(auction.minimum_bid, 1_000_000);
    assert_eq!(auction.duration, 3_600);

    let bidder = Address::generate(&env);
    client.bid_surplus_auction(&bidder, &auction.start_price);

    assert_eq!(client.fee_surplus(), 0);
    assert!(client.get_surplus_auction().is_none());
}

#[test]
fn debt_auction_triggers_only_above_insurance_and_price_descends() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let (_, client) = initialize(&env, &admin);

    client.contribute_insurance(&Address::generate(&env), &50_000_000);
    client.record_bad_debt(&40_000_000);
    assert!(client.get_debt_auction().is_none());

    client.record_bad_debt(&60_000_000);
    let auction = client.get_debt_auction().unwrap();
    assert_eq!(auction.kind, AuctionKind::Debt);
    assert_eq!(auction.lot_amount, 50_000_000);
    assert_eq!(auction.minimum_bid, 1_000_000);
    assert_eq!(client.debt_auction_price(), Some(60_000_000));

    env.ledger().set_timestamp(1_800);
    let halfway_price = client.debt_auction_price().unwrap();
    assert_eq!(halfway_price, 50_000_000);
    assert!(halfway_price < auction.start_price);
    assert!(halfway_price > auction.minimum_price);

    client.bid_debt_auction(&Address::generate(&env), &halfway_price);
    assert_eq!(client.bad_debt(), 50_000_000);
    assert!(client.get_debt_auction().is_none());
}

#[test]
fn expired_debt_auction_restarts_with_a_fresh_duration() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let (_, client) = initialize(&env, &admin);

    client.contribute_insurance(&Address::generate(&env), &10_000_000);
    client.record_bad_debt(&20_000_000);

    env.ledger().set_timestamp(3_601);
    client.expire_debt_auction();

    let relisted = client.get_debt_auction().unwrap();
    assert_eq!(relisted.lot_amount, 10_000_000);
    assert_eq!(relisted.started_at, 3_601);
    assert_eq!(relisted.duration, 3_600);
}
