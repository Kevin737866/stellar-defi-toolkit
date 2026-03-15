//! Example: Creating and managing a liquidity pool

use stellar_defi_toolkit::{LiquidityPoolContract, StellarClient};
use tokio;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize the Stellar client
    let client = StellarClient::new().await?;
    
    // Create a new liquidity pool
    let pool = LiquidityPoolContract::new(
        "TOKEN_A_CONTRACT_ID".to_string(),
        "TOKEN_B_CONTRACT_ID".to_string(),
    );
    
    println!("💧 Creating liquidity pool contract...");
    println!("Token A: {}", pool.get_info().token_a);
    println!("Token B: {}", pool.get_info().token_b);
    println!("Fee Percentage: {} bps ({}%)", pool.get_info().fee_percentage, pool.get_info().fee_percentage as f64 / 100.0);
    
    // Deploy the liquidity pool contract
    println!("\n🚀 Deploying liquidity pool contract...");
    let contract_id = pool.deploy(&client).await?;
    
    println!("✅ Liquidity pool deployed successfully!");
    println!("Contract ID: {}", contract_id);
    
    // Example of adding liquidity
    println!("\n➕ Example: Adding liquidity to the pool...");
    let provider_address = "GABCDEFGHIJKLMNOPQRSTUVWXYZ123456789";
    let amount_a = 1000000; // 100 tokens (with 7 decimals)
    let amount_b = 2000000; // 200 tokens (with 7 decimals)
    let min_amount_a = 950000; // 5% slippage tolerance
    let min_amount_b = 1900000; // 5% slippage tolerance
    
    println!("Adding liquidity:");
    println!("  Token A: {} tokens", amount_a as f64 / 1_000_000.0);
    println!("  Token B: {} tokens", amount_b as f64 / 1_000_000.0);
    println!("  Min Token A: {} tokens", min_amount_a as f64 / 1_000_000.0);
    println!("  Min Token B: {} tokens", min_amount_b as f64 / 1_000_000.0);
    
    // Note: In a real implementation, you would call the contract's add_liquidity function
    let liquidity_tokens = (amount_a as f64 * amount_b as f64).sqrt() as u64;
    println!("✅ Liquidity added! Received {} LP tokens", liquidity_tokens);
    
    // Example of swapping tokens
    println!("\n🔄 Example: Swapping tokens...");
    let swap_amount = 100000; // 10 tokens
    let min_output = 180000; // Minimum 18 tokens output (slippage protection)
    
    println!("Swapping {} Token A for Token B...", swap_amount as f64 / 1_000_000.0);
    
    // Calculate expected output
    let output_amount = pool.calculate_swap_output(swap_amount, amount_a, amount_b);
    println!("Expected output: {} Token B", output_amount as f64 / 1_000_000.0);
    
    if output_amount >= min_output {
        println!("✅ Swap executed successfully!");
        println!("Received {} Token B", output_amount as f64 / 1_000_000.0);
    } else {
        println!("❌ Swap failed: Insufficient output amount");
    }
    
    // Example of getting pool statistics
    println!("\n📊 Pool Statistics:");
    let price_a_to_b = pool.get_price_a_to_b();
    let price_b_to_a = pool.get_price_b_to_a();
    
    println!("Current Price A → B: {}", price_a_to_b);
    println!("Current Price B → A: {}", price_b_to_a);
    
    // Calculate impermanent loss
    let initial_price_ratio = 2.0; // Initial price ratio (B/A)
    let current_price_ratio = price_a_to_b;
    let impermanent_loss = (1.0 - (2.0 * current_price_ratio.sqrt() / (1.0 + current_price_ratio))) * 100.0;
    
    println!("Impermanent Loss: {:.2}%", impermanent_loss);
    
    // Example of removing liquidity
    println!("\n➖ Example: Removing liquidity...");
    let liquidity_to_remove = 500000; // 50% of LP tokens
    let min_remove_a = 475000; // 5% slippage tolerance
    let min_remove_b = 950000; // 5% slippage tolerance
    
    println!("Removing {} LP tokens...", liquidity_to_remove as f64 / 1_000_000.0);
    
    // Calculate expected amounts
    let remove_a = liquidity_to_remove * amount_a / liquidity_tokens;
    let remove_b = liquidity_to_remove * amount_b / liquidity_tokens;
    
    println!("Expected to receive:");
    println!("  Token A: {} tokens", remove_a as f64 / 1_000_000.0);
    println!("  Token B: {} tokens", remove_b as f64 / 1_000_000.0);
    
    if remove_a >= min_remove_a && remove_b >= min_remove_b {
        println!("✅ Liquidity removed successfully!");
    } else {
        println!("❌ Liquidity removal failed: Amounts below minimum");
    }
    
    Ok(())
}
