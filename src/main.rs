use clap::{Parser, Subcommand};
use log::info;
use soroban_sdk::Env;
use stellar_defi_toolkit::contracts::{TokenContract, LiquidityPoolContract};
use stellar_defi_toolkit::utils::StellarClient;

#[derive(Parser)]
#[command(name = "stellar-defi-cli")]
#[command(about = "Lending and borrowing protocol playground for Soroban")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Print the annualized borrow rate for a given utilization.
    QuoteRate {
        #[arg(long, help = "Utilization in basis points, e.g. 8000 for 80%")]
        utilization_bps: u32,
    },
    
    /// Liquidate an undercollateralized position in the lending protocol.
    Liquidate {
        #[arg(long, help = "Address of the liquidator")]
        liquidator: String,
        
        #[arg(long, help = "Address of the borrower to liquidate")]
        borrower: String,
        
        #[arg(long, help = "Asset symbol for the debt (e.g., USDC)")]
        debt_asset: String,
        
        #[arg(long, help = "Asset symbol for the collateral (e.g., XLM)")]
        collateral_asset: String,
        
        #[arg(long, help = "Amount of debt to repay (in smallest unit)")]
        repay_amount: i128,
        
        #[arg(long, help = "Price of debt asset in USD (with 18 decimals)", default_value = "1000000000000000000")]
        debt_price: i128,
        
        #[arg(long, help = "Price of collateral asset in USD (with 18 decimals)", default_value = "1000000000000000000")]
        collateral_price: i128,
        
        #[arg(long, help = "Current timestamp (unix seconds)", default_value = "0")]
        timestamp: u64,
        
        #[arg(long, help = "Simulate liquidation without executing", default_value = "false")]
        dry_run: bool,
    },
    
    /// Check if a position is liquidatable.
    CheckLiquidation {
        #[arg(long, help = "Address of the borrower to check")]
        borrower: String,
        
        #[arg(long, help = "Asset symbol for the debt (e.g., USDC)")]
        debt_asset: String,
        
        #[arg(long, help = "Asset symbol for the collateral (e.g., XLM)")]
        collateral_asset: String,
        
        #[arg(long, help = "Price of debt asset in USD (with 18 decimals)", default_value = "1000000000000000000")]
        debt_price: i128,
        
        #[arg(long, help = "Price of collateral asset in USD (with 18 decimals)", default_value = "1000000000000000000")]
        collateral_price: i128,
    /// Repay a borrowed asset.
    Repay {
        #[arg(long, help = "The account repaying the debt")]
        payer: String,
        #[arg(long, help = "The account whose debt is being repaid")]
        borrower: String,
        
        #[arg(long, help = "Asset symbol for the debt (e.g., USDC)")]
        debt_asset: String,
        
        #[arg(long, help = "Asset symbol for the collateral (e.g., XLM)")]
        collateral_asset: String,
        
        #[arg(long, help = "Amount of debt to repay (in smallest unit)")]
        repay_amount: i128,
        
        #[arg(long, help = "Price of debt asset in USD (with 18 decimals)", default_value = "1000000000000000000")]
        debt_price: i128,
        
        #[arg(long, help = "Price of collateral asset in USD (with 18 decimals)", default_value = "1000000000000000000")]
        collateral_price: i128,
        
        #[arg(long, help = "Current timestamp (unix seconds)", default_value = "0")]
        timestamp: u64,
        
        #[arg(long, help = "Simulate liquidation without executing", default_value = "false")]
        dry_run: bool,
    },
    
    /// Check if a position is liquidatable.
    CheckLiquidation {
        #[arg(long, help = "Address of the borrower to check")]
        borrower: String,
        
        #[arg(long, help = "Asset symbol for the debt (e.g., USDC)")]
        debt_asset: String,
        
        #[arg(long, help = "Asset symbol for the collateral (e.g., XLM)")]
        collateral_asset: String,
        
        #[arg(long, help = "Price of debt asset in USD (with 18 decimals)", default_value = "1000000000000000000")]
        debt_price: i128,
        
        #[arg(long, help = "Price of collateral asset in USD (with 18 decimals)", default_value = "1000000000000000000")]
        collateral_price: i128,
    },
    /// Lending protocol operations
    #[command(subcommand)]
    Lend(LendCommands),

    /// Start the GraphQL API server
    ServeApi {
        /// Port to listen on
        #[arg(short, long, default_value = "4000")]
        port: u16,
    },
}

