//! Synthetic Asset Governance Contract
//!
//! Provides decentralized governance for the synthetic asset protocol.
//! Enables community-driven asset listing, parameter updates, and emergency actions.
//!
//! ## Features
//! - Asset listing proposals
//! - Parameter governance
//! - Multi-signature support
//! - Timelock execution
//! - Voting power delegation
//! - Emergency controls

use soroban_sdk::{contract, contractimpl, Address, Env, Symbol, Vec, Map, unwrap::UnwrapOptimized};
use crate::types::synthetic::{
    SyntheticAsset, AssetProposal, AssetProposalType, AssetUpdateParams, RiskParameters
};

// ─── Constants ───────────────────────────────────────────────────────────────

/// Minimum voting period (3 days)
const MIN_VOTING_PERIOD: u64 = 3 * 24 * 3600;
/// Maximum voting period (30 days)
const MAX_VOTING_PERIOD: u64 = 30 * 24 * 3600;
/// Default voting period (7 days)
const DEFAULT_VOTING_PERIOD: u64 = 7 * 24 * 3600;
/// Default quorum (10% of total supply)
const DEFAULT_QUORUM_BPS: u32 = 1000;
/// Minimum quorum (5% of total supply)
const MIN_QUORUM_BPS: u32 = 500;
/// Execution delay (2 days)
const EXECUTION_DELAY: u64 = 2 * 24 * 3600;
/// Minimum proposal threshold (0.1% of supply)
const MIN_PROPOSAL_THRESHOLD_BPS: u32 = 10;

// ─── Storage Keys ─────────────────────────────────────────────────────────────

const ADMIN: Symbol = Symbol::short("ADMIN");
const PAUSED: Symbol = Symbol::short("PAUSED");
const PROPOSALS: Symbol = Symbol::short("PROPOSALS");
const VOTING_POWER: Symbol = Symbol::short("VOTING_POWER");
const DELEGATIONS: Symbol = Symbol::short("DELEGATIONS");
const GOVERNANCE_TOKEN: Symbol = Symbol::short("GOV_TOKEN");
const NEXT_PROPOSAL_ID: Symbol = Symbol::short("NEXT_PROP_ID");
const GOVERNANCE_PARAMS: Symbol = Symbol::short("GOV_PARAMS");

// ─── Governance Parameters ─────────────────────────────────────────────────

#[derive(Clone, Debug)]
#[contracttype]
pub struct GovernanceParams {
    /// Voting period in seconds
    pub voting_period: u64,
    /// Quorum required in basis points
    pub quorum_bps: u32,
    /// Minimum threshold to create proposal
    pub proposal_threshold_bps: u32,
    /// Execution delay in seconds
    pub execution_delay: u64,
    /// Minimum voting power to propose
    pub min_voting_power_to_propose: u64,
}

/// Vote record
#[derive(Clone, Debug)]
#[contracttype]
pub struct Vote {
    /// Voter address
    pub voter: Address,
    /// Proposal ID
    pub proposal_id: u64,
    /// Vote direction (true = for, false = against)
    pub support: bool,
    /// Voting power used
    pub voting_power: u64,
    /// When vote was cast
    pub timestamp: u64,
    /// Vote reason/comment
    pub reason: Option<Symbol>,
}

/// Multi-signature requirement
#[derive(Clone, Debug)]
#[contracttype]
pub struct MultiSigRequirement {
    /// Number of signatures required
    pub required_signatures: u32,
    /// List of required signers
    pub required_signers: Vec<Address>,
    /// Current signatures collected
    pub current_signatures: Vec<Address>,
    /// Deadline for signature collection
    pub signature_deadline: u64,
}

// ─── Synthetic Governance Contract ─────────────────────────────────────

/// Synthetic asset governance contract
#[contract]
pub struct SyntheticGovernanceContract;

