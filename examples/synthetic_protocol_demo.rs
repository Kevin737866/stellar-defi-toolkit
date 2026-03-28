//! Synthetic Asset Protocol Demo
//!
//! This example demonstrates the complete synthetic asset protocol workflow
//! including asset listing, position management, and governance.
//!
//! ## Features Demonstrated
//! - Asset listing and management
//! - Oracle price feed integration
//! - Position creation and monitoring
//! - Governance participation
//! - Risk management and alerts
//! - Batch operations

use soroban_sdk::{Address, Env, Symbol};
use stellar_defi_toolkit::contracts::{
    SyntheticProtocolContract, OracleManagerContract, SyntheticGovernanceContract, PositionManagerContract
};
use stellar_defi_toolkit::types::synthetic::{
    SyntheticAsset, AssetType, AssetProposalType, BatchOperationType, AlertSeverity
};

fn main() {
    let env = Env::default();
    
    // Initialize the complete synthetic protocol system
    let demo = SyntheticProtocolDemo::new(&env);
    
    // Run the complete demonstration
    demo.run_complete_demo();
}

struct SyntheticProtocolDemo {
    env: Env,
    admin: Address,
    user1: Address,
    user2: Address,
    oracle1: Address,
    oracle2: Address,
    oracle3: Address,
    
    // Contract addresses
    synthetic_protocol: Address,
    oracle_manager: Address,
    governance: Address,
    position_manager: Address,
    
    // Asset IDs
    s_tsla_id: u32,
    s_btc_id: u32,
    s_gold_id: u32,
}

impl SyntheticProtocolDemo {
    fn new(env: &Env) -> Self {
        // Generate test addresses
        let admin = Address::generate(env);
        let user1 = Address::generate(env);
        let user2 = Address::generate(env);
        let oracle1 = Address::generate(env);
        let oracle2 = Address::generate(env);
        let oracle3 = Address::generate(env);
        
        // Generate contract addresses
        let synthetic_protocol = Address::generate(env);
        let oracle_manager = Address::generate(env);
        let governance = Address::generate(env);
        let position_manager = Address::generate(env);
        
        Self {
            env: env.clone(),
            admin,
            user1,
            user2,
            oracle1,
            oracle2,
            oracle3,
            synthetic_protocol,
            oracle_manager,
            governance,
            position_manager,
            s_tsla_id: 1,
            s_btc_id: 2,
            s_gold_id: 3,
        }
    }
    
    fn run_complete_demo(&self) {
        println!("🚀 Starting Synthetic Asset Protocol Demo\n");
        
        // 1. Initialize all contracts
        self.initialize_contracts();
        
        // 2. Set up oracle system
        self.setup_oracle_system();
        
        // 3. List synthetic assets via governance
        self.list_synthetic_assets();
        
        // 4. User creates positions
        self.user_creates_positions();
        
        // 5. Monitor positions and handle alerts
        self.monitor_positions_and_alerts();
        
        // 6. Demonstrate batch operations
        self.demonstrate_batch_operations();
        
        // 7. Governance participation
        self.demonstrate_governance();
        
        // 8. Risk management demonstration
        self.demonstrate_risk_management();
        
        println!("✅ Synthetic Asset Protocol Demo completed successfully!");
    }
    
    fn initialize_contracts(&self) {
        println!("📋 1. Initializing all contracts...");
        
        // Initialize synthetic protocol
        SyntheticProtocolContract::initialize(self.env.clone(), self.admin);
        
        // Initialize oracle manager
        OracleManagerContract::initialize(self.env.clone(), self.admin);
        
        // Initialize governance
        SyntheticGovernanceContract::initialize(self.env.clone(), self.admin, Address::generate(&self.env));
        
        // Initialize position manager
        PositionManagerContract::initialize(self.env.clone(), self.admin);
        
        println!("   ✅ All contracts initialized successfully\n");
    }
    
    fn setup_oracle_system(&self) {
        println!("💰 2. Setting up oracle system...");
        
        // Register multiple oracles for price aggregation
        OracleManagerContract::register_oracle(
            self.env.clone(),
            self.oracle1,
            Symbol::short("CHAINLINK"),
            4000, // 40% weight
        );
        
        OracleManagerContract::register_oracle(
            self.env.clone(),
            self.oracle2,
            Symbol::short("BAND"),
            3000, // 30% weight
        );
        
        OracleManagerContract::register_oracle(
            self.env.clone(),
            self.oracle3,
            Symbol::short("PYTH"),
            3000, // 30% weight
        );
        
        // Submit prices for our synthetic assets
        self.submit_oracle_prices();
        
        println!("   ✅ Oracle system configured with 3 price sources\n");
    }
    
