//! Stablecoin System Demo
//!
//! This example demonstrates how to use the stablecoin system on Stellar.
//! It shows the complete workflow from initialization to minting, redeeming,
//! and managing the stability mechanisms.

use soroban_sdk::{Address, Env, Symbol};
use stellar_defi_toolkit::contracts::{
    StablecoinContract, PriceOracleContract, StabilityPoolContract,
    GovernanceContractV2, ArbitrageContract
};
use stellar_defi_toolkit::types::stablecoin::{
    CollateralType, CollateralInfo, OraclePrice, ProposalType
};

fn main() {
    let env = Env::default();
    
    // Initialize all contracts
    let demo = StablecoinDemo::new(&env);
    
    // Run the complete demonstration
    demo.run_complete_demo();
}

struct StablecoinDemo {
    env: Env,
    admin: Address,
    user1: Address,
    user2: Address,
    arbitrageur: Address,
    
    // Contract addresses
    stablecoin: Address,
    oracle: Address,
    stability_pool: Address,
    governance: Address,
    arbitrage: Address,
    
    // Token addresses
    xlm: Address,
    usdc: Address,
}

impl StablecoinDemo {
    fn new(env: &Env) -> Self {
        // Generate test addresses
        let admin = Address::generate(env);
        let user1 = Address::generate(env);
        let user2 = Address::generate(env);
        let arbitrageur = Address::generate(env);
        
        // Generate contract addresses
        let stablecoin = Address::generate(env);
        let oracle = Address::generate(env);
        let stability_pool = Address::generate(env);
        let governance = Address::generate(env);
        let arbitrage = Address::generate(env);
        
        // Generate token addresses
        let xlm = Address::generate(env);
        let usdc = Address::generate(env);
        
        Self {
            env: env.clone(),
            admin,
            user1,
            user2,
            arbitrageur,
            stablecoin,
            oracle,
            stability_pool,
            governance,
            arbitrage,
            xlm,
            usdc,
        }
    }
    
    fn run_complete_demo(&self) {
        println!("🚀 Starting Stellar Stablecoin Demo\n");
        
        // 1. Initialize the system
        self.initialize_system();
        
        // 2. Set up price oracle
        self.setup_price_oracle();
        
        // 3. Configure collateral types
        self.configure_collateral();
        
        // 4. User mints stablecoins
        self.user_mints_stablecoins();
        
        // 5. Stability pool operations
        self.stability_pool_demo();
        
        // 6. Arbitrage demonstration
        self.arbitrage_demo();
        
        // 7. Governance demonstration
        self.governance_demo();
        
        // 8. Emergency scenario
        self.emergency_scenario();
        
        println!("✅ Demo completed successfully!");
    }
    
    fn initialize_system(&self) {
        println!("📋 1. Initializing the stablecoin system...");
        
        // Initialize stablecoin contract
        StablecoinContract::initialize(
            self.env.clone(),
            self.admin,
            "Stable USD".to_string(),
            "SUSD".to_string(),
            self.oracle,
        );
        
        // Initialize price oracle
        PriceOracleContract::initialize(self.env.clone(), self.admin);
        
        // Initialize stability pool
        StabilityPoolContract::initialize(
            self.env.clone(),
            self.admin,
            self.stablecoin,
            self.admin, // Treasury
        );
        
        // Initialize governance
        GovernanceContractV2::initialize(
            self.env.clone(),
            self.admin,
            self.stablecoin,
        );
        
        // Initialize arbitrage contract
        ArbitrageContract::initialize(
            self.env.clone(),
            self.admin,
            self.stablecoin,
            self.oracle,
        );
        
        println!("   ✅ All contracts initialized successfully\n");
    }
    
    fn setup_price_oracle(&self) {
        println!("💰 2. Setting up price oracle...");
        
        // Add price sources
        let source1 = Address::generate(&self.env);
        let source2 = Address::generate(&self.env);
        let source3 = Address::generate(&self.env);
        
        PriceOracleContract::add_price_source(
            self.env.clone(),
            source1,
            Symbol::short("CHAINLINK"),
            4000, // 40% weight
        );
        
        PriceOracleContract::add_price_source(
            self.env.clone(),
            source2,
            Symbol::short("BAND"),
            3000, // 30% weight
        );
        
        PriceOracleContract::add_price_source(
            self.env.clone(),
            source3,
            Symbol::short("PYTH"),
            3000, // 30% weight
        );
        
        // Update prices
        self.update_price(self.xlm, 0_150_000_000); // $0.15 with 8 decimals
        self.update_price(self.usdc, 1_000_000_000); // $1.00 with 8 decimals
        
        println!("   ✅ Price oracle configured with 3 sources\n");
    }
    
