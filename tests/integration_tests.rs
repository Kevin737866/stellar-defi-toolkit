//! Integration tests for Stellar DeFi Toolkit

use stellar_defi_toolkit::{
    TokenContract, LiquidityPoolContract, StakingContract, StellarClient,
};
use tokio_test;

#[tokio::test]
async fn test_token_deployment() {
    let client = StellarClient::new().await.unwrap();
    
    let token = TokenContract::new(
        "Test Token".to_string(),
        "TEST".to_string(),
        1000000,
    );
    
    // Test token creation
    let info = token.get_info();
    assert_eq!(info.name, "Test Token");
    assert_eq!(info.symbol, "TEST");
    assert_eq!(info.total_supply, 1000000);
    assert_eq!(info.decimals, 7);
    
    // Test deployment (mock)
    let contract_id = token.deploy(&client).await.unwrap();
    assert!(!contract_id.is_empty());
    assert!(contract_id.starts_with("TOKEN_CONTRACT_"));
}

#[tokio::test]
async fn test_liquidity_pool_deployment() {
    let client = StellarClient::new().await.unwrap();
    
    let pool = LiquidityPoolContract::new(
        "TOKEN_A_CONTRACT".to_string(),
        "TOKEN_B_CONTRACT".to_string(),
    );
    
    // Test pool creation
    let info = pool.get_info();
    assert_eq!(info.token_a, "TOKEN_A_CONTRACT");
    assert_eq!(info.token_b, "TOKEN_B_CONTRACT");
    assert_eq!(info.reserve_a, 0);
    assert_eq!(info.reserve_b, 0);
    assert_eq!(info.total_liquidity, 0);
    assert_eq!(info.fee_percentage, 30);
    
    // Test deployment (mock)
    let contract_id = pool.deploy(&client).await.unwrap();
    assert!(!contract_id.is_empty());
    assert!(contract_id.starts_with("POOL_CONTRACT_"));
}

#[tokio::test]
async fn test_staking_contract_deployment() {
    let client = StellarClient::new().await.unwrap();
    
    let staking = StakingContract::new(
        "STAKING_TOKEN".to_string(),
        "REWARD_TOKEN".to_string(),
        1000,
    );
    
    // Test staking contract creation
    let info = staking.get_info();
    assert_eq!(info.staking_token, "STAKING_TOKEN");
    assert_eq!(info.reward_token, "REWARD_TOKEN");
    assert_eq!(info.reward_rate, 1000);
    assert_eq!(info.total_staked, 0);
    
    // Test deployment (mock)
    let contract_id = staking.deploy(&client).await.unwrap();
    assert!(!contract_id.is_empty());
    assert!(contract_id.starts_with("STAKING_CONTRACT_"));
}

#[tokio::test]
async fn test_contract_info_retrieval() {
    let client = StellarClient::new().await.unwrap();
    
    // Test getting contract info
    let contract_id = "TEST_CONTRACT_ID";
    let info = client.get_contract_info(contract_id).await.unwrap();
    
    assert_eq!(info["contract_id"], contract_id);
    assert!(info.contains_key("network"));
    assert!(info.contains_key("horizon_url"));
    assert!(info.contains_key("status"));
}

#[tokio::test]
async fn test_account_operations() {
    let client = StellarClient::new().await.unwrap();
    
    let public_key = "GABCDEFGHIJKLMNOPQRSTUVWXYZ123456789";
    let account_info = client.get_account(public_key).await.unwrap();
    
    assert_eq!(account_info["account_id"], public_key);
    assert!(account_info.contains_key("balance"));
    assert!(account_info.contains_key("sequence"));
    assert!(account_info.contains_key("network"));
}

#[tokio::test]
async fn test_network_fee() {
    let client = StellarClient::new().await.unwrap();
    
    let fee = client.get_network_fee().await.unwrap();
    assert!(fee > 0);
}

#[tokio::test]
async fn test_testnet_funding() {
    let client = StellarClient::new().await.unwrap();
    
    let public_key = "GABCDEFGHIJKLMNOPQRSTUVWXYZ123456789";
    let result = client.fund_testnet_account(public_key).await;
    assert!(result.is_ok());
}

