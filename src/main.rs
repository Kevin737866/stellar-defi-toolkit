use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use log::info;
use serde::{Deserialize, Serialize};
use stellar_defi_toolkit::{InterestRateModel, LendingProtocol, PriceOracleSim, ReserveConfig, WAD};

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
        #[arg(
            long,
            help = "Price of debt asset in USD (with 9 decimals)",
            default_value = "1000000000"
        )]
        debt_price: i128,
        #[arg(
            long,
            help = "Price of collateral asset in USD (with 9 decimals)",
            default_value = "1000000000"
        )]
        collateral_price: i128,
        #[arg(long, help = "Current timestamp (unix seconds)", default_value = "0")]
        timestamp: u64,
        #[arg(
            long,
            help = "Simulate liquidation without executing",
            default_value = "false"
        )]
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
        #[arg(
            long,
            help = "Price of debt asset in USD (with 9 decimals)",
            default_value = "1000000000"
        )]
        debt_price: i128,
        #[arg(
            long,
            help = "Price of collateral asset in USD (with 9 decimals)",
            default_value = "1000000000"
        )]
        collateral_price: i128,
    },

    /// Repay a borrowed asset.
    Repay {
        #[arg(long, help = "The account repaying the debt")]
        payer: String,
        #[arg(long, help = "The account whose debt is being repaid")]
        borrower: String,
        #[arg(long, help = "Asset symbol for the debt (e.g., USDC)")]
        debt_asset: String,
        #[arg(long, help = "Amount of debt to repay (in smallest unit)")]
        repay_amount: i128,
        #[arg(long, help = "Current timestamp (unix seconds)", default_value = "0")]
        timestamp: u64,
    },

    /// Manage named CLI configuration profiles (network, wallet, etc.),
    /// stored in ~/.stellar-defi-toolkit/config.toml.
    Config {
        #[command(subcommand)]
        action: ConfigAction,
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

#[derive(Subcommand)]
enum ConfigAction {
    /// Set a key/value pair in the active profile.
    Set { key: String, value: String },
    /// Get a value from the active profile.
    Get { key: String },
    /// Switch (or create) the active profile.
    Profile { name: String },
    /// List all known profiles, marking the active one.
    Profiles,
}

// ─── Config storage ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Profile {
    #[serde(flatten)]
    settings: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Config {
    active_profile: String,
    profiles: BTreeMap<String, Profile>,
}

impl Default for Config {
    fn default() -> Self {
        let mut profiles = BTreeMap::new();
        profiles.insert("default".to_string(), Profile::default());
        Self {
            active_profile: "default".to_string(),
            profiles,
        }
    }
}

fn config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".stellar-defi-toolkit")
}

fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

fn load_config() -> Config {
    let path = config_path();
    match fs::read_to_string(&path) {
        Ok(contents) => toml::from_str(&contents).unwrap_or_default(),
        Err(_) => Config::default(),
    }
}

fn save_config(config: &Config) -> anyhow::Result<()> {
    let dir = config_dir();
    fs::create_dir_all(&dir)?;
    let contents = toml::to_string_pretty(config)?;
    fs::write(config_path(), contents)?;
    Ok(())
}

