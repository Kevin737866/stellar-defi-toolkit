#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env, Vec, symbol_short};
use stellar_defi_toolkit::contracts::multi_asset_oracle::{
    MultiAssetOracleContract, MultiAssetOracleContractClient
};
use stellar_defi_toolkit::types::asset::{AssetPrice, StellarAssetId};

fn setup_test(env: &Env) -> (MultiAssetOracleContractClient<'static>, Address) {
    let contract_id = env.register_contract(None, MultiAssetOracleContract);
    let client = MultiAssetOracleContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let registry = Address::generate(env);
    client.initialize(&admin, &registry);
    (client, admin)
}

#[test]
fn test_batch_price_submission() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup_test(&env);

    let mut prices = Vec::new(&env);
    let source = Address::generate(&env);

    for i in 0..10 {
        let asset = StellarAssetId::Custom(symbol_short!("TOKEN"));
        let price = AssetPrice {
            asset_id: asset.clone(),
            price: 1000_000 + i,
            decimals: 6,
            confidence: 10000,
            timestamp: env.ledger().timestamp(),
            source: source.clone(),
            price_change_24h: 0,
            high_24h: 1000_000,
            low_24h: 1000_000,
            volume_24h: 0,
        };
        prices.push_back((asset, price));
    }

    client.submit_batch_prices(&prices);

    let retrieved = client.get_price(&StellarAssetId::Custom(symbol_short!("TOKEN")));
    assert!(retrieved.price >= 1000_000);
}

#[test]
#[should_panic(expected = "Batch size exceeds maximum of 50")]
fn test_batch_price_submission_exceeds_max() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup_test(&env);

    let mut prices = Vec::new(&env);
    let source = Address::generate(&env);

    for _ in 0..51 {
        let asset = StellarAssetId::Custom(symbol_short!("TOKEN"));
        let price = AssetPrice {
            asset_id: asset.clone(),
            price: 1000_000,
            decimals: 6,
            confidence: 10000,
            timestamp: env.ledger().timestamp(),
            source: source.clone(),
            price_change_24h: 0,
            high_24h: 1000_000,
            low_24h: 1000_000,
            volume_24h: 0,
        };
        prices.push_back((asset, price));
    }

    client.submit_batch_prices(&prices);
}
