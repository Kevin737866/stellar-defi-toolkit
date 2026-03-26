//! Example: Yield Farming Vault
//!
//! Demonstrates deploying and interacting with the YieldVaultContract:
//! - Initializing the vault with strategies
//! - Depositing assets and receiving shares
//! - Harvesting and auto-compounding rewards
//! - Switching strategies based on yield optimization
//! - Emergency pause and exit

use stellar_defi_toolkit::contracts::vault::YieldVaultContract;
use stellar_defi_toolkit::types::vault::{VaultStrategy, StrategyType};
use soroban_sdk::{Address, Env, Symbol, String as SorobanString};
use soroban_sdk::testutils::Address as _;

fn main() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);

    // ── 1. Create and initialize the vault ───────────────────────────────────
    let mut vault =
        YieldVaultContract::new(
            &env, 
            SorobanString::from_str(&env, "USDC_CONTRACT"), 
            SorobanString::from_str(&env, "vUSDC_CONTRACT")
        )
        .initialize(&env, admin.clone(), treasury.clone(), 1000) // 10% performance fee
        .expect("vault initialization failed");

    println!("Vault initialized");
    println!("  Share price: {:.4}", vault.get_share_price());

    // ── 2. Register yield strategies ─────────────────────────────────────────
    let staking_strategy = VaultStrategy {
        name: Symbol::new(&env, "XLM_STAK"),
        contract_address: Address::generate(&env),
        strategy_type: StrategyType::Staking,
        estimated_apy: 850, // 8.5% in basis points
        allocated_amount: 0,
        active: true,
    };

    let lp_strategy = VaultStrategy {
        name: Symbol::new(&env, "USDC_XLM"),
        contract_address: Address::generate(&env),
        strategy_type: StrategyType::LiquidityPool,
        estimated_apy: 1820, // 18.2%
        allocated_amount: 0,
        active: true,
    };

    let lending_strategy = VaultStrategy {
        name: Symbol::new(&env, "USDC_LEND"),
        contract_address: Address::generate(&env),
        strategy_type: StrategyType::Lending,
        estimated_apy: 600, // 6.0%
        allocated_amount: 0,
        active: true,
    };

    vault.add_strategy(staking_strategy).expect("add staking strategy");
    vault.add_strategy(lp_strategy).expect("add lp strategy");
    vault.add_strategy(lending_strategy).expect("add lending strategy");

    println!("\nStrategies registered: {}", vault.get_info(&env).strategy_count);

    // ── 3. Deposit assets ────────────────────────────────────────────────────
    let deposit_a = vault
        .deposit(user_a.clone(), 1_000_000_000) // 1000 USDC (7 decimals)
        .expect("deposit user_a");

    println!("\nUser A deposited:");
    println!("  Amount: {}", deposit_a.amount_deposited);
    println!("  Shares minted: {}", deposit_a.shares_minted);
    println!("  Share price: {:.4}", deposit_a.share_price);

    let deposit_b = vault
        .deposit(user_b.clone(), 500_000_000) // 500 USDC
        .expect("deposit user_b");

    println!("\nUser B deposited:");
    println!("  Amount: {}", deposit_b.amount_deposited);
    println!("  Shares minted: {}", deposit_b.shares_minted);

    // ── 4. Simulate yield accrual and harvest ────────────────────────────────
    // Simulate time passing and rewards accumulating
    vault.total_assets += 50_000_000; // +50 USDC yield

    let harvest = vault.harvest().expect("harvest");
    println!("\nHarvest result:");
    println!("  Raw rewards: {}", harvest.raw_rewards);
    println!("  Performance fee (10%): {}", harvest.performance_fee);
    println!("  Net compounded: {}", harvest.compounded_amount);
    println!("  New total assets: {}", harvest.new_total_assets);

    // ── 5. Yield optimization ────────────────────────────────────────────────
    let optimal_idx = vault.get_optimal_strategy_index();
    println!("\nOptimal strategy index: {} (USDC/XLM LP @ 18.2% APY)", optimal_idx);

    let switched = vault.optimize_strategy(200).expect("optimize"); // switch if >2% better
    println!("Strategy switched: {}", switched);

    // ── 6. Collect performance fees ──────────────────────────────────────────
    let fees = vault.collect_fees().unwrap_or(0);
    println!("\nFees collected to treasury: {}", fees);

    // ── 7. Withdrawal ────────────────────────────────────────────────────────
    let preview = vault.preview_withdraw(deposit_a.shares_minted / 2);
    println!("\nUser A preview withdraw (half shares): {}", preview);

    let withdrawal = vault
        .withdraw(user_a.clone(), deposit_a.shares_minted / 2)
        .expect("withdraw user_a");

    println!("User A withdrew:");
    println!("  Shares burned: {}", withdrawal.shares_burned);
    println!("  Assets received: {}", withdrawal.amount_withdrawn);
    println!("  Share price: {:.4}", withdrawal.share_price);

    // ── 8. Emergency pause demo ──────────────────────────────────────────────
    vault.pause().expect("pause");
    println!("\nVault paused: {}", vault.get_info(&env).paused);

    let blocked = vault.deposit(user_b.clone(), 100_000);
    println!("Deposit while paused (should fail): {}", blocked.is_err());

    vault.unpause().expect("unpause");
    println!("Vault unpaused: {}", !vault.get_info(&env).paused);

    // ── 9. Final stats ───────────────────────────────────────────────────────
    let stats = vault.get_stats();
    println!("\n── Final Vault Stats ──────────────────────────");
    println!("  Total assets:  {}", stats.total_assets);
    println!("  Total shares:  {}", stats.total_shares);
    println!("  Share price:   {:.6}", stats.share_price);
    println!("  Current APY:   {:.2}%", stats.current_apy);
    println!("  Paused:        {}", stats.paused);
}