#[derive(Subcommand)]
enum LendCommands {
    /// Deposit assets into the lending protocol
    Deposit {
        #[arg(long, help = "User account")]
        user: String,
        #[arg(help = "Asset symbol (e.g., USDC)")]
        asset: String,
        #[arg(help = "Amount to deposit (in smallest unit)")]
        amount: i128,
    },
    /// Withdraw assets from the lending protocol
    Withdraw {
        #[arg(long, help = "User account")]
        user: String,
        #[arg(help = "Asset symbol (e.g., USDC)")]
        asset: String,
        #[arg(help = "Amount to withdraw (in smallest unit)")]
        amount: i128,
    },
    /// Borrow assets from the lending protocol
    Borrow {
        #[arg(long, help = "User account")]
        user: String,
        #[arg(help = "Asset symbol (e.g., USDC)")]
        asset: String,
        #[arg(help = "Amount to borrow (in smallest unit)")]
        amount: i128,
    },
    /// Repay a borrowed asset
    Repay {
        #[arg(long, help = "User account repaying the debt")]
        user: String,
        #[arg(help = "Asset symbol (e.g., USDC)")]
        asset: String,
        #[arg(help = "Amount to repay (in smallest unit)")]
        amount: i128,
    },
    /// Show current position and health
    Position {
        #[arg(long, help = "User account")]
        user: String,
    },
}

fn main() {
    env_logger::init();

    let cli = Cli::parse();
    match cli.command {
        Commands::DeployToken { name, symbol, supply } => {
            info!("Deploying token contract: {} ({})", name, symbol);
            let env = Env::default();
            let token_contract = TokenContract::new_std(&env, name, symbol, supply);
            let contract_id = token_contract.deploy(&client).await?;
            println!("Token deployed successfully! Contract ID: {}", contract_id);
        }
        Commands::CreatePool { token_a, token_b } => {
            info!("Creating liquidity pool between {} and {}", token_a, token_b);
            let env = Env::default();
            let pool = LiquidityPoolContract::new_std(&env, token_a, token_b);
            let contract_id = pool.deploy(&client).await?;
            println!("Liquidity pool created! Contract ID: {}", contract_id);
        }
        Commands::Lend(lend_cmd) => {
            handle_lend_command(lend_cmd);
        }
    } else {
        println!("⚡ EXECUTING LIQUIDATION...\n");
        
        match protocol.liquidate(
            liquidator,
            borrower,
            debt_asset,
            collateral_asset,
            repay_amount,
            &oracle,
            now,
        ) {
            Ok(result) => {
                println!("✅ Liquidation Successful!");
                println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                println!("Repaid Amount:         {:.6} {}", result.repaid_amount as f64 / WAD as f64, debt_asset);
                println!("Seized Collateral:     {:.6} {}", result.seized_collateral as f64 / WAD as f64, collateral_asset);
                println!("Liquidator Profit:     ${:.2}", result.liquidator_discount_value as f64 / WAD as f64);
                println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            }
            Err(e) => {
                println!("❌ Liquidation Failed!");
                println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                println!("Error: {:?}", e);
                println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                
                match e {
                    stellar_defi_toolkit::ProtocolError::PositionNotLiquidatable => {
                        println!("\n💡 Tip: The position's health factor is >= 1.0");
                        println!("   Use the 'check-liquidation' command to view position details.");
                    }
                    stellar_defi_toolkit::ProtocolError::InsufficientBalance => {
                        println!("\n💡 Tip: The borrower doesn't have enough collateral to seize.");
                    }
                    stellar_defi_toolkit::ProtocolError::InsufficientLiquidity => {
                        println!("\n💡 Tip: The protocol doesn't have enough liquidity for this liquidation.");
                    }
                    _ => {}
                }
            }
        }
        Commands::ServeApi { port } => {
            info!("Starting Stellar Analytics GraphQL API on port {}", port);
            stellar_defi_toolkit::api::start_api_server(port, client).await?;
        }
    }
}

