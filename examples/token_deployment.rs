//! Example: Deploying a new token contract

use stellar_defi_toolkit::{TokenContract, StellarClient};
use tokio;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize the Stellar client
    let client = StellarClient::new().await?;
    
    // Create a new token
    let token = TokenContract::new(
        "Example Token".to_string(),
        "EXMPL".to_string(),
        1000000000, // 1 billion tokens with 7 decimals
    );
    
    println!("🪙 Creating token contract...");
    println!("Name: {}", token.get_info().name);
    println!("Symbol: {}", token.get_info().symbol);
    println!("Initial Supply: {}", token.get_info().total_supply);
    println!("Decimals: {}", token.get_info().decimals);
    
    // Deploy the token contract
    println!("\n🚀 Deploying token contract...");
    let contract_id = token.deploy(&client).await?;
    
    println!("✅ Token deployed successfully!");
    println!("Contract ID: {}", contract_id);
    
    // Get contract information
    println!("\n📊 Getting contract information...");
    let contract_info = client.get_contract_info(&contract_id).await?;
    println!("Contract Info: {:#}", serde_json::to_string_pretty(&contract_info)?);
    
    // Example of minting additional tokens
    println!("\n🏗️  Example: Minting additional tokens...");
    let recipient_address = "GABCDEFGHIJKLMNOPQRSTUVWXYZ123456789";
    let mint_amount = 500000000; // 500 additional tokens
    
    println!("Minting {} tokens to address: {}", mint_amount, recipient_address);
    // Note: In a real implementation, you would call the contract's mint function
    println!("✅ Tokens minted successfully!");
    
    // Example of checking balance
    println!("\n💰 Example: Checking token balance...");
    println!("Balance for {}: 0 tokens", recipient_address);
    // Note: In a real implementation, you would query the contract state
    
    Ok(())
}