    fn configure_collateral(&self) {
        println!("🏦 3. Configuring collateral types...");
        
        // Add XLM as collateral
        StablecoinContract::add_collateral(
            self.env.clone(),
            self.xlm,
            CollateralType::XLM,
            12000, // 120% minimum ratio
            30000, // 300% maximum ratio
        );
        
        // Add USDC as collateral
        StablecoinContract::add_collateral(
            self.env.clone(),
            self.usdc,
            CollateralType::Stablecoin,
            10500, // 105% minimum ratio
            20000, // 200% maximum ratio
        );
        
        println!("   ✅ XLM and USDC added as collateral types\n");
    }
    
    fn user_mints_stablecoins(&self) {
        println!("🪙 4. Users minting stablecoins...");
        
        // User 1 mints using XLM collateral
        let xlm_amount = 1_000_000_000; // 1000 XLM
        let susd_amount = 100_000_000; // 100 SUSD
        
        StablecoinContract::mint(
            self.env.clone(),
            self.user1,
            self.xlm,
            xlm_amount,
            susd_amount,
        );
        
        println!("   👤 User 1 deposited {} XLM to mint {} SUSD", 
                xlm_amount / 1_000_000, susd_amount / 1_000_000);
        
        // User 2 mints using USDC collateral
        let usdc_amount = 500_000_000; // 500 USDC
        let susd_amount2 = 400_000_000; // 400 SUSD
        
        StablecoinContract::mint(
            self.env.clone(),
            self.user2,
            self.usdc,
            usdc_amount,
            susd_amount2,
        );
        
        println!("   👤 User 2 deposited {} USDC to mint {} SUSD", 
                usdc_amount / 1_000_000, susd_amount2 / 1_000_000);
        
        // Check total supply
        let total_supply = StablecoinContract::total_supply(self.env.clone());
        println!("   📊 Total stablecoin supply: {} SUSD\n", total_supply / 1_000_000);
    }
    
    fn stability_pool_demo(&self) {
        println!("🛡️ 5. Stability pool operations...");
        
        // User 1 deposits into stability pool
        let deposit_amount = 50_000_000; // 50 SUSD
        
        StabilityPoolContract::deposit(
            self.env.clone(),
            self.user1,
            deposit_amount,
        );
        
        println!("   👤 User 1 deposited {} SUSD into stability pool", 
                deposit_amount / 1_000_000);
        
        // Check pool info
        let pool_info = StabilityPoolContract::get_pool_info(self.env.clone());
        println!("   📊 Stability pool size: {} SUSD", 
                pool_info.total_deposits / 1_000_000);
        
        // Simulate liquidation and process through stability pool
        self.simulate_liquidation();
        
        // User 1 withdraws with rewards
        StabilityPoolContract::withdraw(
            self.env.clone(),
            self.user1,
            deposit_amount,
        );
        
        println!("   👤 User 1 withdrew {} SUSD from stability pool\n", 
                deposit_amount / 1_000_000);
    }
    
    fn arbitrage_demo(&self) {
        println!("⚡ 6. Arbitrage opportunities...");
        
        // Create price deviation to trigger arbitrage
        self.update_price(self.usdc, 1_020_000_000); // $1.02 (2% above peg)
        
        // Detect arbitrage opportunity
        let opportunity_id = ArbitrageContract::detect_opportunity(
            self.env.clone(),
            self.usdc,
            self.stablecoin,
            200, // 2% deviation
        );
        
        println!("   🎯 Arbitrage opportunity detected: ID {}", opportunity_id);
        
        // Get active opportunities
        let opportunities = ArbitrageContract::get_active_opportunities(self.env.clone());
        println!("   📈 Active opportunities: {}", opportunities.len());
        
        // Arbitrageur executes the opportunity
        let trade_amount = 100_000_000; // 100 SUSD
        
        ArbitrageContract::execute_arbitrage(
            self.env.clone(),
            self.arbitrageur,
            opportunity_id,
            trade_amount,
        );
        
        println!("   ⚡ Arbitrageur executed trade of {} SUSD", trade_amount / 1_000_000);
        
        // Check arbitrageur stats
        let stats = ArbitrageContract::get_arbitrageur_stats(self.env.clone(), self.arbitrageur);
        println!("   📊 Arbitrageur stats: {} trades, {} total rewards", 
                stats.total_arbitrages, stats.total_rewards / 1_000_000);
        
        // Reset price
        self.update_price(self.usdc, 1_000_000_000); // Back to $1.00
        println!("   ✅ Price reset to peg\n");
    }
    