fn check_liquidation_status(
    borrower: &str,
    debt_asset: &str,
    collateral_asset: &str,
    debt_price: i128,
    collateral_price: i128,
) {
    use stellar_defi_toolkit::{InterestRateModel, ReserveConfig, WAD};
    
    println!("🔍 Checking Liquidation Status");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Borrower:          {}", borrower);
    println!("Debt Asset:        {}", debt_asset);
    println!("Collateral Asset:  {}", collateral_asset);
    println!("Debt Price:        ${:.6}", debt_price as f64 / WAD as f64);
    println!("Collateral Price:  ${:.6}", collateral_price as f64 / WAD as f64);
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
    
    // Create a mock protocol for demonstration
    let model = InterestRateModel::default();
    let mut protocol = LendingProtocol::new("admin", "treasury", model);
    
    // Create a mock oracle with the provided prices
    let mut oracle = PriceOracle::new("oracle-admin");
    oracle.set_price("oracle-admin", debt_asset, debt_price).unwrap();
    oracle.set_price("oracle-admin", collateral_asset, collateral_price).unwrap();
    
    // Register assets with reasonable default configurations
    let debt_config = ReserveConfig {
        asset: debt_asset.to_string(),
        decimals: 6,
        collateral_factor_bps: 8000,
        liquidation_threshold_bps: 8500,
        liquidation_bonus_bps: 500,
        reserve_factor_bps: 1000,
        flash_loan_fee_bps: 9,
        borrow_enabled: true,
        deposit_enabled: true,
        flash_loan_enabled: true,
    };
    
    let collateral_config = ReserveConfig {
        asset: collateral_asset.to_string(),
        decimals: 7,
        collateral_factor_bps: 7500,
        liquidation_threshold_bps: 8000,
        liquidation_bonus_bps: 1000,
        reserve_factor_bps: 1000,
        flash_loan_fee_bps: 9,
        borrow_enabled: true,
        deposit_enabled: true,
        flash_loan_enabled: true,
    };
    
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    
    protocol.register_asset("admin", debt_config.clone(), now).unwrap();
    protocol.register_asset("admin", collateral_config.clone(), now).unwrap();
    
    match protocol.position(borrower, &oracle) {
        Ok(snapshot) => {
            println!("📊 Position Details:");
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            
            // Display supplied amounts
            println!("\n💰 Supplied Assets:");
            for (asset, amount) in &snapshot.supplied_amounts {
                println!("   {}: {:.6}", asset, *amount as f64 / WAD as f64);
            }
            
            // Display debt amounts
            println!("\n📉 Debt Assets:");
            for (asset, amount) in &snapshot.debt_amounts {
                println!("   {}: {:.6}", asset, *amount as f64 / WAD as f64);
            }
            
            // Display values
            println!("\n💵 Position Values:");
            println!("   Collateral Value:   ${:.2}", snapshot.collateral_value as f64 / WAD as f64);
            println!("   Liquidation Value:  ${:.2}", snapshot.liquidation_value as f64 / WAD as f64);
            println!("   Debt Value:         ${:.2}", snapshot.debt_value as f64 / WAD as f64);
            
            // Display health factor
            println!("\n🏥 Health Factor: {:.4}", snapshot.health_factor as f64 / WAD as f64);
            
            if snapshot.debt_value == 0 {
                println!("\n✅ Status: NO DEBT");
                println!("   The position has no outstanding debt.");
            } else if snapshot.health_factor >= WAD {
                println!("\n✅ Status: HEALTHY");
                println!("   The position is well-collateralized and cannot be liquidated.");
                
                let buffer = ((snapshot.health_factor as f64 / WAD as f64) - 1.0) * 100.0;
                println!("   Safety Buffer: {:.2}%", buffer);
            } else {
                println!("\n⚠️  Status: LIQUIDATABLE");
                println!("   The position is undercollateralized and can be liquidated!");
                
                let deficit = (1.0 - (snapshot.health_factor as f64 / WAD as f64)) * 100.0;
                println!("   Collateral Deficit: {:.2}%", deficit);
                
                println!("\n💡 Liquidation Opportunity:");
                println!("   You can liquidate this position to earn a liquidation bonus.");
                println!("   Use the 'liquidate' command to execute the liquidation.");
            }
            
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        }
        Err(e) => {
            println!("❌ Error checking position: {:?}", e);
        }
    }
}

