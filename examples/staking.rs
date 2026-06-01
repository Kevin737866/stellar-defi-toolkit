//! Example demonstrating the Staking Contract functionality
//! 
//! This example shows how to:
//! - Initialize a staking contract
//! - Stake tokens
//! - Earn rewards over time
//! - Claim rewards
//! - Unstake tokens

use soroban_sdk::{Env, Address, testutils::Address as _};

// Note: In a real deployment, you would import the actual contract client
// For this example, we'll demonstrate the conceptual flow

fn main() {
    println!("=== Stellar DeFi Toolkit - Staking Contract Example ===\n");

    // Create a test environment
    let env = Env::default();
    env.mock_all_auths();

    // Generate addresses for demonstration
    let admin = Address::generate(&env);
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);

    println!("📋 Setup:");
    println!("  Admin: {}", admin);
    println!("  User 1: {}", user1);
    println!("  User 2: {}", user2);
    println!();

    // In a real scenario, you would:
    // 1. Deploy the staking contract
    // 2. Deploy or reference existing token contracts
    // 3. Initialize the staking contract with parameters

    println!("🚀 Step 1: Initialize Staking Contract");
    println!("  - Staking Token: USDC");
    println!("  - Reward Token: REWARD");
    println!("  - Reward Duration: 7 days (120,960 ledgers)");
    println!();

    // Example initialization (pseudo-code):
    // staking_contract.initialize(
    //     &admin,
    //     &staking_token_address,
    //     &reward_token_address,
    //     &120960, // 7 days in ledgers
    // );

    println!("💰 Step 2: Admin Sets Reward Amount");
    println!("  - Total Rewards: 10,000 REWARD tokens");
    println!("  - Reward Rate: ~0.083 REWARD per ledger");
    println!();

    // staking_contract.notify_reward_amount(&admin, &10_000_000_000);

    println!("🔒 Step 3: Users Stake Tokens");
    println!("  - User 1 stakes: 1,000 USDC");
    println!("  - User 2 stakes: 500 USDC");
    println!("  - Total Staked: 1,500 USDC");
    println!();

    // staking_contract.stake(&user1, &1_000_000_000);
    // staking_contract.stake(&user2, &500_000_000);

    println!("⏰ Step 4: Time Passes (simulating 1 day = 17,280 ledgers)");
    println!("  - Rewards accumulate based on stake proportion");
    println!("  - User 1 (66.7% stake) earns ~66.7% of rewards");
    println!("  - User 2 (33.3% stake) earns ~33.3% of rewards");
    println!();

    // Simulate time passing
    // env.ledger().with_mut(|li| {
    //     li.sequence_number += 17280;
    // });

    println!("📊 Step 5: Check Earned Rewards");
    println!("  - User 1 earned: ~952 REWARD tokens");
    println!("  - User 2 earned: ~476 REWARD tokens");
    println!();

    // let earned1 = staking_contract.get_earned(&user1);
    // let earned2 = staking_contract.get_earned(&user2);

    println!("💸 Step 6: User 1 Claims Rewards");
    println!("  - Claimed: 952 REWARD tokens");
    println!("  - Remaining staked: 1,000 USDC");
    println!();

    // let claimed = staking_contract.claim_rewards(&user1);

    println!("🔓 Step 7: User 2 Unstakes Partially");
    println!("  - Unstaked: 200 USDC");
    println!("  - Remaining staked: 300 USDC");
    println!("  - Unclaimed rewards: 476 REWARD tokens");
    println!();

    // staking_contract.unstake(&user2, &200_000_000);

    println!("📈 Step 8: View Contract Statistics");
    println!("  - Total Staked: 1,300 USDC");
    println!("  - Reward Rate: 0.083 REWARD/ledger");
    println!("  - Period Finish: 103,680 ledgers remaining");
    println!();

    // let info = staking_contract.get_info();

    println!("✅ Example Complete!");
    println!();
    println!("Key Features Demonstrated:");
    println!("  ✓ Time-based reward distribution");
    println!("  ✓ Proportional rewards based on stake");
    println!("  ✓ Flexible staking and unstaking");
    println!("  ✓ Separate reward claiming");
    println!("  ✓ Admin-controlled reward periods");
    println!();
    println!("Additional Features Available:");
    println!("  • Emergency withdraw (forfeit rewards)");
    println!("  • Multiple reward periods");
    println!("  • Real-time reward calculations");
    println!("  • Event emission for all actions");
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_example_runs() {
        // This ensures the example compiles and runs
        super::main();
    }
}
