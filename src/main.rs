use clap::{Parser, Subcommand};
use log::info;
use stellar_defi_toolkit::contracts::{TokenContract, LiquidityPoolContract};
use stellar_defi_toolkit::utils::StellarClient;

#[derive(Parser)]
#[command(name = "stellar-defi-toolkit")]
#[command(about = "A comprehensive DeFi toolkit for Stellar blockchain")]
#[command(version = "0.1.0")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Deploy a new token contract
    DeployToken {
        /// Token name
        #[arg(short, long)]
        name: String,
        /// Token symbol
        #[arg(short, long)]
        symbol: String,
        /// Initial supply
        #[arg(short, long, default_value = "1000000")]
        supply: u64,
    },
    /// Create a liquidity pool
    CreatePool {
        /// Token A contract ID
        #[arg(short, long)]
        token_a: String,
        /// Token B contract ID
        #[arg(short, long)]
        token_b: String,
    },
    /// Get contract information
    GetInfo {
        /// Contract ID
        #[arg(short, long)]
        contract_id: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    
    let cli = Cli::parse();
    let client = StellarClient::new().await?;

    match cli.command {
        Commands::DeployToken { name, symbol, supply } => {
            info!("Deploying token contract: {} ({})", name, symbol);
            let token_contract = TokenContract::new(name, symbol, supply);
            let contract_id = token_contract.deploy(&client).await?;
            println!("Token deployed successfully! Contract ID: {}", contract_id);
        }
        Commands::CreatePool { token_a, token_b } => {
            info!("Creating liquidity pool between {} and {}", token_a, token_b);
            let pool = LiquidityPoolContract::new(token_a, token_b);
            let contract_id = pool.deploy(&client).await?;
            println!("Liquidity pool created! Contract ID: {}", contract_id);
        }
        Commands::GetInfo { contract_id } => {
            info!("Getting information for contract: {}", contract_id);
            let info = client.get_contract_info(&contract_id).await?;
            println!("Contract Info: {:#?}", info);
        }
    }

    Ok(())
}