    fn list_synthetic_assets(&self) {
        println!("🏛 3. Listing synthetic assets via governance...");
        
        // Create sTSLA (synthetic Tesla)
        let stsla_asset = SyntheticAsset {
            asset_id: self.s_tsla_id,
            symbol: Symbol::short("sTSLA"),
            name: Symbol::short("Synthetic Tesla"),
            description: Symbol::short("Synthetic version of TSLA stock"),
            asset_type: AssetType::Stock,
            oracle_address: self.oracle_manager,
            min_collateral_ratio: 15000, // 150%
            max_collateral_ratio: 50000, // 500%
            minting_fee_bps: 50, // 0.5%
            active: true,
            listed_at: self.env.ledger().timestamp(),
            total_supply: 0,
            total_collateral: 0,
        };
        
        // Create sBTC (synthetic Bitcoin)
        let sbtc_asset = SyntheticAsset {
            asset_id: self.s_btc_id,
            symbol: Symbol::short("sBTC"),
            name: Symbol::short("Synthetic Bitcoin"),
            description: Symbol::short("Synthetic version of Bitcoin"),
            asset_type: AssetType::Crypto,
            oracle_address: self.oracle_manager,
            min_collateral_ratio: 20000, // 200%
            max_collateral_ratio: 80000, // 800%
            minting_fee_bps: 75, // 0.75%
            active: true,
            listed_at: self.env.ledger().timestamp(),
            total_supply: 0,
            total_collateral: 0,
        };
        
        // Create sGOLD (synthetic Gold)
        let sgold_asset = SyntheticAsset {
            asset_id: self.s_gold_id,
            symbol: Symbol::short("sGOLD"),
            name: Symbol::short("Synthetic Gold"),
            description: Symbol::short("Synthetic version of Gold commodity"),
            asset_type: AssetType::Commodity,
            oracle_address: self.oracle_manager,
            min_collateral_ratio: 12000, // 120%
            max_collateral_ratio: 40000, // 400%
            minting_fee_bps: 25, // 0.25%
            active: true,
            listed_at: self.env.ledger().timestamp(),
            total_supply: 0,
            total_collateral: 0,
        };
        
        // Create governance proposals to list assets
        let stsla_proposal_id = SyntheticGovernanceContract::create_proposal(
            self.env.clone(),
            self.admin,
            AssetProposalType::ListAsset { asset: stsla_asset },
        );
        
        let sbtc_proposal_id = SyntheticGovernanceContract::create_proposal(
            self.env.clone(),
            self.admin,
            AssetProposalType::ListAsset { asset: sbtc_asset },
        );
        
        let sgold_proposal_id = SyntheticGovernanceContract::create_proposal(
            self.env.clone(),
            self.admin,
            AssetProposalType::ListAsset { asset: sgold_asset },
        );
        
        println!("   📜 Created 3 asset listing proposals: sTSLA ({}), sBTC ({}), sGOLD ({})", 
                stsla_proposal_id, sbtc_proposal_id, sgold_proposal_id);
        
        // Simulate voting and execution (in production, this would be actual governance)
        self.simulate_proposal_execution(stsla_proposal_id);
        self.simulate_proposal_execution(sbtc_proposal_id);
        self.simulate_proposal_execution(sgold_proposal_id);
        
        println!("   ✅ All synthetic assets listed successfully\n");
    }
    
    fn user_creates_positions(&self) {
        println!("🏦 4. Users creating synthetic positions...");
        
        // User 1 creates sTSLA position
        let user1_position_id = PositionManagerContract::create_monitored_position(
            self.env.clone(),
            self.user1,
            self.s_tsla_id,
            Address::generate(&self.env), // Collateral token (e.g., USDC)
            10_000_000_000, // 10,000 USDC
            5_000_000_000, // 5,000 sTSLA
            15000, // 150% collateral ratio
        );
        
        // User 2 creates sBTC position
        let user2_position_id = PositionManagerContract::create_monitored_position(
            self.env.clone(),
            self.user2,
            self.s_btc_id,
            Address::generate(&self.env), // Collateral token
            15_000_000_000, // 15,000 USDC
            7_500_000_000, // 7.5 sBTC
            20000, // 200% collateral ratio
        );
        
        println!("   👤 User 1 created sTSLA position: {} with 150% ratio", user1_position_id);
        println!("   👤 User 2 created sBTC position: {} with 200% ratio", user2_position_id);
        
        // Get current prices
        let stsla_price = OracleManagerContract::get_aggregated_price(self.env.clone(), self.s_tsla_id);
        let sbtc_price = OracleManagerContract::get_aggregated_price(self.env.clone(), self.s_btc_id);
        
        println!("   💰 Current sTSLA price: ${:.2}", stsla_price.price as f64 / 1000000.0);
        println!("   💰 Current sBTC price: ${:.2}", sbtc_price.price as f64 / 1000000.0);
    }
    
