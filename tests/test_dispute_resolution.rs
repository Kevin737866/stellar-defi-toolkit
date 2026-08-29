#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env, symbol_short};
use stellar_defi_toolkit::contracts::decentralized_oracle::{
    DecentralizedOracle, DecentralizedOracleClient, DisputeStatus, DecentralizedOracleError, DisputeRecord
};

fn setup_test(env: &Env) -> (DecentralizedOracleClient<'static>, Address) {
    let contract_id = env.register_contract(None, DecentralizedOracle);
    let client = DecentralizedOracleClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    (client, admin)
}

#[test]
fn test_open_dispute() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup_test(&env);

    let oracle = Address::generate(&env);
    // register oracle first
    client.register_oracle(&oracle, &1_000_000);

    let challenger = Address::generate(&env);
    let asset_id = 1u32;
    let bond_amount = 500_000u64;

    let dispute_id = client.open_dispute(&challenger, &oracle, &asset_id, &bond_amount);
    assert_eq!(dispute_id, 0);
}

#[test]
fn test_resolve_dispute_valid() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup_test(&env);

    let oracle = Address::generate(&env);
    client.register_oracle(&oracle, &1_000_000);

    let challenger = Address::generate(&env);
    let dispute_id = client.open_dispute(&challenger, &oracle, &1u32, &500_000);
    
    // Fast forward time to exceed DISPUTE_REVIEW_PERIOD (172800)
    env.ledger().with_mut(|li| {
        li.timestamp += 172801;
    });

    client.resolve_dispute(&admin, &dispute_id, &true);

    let new_stake = client.get_oracle_stake(&oracle);
    // It should be slashed by 10%. 1_000_000 - 100_000 = 900_000
    assert_eq!(new_stake, Ok(900_000));
}

#[test]
fn test_resolve_dispute_invalid() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup_test(&env);

    let oracle = Address::generate(&env);
    client.register_oracle(&oracle, &1_000_000);

    let challenger = Address::generate(&env);
    let dispute_id = client.open_dispute(&challenger, &oracle, &1u32, &500_000);
    
    // Fast forward time to exceed DISPUTE_REVIEW_PERIOD (172800)
    env.ledger().with_mut(|li| {
        li.timestamp += 172801;
    });

    client.resolve_dispute(&admin, &dispute_id, &false);

    let new_stake = client.get_oracle_stake(&oracle);
    // Should gain the challenger's bond (500_000)
    assert_eq!(new_stake, Ok(1_500_000));
}

#[test]
#[should_panic(expected = "DisputeReviewActive")]
fn test_resolve_dispute_too_early() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup_test(&env);

    let oracle = Address::generate(&env);
    client.register_oracle(&oracle, &1_000_000);

    let challenger = Address::generate(&env);
    let dispute_id = client.open_dispute(&challenger, &oracle, &1u32, &500_000);
    
    // Resolving immediately should fail since review period is 48h
    client.resolve_dispute(&admin, &dispute_id, &true);
}
