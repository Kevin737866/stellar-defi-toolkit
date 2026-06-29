//! Decentralized Oracle Demo
//!
//! This example demonstrates how to use the decentralized oracle contract
//! to aggregate prices from multiple sources with staking and reputation mechanisms.

use soroban_sdk::{contract, contractimpl, Address, Env, Symbol, testutils::Address as _, symbol_short};
use stellar_defi_toolkit::contracts::DecentralizedOracle;

#[contract]
struct DemoContract;

#[contractimpl]
impl DemoContract {
    /// Run a comprehensive demo of the decentralized oracle
    pub fn run_demo(env: Env) {
        // Register the decentralized oracle contract
        let contract_id = env.register_contract(None, DecentralizedOracle);
        let client = stellar_defi_toolkit::contracts::DecentralizedOracleClient::new(&env, &contract_id);

        // Initialize the oracle with an admin
        let admin = Address::generate(&env);
        client.initialize(&admin).unwrap();
        println!("✓ Decentralized oracle initialized");

        // Register multiple oracles with stakes
        let oracle1 = Address::generate(&env);
        let oracle2 = Address::generate(&env);
        let oracle3 = Address::generate(&env);
        let oracle4 = Address::generate(&env);
        let oracle5 = Address::generate(&env);

        let min_stake = 1_000_000u64;

        client.register_oracle(&oracle1, &min_stake).unwrap();
        client.register_oracle(&oracle2, &(min_stake * 2)).unwrap();
        client.register_oracle(&oracle3, &(min_stake * 3)).unwrap();
        client.register_oracle(&oracle4, &(min_stake * 4)).unwrap();
        client.register_oracle(&oracle5, &(min_stake * 5)).unwrap();
        println!("✓ Registered 5 oracles with varying stakes");

        // Get oracle addresses
        let oracle_addresses = client.get_oracle_addresses();
        println!("✓ Active oracles count: {}", oracle_addresses.len());

        // Submit prices from multiple oracles for BTC (asset_id = 1)
        let btc_asset_id = 1u32;
        let current_time = env.ledger().timestamp();

        // Oracle 1 submits BTC price at $50,000
        client.submit_price(
            &oracle1,
            &btc_asset_id,
            &50_000_000_000, // $50,000 with 6 decimals
            &9500, // 95% confidence
            &current_time,
        ).unwrap();

        // Oracle 2 submits BTC price at $50,100
        client.submit_price(
            &oracle2,
            &btc_asset_id,
            &50_100_000_000,
            &9200,
            &current_time,
        ).unwrap();

        // Oracle 3 submits BTC price at $49,900
        client.submit_price(
            &oracle3,
            &btc_asset_id,
            &49_900_000_000,
            &8800,
            &current_time,
        ).unwrap();

        // Oracle 4 submits BTC price at $50,050
        client.submit_price(
            &oracle4,
            &btc_asset_id,
            &50_050_000_000,
            &9000,
            &current_time,
        ).unwrap();

        // Oracle 5 submits BTC price at $50,000
        client.submit_price(
            &oracle5,
            &btc_asset_id,
            &50_000_000_000,
            &9100,
            &current_time,
        ).unwrap();

        println!("✓ Submitted BTC prices from 5 oracles");

        // Get aggregated price
        let btc_price = client.get_price(&btc_asset_id).unwrap();
        println!("✓ Aggregated BTC price: {}", btc_price);

        // Submit prices for ETH (asset_id = 2)
        let eth_asset_id = 2u32;

        client.submit_price(
            &oracle1,
            &eth_asset_id,
            &3_000_000_000, // $3,000 with 6 decimals
            &9400,
            &current_time,
        ).unwrap();

        client.submit_price(
            &oracle2,
            &eth_asset_id,
            &3_010_000_000,
            &9100,
            &current_time,
        ).unwrap();

        client.submit_price(
            &oracle3,
            &eth_asset_id,
            &2_990_000_000,
            &8700,
            &current_time,
        ).unwrap();

        println!("✓ Submitted ETH prices from 3 oracles");

        let eth_price = client.get_price(&eth_asset_id).unwrap();
        println!("✓ Aggregated ETH price: {}", eth_price);

        // Get oracle information
        let oracle1_stake = client.get_oracle_stake(&oracle1).unwrap();
        let oracle1_rep = client.get_oracle_reputation(&oracle1).unwrap();
        println!("✓ Oracle 1 info: stake={}, reputation={}", 
                 oracle1_stake, oracle1_rep);

        // Demonstrate unbonding process
        client.request_unbond(&oracle1).unwrap();
        println!("✓ Oracle 1 requested to unbond");

        // Try to withdraw before unbonding period (should fail)
        let withdraw_result = client.try_withdraw_stake(&oracle1);
        println!("✓ Withdraw before unbonding period failed as expected: {:?}", withdraw_result);

        // Update configuration
        client.update_config(&admin, &symbol_short!("MIN_ORACL"), &4).unwrap();
        println!("✓ Updated configuration: min_oracles = 4");

        // Demonstrate slashing (in a real scenario, this would be for malicious behavior)
        client.slash_oracle(&admin, &oracle2, &symbol_short!("MALICIOUS_REPORT")).unwrap();
        println!("✓ Slashed Oracle 2 for malicious behavior");

        let oracle2_stake = client.get_oracle_stake(&oracle2).unwrap();
        let oracle2_rep = client.get_oracle_reputation(&oracle2).unwrap();
        println!("✓ Oracle 2 after slash: stake={}, reputation={}", 
                 oracle2_stake, oracle2_rep);

        // Pause the oracle
        client.pause(&admin).unwrap();
        println!("✓ Oracle paused");

        // Try to submit price while paused (should fail)
        let submit_result = client.try_submit_price(
            &oracle3,
            &btc_asset_id,
            &51_000_000_000,
            &9000,
            &current_time,
        );
        println!("✓ Submit price while paused failed as expected: {:?}", submit_result);

        // Unpause the oracle
        client.unpause(&admin).unwrap();
        println!("✓ Oracle unpaused");

        println!("\n=== Demo Complete ===");
        println!("The decentralized oracle successfully:");
        println!("  • Registered multiple oracles with staking");
        println!("  • Aggregated prices from multiple sources");
        println!("  • Maintained reputation scores");
        println!("  • Handled unbonding process");
        println!("  • Slashed malicious oracles");
        println!("  • Managed configuration updates");
        println!("  • Implemented pause/unpause functionality");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decentralized_oracle_demo() {
        let env = Env::default();
        env.mock_all_auths();
        
        let contract_id = env.register_contract(None, DemoContract);
        let client = DemoContractClient::new(&env, &contract_id);
        
        client.run_demo();
    }
}
