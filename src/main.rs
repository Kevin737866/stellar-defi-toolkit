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