    fn monitor_positions_and_alerts(&self) {
        println!("📊 5. Monitoring positions and handling alerts...");
        
        // Monitor all positions
        PositionManagerContract::monitor_positions(self.env.clone());
        
        // Get user alerts
        let user1_alerts = PositionManagerContract::get_user_alerts(self.env.clone(), self.user1);
        let user2_alerts = PositionManagerContract::get_user_alerts(self.env.clone(), self.user2);
        
        println!("   🚨 User 1 has {} alerts", user1_alerts.len());
        println!("   🚨 User 2 has {} alerts", user2_alerts.len());
        
        // Acknowledge critical alerts
        for alert in user1_alerts.iter() {
            if alert.severity == AlertSeverity::Critical {
                PositionManagerContract::acknowledge_alert(self.env.clone(), alert.alert_id);
                println!("   ✅ Acknowledged critical alert: {}", alert.alert_id);
            }
        }
    }
    
    fn demonstrate_batch_operations(&self) {
        println!("⚡ 6. Demonstrating batch operations...");
        
        // Create batch minting operations
        let mut batch_operations = Vec::new(&self.env);
        
        // Operation 1: Mint multiple sTSLA
        let mut op1_items = Vec::new(&self.env);
        op1_items.push_back(stellar_defi_toolkit::types::synthetic::BatchOperationItem {
            asset_id: self.s_tsla_id,
            collateral_token: Address::generate(&self.env),
            collateral_amount: 2_000_000_000, // 2,000 USDC
            synthetic_amount: 1_000_000_000, // 1,000 sTSLA
        });
        
        // Operation 2: Mint multiple sBTC
        let mut op2_items = Vec::new(&self.env);
        op2_items.push_back(stellar_defi_toolkit::types::synthetic::BatchOperationItem {
            asset_id: self.s_btc_id,
            collateral_token: Address::generate(&self.env),
            collateral_amount: 3_000_000_000, // 3,000 USDC
            synthetic_amount: 1_500_000_000, // 1.5 sBTC
        });
        
        let batch1_id = PositionManagerContract::create_batch_operation(
            self.env.clone(),
            self.user1,
            BatchOperationType::MintMultiple,
            op1_items,
        );
        
        let batch2_id = PositionManagerContract::create_batch_operation(
            self.env.clone(),
            self.user2,
            BatchOperationType::MintMultiple,
            op2_items,
        );
        
        // Execute batch operations
        PositionManagerContract::execute_batch_operation(self.env.clone(), batch1_id);
        PositionManagerContract::execute_batch_operation(self.env.clone(), batch2_id);
        
        println!("   ✅ Executed 2 batch operations: {} and {}", batch1_id, batch2_id);
    }
    
    fn demonstrate_governance(&self) {
        println!("🗳️ 7. Demonstrating governance participation...");
        
        // User delegates voting power to admin
        SyntheticGovernanceContract::delegate(self.env.clone(), self.user1, self.admin);
        
        // Get voting power
        let user1_voting_power = SyntheticGovernanceContract::get_voting_power(self.env.clone(), self.user1);
        let user2_voting_power = SyntheticGovernanceContract::get_voting_power(self.env.clone(), self.user2);
        
        println!("   🗳️ User 1 voting power: {} (delegated to admin)", user1_voting_power);
        println!("   🗳️ User 2 voting power: {}", user2_voting_power);
        
        // Get active proposals
        let active_proposals = SyntheticGovernanceContract::get_active_proposals(self.env.clone());
        println!("   📋 Active proposals: {}", active_proposals.len());
        
        // Get governance parameters
        let gov_params = SyntheticGovernanceContract::get_governance_params(self.env.clone());
        println!("   ⚙️ Governance parameters:");
        println!("      Voting period: {} seconds", gov_params.voting_period);
        println!("      Quorum: {} bps", gov_params.quorum_bps);
        println!("      Execution delay: {} seconds", gov_params.execution_delay);
    }
    