fn handle_liquidation(
    liquidator: &str,
    borrower: &str,
    debt_asset: &str,
    collateral_asset: &str,
    repay_amount: i128,
    debt_price: i128,
    collateral_price: i128,
    timestamp: u64,
    dry_run: bool,
) {
    use stellar_defi_toolkit::{InterestRateModel, ReserveConfig, WAD};
    
    println!("🔍 Liquidation Request");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Liquidator:        {}", liquidator);
    println!("Borrower:          {}", borrower);
    println!("Debt Asset:        {}", debt_asset);
    println!("Collateral Asset:  {}", collateral_asset);
    println!("Repay Amount:      {}", repay_amount);
    println!("Debt Price:        ${:.6}", debt_price as f64 / WAD as f64);
    println!("Collateral Price:  ${:.6}", collateral_price as f64 / WAD as f64);
    println!("Dry Run:           {}", if dry_run { "Yes" } else { "No" });
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
    
    // Create a mock protocol for demonstration
    let model = InterestRateModel::default();
    let mut protocol = LendingProtocol::new("admin", "treasury", model);
    
    // Create a mock oracle with the provided prices
    let mut oracle = PriceOracle::new("oracle-admin");
    oracle.set_price("oracle-admin", debt_asset, debt_price).unwrap();
    oracle.set_price("oracle-admin", collateral_asset, collateral_price).unwrap();
    
    // Use current time if timestamp is 0
    let now = if timestamp == 0 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    } else {
        timestamp
    };
    
    // Register assets with reasonable default configurations
    let debt_config = ReserveConfig {
        asset: debt_asset.to_string(),
        decimals: 6,
        collateral_factor_bps: 8000,      // 80%
        liquidation_threshold_bps: 8500,  // 85%
        liquidation_bonus_bps: 500,       // 5% bonus
        reserve_factor_bps: 1000,         // 10%
        flash_loan_fee_bps: 9,            // 0.09%
        borrow_enabled: true,
        deposit_enabled: true,
        flash_loan_enabled: true,
    };
    
    let collateral_config = ReserveConfig {
        asset: collateral_asset.to_string(),
        decimals: 7,
        collateral_factor_bps: 7500,      // 75%
        liquidation_threshold_bps: 8000,  // 80%
        liquidation_bonus_bps: 1000,      // 10% bonus
        reserve_factor_bps: 1000,         // 10%
        flash_loan_fee_bps: 9,            // 0.09%
        borrow_enabled: true,
        deposit_enabled: true,
        flash_loan_enabled: true,
    };
    
    protocol.register_asset("admin", debt_config.clone(), now).unwrap();
    protocol.register_asset("admin", collateral_config.clone(), now).unwrap();
    
    // Use current time if timestamp is 0 (already defined above, remove duplicate)
    
    if dry_run {
        println!("🔬 DRY RUN MODE - Simulating liquidation...\n");
        
        // Check if position is liquidatable
        match protocol.position(borrower, &oracle) {
            Ok(snapshot) => {
                println!("📊 Position Snapshot:");
                println!("   Collateral Value:   ${:.2}", snapshot.collateral_value as f64 / WAD as f64);
                println!("   Liquidation Value:  ${:.2}", snapshot.liquidation_value as f64 / WAD as f64);
                println!("   Debt Value:         ${:.2}", snapshot.debt_value as f64 / WAD as f64);
                println!("   Health Factor:      {:.4}", snapshot.health_factor as f64 / WAD as f64);
                println!();
                
                if snapshot.health_factor >= WAD {
                    println!("❌ Position is NOT liquidatable (health factor >= 1.0)");
                    println!("   The position is healthy and cannot be liquidated.");
                    return;
                }
                
                println!("✅ Position IS liquidatable (health factor < 1.0)");
                println!("   The position is undercollateralized and can be liquidated.\n");
                
                // Simulate the liquidation calculation
                println!("💰 Liquidation Calculation:");
                let repay_value = (repay_amount as f64 / WAD as f64) * (debt_price as f64 / WAD as f64);
                let bonus_multiplier = 1.0 + (collateral_config.liquidation_bonus_bps as f64 / 10000.0);
                let discounted_value = repay_value * bonus_multiplier;
                let seize_amount = (discounted_value * WAD as f64) / (collateral_price as f64);
                
                println!("   Repay Value:        ${:.2}", repay_value);
                println!("   Liquidation Bonus:  {}%", collateral_config.liquidation_bonus_bps as f64 / 100.0);
                println!("   Discounted Value:   ${:.2}", discounted_value);
                println!("   Seize Amount:       {:.6} {}", seize_amount / WAD as f64, collateral_asset);
                println!("   Liquidator Profit:  ${:.2}", discounted_value - repay_value);
            }
            Err(e) => {
                println!("❌ Error checking position: {:?}", e);
                return;
            }
        }
    } else {
        println!("⚡ EXECUTING LIQUIDATION...\n");
        
        match protocol.liquidate(
            liquidator,
            borrower,
            debt_asset,
            collateral_asset,
            repay_amount,
            &oracle,
            now,
        ) {
            Ok(result) => {
                println!("✅ Liquidation Successful!");
                println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                println!("Repaid Amount:         {:.6} {}", result.repaid_amount as f64 / WAD as f64, debt_asset);
                println!("Seized Collateral:     {:.6} {}", result.seized_collateral as f64 / WAD as f64, collateral_asset);
                println!("Liquidator Profit:     ${:.2}", result.liquidator_discount_value as f64 / WAD as f64);
                println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            }
            Err(e) => {
                println!("❌ Liquidation Failed!");
                println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                println!("Error: {:?}", e);
                println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                
                match e {
                    stellar_defi_toolkit::ProtocolError::PositionNotLiquidatable => {
                        println!("\n💡 Tip: The position's health factor is >= 1.0");
                        println!("   Use the 'check-liquidation' command to view position details.");
                    }
                    stellar_defi_toolkit::ProtocolError::InsufficientBalance => {
                        println!("\n💡 Tip: The borrower doesn't have enough collateral to seize.");
                    }
                    stellar_defi_toolkit::ProtocolError::InsufficientLiquidity => {
                        println!("\n💡 Tip: The protocol doesn't have enough liquidity for this liquidation.");
                    }
                    _ => {}
                }
            }
        }

fn check_liquidation_status(
    borrower: &str,
    debt_asset: &str,
    collateral_asset: &str,
    debt_price: i128,
    collateral_price: i128,
) {
    use stellar_defi_toolkit::{InterestRateModel, ReserveConfig, WAD};
    
    println!("🔍 Checking Liquidation Status");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Borrower:          {}", borrower);
    println!("Debt Asset:        {}", debt_asset);
    println!("Collateral Asset:  {}", collateral_asset);
    println!("Debt Price:        ${:.6}", debt_price as f64 / WAD as f64);
    println!("Collateral Price:  ${:.6}", collateral_price as f64 / WAD as f64);
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
    
    // Create a mock protocol for demonstration
    let model = InterestRateModel::default();
    let mut protocol = LendingProtocol::new("admin", "treasury", model);
    
    // Create a mock oracle with the provided prices
    let mut oracle = PriceOracle::new("oracle-admin");
    oracle.set_price("oracle-admin", debt_asset, debt_price).unwrap();
    oracle.set_price("oracle-admin", collateral_asset, collateral_price).unwrap();
    
    // Register assets with reasonable default configurations
    let debt_config = ReserveConfig {
        asset: debt_asset.to_string(),
        decimals: 6,
        collateral_factor_bps: 8000,
        liquidation_threshold_bps: 8500,
        liquidation_bonus_bps: 500,
        reserve_factor_bps: 1000,
        flash_loan_fee_bps: 9,
        borrow_enabled: true,
        deposit_enabled: true,
        flash_loan_enabled: true,
    };
    
    let collateral_config = ReserveConfig {
        asset: collateral_asset.to_string(),
        decimals: 7,
        collateral_factor_bps: 7500,
        liquidation_threshold_bps: 8000,
        liquidation_bonus_bps: 1000,
        reserve_factor_bps: 1000,
        flash_loan_fee_bps: 9,
        borrow_enabled: true,
        deposit_enabled: true,
        flash_loan_enabled: true,
    };
    
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    
    protocol.register_asset("admin", debt_config.clone(), now).unwrap();
    protocol.register_asset("admin", collateral_config.clone(), now).unwrap();
    
    match protocol.position(borrower, &oracle) {
        Ok(snapshot) => {
            println!("📊 Position Details:");
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            
            // Display supplied amounts
            println!("\n💰 Supplied Assets:");
            for (asset, amount) in &snapshot.supplied_amounts {
                println!("   {}: {:.6}", asset, *amount as f64 / WAD as f64);
            }
            
            // Display debt amounts
            println!("\n📉 Debt Assets:");
            for (asset, amount) in &snapshot.debt_amounts {
                println!("   {}: {:.6}", asset, *amount as f64 / WAD as f64);
            }
            
            // Display values
            println!("\n💵 Position Values:");
            println!("   Collateral Value:   ${:.2}", snapshot.collateral_value as f64 / WAD as f64);
            println!("   Liquidation Value:  ${:.2}", snapshot.liquidation_value as f64 / WAD as f64);
            println!("   Debt Value:         ${:.2}", snapshot.debt_value as f64 / WAD as f64);
            
            // Display health factor
            println!("\n🏥 Health Factor: {:.4}", snapshot.health_factor as f64 / WAD as f64);
            
            if snapshot.debt_value == 0 {
                println!("\n✅ Status: NO DEBT");
                println!("   The position has no outstanding debt.");
            } else if snapshot.health_factor >= WAD {
                println!("\n✅ Status: HEALTHY");
                println!("   The position is well-collateralized and cannot be liquidated.");
                
                let buffer = ((snapshot.health_factor as f64 / WAD as f64) - 1.0) * 100.0;
                println!("   Safety Buffer: {:.2}%", buffer);
            } else {
                println!("\n⚠️  Status: LIQUIDATABLE");
                println!("   The position is undercollateralized and can be liquidated!");
                
                let deficit = (1.0 - (snapshot.health_factor as f64 / WAD as f64)) * 100.0;
                println!("   Collateral Deficit: {:.2}%", deficit);
                
                println!("\n💡 Liquidation Opportunity:");
                println!("   You can liquidate this position to earn a liquidation bonus.");
                println!("   Use the 'liquidate' command to execute the liquidation.");
            }
            
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        }
        Err(e) => {
            println!("❌ Error checking position: {:?}", e);
        }
    }
}
    }
}

