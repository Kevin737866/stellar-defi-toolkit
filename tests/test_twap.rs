#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env, Symbol, symbol_short};
use stellar_defi_toolkit::contracts::price_oracle::{
    PriceOracleContract, PriceOracleContractClient
};

fn setup_test(env: &Env) -> (PriceOracleContractClient<'static>, Address) {
    let contract_id = env.register_contract(None, PriceOracleContract);
    let client = PriceOracleContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    (client, admin)
}

#[test]
fn test_twap_calculation() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup_test(&env);

    let source = Address::generate(&env);
    client.add_price_source(&source, &symbol_short!("SRC1"), &10000);

    let asset = Address::generate(&env);

    // Initial price
    client.update_price(&source, &asset, &1000_000, &6);

    // Fast forward 1 hour (3600 seconds)
    env.ledger().with_mut(|li| {
        li.timestamp += 3600;
    });

    // Update price to 2000_000
    // To bypass the update threshold for testing without changing config,
    // we just use a sufficiently large price difference.
    client.update_price(&source, &asset, &2000_000, &6);

    // Fast forward 1 hour (3600 seconds)
    env.ledger().with_mut(|li| {
        li.timestamp += 3600;
    });

    // We have 1hr at 1000_000 and 1hr at 2000_000
    let twap = client.get_twap(&asset, &7200);

    // Expected TWAP over 2 hours is 1500_000
    assert_eq!(twap.price, 1500_000);
}