    fn demonstrate_risk_management(&self) {
        println!("🛡️ 8. Demonstrating risk management...");
        
        // Get position analytics for users
        let user1_analytics = PositionManagerContract::get_position_analytics(self.env.clone(), self.user1);
        let user2_analytics = PositionManagerContract::get_position_analytics(self.env.clone(), self.user2);
        
        println!("   📊 User 1 analytics:");
        for analytics in user1_analytics.iter() {
            println!("      Position {}: Risk Score {}, Status {:?}", 
                    analytics.position_id, analytics.risk_score, analytics.status);
        }
        
        println!("   📊 User 2 analytics:");
        for analytics in user2_analytics.iter() {
            println!("      Position {}: Risk Score {}, Status {:?}", 
                    analytics.position_id, analytics.risk_score, analytics.status);
        }
        
        // Demonstrate rebalancing
        println!("   ⚖️ Rebalancing User 1's high-risk position...");
        PositionManagerContract::rebalance_position(
            self.env.clone(),
            self.user1,
            user1_analytics.get(0).unwrap().position_id,
            25000, // Increase to 250% ratio
        );
    }
    
    fn submit_oracle_prices(&self) {
        // Submit prices for sTSLA ($200)
        OracleManagerContract::submit_price(
            self.env.clone(),
            self.oracle1,
            self.s_tsla_id,
            200_000_000, // $200 with 6 decimals
            9500, // 95% confidence
            self.env.ledger().timestamp(),
        );
        
        OracleManagerContract::submit_price(
            self.env.clone(),
            self.oracle2,
            self.s_tsla_id,
            198_000_000, // $198 with 6 decimals
            9000, // 90% confidence
            self.env.ledger().timestamp(),
        );
        
        OracleManagerContract::submit_price(
            self.env.clone(),
            self.oracle3,
            self.s_tsla_id,
            202_000_000, // $202 with 6 decimals
            9200, // 92% confidence
            self.env.ledger().timestamp(),
        );
        
        // Submit prices for sBTC ($45,000)
        OracleManagerContract::submit_price(
            self.env.clone(),
            self.oracle1,
            self.s_btc_id,
            45_000_000_000, // $45,000 with 6 decimals
            9800, // 98% confidence
            self.env.ledger().timestamp(),
        );
        
        OracleManagerContract::submit_price(
            self.env.clone(),
            self.oracle2,
            self.s_btc_id,
            44_500_000_000, // $44,500 with 6 decimals
            9500, // 95% confidence
            self.env.ledger().timestamp(),
        );
        
        OracleManagerContract::submit_price(
            self.env.clone(),
            self.oracle3,
            self.s_btc_id,
            45_500_000_000, // $45,500 with 6 decimals
            9700, // 97% confidence
            self.env.ledger().timestamp(),
        );
        
        // Submit prices for sGOLD ($1,850)
        OracleManagerContract::submit_price(
            self.env.clone(),
            self.oracle1,
            self.s_gold_id,
            1_850_000_000, // $1,850 with 6 decimals
            9900, // 99% confidence
            self.env.ledger().timestamp(),
        );
        
        OracleManagerContract::submit_price(
            self.env.clone(),
            self.oracle2,
            self.s_gold_id,
            1_845_000_000, // $1,845 with 6 decimals
            9800, // 98% confidence
            self.env.ledger().timestamp(),
        );
        
        OracleManagerContract::submit_price(
            self.env.clone(),
            self.oracle3,
            self.s_gold_id,
            1_855_000_000, // $1,855 with 6 decimals
            9600, // 96% confidence
            self.env.ledger().timestamp(),
        );
    }
    
    fn simulate_proposal_execution(&self, proposal_id: u64) {
        // In production, this would involve actual voting
        // For demo, we'll simulate successful execution
        println!("   ✅ Executing proposal {} (simulated)", proposal_id);
        
        // In a real system, this would call:
        // SyntheticGovernanceContract::execute_proposal(env, executor, proposal_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_synthetic_protocol_demo() {
        let env = Env::default();
        let demo = SyntheticProtocolDemo::new(&env);
        demo.run_complete_demo();
    }
}