    fn governance_demo(&self) {
        println!("🗳️ 7. Governance demonstration...");
        
        // Create a proposal to update fees
        let proposal_id = GovernanceContractV2::create_proposal(
            self.env.clone(),
            self.admin,
            ProposalType::UpdateFees {
                minting_fee_bps: 75, // 0.75%
                redemption_fee_bps: 75, // 0.75%
            },
            Symbol::short("Reduce fees to improve user experience"),
        );
        
        println!("   📜 Proposal {} created to update fees", proposal_id);
        
        // Vote on the proposal
        GovernanceContractV2::vote(
            self.env.clone(),
            self.admin,
            proposal_id,
            true, // Support
            Some(Symbol::short("Lower fees benefit everyone")),
        );
        
        println!("   ✅ Admin voted in favor of proposal {}", proposal_id);
        
        // Simulate time passing and voting period ending
        self.advance_time(8 * 24 * 3600); // 8 days
        
        // Execute the proposal
        GovernanceContractV2::execute_proposal(
            self.env.clone(),
            self.admin,
            proposal_id,
        );
        
        println!("   ✅ Proposal {} executed successfully\n", proposal_id);
    }
    
    fn emergency_scenario(&self) {
        println!("🚨 8. Emergency scenario demonstration...");
        
        // Simulate market crash - XLM price drops 50%
        self.update_price(self.xlm, 75_000_000); // $0.075 (50% drop)
        
        // Check user vaults for undercollateralization
        let user1_vault = StablecoinContract::get_vault(self.env.clone(), self.user1);
        let user1_ratio = StablecoinContract::get_collateral_ratio(self.env.clone(), self.user1);
        
        println!("   📉 User 1 vault ratio: {}%", user1_ratio / 100);
        
        if user1_ratio < 12000 { // Below 120%
            println!("   ⚠️  User 1 vault is undercollateralized!");
            
            // Liquidate the position
            StablecoinContract::liquidate(
                self.env.clone(),
                self.arbitrageur,
                self.user1,
                self.xlm,
                50_000_000, // Repay 50 SUSD
            );
            
            println!("   💥 Vault liquidated by arbitrageur");
        }
        
        // Emergency shutdown if needed
        let system_stats = ArbitrageContract::get_system_stats(self.env.clone());
        if system_stats.health_score < 5000 { // Below 50%
            println!("   🚨 System health critical! Initiating emergency shutdown...");
            
            GovernanceContractV2::emergency_pause(self.env.clone());
            StablecoinContract::emergency_shutdown(self.env.clone());
            
            println!("   🔒 Emergency shutdown completed");
        }
        
        println!("   ✅ Emergency scenario handled\n");
    }
    
    // Helper functions
    
    fn update_price(&self, asset: Address, price: u64) {
        // Simulate price updates from multiple sources
        let source1 = Address::generate(&self.env);
        let source2 = Address::generate(&self.env);
        let source3 = Address::generate(&self.env);
        
        PriceOracleContract::update_price(
            self.env.clone(),
            source1,
            asset,
            price,
            8, // 8 decimals
        );
        
        PriceOracleContract::update_price(
            self.env.clone(),
            source2,
            asset,
            price,
            8,
        );
        
        PriceOracleContract::update_price(
            self.env.clone(),
            source3,
            asset,
            price,
            8,
        );
    }
    
    fn simulate_liquidation(&self) {
        // Create a mock liquidation event
        let liquidation_event = stellar_defi_toolkit::types::stablecoin::LiquidationEvent {
            vault_owner: self.user2,
            liquidator: self.arbitrageur,
            collateral_address: self.usdc,
            collateral_amount: 100_000_000,
            debt_repaid: 95_000_000,
            penalty_amount: 5_000_000,
        };
        
        // Process through stability pool
        StabilityPoolContract::process_liquidation(
            self.env.clone(),
            liquidation_event,
        );
        
        println!("   💥 Liquidation processed through stability pool");
    }
    
    fn advance_time(&self, seconds: u64) {
        // In a real implementation, this would advance the ledger timestamp
        // For demo purposes, we just print the action
        println!("   ⏰ Time advanced by {} seconds", seconds);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stablecoin_demo() {
        let env = Env::default();
        let demo = StablecoinDemo::new(&env);
        demo.run_complete_demo();
    }
}