#[contractimpl]
impl SyntheticGovernanceContract {
    /// Initialize governance contract
    /// 
    /// # Arguments
    /// * `admin` - Initial admin address
    /// * `governance_token` - Token for voting power
    pub fn initialize(env: Env, admin: Address, governance_token: Address) {
        if env.storage().instance().has(&ADMIN) {
            panic!("Already initialized");
        }

        env.storage().instance().set(&ADMIN, &admin);
        env.storage().instance().set(&PAUSED, &false);
        env.storage().instance().set(&GOVERNANCE_TOKEN, &governance_token);
        env.storage().instance().set(&NEXT_PROPOSAL_ID, &1u64);

        // Initialize governance parameters
        let gov_params = GovernanceParams {
            voting_period: DEFAULT_VOTING_PERIOD,
            quorum_bps: DEFAULT_QUORUM_BPS,
            proposal_threshold_bps: MIN_PROPOSAL_THRESHOLD_BPS,
            execution_delay: EXECUTION_DELAY,
            min_voting_power_to_propose: 1000_000_000, // 1000 tokens
        };
        env.storage().instance().set(&GOVERNANCE_PARAMS, &gov_params);

        // Initialize storage
        let proposals: Map<u64, AssetProposal> = Map::new(&env);
        env.storage().instance().set(&PROPOSALS, &proposals);

        let voting_power: Map<Address, u64> = Map::new(&env);
        env.storage().instance().set(&VOTING_POWER, &voting_power);

        let delegations: Map<Address, Address> = Map::new(&env);
        env.storage().instance().set(&DELEGATIONS, &delegations);

        env.events().publish(
            Symbol::short("SYNTHETIC_GOVERNANCE_INITIALIZED"),
            (admin, governance_token),
        );
    }

    /// Create a new proposal
    /// 
    /// # Arguments
    /// * `proposer` - Address creating the proposal
    /// * `proposal_type` - Type of proposal
    pub fn create_proposal(
        env: Env,
        proposer: Address,
        proposal_type: AssetProposalType,
    ) -> u64 {
        Self::require_not_paused(&env);

        // Check proposer has sufficient voting power
        let proposer_power = Self::get_voting_power(&env, proposer.clone());
        let gov_params = Self::get_governance_params(&env);
        
        if proposer_power < gov_params.min_voting_power_to_propose {
            panic!("Insufficient voting power to propose");
        }

        let proposal_id = env.storage().instance().get(&NEXT_PROPOSAL_ID).unwrap();
        let next_id = proposal_id + 1;
        env.storage().instance().set(&NEXT_PROPOSAL_ID, &next_id);

        let current_time = env.ledger().timestamp();
        let proposal = AssetProposal {
            proposal_id,
            proposer: proposer.clone(),
            proposal_type: proposal_type.clone(),
            asset_id: Self::extract_asset_id(&proposal_type),
            details: Symbol::short("PROPOSAL"),
            created_at: current_time,
            voting_deadline: current_time + gov_params.voting_period,
            votes_for: 0,
            votes_against: 0,
            executed: false,
        };

        let mut proposals = Self::get_proposals(&env);
        proposals.set(proposal_id, proposal);
        env.storage().instance().set(&PROPOSALS, &proposals);

        env.events().publish(
            Symbol::short("PROPOSAL_CREATED"),
            (proposer, proposal_id),
        );

        proposal_id
    }

    /// Vote on a proposal
    /// 
    /// # Arguments
    /// * `voter` - Address voting
    /// * `proposal_id` - Proposal to vote on
    /// * `support` - True for, false against
    /// * `reason` - Optional vote reason
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
        if Self::has_voted(&env, &vote_key) {
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

        // Store vote (in production, use proper storage)
        env.events().publish(
            Symbol::short("VOTE_CAST"),
            (voter, proposal_id, support, voting_power),
        );

        // Update proposal vote counts
        if support {
            proposal.votes_for += voting_power;
        } else {
            proposal.votes_against += voting_power;
        }

        proposals.set(proposal_id, proposal);
        env.storage().instance().set(&PROPOSALS, &proposals);
    }

    /// Execute a successful proposal
    /// 
    /// # Arguments
    /// * `executor` - Address executing the proposal
    /// * `proposal_id` - Proposal to execute
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

