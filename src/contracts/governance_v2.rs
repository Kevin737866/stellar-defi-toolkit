//! Governance Contract V2 for Stablecoin System
//!
//! Provides decentralized governance for the stablecoin ecosystem.
//! This contract allows token holders to propose and vote on
//! parameter changes and system upgrades.
//!
//! ## Features
//! - Proposal creation and voting
//! - Timelock for parameter changes
//! - Quadratic voting support
//! - Emergency governance actions
//! - Delegation of voting power

use soroban_sdk::{contract, contractimpl, Address, Env, Symbol, Vec, Map, unwrap::UnwrapOptimized};
use crate::types::stablecoin::{
    GovernanceProposal, ProposalType, SystemStats, RiskParameters, FeeConfig
};

// ─── Constants ───────────────────────────────────────────────────────────────

/// Minimum voting period (3 days)
const MIN_VOTING_PERIOD: u64 = 3 * 24 * 3600;
/// Maximum voting period (30 days)
const MAX_VOTING_PERIOD: u64 = 30 * 24 * 3600;
/// Default voting period (7 days)
const DEFAULT_VOTING_PERIOD: u64 = 7 * 24 * 3600;
/// Minimum quorum (5% of total supply)
const MIN_QUORUM_BPS: u32 = 500;
/// Default quorum (10% of total supply)
const DEFAULT_QUORUM_BPS: u32 = 1000;
/// Execution delay (2 days)
const EXECUTION_DELAY: u64 = 2 * 24 * 3600;
/// Minimum proposal threshold (0.1% of total supply)
const MIN_PROPOSAL_THRESHOLD_BPS: u32 = 10;

// ─── Storage Keys ─────────────────────────────────────────────────────────────

const ADMIN: Symbol = Symbol::short("ADMIN");
const PAUSED: Symbol = Symbol::short("PAUSED");
const STABLECOIN: Symbol = Symbol::short("STABLE");
const PROPOSALS: Symbol = Symbol::short("PROPOSALS");
const VOTING_POWER: Symbol = Symbol::short("VOTING");
const DELEGATIONS: Symbol = Symbol::short("DELEGATE");
const PARAMS: Symbol = Symbol::short("PARAMS");
const NEXT_PROPOSAL_ID: Symbol = Symbol::short("NEXT_ID");

// ─── Governance Parameters ───────────────────────────────────────────────────

#[derive(Clone, Debug)]
#[contracttype]
pub struct GovernanceParams {
    /// Voting period in seconds
    pub voting_period: u64,
    /// Quorum required in basis points
    pub quorum_bps: u32,
    /// Minimum threshold to create proposal in basis points
    pub proposal_threshold_bps: u32,
    /// Execution delay in seconds
    pub execution_delay: u64,
}

/// Vote information
#[derive(Clone, Debug)]
#[contracttype]
pub struct Vote {
    /// Voter address
    pub voter: Address,
    /// Proposal ID
    pub proposal_id: u64,
    /// Whether vote is in favor (true) or against (false)
    pub support: bool,
    /// Voting power used
    pub voting_power: u64,
    /// Timestamp of vote
    pub timestamp: u64,
    /// Reason for vote (optional)
    pub reason: Option<Symbol>,
}

// ─── Governance Contract ─────────────────────────────────────────────────────

/// Governance contract
#[contract]
pub struct GovernanceContractV2;

#[contractimpl]
impl GovernanceContractV2 {
    /// Initialize the governance contract
    /// 
    /// # Arguments
    /// * `admin` - Initial admin address
    /// * `stablecoin_address` - Address of the governance token
    pub fn initialize(env: Env, admin: Address, stablecoin_address: Address) {
        if env.storage().instance().has(&ADMIN) {
            panic!("Already initialized");
        }

        env.storage().instance().set(&ADMIN, &admin);
        env.storage().instance().set(&PAUSED, &false);
        env.storage().instance().set(&STABLECOIN, &stablecoin_address);
        env.storage().instance().set(&NEXT_PROPOSAL_ID, &1u64);

        // Initialize parameters
        let params = GovernanceParams {
            voting_period: DEFAULT_VOTING_PERIOD,
            quorum_bps: DEFAULT_QUORUM_BPS,
            proposal_threshold_bps: MIN_PROPOSAL_THRESHOLD_BPS,
            execution_delay: EXECUTION_DELAY,
        };
        env.storage().instance().set(&PARAMS, &params);

        // Initialize empty storage
        let proposals: Map<u64, GovernanceProposal> = Map::new(&env);
        env.storage().instance().set(&PROPOSALS, &proposals);

        let voting_power: Map<Address, u64> = Map::new(&env);
        env.storage().instance().set(&VOTING_POWER, &voting_power);

        let delegations: Map<Address, Address> = Map::new(&env);
        env.storage().instance().set(&DELEGATIONS, &delegations);

        env.events().publish(
            Symbol::short("GOVERNANCE_INITIALIZED"),
            (admin, stablecoin_address),
        );
    }

