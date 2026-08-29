#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Bytes, Env, Symbol, symbol_short};
use stellar_defi_toolkit::contracts::multi_asset_oracle::{
    MultiAssetOracleContract, MultiAssetOracleContractClient
};
use stellar_defi_toolkit::contracts::price_feed_adapters::BridgeRelayConfig;
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
fn test_cross_chain_update_valid() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup_test(&env);

    let asset = StellarAssetId::Native;
    let price = AssetPrice {
        asset_id: asset.clone(),
        price: 1500_000,
        decimals: 6,
        confidence: 10000,
        timestamp: env.ledger().timestamp(),
        source: Address::generate(&env),
        price_change_24h: 0,
        high_24h: 1500_000,
        low_24h: 1500_000,
        volume_24h: 0,
    };

    let config = BridgeRelayConfig {
        provider: symbol_short!("PYTH"),
        expected_chain: symbol_short!("SOLANA"),
        max_staleness: 60,
    };

    let mut payload_buf = [0u8; 10];
    let mut sig_buf = [1u8; 10];
    let payload = Bytes::from_slice(&env, &payload_buf);
    let signature = Bytes::from_slice(&env, &sig_buf);

    client.update_cross_chain_price(
        &asset,
        &price,
        &payload,
        &signature,
        &symbol_short!("SOLANA"),
        &config,
    );

    let retrieved_price = client.get_price(&asset);
    assert_eq!(retrieved_price.price, 1500_000);
}

#[test]
#[should_panic(expected = "Invalid cross-chain payload or signature")]
fn test_cross_chain_update_stale() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup_test(&env);

    let asset = StellarAssetId::Native;
    let config = BridgeRelayConfig {
        provider: symbol_short!("PYTH"),
        expected_chain: symbol_short!("SOLANA"),
        max_staleness: 60,
    };

    // Fast forward ledger time
    env.ledger().with_mut(|li| {
        li.timestamp = 200;
    });

    let price = AssetPrice {
        asset_id: asset.clone(),
        price: 1500_000,
        decimals: 6,
        confidence: 10000,
        timestamp: 100, // 100 seconds ago, staleness threshold is 60
        source: Address::generate(&env),
        price_change_24h: 0,
        high_24h: 1500_000,
        low_24h: 1500_000,
        volume_24h: 0,
    };

    let mut payload_buf = [0u8; 10];
    let mut sig_buf = [1u8; 10];
    let payload = Bytes::from_slice(&env, &payload_buf);
    let signature = Bytes::from_slice(&env, &sig_buf);

    client.update_cross_chain_price(
        &asset,
        &price,
        &payload,
        &signature,
        &symbol_short!("SOLANA"),
        &config,
    );
}
