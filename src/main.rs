use clap::{Parser, Subcommand};
use stellar_defi_toolkit::{InterestRateModel, LendingProtocol, PriceOracleSim, ReserveConfig};

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
}

fn main() {
    env_logger::init();

    let cli = Cli::parse();
    match cli.command {
        Commands::QuoteRate { utilization_bps } => {
            let model = InterestRateModel::default();
            let utilization = i128::from(utilization_bps) * 100_000;
            let yearly_rate = model.borrow_rate(utilization);
            let rate_percent = yearly_rate as f64 / 10_000_000.0 * 100.0;

            let protocol = LendingProtocol::new(vec!["admin".to_string()], 1, "treasury", model);
            let oracle = PriceOracle::new("oracle-admin");

            println!(
                "borrow_rate={rate_percent:.4}% protocol_admins={:?} oracle_admin={}",
                protocol.admins(),
                oracle.admin()
            );
        }
        
        Commands::Liquidate {
            liquidator,
            borrower,
            debt_asset,
            collateral_asset,
            repay_amount,
            debt_price,
            collateral_price,
            timestamp,
            dry_run,
        } => {
            handle_liquidation(
                &liquidator,
                &borrower,
                &debt_asset,
                &collateral_asset,
                repay_amount,
                debt_price,
                collateral_price,
                timestamp,
                dry_run,
            );
        }
        
        Commands::CheckLiquidation {
            borrower,
            debt_asset,
            collateral_asset,
            debt_price,
            collateral_price,
        } => {
            check_liquidation_status(
                &borrower,
                &debt_asset,
                &collateral_asset,
                debt_price,
                collateral_price,
            );
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
        Commands::Repay {
            payer,
            borrower,
            debt_asset,
            collateral_asset,
            repay_amount,
            debt_price,
            collateral_price,
            timestamp,
            dry_run,
        } => {
            handle_liquidation(
                &liquidator,
                &borrower,
                &debt_asset,
                &collateral_asset,
                repay_amount,
                debt_price,
                collateral_price,
                timestamp,
                dry_run,
            );
        }
        
        Commands::CheckLiquidation {
            borrower,
            debt_asset,
            collateral_asset,
            debt_price,
            collateral_price,
        } => {
            check_liquidation_status(
                &borrower,
                &debt_asset,
                &collateral_asset,
                debt_price,
                collateral_price,
            );
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