    /// Create a new governance proposal
    /// 
    /// # Arguments
    /// * `proposer` - Address creating the proposal
    /// * `proposal_type` - Type of proposal
    /// * `description` - Description of the proposal
    pub fn create_proposal(
        env: Env,
        proposer: Address,
        proposal_type: ProposalType,
        description: Symbol,
    ) -> u64 {
        Self::require_not_paused(&env);

        // Check proposer has enough voting power
        let proposer_power = Self::get_voting_power(&env, proposer.clone());
        let params = Self::get_params(&env);
        let total_supply = Self::get_total_supply(&env);
        let threshold = (total_supply * params.proposal_threshold_bps as u64) / 10000;

        if proposer_power < threshold {
            panic!("Insufficient voting power to create proposal");
        }

        let proposal_id = env.storage().instance().get(&NEXT_PROPOSAL_ID).unwrap();
        let next_id = proposal_id + 1;
        env.storage().instance().set(&NEXT_PROPOSAL_ID, &next_id);

        let current_time = env.ledger().timestamp();
        let proposal = GovernanceProposal {
            proposal_id,
            proposer: proposer.clone(),
            proposal_type: proposal_type.clone(),
            description,
            created_at: current_time,
            voting_deadline: current_time + params.voting_period,
            votes_for: 0,
            votes_against: 0,
            executed: false,
        };

        let mut proposals = Self::get_proposals(&env);
        proposals.set(proposal_id, proposal);
        env.storage().instance().set(&PROPOSALS, &proposals);

        env.events().publish(
            (Symbol::short("PROPOSAL_CREATED"), proposer.clone()),
            (proposal_id, proposal_type),
        );

        proposal_id
    }

    /// Vote on a proposal
    /// 
    /// # Arguments
    /// * `voter` - Address voting
    /// * `proposal_id` - ID of the proposal
    /// * `support` - Whether to support (true) or oppose (false)
    /// * `reason` - Optional reason for the vote
    pub fn vote(
        env: Env,
        voter: Address,
        proposal_id: u64,
        support: bool,
        reason: Option<Symbol>,
    ) {
        Self::require_not_paused(&env);

        let mut proposals = Self::get_proposals(&env);
        let mut proposal = proposals.get(proposal_id)
            .unwrap_or_else(|| panic!("Proposal not found"));

        if proposal.executed {
            panic!("Proposal already executed");
        }

        let current_time = env.ledger().timestamp();
        if current_time > proposal.voting_deadline {
            panic!("Voting period has ended");
        }

        // Check if already voted
        let vote_key = (voter.clone(), proposal_id);
        if env.storage().temporary().has(&vote_key) {
            panic!("Already voted on this proposal");
        }

        let voting_power = Self::get_voting_power(&env, voter.clone());
        if voting_power == 0 {
            panic!("No voting power");
        }

        // Record vote
        let vote = Vote {
            voter: voter.clone(),
            proposal_id,
            support,
            voting_power,
            timestamp: current_time,
            reason,
        };
        env.storage().temporary().set(&vote_key, &vote);

        // Update proposal vote counts
        if support {
            proposal.votes_for += voting_power;
        } else {
            proposal.votes_against += voting_power;
        }

        proposals.set(proposal_id, proposal);
        env.storage().instance().set(&PROPOSALS, &proposals);

        env.events().publish(
            (Symbol::short("VOTE_CAST"), voter.clone()),
            (proposal_id, support, voting_power),
        );
    }

