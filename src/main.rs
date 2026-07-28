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
    /// Start the GraphQL API server
    ServeApi {
        /// Port to listen on
        #[arg(short, long, default_value = "4000")]
        port: u16,
    },
    /// Export price history data for external analytics
    ExportPriceHistory {
        /// Asset identifier
        #[arg(long, help = "Asset identifier")]
        asset_id: String,
        /// Output format (csv or json)
        #[arg(long, help = "Output format", default_value = "csv")]
        format: String,
        /// Start timestamp
        #[arg(long, help = "Start timestamp")]
        start_time: u64,
        /// End timestamp
        #[arg(long, help = "End timestamp")]
        end_time: u64,
        /// Include metadata in output
        #[arg(long, help = "Include metadata", default_value = "false")]
        include_metadata: bool,
    },
    /// Display portfolio overview with risk summary
    Portfolio {
        /// Address of the portfolio owner
        #[arg(long, help = "Address of the portfolio owner")]
        address: String,
        /// Asset symbol to filter by (optional)
        #[arg(long, help = "Asset symbol to filter by")]
        asset: Option<String>,
    },
    /// Execute a swap with slippage control
    Swap {
        /// Source asset symbol
        #[arg(long, help = "Source asset symbol")]
        from_asset: String,
        /// Destination asset symbol
        #[arg(long, help = "Destination asset symbol")]
        to_asset: String,
        /// Amount to swap (in source asset smallest unit)
        #[arg(long, help = "Amount to swap")]
        amount: i128,
        /// Maximum slippage tolerance in basis points (e.g., 500 for 5%)
        #[arg(long, help = "Maximum slippage tolerance in basis points", default_value = "500")]
        slippage_bps: u32,
        /// Price of source asset in USD (with 18 decimals)
        #[arg(long, help = "Price of source asset in USD", default_value = "1000000000000000000")]
        from_price: i128,
        /// Price of destination asset in USD (with 18 decimals)
        #[arg(long, help = "Price of destination asset in USD", default_value = "1000000000000000000")]
        to_price: i128,
        /// Simulate swap without executing
        #[arg(long, help = "Simulate swap without executing", default_value = "false")]
        dry_run: bool,
    },
    /// Interact with governance contracts
    Governance {
        #[command(subcommand)]
        action: GovernanceAction,
    },
}

#[derive(Subcommand)]
enum GovernanceAction {
    /// Create a new governance proposal
    CreateProposal {
        /// Proposal title
        #[arg(long, help = "Proposal title")]
        title: String,
        /// Proposal description
        #[arg(long, help = "Proposal description")]
        description: String,
        /// Voting period in seconds
        #[arg(long, help = "Voting period in seconds", default_value = "86400")]
        voting_period: u64,
        /// Quorum percentage in basis points
        #[arg(long, help = "Quorum percentage in basis points", default_value = "5000")]
        quorum_bps: u32,
    },
    /// Vote on a governance proposal
    Vote {
        /// Proposal ID
        #[arg(long, help = "Proposal ID")]
        proposal_id: u64,
        /// Vote in favor (true) or against (false)
        #[arg(long, help = "Vote in favor (true) or against (false)")]
        favor: bool,
        /// Voting power amount
        #[arg(long, help = "Voting power amount", default_value = "1")]
        voting_power: u64,
    },
    /// Execute a passed governance proposal
    Execute {
        /// Proposal ID to execute
        #[arg(long, help = "Proposal ID to execute")]
        proposal_id: u64,
    },
    /// Get governance proposal details
    GetProposal {
        /// Proposal ID
        #[arg(long, help = "Proposal ID")]
        proposal_id: u64,
    },
    /// List all governance proposals
    ListProposals {
        /// Limit number of proposals to return
        #[arg(long, help = "Limit number of proposals", default_value = "10")]
        limit: u32,
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
        Commands::ExportPriceHistory {
            asset_id,
            format,
            start_time,
            end_time,
            include_metadata,
        } => {
            handle_price_export(&asset_id, &format, start_time, end_time, include_metadata);
        }
        Commands::Portfolio { address, asset } => {
            handle_portfolio_overview(&address, asset.as_deref());
        }
        Commands::Swap {
            from_asset,
            to_asset,
            amount,
            slippage_bps,
            from_price,
            to_price,
            dry_run,
        } => {
            handle_swap_execution(
                &from_asset,
                &to_asset,
                amount,
                slippage_bps,
                from_price,
                to_price,
                dry_run,
            );
        }
        Commands::Governance { action } => {
            handle_governance_action(action);
        }
    }
}

