//! Example: Setting up and using a staking contract

use stellar_defi_toolkit::{StakingContract, StellarClient};
use tokio;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize the Stellar client
    let client = StellarClient::new().await?;
    
    // Create a new staking contract
    let staking = StakingContract::new(
        "STAKING_TOKEN_CONTRACT_ID".to_string(),
        "REWARD_TOKEN_CONTRACT_ID".to_string(),
        1000, // 1000 reward tokens per second
    );
    
    println!("🌾 Creating staking contract...");
    println!("Staking Token: {}", staking.get_info().staking_token);
    println!("Reward Token: {}", staking.get_info().reward_token);
    println!("Reward Rate: {} tokens per second", staking.get_info().reward_rate);
    
    // Deploy the staking contract
    println!("\n🚀 Deploying staking contract...");
    let contract_id = staking.deploy(&client).await?;
    
    println!("✅ Staking contract deployed successfully!");
    println!("Contract ID: {}", contract_id);
    
    // Example of staking tokens
    println!("\n📈 Example: Staking tokens...");
    let user_address = "GABCDEFGHIJKLMNOPQRSTUVWXYZ123456789";
    let stake_amount = 500000000; // 500 tokens
    
    println!("Staking {} tokens for address: {}", stake_amount as f64 / 1_000_000.0, user_address);
    
    // Note: In a real implementation, you would call the contract's stake function
    println!("✅ Tokens staked successfully!");
    
    // Calculate rewards
    println!("\n💰 Calculating rewards...");
    let staking_duration_days = 30;
    let rewards_per_day = staking.get_info().reward_rate * 86400; // 86400 seconds in a day
    let total_rewards = rewards_per_day * staking_duration_days;
    
    println!("Staking duration: {} days", staking_duration_days);
    println!("Rewards per day: {} tokens", rewards_per_day as f64 / 1_000_000.0);
    println!("Total rewards after {} days: {} tokens", 
             staking_duration_days, 
             total_rewards as f64 / 1_000_000.0);
    
    // Calculate APY
    let apy = staking.get_apy();
    println!("Annual Percentage Yield (APY): {:.2}%", apy);
    
    // Example of claiming rewards
    println!("\n🎁 Example: Claiming rewards...");
    let pending_rewards = total_rewards;
    
    println!("Claiming {} reward tokens...", pending_rewards as f64 / 1_000_000.0);
    // Note: In a real implementation, you would call the contract's claim_rewards function
    println!("✅ Rewards claimed successfully!");
    
    // Example of unstaking
    println!("\n📉 Example: Unstaking tokens...");
    let unstake_amount = 200000000; // 200 tokens
    
    println!("Unstaking {} tokens...", unstake_amount as f64 / 1_000_000.0);
    // Note: In a real implementation, you would call the contract's unstake function
    println!("✅ Tokens unstaked successfully!");
    
    // Example of updating reward rate (admin function)
    println!("\n⚙️  Example: Updating reward rate...");
    let new_reward_rate = 1500; // Increased to 1500 tokens per second
    
    println!("Updating reward rate from {} to {} tokens per second", 
             staking.get_info().reward_rate, 
             new_reward_rate);
    
    // Note: In a real implementation, this would be called through a governance proposal
    println!("✅ Reward rate updated!");
    
    // Show updated APY
    let updated_apy = (new_reward_rate as f64 * 365.0 * 24.0 * 60.0 * 60.0 / 300000000.0) * 100.0;
    println!("Updated APY: {:.2}%", updated_apy);
    
    // Example of emergency withdraw
    println!("\n🚨 Example: Emergency withdraw...");
    let emergency_withdraw_amount = 300000000; // 300 tokens
    
    println!("Emergency withdrawing {} tokens without rewards...", 
             emergency_withdraw_amount as f64 / 1_000_000.0);
    // Note: In a real implementation, you would call the contract's emergency_withdraw function
    println!("✅ Emergency withdraw completed!");
    
    // Summary
    println!("\n📊 Staking Summary:");
    println!("Total Staked: {} tokens", 300000000 as f64 / 1_000_000.0);
    println!("Total Rewards Claimed: {} tokens", total_rewards as f64 / 1_000_000.0);
    println!("Final APY: {:.2}%", updated_apy);
    
    Ok(())
}