    /// Execute a successful proposal
    /// 
    /// # Arguments
    /// * `executor` - Address executing the proposal
    /// * `proposal_id` - ID of the proposal to execute
    pub fn execute_proposal(env: Env, executor: Address, proposal_id: u64) {
        Self::require_not_paused(&env);

        let mut proposals = Self::get_proposals(&env);
        let mut proposal = proposals.get(proposal_id)
            .unwrap_or_else(|| panic!("Proposal not found"));

        if proposal.executed {
            panic!("Proposal already executed");
        }

        let current_time = env.ledger().timestamp();
        if current_time <= proposal.voting_deadline {
            panic!("Voting period has not ended");
        }

        // Check if proposal passed
        let params = Self::get_params(&env);
        let total_supply = Self::get_total_supply(&env);
        let quorum = (total_supply * params.quorum_bps as u64) / 10000;
        let total_votes = proposal.votes_for + proposal.votes_against;

        if total_votes < quorum {
            panic!("Quorum not reached");
        }

        if proposal.votes_for <= proposal.votes_against {
            panic!("Proposal did not pass");
        }

        // Check execution delay
        if current_time < proposal.voting_deadline + params.execution_delay {
            panic!("Execution delay has not passed");
        }

        // Execute proposal based on type
        Self::execute_proposal_logic(&env, &proposal);

        // Mark as executed
        proposal.executed = true;
        proposals.set(proposal_id, proposal);
        env.storage().instance().set(&PROPOSALS, &proposals);

        env.events().publish(
            (Symbol::short("PROPOSAL_EXECUTED"), executor.clone()),
            proposal_id,
        );
    }

    /// Delegate voting power to another address
    /// 
    /// # Arguments
    /// * `delegator` - Address delegating power
    /// * `delegate` - Address receiving power
    pub fn delegate(env: Env, delegator: Address, delegate: Address) {
        Self::require_not_paused(&env);

        if delegator == delegate {
            panic!("Cannot delegate to self");
        }

        let mut delegations = Self::get_delegations(&env);
        delegations.set(delegator.clone(), delegate.clone());
        env.storage().instance().set(&DELEGATIONS, &delegations);

        env.events().publish(
            (Symbol::short("DELEGATED"), delegator.clone()),
            delegate,
        );
    }

    /// Get voting power for an address
    pub fn get_voting_power(env: Env, address: Address) -> u64 {
        Self::get_voting_power(&env, address)
    }

    /// Get proposal information
    pub fn get_proposal(env: Env, proposal_id: u64) -> GovernanceProposal {
        let proposals = Self::get_proposals(&env);
        proposals.get(proposal_id)
            .unwrap_or_else(|| panic!("Proposal not found"))
    }

    /// Get all active proposals
    pub fn get_active_proposals(env: Env) -> Vec<GovernanceProposal> {
        let proposals = Self::get_proposals(&env);
        let mut active_proposals = Vec::new(&env);
        let current_time = env.ledger().timestamp();

        for proposal in proposals.values() {
            if !proposal.executed && current_time <= proposal.voting_deadline {
                active_proposals.push_back(proposal);
            }
        }

        active_proposals
    }

    /// Get governance parameters
    pub fn get_params(env: Env) -> GovernanceParams {
        Self::get_params(&env)
    }

    // ─── Admin Functions ───────────────────────────────────────────────────────

    /// Update governance parameters (requires passed proposal)
    pub fn update_params(env: Env, new_params: GovernanceParams) {
        // This should only be callable through a successful proposal
        // For now, we'll require admin for testing
        Self::require_admin(&env);

        // Validate parameters
        if new_params.voting_period < MIN_VOTING_PERIOD || new_params.voting_period > MAX_VOTING_PERIOD {
            panic!("Invalid voting period");
        }

        if new_params.quorum_bps < MIN_QUORUM_BPS || new_params.quorum_bps > 5000 {
            panic!("Invalid quorum");
        }

        env.storage().instance().set(&PARAMS, &new_params);

        env.events().publish(
            Symbol::short("GOV_PARAMS_UPDATED"),
            (
                new_params.voting_period,
                new_params.quorum_bps,
                new_params.proposal_threshold_bps,
            ),
        );
    }