#[test]
fn test_token_operations() {
    let mut token = TokenContract::new(
        "Test Token".to_string(),
        "TEST".to_string(),
        1000000,
    );
    
    // Test minting
    let address = soroban_sdk::Address::generate(&soroban_sdk::Env::default());
    let initial_supply = token.total_supply;
    token.mint(address.clone(), 500000).unwrap();
    assert_eq!(token.total_supply, initial_supply + 500000);
    
    // Test burning
    token.burn(address, 100000).unwrap();
    assert_eq!(token.total_supply, initial_supply + 400000);
}

#[test]
fn test_liquidity_pool_operations() {
    let mut pool = LiquidityPoolContract::new(
        "TOKEN_A_CONTRACT".to_string(),
        "TOKEN_B_CONTRACT".to_string(),
    );
    
    let provider = soroban_sdk::Address::generate(&soroban_sdk::Env::default());
    
    // Test adding initial liquidity
    let liquidity = pool
        .add_liquidity(provider, 1000, 2000, 1000, 2000)
        .unwrap();
    assert_eq!(pool.reserve_a, 1000);
    assert_eq!(pool.reserve_b, 2000);
    assert_eq!(pool.total_liquidity, liquidity);
    
    // Test price calculation
    let price_a_to_b = pool.get_price_a_to_b();
    let price_b_to_a = pool.get_price_b_to_a();
    assert_eq!(price_a_to_b, 2.0);
    assert_eq!(price_b_to_a, 0.5);
    
    // Test swap calculation
    let output = pool.calculate_swap_output(100, pool.reserve_a, pool.reserve_b);
    assert!(output > 180 && output < 182);
}

#[test]
fn test_staking_operations() {
    let mut staking = StakingContract::new(
        "STAKING_TOKEN".to_string(),
        "REWARD_TOKEN".to_string(),
        1000,
    );
    
    let user = soroban_sdk::Address::generate(&soroban_sdk::Env::default());
    
    // Test staking
    staking.stake(user, 5000).unwrap();
    assert_eq!(staking.get_total_staked(), 5000);
    
    // Test unstaking
    staking.unstake(user, 2000).unwrap();
    assert_eq!(staking.get_total_staked(), 3000);
    
    // Test APY calculation
    let apy = staking.get_apy();
    assert!(apy > 0.0);
}

#[test]
fn test_utility_functions() {
    use stellar_defi_toolkit::utils::*;
    
    // Test address generation
    let address = generate_address();
    // Address should be valid (basic check)
    
    // Test public key validation
    assert!(validate_public_key("GABCDEFGHIJKLMNOPQRSTUVWXYZ123456789").unwrap());
    assert!(!validate_public_key("invalid").unwrap());
    
    // Test balance formatting
    assert_eq!(format_balance(1000000000, 7), "100.0000000");
    assert_eq!(format_balance(123456789, 7), "12.3456789");
    
    // Test balance parsing
    assert_eq!(parse_balance("100.0000000", 7).unwrap(), 1000000000);
    assert_eq!(parse_balance("12.3456789", 7).unwrap(), 123456789);
    
    // Test minimum calculations
    let (min_a, min_b) = calculate_minimum_liquidity(1000, 2000, 500);
    assert_eq!(min_a, 950);
    assert_eq!(min_b, 1900);
    
    let min_out = calculate_minimum_output(1000, 300);
    assert_eq!(min_out, 700);
}

#[test]
fn test_type_validations() {
    use stellar_defi_toolkit::types::*;
    
    // Test token metadata validation
    let mut metadata = token::TokenMetadata::new("Test Token".to_string(), "TEST".to_string(), 1000000);
    assert!(metadata.validate().is_ok());
    
    // Test invalid metadata
    metadata.name = "".to_string();
    assert!(metadata.validate().is_err());
    
    // Test pool info
    let pool_info = pool::PoolInfo::new("TOKEN_A".to_string(), "TOKEN_B".to_string(), 30);
    assert_eq!(pool_info.token_a, "TOKEN_A");
    assert_eq!(pool_info.token_b, "TOKEN_B");
    assert_eq!(pool_info.fee_percentage, 30);
    
    // Test price calculation
    let mut pool_info = pool::PoolInfo::default();
    pool_info.reserve_a = 1000;
    pool_info.reserve_b = 2000;
    assert_eq!(pool_info.get_price_a_to_b(), 2.0);
    assert_eq!(pool_info.get_price_b_to_a(), 0.5);
}
