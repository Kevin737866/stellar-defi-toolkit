use clap::{Parser, Subcommand};
use stellar_defi_toolkit::{InterestRateModel, LendingProtocol, PriceOracle};

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
    }
}
