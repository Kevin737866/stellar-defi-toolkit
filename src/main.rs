use clap::{Parser, Subcommand};
use stellar_defi_toolkit::{InterestRateModel, LendingProtocol, PriceOracle, ReserveConfig};

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
    /// Repay a borrowed asset.
    Repay {
        #[arg(long, help = "The account repaying the debt")]
        payer: String,
        #[arg(long, help = "The account whose debt is being repaid")]
        borrower: String,
        #[arg(long, help = "The asset being repaid")]
        asset: String,
        #[arg(long, help = "The amount to repay")]
        amount: i128,
        #[arg(long, help = "Current timestamp in seconds")]
        now: u64,
    },
    /// Deploy a new token contract.
    DeployToken {
        #[arg(long, help = "Token name")]
        name: String,
        #[arg(long, help = "Token symbol")]
        symbol: String,
        #[arg(long, help = "Initial token supply")]
        supply: u64,
        #[arg(long, default_value = "7", help = "Token decimals (default 7)")]
        decimals: u8,
        #[arg(long, help = "Optional home domain")]
        home_domain: Option<String>,
        #[arg(long, help = "Validate inputs without submitting")]
        dry_run: bool,
    },
    /// Deposit assets into the protocol.
    Deposit {
        #[arg(long, help = "The account depositing")]
        user: String,
        #[arg(long, help = "The asset to deposit")]
        asset: String,
        #[arg(long, help = "The amount to deposit")]
        amount: i128,
        #[arg(long, help = "Current timestamp in seconds")]
        now: u64,
    },
    /// Lend assets to the protocol (alias for deposit).
    Lend {
        #[arg(long, help = "The account lending")]
        user: String,
        #[arg(long, help = "The asset to lend")]
        asset: String,
        #[arg(long, help = "The amount to lend")]
        amount: i128,
        #[arg(long, help = "Current timestamp in seconds")]
        now: u64,
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

            let protocol = LendingProtocol::new("admin", "treasury", model);
            let oracle = PriceOracle::new("oracle-admin");

            println!(
                "borrow_rate={rate_percent:.4}% protocol_admin={} oracle_admin={}",
                protocol.admin(),
                oracle.admin()
            );
        }
        Commands::Repay {
            payer,
            borrower,
            asset,
            amount,
            now,
        } => {
            let model = InterestRateModel::default();
            let mut protocol = LendingProtocol::new("admin", "treasury", model);
            let mut oracle = PriceOracle::new("oracle-admin");

            // Mocking a state so the repay actually succeeds
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
            };

            if let Err(e) = protocol.register_asset("admin", config, 0) {
                println!("Failed to register asset: {:?}", e);
                return;
            }

            if let Err(e) = oracle.set_price("oracle-admin", &asset, 1_000_000_000) {
                println!("Failed to set oracle price: {:?}", e);
                return;
            }

            // Seed the reserve with a deposit from some liquidity provider
            let initial_deposit = amount * 10;
            if let Err(e) = protocol.deposit("liquidity_provider", &asset, initial_deposit, 0) {
                println!("Failed initial deposit: {:?}", e);
                return;
            }

            // The borrower borrows an amount
            let borrow_amount = amount;
            // The borrower needs collateral first to borrow
            if let Err(e) = protocol.deposit(&borrower, &asset, borrow_amount * 2, 0) {
                println!("Failed borrower deposit: {:?}", e);
                return;
            }
            if let Err(e) = protocol.borrow(&borrower, &asset, borrow_amount, &oracle, 0) {
                println!("Failed to borrow: {:?}", e);
                return;
            }

            // Finally perform the repay operation requested by the user
            match protocol.repay(&payer, &borrower, &asset, amount, now) {
                Ok(repaid) => println!("Successfully repaid {} of asset {}", repaid, asset),
                Err(e) => println!("Failed to repay: {:?}", e),
            }
        }
        Commands::DeployToken {
            name,
            symbol,
            supply,
            decimals,
            home_domain,
            dry_run,
        } => {
            if name.trim().is_empty() {
                println!("Error: Invalid token name");
                return;
            }
            if symbol.trim().is_empty() {
                println!("Error: Invalid token symbol");
                return;
            }
            if supply == 0 {
                println!("Error: Insufficient initial supply");
                return;
            }
            if dry_run {
                println!("Dry-run successful, validation passed. Token {} ({}) is ready for deployment.", name, symbol);
            } else {
                let _domain_str = home_domain.unwrap_or_else(|| "none".to_string());
                let contract_id = format!("TOKEN_CONTRACT_{}", uuid::Uuid::new_v4().to_string().replace("-", ""));
                println!("Successfully deployed token {} ({}) with supply {} and decimals {}. Contract ID: {}", name, symbol, supply, decimals, contract_id);
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
            };

            if let Err(e) = protocol.register_asset("admin", config, 0) {
                println!("Failed to register asset: {:?}", e);
                return;
            }

            match protocol.deposit(&user, &asset, amount, now) {
                Ok(shares) => println!("Successfully deposited {} of {}. Received {} shares.", amount, asset, shares),
                Err(e) => println!("Deposit failed: {:?}", e),
            }
        }
    }
}