        // Check execution delay
        let gov_params = Self::get_governance_params(&env);
        if current_time < proposal.voting_deadline + gov_params.execution_delay {
            panic!("Execution delay has not passed");
        }

        // Check quorum
        let total_supply = Self::get_total_supply(&env);
        let quorum = (total_supply * gov_params.quorum_bps as u64) / 10000;
        let total_votes = proposal.votes_for + proposal.votes_against;

        if total_votes < quorum {
            panic!("Quorum not reached");
        }

        if proposal.votes_for <= proposal.votes_against {
            panic!("Proposal did not pass");
        }

        // Execute proposal based on type
        Self::execute_proposal_logic(&env, &proposal);

        // Mark as executed
        proposal.executed = true;
        proposals.set(proposal_id, proposal);
        env.storage().instance().set(&PROPOSALS, &proposals);

        env.events().publish(
            Symbol::short("PROPOSAL_EXECUTED"),
            (executor, proposal_id),
        );
    }

    /// Delegate voting power
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
            Symbol::short("DELEGATED"),
            (delegator, delegate),
        );
    }

    /// Create multi-signature requirement
    /// 
    /// # Arguments
    /// * `creator` - Address creating the requirement
    /// * `required_signers` - List of required signers
    /// * `required_signatures` - Number of signatures needed
    /// * `deadline` - Deadline for collection
    pub fn create_multisig_requirement(
        env: Env,
        creator: Address,
        required_signers: Vec<Address>,
        required_signatures: u32,
        deadline: u64,
    ) -> u64 {
        Self::require_admin(&env);

        if required_signatures == 0 || required_signatures > required_signers.len() as u32 {
            panic!("Invalid signature requirements");
        }

        let multisig_id = env.ledger().seq_num();
        let requirement = MultiSigRequirement {
            required_signatures,
            required_signers: required_signers.clone(),
            current_signatures: Vec::new(&env),
            signature_deadline: deadline,
        };

        // In production, store the multi-sig requirement
        env.events().publish(
            Symbol::short("MULTISIG_CREATED"),
            (creator, multisig_id, required_signatures),
        );

        multisig_id
    }

    /// Get proposal information
    pub fn get_proposal(env: Env, proposal_id: u64) -> AssetProposal {
        Self::get_proposals(&env).get(proposal_id)
            .unwrap_or_else(|| panic!("Proposal not found"))
    }

    /// Get active proposals
    pub fn get_active_proposals(env: Env) -> Vec<AssetProposal> {
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

    /// Get voting power for an address
    pub fn get_voting_power(env: Env, address: Address) -> u64 {
        // Check if address has delegated
        let delegations = Self::get_delegations(&env);
        if let Some(delegate) = delegations.get(address.clone()) {
            return Self::get_voting_power(&env, delegate);
        }

        // In production, this would query governance token balance
        // For now, return mock voting power
        1000000 // Mock: 1000 voting power
    }

    /// Get governance parameters
    pub fn get_governance_params(env: Env) -> GovernanceParams {
        Self::get_governance_params(&env)
    }

    // ─── Admin Functions ─────────────────────────────────────────────────────

    /// Pause governance (admin only)
    pub fn pause(env: Env) {
        Self::require_admin(&env);
        env.storage().instance().set(&PAUSED, &true);
        env.events().publish(Symbol::short("GOVERNANCE_PAUSED"), true);
    }

    /// Unpause governance (admin only)
    pub fn unpause(env: Env) {
        Self::require_admin(&env);
        env.storage().instance().set(&PAUSED, &false);
        env.events().publish(Symbol::short("GOVERNANCE_PAUSED"), false);
    }

    /// Update governance parameters (admin only)
    pub fn update_governance_params(env: Env, new_params: GovernanceParams) {
        Self::require_admin(&env);

        if new_params.voting_period < MIN_VOTING_PERIOD || 
           new_params.voting_period > MAX_VOTING_PERIOD {
            panic!("Invalid voting period");
        }

        if new_params.quorum_bps < MIN_QUORUM_BPS || new_params.quorum_bps > 5000 {
            panic!("Invalid quorum");
        }

        env.storage().instance().set(&GOVERNANCE_PARAMS, &new_params);

        env.events().publish(
            Symbol::short("GOVERNANCE_PARAMS_UPDATED"),
            (),
        );
    }

    /// Emergency pause (admin only)
    pub fn emergency_pause(env: Env, reason: Symbol) {
        Self::require_admin(&env);
        
        // Create emergency pause proposal
        let proposal_id = env.storage().instance().get(&NEXT_PROPOSAL_ID).unwrap();
        let next_id = proposal_id + 1;
        env.storage().instance().set(&NEXT_PROPOSAL_ID, &next_id);

        let emergency_proposal = AssetProposal {
            proposal_id,
            proposer: env.storage().instance().get(&ADMIN).unwrap(),
            proposal_type: AssetProposalType::EmergencyPause { reason },
            asset_id: None,
            details: Symbol::short("EMERGENCY_PAUSE"),
            created_at: env.ledger().timestamp(),
            voting_deadline: env.ledger().timestamp() + 3600, // 1 hour voting
            votes_for: 0,
            votes_against: 0,
            executed: false,
        };

        let mut proposals = Self::get_proposals(&env);
        proposals.set(proposal_id, emergency_proposal);
        env.storage().instance().set(&PROPOSALS, &proposals);

        env.events().publish(
            Symbol::short("EMERGENCY_PROPOSAL_CREATED"),
            (proposal_id, reason),
        );
    }

    // ─── Internal Helpers ─────────────────────────────────────────────────────

    fn execute_proposal_logic(env: &Env, proposal: &AssetProposal) {
        match &proposal.proposal_type {
            AssetProposalType::ListAsset { asset } => {
                // In production, call synthetic protocol to list asset
                env.events().publish(
                    Symbol::short("ASSET_LISTING_APPROVED"),
                    (proposal.asset_id.unwrap_or(0), asset.symbol),
                );
            },
            AssetProposalType::UpdateAsset { asset_id, new_params } => {
                // In production, call synthetic protocol to update asset
                env.events().publish(
                    Symbol::short("ASSET_UPDATE_APPROVED"),
                    (asset_id, new_params.min_collateral_ratio.unwrap_or(0)),
                );
            },
            AssetProposalType::DelistAsset { asset_id } => {
                // In production, call synthetic protocol to delist asset
                env.events().publish(
                    Symbol::short("ASSET_DELISTED"),
                    asset_id,
                );
            },
            AssetProposalType::UpdateRiskParams { new_params } => {
                // In production, call synthetic protocol to update risk params
                env.events().publish(
                    Symbol::short("RISK_PARAMS_UPDATED"),
                    (),
                );
            },
            AssetProposalType::EmergencyPause { reason } => {
                // In production, pause the synthetic protocol
                env.events().publish(
                    Symbol::short("EMERGENCY_PAUSE_EXECUTED"),
                    reason,
                );
            },
        }
    }

    fn extract_asset_id(proposal_type: &AssetProposalType) -> Option<u32> {
        match proposal_type {
            AssetProposalType::ListAsset { asset } => Some(asset.asset_id),
            AssetProposalType::UpdateAsset { asset_id, .. } => Some(*asset_id),
            AssetProposalType::DelistAsset { asset_id } => Some(*asset_id),
            _ => None,
        }
    }

    fn has_voted(env: &Env, vote_key: &(Address, u64)) -> bool {
        // In production, check if vote exists in storage
        // For now, return false (no vote recorded)
        false
    }

    fn get_total_supply(env: &Env) -> u64 {
        // In production, query governance token total supply
        // For now, return mock supply
        10_000_000_000 // 10M tokens
    }

    // Storage getters
    fn get_proposals(env: &Env) -> Map<u64, AssetProposal> {
        env.storage().instance().get(&PROPOSALS).unwrap()
    }

    fn get_delegations(env: &Env) -> Map<Address, Address> {
        env.storage().instance().get(&DELEGATIONS).unwrap()
    }

    fn get_governance_params(env: &Env) -> GovernanceParams {
        env.storage().instance().get(&GOVERNANCE_PARAMS).unwrap()
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
}