fn handle_lend_command(cmd: LendCommands) {
    use stellar_defi_toolkit::{
        InterestRateModel, PriceOracleSim, ReserveConfig, LendingProtocol, WAD,
    };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let model = InterestRateModel::default();
    let mut protocol = LendingProtocol::new(vec!["admin".to_string()], 1, "treasury", model);

    let default_config = |asset: &str| ReserveConfig {
        asset: asset.to_string(),
        decimals: 7,
        collateral_factor_bps: 8000,
        liquidation_threshold_bps: 8500,
        liquidation_bonus_bps: 500,
        reserve_factor_bps: 1000,
        flash_loan_fee_bps: 9,
        borrow_enabled: true,
        deposit_enabled: true,
        flash_loan_enabled: true,
        supply_cap: 0,
        borrow_cap: 0,
        interest_rate_model: None,
    };

    match cmd {
        LendCommands::Deposit { user, asset, amount } => {
            protocol.register_asset("admin", default_config(&asset), now).unwrap();

            println!("💰 Deposit");
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            println!("User:    {}", user);
            println!("Asset:   {}", asset);
            println!("Amount:  {:.6}", amount as f64 / WAD as f64);
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

            match protocol.deposit(&user, &asset, amount, now) {
                Ok(shares) => {
                    println!("✅ Deposit successful!");
                    println!("   Deposited: {:.6} {}", amount as f64 / WAD as f64, asset);
                    println!("   Shares:    {}", shares);
                }
                Err(e) => println!("❌ Deposit failed: {:?}", e),
            }
        }
        LendCommands::Withdraw { user, asset, amount } => {
            protocol.register_asset("admin", default_config(&asset), now).unwrap();
            protocol.deposit(&user, &asset, amount, now).unwrap();

            let mut oracle = PriceOracleSim::new("oracle-admin");
            oracle.set_price("oracle-admin", &asset, 1_000_000_000_000_000_000).unwrap();

            println!("💸 Withdraw");
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            println!("User:    {}", user);
            println!("Asset:   {}", asset);
            println!("Amount:  {:.6}", amount as f64 / WAD as f64);
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

            match protocol.withdraw(&user, &asset, amount, &oracle, now) {
                Ok(actual) => {
                    println!("✅ Withdrawal successful!");
                    println!("   Withdrew: {:.6} {}", actual as f64 / WAD as f64, asset);
                }
                Err(e) => println!("❌ Withdrawal failed: {:?}", e),
            }
        }
        LendCommands::Borrow { user, asset, amount } => {
            protocol.register_asset("admin", default_config(&asset), now).unwrap();
            let collateral_config = ReserveConfig {
                asset: "XLM".to_string(),
                decimals: 7,
                collateral_factor_bps: 7500,
                liquidation_threshold_bps: 8000,
                liquidation_bonus_bps: 1000,
                reserve_factor_bps: 1000,
                flash_loan_fee_bps: 9,
                borrow_enabled: true,
                deposit_enabled: true,
                flash_loan_enabled: true,
                supply_cap: 0,
                borrow_cap: 0,
                interest_rate_model: None,
            };
            protocol.register_asset("admin", collateral_config, now).unwrap();
            let collateral_amount = amount * 3;
            protocol.deposit(&user, "XLM", collateral_amount, now).unwrap();

            let mut oracle = PriceOracleSim::new("oracle-admin");
            oracle.set_price("oracle-admin", &asset, 1_000_000_000_000_000_000).unwrap();
            oracle.set_price("oracle-admin", "XLM", 500_000_000_000_000_000).unwrap();

            println!("📉 Borrow");
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            println!("User:    {}", user);
            println!("Asset:   {}", asset);
            println!("Amount:  {:.6}", amount as f64 / WAD as f64);
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

            match protocol.borrow(&user, &asset, amount, &oracle, now) {
                Ok(shares) => {
                    println!("✅ Borrow successful!");
                    println!("   Borrowed: {:.6} {}", amount as f64 / WAD as f64, asset);
                    println!("   Shares:   {}", shares);
                }
                Err(e) => println!("❌ Borrow failed: {:?}", e),
            }
        }
        LendCommands::Repay { user, asset, amount } => {
            protocol.register_asset("admin", default_config(&asset), now).unwrap();
            let collateral_config = ReserveConfig {
                asset: "XLM".to_string(),
                decimals: 7,
                collateral_factor_bps: 7500,
                liquidation_threshold_bps: 8000,
                liquidation_bonus_bps: 1000,
                reserve_factor_bps: 1000,
                flash_loan_fee_bps: 9,
                borrow_enabled: true,
                deposit_enabled: true,
                flash_loan_enabled: true,
                supply_cap: 0,
                borrow_cap: 0,
                interest_rate_model: None,
            };
            protocol.register_asset("admin", collateral_config, now).unwrap();
            let collateral_amount = amount * 3;
            protocol.deposit(&user, "XLM", collateral_amount, now).unwrap();

            let mut oracle = PriceOracleSim::new("oracle-admin");
            oracle.set_price("oracle-admin", &asset, 1_000_000_000_000_000_000).unwrap();
            oracle.set_price("oracle-admin", "XLM", 500_000_000_000_000_000).unwrap();

            protocol.borrow(&user, &asset, amount, &oracle, now).unwrap();

            println!("💳 Repay");
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            println!("User:    {}", user);
            println!("Asset:   {}", asset);
            println!("Amount:  {:.6}", amount as f64 / WAD as f64);
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

            match protocol.repay(&user, &user, &asset, amount, now) {
                Ok(actual) => {
                    println!("✅ Repayment successful!");
                    println!("   Repaid: {:.6} {}", actual as f64 / WAD as f64, asset);
                }
                Err(e) => println!("❌ Repayment failed: {:?}", e),
            }
        }
        LendCommands::Position { user } => {
            let mut oracle = PriceOracleSim::new("oracle-admin");
            oracle.set_price("oracle-admin", "USDC", 1_000_000_000_000_000_000).unwrap();
            oracle.set_price("oracle-admin", "XLM", 500_000_000_000_000_000).unwrap();

            // Register some default assets so the position query works
            protocol.register_asset("admin", default_config("USDC"), now).unwrap();
            protocol.register_asset("admin", default_config("XLM"), now).unwrap();

            println!("📊 Position");
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            println!("User:    {}", user);
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

            match protocol.position(&user, &oracle) {
                Ok(snapshot) => {
                    println!("💰 Supplied Assets:");
                    for (asset, amount) in &snapshot.supplied_amounts {
                        println!("   {}: {:.6}", asset, *amount as f64 / WAD as f64);
                    }
                    println!("\n📉 Debt Assets:");
                    for (asset, amount) in &snapshot.debt_amounts {
                        println!("   {}: {:.6}", asset, *amount as f64 / WAD as f64);
                    }
                    println!("\n💵 Position Values:");
                    println!("   Collateral Value:   ${:.2}", snapshot.collateral_value as f64 / WAD as f64);
                    println!("   Liquidation Value:  ${:.2}", snapshot.liquidation_value as f64 / WAD as f64);
                    println!("   Debt Value:         ${:.2}", snapshot.debt_value as f64 / WAD as f64);
                    println!("\n🏥 Health Factor: {:.4}", snapshot.health_factor as f64 / WAD as f64);

                    if snapshot.debt_value == 0 {
                        println!("\n✅ Status: NO DEBT");
                    } else if snapshot.health_factor >= WAD {
                        println!("\n✅ Status: HEALTHY");
                        let buffer = ((snapshot.health_factor as f64 / WAD as f64) - 1.0) * 100.0;
                        println!("   Safety Buffer: {:.2}%", buffer);
                    } else {
                        println!("\n⚠️  Status: LIQUIDATABLE");
                        let deficit = (1.0 - (snapshot.health_factor as f64 / WAD as f64)) * 100.0;
                        println!("   Collateral Deficit: {:.2}%", deficit);
                    }
                    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                }
                Err(e) => println!("❌ Error fetching position: {:?}", e),
            }
        }
    }
}