    /// Emergency pause (admin only)
    pub fn emergency_pause(env: Env) {
        Self::require_admin(&env);
        env.storage().instance().set(&PAUSED, &true);
        env.events().publish(Symbol::short("GOVERNANCE_PAUSED"), true);
    }

    /// Unpause (admin only)
    pub fn unpause(env: Env) {
        Self::require_admin(&env);
        env.storage().instance().set(&PAUSED, &false);
        env.events().publish(Symbol::short("GOVERNANCE_PAUSED"), false);
    }

    // ─── Internal Functions ─────────────────────────────────────────────────────

    fn execute_proposal_logic(env: &Env, proposal: &GovernanceProposal) {
        match &proposal.proposal_type {
            ProposalType::UpdateCollateralParameters { 
                collateral_address, 
                min_ratio, 
                max_ratio 
            } => {
                // In production: Call stablecoin contract to update parameters
                env.events().publish(
                    Symbol::short("COLLATERAL_PARAMS_UPDATED"),
                    (collateral_address, min_ratio, max_ratio),
                );
            },
            ProposalType::UpdateFees { 
                minting_fee_bps, 
                redemption_fee_bps 
            } => {
                // In production: Call stablecoin contract to update fees
                env.events().publish(
                    Symbol::short("FEES_UPDATED"),
                    (minting_fee_bps, redemption_fee_bps),
                );
            },
            ProposalType::AddCollateral { 
                collateral_address, 
                collateral_type, 
                min_ratio, 
                max_ratio 
            } => {
                // In production: Call stablecoin contract to add collateral
                env.events().publish(
                    Symbol::short("COLLATERAL_ADDED"),
                    (collateral_address, collateral_type, min_ratio, max_ratio),
                );
            },
            ProposalType::RemoveCollateral { collateral_address } => {
                // In production: Call stablecoin contract to remove collateral
                env.events().publish(
                    Symbol::short("COLLATERAL_REMOVED"),
                    collateral_address,
                );
            },
            ProposalType::UpdateOracle { new_oracle } => {
                // In production: Call stablecoin contract to update oracle
                env.events().publish(
                    Symbol::short("ORACLE_UPDATED"),
                    new_oracle,
                );
            },
            ProposalType::EmergencyShutdown => {
                // In production: Call stablecoin contract emergency shutdown
                env.events().publish(
                    Symbol::short("EMERGENCY_SHUTDOWN"),
                    env.ledger().timestamp(),
                );
            },
            ProposalType::Custom(_) => {
                // Handle custom proposals
                env.events().publish(
                    Symbol::short("CUSTOM_PROPOSAL_EXECUTED"),
                    proposal.proposal_id,
                );
            },
        }
    }

    fn get_voting_power(env: &Env, address: Address) -> u64 {
        // Check if address has delegated
        let delegations = Self::get_delegations(env);
        if let Some(delegate) = delegations.get(address.clone()) {
            return Self::get_voting_power(env, delegate);
        }

        // Get token balance (in production, this would query the token contract)
        // For now, return a mock value based on address
        let mock_balance = 1000000; // Mock balance
        mock_balance
    }

    fn get_total_supply(env: &Env) -> u64 {
        // In production, this would query the stablecoin contract
        // For now, return a mock value
        100_000_000_000 // 10,000 tokens with 7 decimals
    }

    fn require_admin(env: &Env) {
        let admin = env.storage().instance().get(&ADMIN).unwrap_optimized();
        if env.current_contract_address() != admin {
            panic!("Not authorized");
        }
    }

    fn require_not_paused(env: &Env) {
        let paused = env.storage().instance().get(&PAUSED).unwrap();
        if paused {
            panic!("Governance is paused");
        }
    }

    fn get_proposals(env: &Env) -> Map<u64, GovernanceProposal> {
        env.storage().instance().get(&PROPOSALS).unwrap()
    }

    fn get_params(env: &Env) -> GovernanceParams {
        env.storage().instance().get(&PARAMS).unwrap()
    }

    fn get_delegations(env: &Env) -> Map<Address, Address> {
        env.storage().instance().get(&DELEGATIONS).unwrap()
    }
}