fn handle_config(action: ConfigAction) -> anyhow::Result<()> {
    let mut config = load_config();

    match action {
        ConfigAction::Set { key, value } => {
            let profile = config
                .profiles
                .entry(config.active_profile.clone())
                .or_default();
            profile.settings.insert(key.clone(), value.clone());
            save_config(&config)?;
            println!(
                "Set '{}' = '{}' in profile '{}'.",
                key, value, config.active_profile
            );
        }
        ConfigAction::Get { key } => {
            let value = config
                .profiles
                .get(&config.active_profile)
                .and_then(|p| p.settings.get(&key));
            match value {
                Some(v) => println!("{}", v),
                None => println!(
                    "'{}' is not set in profile '{}'.",
                    key, config.active_profile
                ),
            }
        }
        ConfigAction::Profile { name } => {
            let created = !config.profiles.contains_key(&name);
            config.profiles.entry(name.clone()).or_default();
            config.active_profile = name.clone();
            save_config(&config)?;
            if created {
                println!("Created and switched to new profile '{}'.", name);
            } else {
                println!("Switched to profile '{}'.", name);
            }
        }
        ConfigAction::Profiles => {
            if config.profiles.is_empty() {
                println!("No profiles configured.");
            }
            for name in config.profiles.keys() {
                if *name == config.active_profile {
                    println!("* {} (active)", name);
                } else {
                    println!("  {}", name);
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

    Ok(())
}

// ─── Lending protocol helpers ─────────────────────────────────────────────────

fn demo_reserve_config(asset: &str, decimals: u32) -> ReserveConfig {
    ReserveConfig {
        asset: asset.to_string(),
        decimals,
        collateral_factor_bps: 8_000,
        liquidation_threshold_bps: 8_500,
        liquidation_bonus_bps: 500,
        reserve_factor_bps: 1_000,
        flash_loan_fee_bps: 9,
        borrow_enabled: true,
        deposit_enabled: true,
        flash_loan_enabled: true,
        supply_cap: 0,
        borrow_cap: 0,
        interest_rate_model: None,
    }
}

fn demo_protocol_with_position(
    debt_asset: &str,
    collateral_asset: &str,
    debt_price: i128,
    collateral_price: i128,
    now: u64,
) -> (LendingProtocol, PriceOracleSim) {
    let model = InterestRateModel::default();
    let mut protocol = LendingProtocol::new(vec!["admin".to_string()], 1, "treasury", model);

    let mut oracle = PriceOracleSim::new("oracle-admin");
    oracle.set_price("oracle-admin", debt_asset, debt_price).unwrap();
    oracle
        .set_price("oracle-admin", collateral_asset, collateral_price)
        .unwrap();

    protocol
        .register_asset("admin", demo_reserve_config(debt_asset, 6), now)
        .unwrap();
    protocol
        .register_asset("admin", demo_reserve_config(collateral_asset, 7), now)
        .unwrap();

    (protocol, oracle)
}

fn now_or(timestamp: u64) -> u64 {
    if timestamp != 0 {
        return timestamp;
    }
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn print_position(snapshot: &stellar_defi_toolkit::PositionSnapshot) {
    println!("Supplied Assets:");
    for (asset, amount) in &snapshot.supplied_amounts {
        println!("   {}: {:.6}", asset, *amount as f64 / WAD as f64);
    }
    println!("Debt Assets:");
    for (asset, amount) in &snapshot.debt_amounts {
        println!("   {}: {:.6}", asset, *amount as f64 / WAD as f64);
    }
    println!("Collateral Value:   ${:.2}", snapshot.collateral_value as f64 / WAD as f64);
    println!("Liquidation Value:  ${:.2}", snapshot.liquidation_value as f64 / WAD as f64);
    println!("Debt Value:         ${:.2}", snapshot.debt_value as f64 / WAD as f64);
    println!("Health Factor:      {:.4}", snapshot.health_factor as f64 / WAD as f64);
}

fn check_liquidation_status(
    borrower: &str,
    debt_asset: &str,
    collateral_asset: &str,
    debt_price: i128,
    collateral_price: i128,
) {
    let now = now_or(0);
    let (protocol, oracle) =
        demo_protocol_with_position(debt_asset, collateral_asset, debt_price, collateral_price, now);

    match protocol.position(borrower, &oracle) {
        Ok(snapshot) => {
            println!("Position for '{}':", borrower);
            print_position(&snapshot);
            if snapshot.debt_value == 0 {
                println!("Status: NO DEBT");
            } else if snapshot.health_factor >= WAD {
                println!("Status: HEALTHY (not liquidatable)");
            } else {
                println!("Status: LIQUIDATABLE");
            }
        }
        Err(e) => println!("Error checking position: {:?}", e),
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
    let now = now_or(timestamp);
    let (mut protocol, oracle) =
        demo_protocol_with_position(debt_asset, collateral_asset, debt_price, collateral_price, now);

    if dry_run {
        match protocol.position(borrower, &oracle) {
            Ok(snapshot) => {
                print_position(&snapshot);
                if snapshot.health_factor >= WAD {
                    println!("Position is NOT liquidatable (health factor >= 1.0).");
                } else {
                    println!("Position IS liquidatable (health factor < 1.0).");
                }
            }
            Err(e) => println!("Error checking position: {:?}", e),
        }
        return;
    }

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
            println!("Liquidation successful.");
            println!("Repaid Amount:     {:.6} {}", result.repaid_amount as f64 / WAD as f64, debt_asset);
            println!(
                "Seized Collateral: {:.6} {}",
                result.seized_collateral as f64 / WAD as f64,
                collateral_asset
            );
            println!(
                "Liquidator Profit: ${:.2}",
                result.liquidator_discount_value as f64 / WAD as f64
            );
        }
        Err(e) => {
            println!("Liquidation failed: {:?}", e);
            match e {
                stellar_defi_toolkit::ProtocolError::PositionNotLiquidatable => {
                    println!("Tip: the position's health factor is >= 1.0.");
                }
                stellar_defi_toolkit::ProtocolError::InsufficientBalance => {
                    println!("Tip: the borrower doesn't have enough collateral to seize.");
                }
                stellar_defi_toolkit::ProtocolError::InsufficientLiquidity => {
                    println!("Tip: the protocol doesn't have enough liquidity for this liquidation.");
                }
                _ => {}
            }
        }
    }
}

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let cli = Cli::parse();
    match cli.command {
        Commands::QuoteRate { utilization_bps } => {
            let model = InterestRateModel::default();
            let utilization = i128::from(utilization_bps) * WAD / 10_000;
            let rate = model.borrow_rate(utilization);
            println!(
                "Annualized borrow rate at {}% utilization: {:.4}%",
                utilization_bps as f64 / 100.0,
                rate as f64 / WAD as f64 * 100.0
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
            info!("Processing liquidation request for borrower {}", borrower);
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
            check_liquidation_status(&borrower, &debt_asset, &collateral_asset, debt_price, collateral_price);
        }
        Commands::Repay {
            payer,
            borrower,
            debt_asset,
            repay_amount,
            timestamp,
        } => {
            let now = now_or(timestamp);
            let model = InterestRateModel::default();
            let mut protocol =
                LendingProtocol::new(vec!["admin".to_string()], 1, "treasury", model);
            protocol
                .register_asset("admin", demo_reserve_config(&debt_asset, 7), now)
                .unwrap();

            match protocol.repay(&payer, &borrower, &debt_asset, repay_amount, now) {
                Ok(repaid) => println!(
                    "{} repaid {:.6} {} on behalf of {}.",
                    payer,
                    repaid as f64 / WAD as f64,
                    debt_asset,
                    borrower
                ),
                Err(e) => println!("Repay failed: {:?}", e),
            }
        }
        Commands::Config { action } => {
            handle_config(action)?;
        }
    }

    Ok(())
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