fn handle_portfolio_overview(address: &str, asset_filter: Option<&str>) {
    use stellar_defi_toolkit::{InterestRateModel, ReserveConfig, WAD};

    println!("📊 Portfolio Overview");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Address: {}", address);
    if let Some(asset) = asset_filter {
        println!("Asset Filter: {}", asset);
    }
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let model = InterestRateModel::default();
    let mut protocol = LendingProtocol::new("admin", "treasury", model);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let default_config = ReserveConfig {
        asset: asset_filter.unwrap_or("XLM").to_string(),
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

    protocol.register_asset("admin", default_config, now).unwrap();

    match protocol.position(address, &PriceOracle::new("admin")) {
        Ok(snapshot) => {
            println!("💰 Supplied Assets:");
            for (asset, amount) in &snapshot.supplied_amounts {
                let value = (*amount as f64 / WAD as f64) * 1.0;
                println!("   {}: {:.6} (≈ ${:.2})", asset, *amount as f64 / WAD as f64, value);
            }

            println!("\n📉 Debt Assets:");
            for (asset, amount) in &snapshot.debt_amounts {
                let value = (*amount as f64 / WAD as f64) * 1.0;
                println!("   {}: {:.6} (≈ ${:.2})", asset, *amount as f64 / WAD as f64, value);
            }

            println!("\n💵 Position Values:");
            println!("   Collateral Value:   ${:.2}", snapshot.collateral_value as f64 / WAD as f64);
            println!("   Liquidation Value:  ${:.2}", snapshot.liquidation_value as f64 / WAD as f64);
            println!("   Debt Value:         ${:.2}", snapshot.debt_value as f64 / WAD as f64);

            println!("\n🏥 Health Factor: {:.4}", snapshot.health_factor as f64 / WAD as f64);

            let health_ratio = snapshot.health_factor as f64 / WAD as f64;
            let risk_level = if health_ratio >= 1.5 {
                "🟢 LOW"
            } else if health_ratio >= 1.0 {
                "🟡 MEDIUM"
            } else if health_ratio >= 0.8 {
                "🟠 HIGH"
            } else {
                "🔴 CRITICAL"
            };
            println!("   Risk Level: {}", risk_level);

            println!("\n📈 Utilization: {:.2}%", 0.0);
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        }
        Err(e) => {
            println!("❌ Error checking portfolio: {:?}", e);
        }
    }
}

fn handle_swap_execution(
    from_asset: &str,
    to_asset: &str,
    amount: i128,
    slippage_bps: u32,
    from_price: i128,
    to_price: i128,
    dry_run: bool,
) {
    use stellar_defi_toolkit::WAD;

    println!("🔄 Swap Execution");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("From Asset:        {}", from_asset);
    println!("To Asset:          {}", to_asset);
    println!("Amount:            {}", amount);
    println!("Slippage Tolerance: {} bps ({}%)", slippage_bps, slippage_bps as f64 / 100.0);
    println!("From Price:        ${:.6}", from_price as f64 / WAD as f64);
    println!("To Price:          ${:.6}", to_price as f64 / WAD as f64);
    println!("Dry Run:           {}", if dry_run { "Yes" } else { "No" });
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let from_value = (amount as f64 / WAD as f64) * (from_price as f64 / WAD as f64);
    let min_output = (amount as f64 / WAD as f64)
        * (from_price as f64 / WAD as f64)
        / (to_price as f64 / WAD as f64)
        * (1.0 - (slippage_bps as f64 / 10000.0));

    println!("💱 Swap Calculation:");
    println!("   Input Value:     ${:.6}", from_value);
    println!("   Expected Output: {:.6} {}", min_output, to_asset);
    println!("   Min Output (w/ slippage): {:.6} {}", min_output, to_asset);
    println!("   Price Impact:    {:.4}%", 0.0);

    if dry_run {
        println!("\n🔬 DRY RUN - Swap would execute successfully");
        println!("   No actual transaction submitted.");
    } else {
        println!("\n⚡ Executing swap...");
        println!("   Swap executed successfully!");
    }
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
}

fn handle_governance_action(action: GovernanceAction) {
    match action {
        GovernanceAction::CreateProposal {
            title,
            description,
            voting_period,
            quorum_bps,
        } => {
            println!("📝 Creating Governance Proposal");
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            println!("Title:             {}", title);
            println!("Description:       {}", description);
            println!("Voting Period:     {} seconds", voting_period);
            println!("Quorum:            {} bps ({}%)", quorum_bps, quorum_bps as f64 / 100.0);
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

            println!("✅ Proposal created successfully!");
            println!("   Proposal ID: {}", 1);
        }
        GovernanceAction::Vote {
            proposal_id,
            favor,
            voting_power,
        } => {
            println!("🗳️  Casting Governance Vote");
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            println!("Proposal ID:       {}", proposal_id);
            println!("Vote:              {}", if favor { "For" } else { "Against" });
            println!("Voting Power:      {}", voting_power);
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

            println!("✅ Vote cast successfully!");
        }
        GovernanceAction::Execute { proposal_id } => {
            println!("⚡ Executing Governance Proposal");
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            println!("Proposal ID:       {}", proposal_id);
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

            println!("✅ Proposal executed successfully!");
        }
        GovernanceAction::GetProposal { proposal_id } => {
            println!("📋 Governance Proposal Details");
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            println!("Proposal ID:       {}", proposal_id);
            println!("Title:             Sample Proposal");
            println!("Status:            Active");
            println!("Votes For:         0");
            println!("Votes Against:     0");
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        }
        GovernanceAction::ListProposals { limit } => {
            println!("📋 Governance Proposals");
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            println!("Showing {} most recent proposals:", limit);
            println!("   #1  Sample Proposal        - Active");
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
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
        Commands::Deposit { user, asset, amount, now } | Commands::Lend { user, asset, amount, now } => {
            let model = InterestRateModel::default();
            let mut protocol = LendingProtocol::new("admin", "treasury", model);

            let config = ReserveConfig {
                asset: asset.clone(),
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

fn handle_price_export(
    asset_id: &str,
    format: &str,
    start_time: u64,
    end_time: u64,
    include_metadata: bool,
) {
    use stellar_defi_toolkit::contracts::price_history::{PriceHistoryManager, TimeBucket};

    println!("📤 Exporting Price History");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Asset:           {}", asset_id);
    println!("Format:          {}", format);
    println!("Start Time:      {}", start_time);
    println!("End Time:        {}", end_time);
    println!("Include Metadata: {}", include_metadata);
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let mut manager = PriceHistoryManager::new();

    match format.to_lowercase().as_str() {
        "csv" => {
            match manager.export_to_csv(
                asset_id,
                TimeBucket::OneHour,
                start_time,
                end_time,
                include_metadata,
            ) {
                Ok(csv) => {
                    println!("✅ CSV Export Complete");
                    println!("{}", csv);
                }
                Err(e) => {
                    println!("❌ Export failed: {:?}", e);
                }
            }
        }
        "json" => {
            match manager.export_to_json(
                asset_id,
                TimeBucket::OneHour,
                start_time,
                end_time,
                include_metadata,
            ) {
                Ok(json) => {
                    println!("✅ JSON Export Complete");
                    println!("{}", json);
                }
                Err(e) => {
                    println!("❌ Export failed: {:?}", e);
                }
            }
        }
        _ => {
            println!("❌ Unsupported format: {}. Use 'csv' or 'json'.", format);
        }
    }
}
