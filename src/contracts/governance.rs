//! Governance contract implementation for Stellar DeFi Toolkit
//!
//! Provides decentralized governance functionality for protocol
//! management and decision-making on the Stellar blockchain.
//!
//! ## Access Control
//! This is a plain Rust simulation struct (no Soroban `require_auth` capability
//! exists in this file). See `docs/ACCESS_CONTROL_MATRIX.md` for the full
//! breakdown.
//! - **Governance**: `create_proposal`, `vote`, `cancel_proposal`, `delegate` —
//!   caller identity is a trusted `Address` field, never authenticated; `vote`
//!   also lets the caller self-report their own voting power.
//! - **Keeper**: `execute_proposal` — permissionless by design.
//! - **Admin**: `update_parameters` — doc comment says it should be
//!   proposal-gated, but the code enforces no such restriction.

use soroban_sdk::{contract, contractimpl, Address, Env, Symbol};
use std::collections::HashMap;
use crate::utils::StellarClient;

// ─── Contract Struct ──────────────────────────────────────────────────────────

/// Governance contract for protocol governance
#[contract]
pub struct GovernanceContract {
    /// Governance token contract address
    governance_token: String,
    /// Quorum percentage (in basis points, e.g., 5000 = 50%)
    quorum_percentage: u32,
    /// Voting period in seconds
    voting_period: u64,
    /// Execution delay in seconds
    execution_delay: u64,
    /// Minimum voting power required to create a proposal
    proposal_threshold: u64,
    /// Contract address
    address: Option<Address>,
    /// Proposals stored by ID
    proposals: HashMap<u64, Proposal>,
    /// Vote map: (proposal_id, voter_string) -> voting_power
    votes: HashMap<(u64, String), u64>,
    /// Next proposal ID counter
    next_proposal_id: u64,
}

impl GovernanceContract {
    // ── Constructor ───────────────────────────────────────────────────────────

    /// Create a new governance contract
    pub fn new(
        governance_token: String,
        quorum_percentage: u32,
        voting_period: u64,
        execution_delay: u64,
        proposal_threshold: u64,
    ) -> Self {
        Self {
            governance_token,
            quorum_percentage,
            voting_period,
            execution_delay,
            proposal_threshold,
            address: None,
            proposals: HashMap::new(),
            votes: HashMap::new(),
            next_proposal_id: 1,
        }
    }

    // ── Accessors ─────────────────────────────────────────────────────────────

    /// Get governance contract information
    pub fn get_info(&self) -> GovernanceInfo {
        GovernanceInfo {
            governance_token: self.governance_token.clone(),
            quorum_percentage: self.quorum_percentage,
            voting_period: self.voting_period,
            execution_delay: self.execution_delay,
            proposal_threshold: self.proposal_threshold,
        }
    }

    /// Deploy the governance contract to Stellar
    pub async fn deploy(mut self, client: &StellarClient) -> anyhow::Result<String> {
        let contract_id = client.deploy_governance_contract(&self).await?;
        self.address = Some(Address::from_contract_id(&contract_id));
        Ok(contract_id)
    }

    // ── Proposal Lifecycle ────────────────────────────────────────────────────

    /// Create a new governance proposal.
    ///
    /// # Arguments
    /// * `proposer`    – Address of the proposer.
    /// * `title`       – Short title (1–200 chars).
    /// * `description` – Detailed description (1–5 000 chars).
    /// * `actions`     – At least one action to execute.
    /// * `now`         – Current UNIX timestamp (injected for testability).
    pub fn create_proposal(
        &mut self,
        proposer: Address,
        title: String,
        description: String,
        actions: Vec<ProposalAction>,
        now: u64,
    ) -> Result<u64, String> {
        if self.get_voting_power(proposer.clone()) < self.proposal_threshold {
            return Err("Insufficient voting power to create a proposal".to_string());
        }

        if title.is_empty() || title.len() > 200 {
            return Err("Title must be 1-200 characters".to_string());
        }

        if description.is_empty() || description.len() > 5000 {
            return Err("Description must be 1-5000 characters".to_string());
        }

        if actions.is_empty() {
            return Err("At least one action is required".to_string());
        }

        let proposal_id = self.next_proposal_id;
        self.next_proposal_id += 1;

        let proposal = Proposal {
            id: proposal_id,
            proposer,
            title,
            description,
            actions,
            votes_for: 0,
            votes_against: 0,
            total_voting_power: 0,
            created_at: now,
            voting_deadline: now + self.voting_period,
            execution_time: now + self.voting_period + self.execution_delay,
            status: ProposalStatus::Active,
        };

        self.proposals.insert(proposal_id, proposal);
        Ok(proposal_id)
    }

    /// Vote on a proposal.
    ///
    /// # Arguments
    /// * `voter`        – Voter address.
    /// * `proposal_id`  – Target proposal.
    /// * `support`      – `true` = vote for, `false` = vote against.
    /// * `voting_power` – Amount of voting power to cast.
    pub fn vote(
        &mut self,
        voter: Address,
        proposal_id: u64,
        support: bool,
        voting_power: u64,
    ) -> Result<(), String> {
        if voting_power == 0 {
            return Err("Voting power must be greater than 0".to_string());
        }

        let proposal = self
            .proposals
            .get_mut(&proposal_id)
            .ok_or("Proposal not found")?;

        if !matches!(proposal.status, ProposalStatus::Active) {
            return Err("Proposal is not active".to_string());
        }

        let vote_key = (proposal_id, voter.to_string());
        if self.votes.contains_key(&vote_key) {
            return Err("Already voted".to_string());
        }

        self.votes.insert(vote_key, voting_power);

        if support {
            proposal.votes_for += voting_power;
        } else {
            proposal.votes_against += voting_power;
        }
        proposal.total_voting_power += voting_power;

        Ok(())
    }

    /// Execute a proposal after the voting and execution-delay periods.
    pub fn execute_proposal(
        &mut self,
        executor: Address,
        proposal_id: u64,
        now: u64,
    ) -> Result<(), String> {
        let proposal = self
            .proposals
            .get_mut(&proposal_id)
            .ok_or("Proposal not found")?;

        if now < proposal.voting_deadline {
            return Err("Voting period has not ended".to_string());
        }

        if !matches!(proposal.status, ProposalStatus::Active | ProposalStatus::Succeeded) {
            return Err("Proposal cannot be executed".to_string());
        }

        let total_possible_votes: u64 = 10_000; // Mock total supply
        let quorum_votes = (total_possible_votes * self.quorum_percentage as u64) / 10_000;

        if proposal.total_voting_power < quorum_votes
            || proposal.votes_for <= proposal.votes_against
        {
            proposal.status = ProposalStatus::Defeated;
            return Err("Proposal did not pass".to_string());
        }

        if now < proposal.execution_time {
            proposal.status = ProposalStatus::Succeeded;
            return Err("Execution delay has not passed".to_string());
        }

        proposal.status = ProposalStatus::Executed;
        Ok(())
    }

    /// Cancel a proposal (only by original proposer).
    pub fn cancel_proposal(
        &mut self,
        proposer: Address,
        proposal_id: u64,
    ) -> Result<(), String> {
        let proposal = self
            .proposals
            .get_mut(&proposal_id)
            .ok_or("Proposal not found")?;

        if proposal.proposer != proposer {
            return Err("Only proposer can cancel".to_string());
        }

        if matches!(proposal.status, ProposalStatus::Executed) {
            return Err("Cannot cancel executed proposal".to_string());
        }

        proposal.status = ProposalStatus::Cancelled;
        Ok(())
    }

    // ── Queries ───────────────────────────────────────────────────────────────

    /// Get proposal details.
    pub fn get_proposal(&self, proposal_id: u64) -> Option<Proposal> {
        self.proposals.get(&proposal_id).cloned()
    }

    /// Get all proposals.
    pub fn get_all_proposals(&self) -> Vec<Proposal> {
        self.proposals.values().cloned().collect()
    }

    /// Get active proposals.
    pub fn get_active_proposals(&self) -> Vec<Proposal> {
        self.proposals
            .values()
            .filter(|p| matches!(p.status, ProposalStatus::Active))
            .cloned()
            .collect()
    }

    /// Check whether a proposal has passed quorum and majority.
    pub fn has_proposal_passed(&self, proposal_id: u64) -> bool {
        if let Some(proposal) = self.get_proposal(proposal_id) {
            let total_possible_votes: u64 = 10_000;
            let quorum_votes =
                (total_possible_votes * self.quorum_percentage as u64) / 10_000;
            proposal.total_voting_power >= quorum_votes
                && proposal.votes_for > proposal.votes_against
        } else {
            false
        }
    }

    /// Get voting power for an address.
    ///
    /// In a real implementation this queries the governance-token balance.
    pub fn get_voting_power(&self, _voter: Address) -> u64 {
        0
    }

    // ── Parameter Management ──────────────────────────────────────────────────

    /// Update governance parameters (should be proposal-gated in production).
    pub fn update_parameters(
        &mut self,
        new_quorum: u32,
        new_voting_period: u64,
        new_execution_delay: u64,
    ) -> Result<(), String> {
        if new_quorum > 10_000 {
            return Err("Quorum must be <= 10000 basis points".to_string());
        }
        self.quorum_percentage = new_quorum;
        self.voting_period = new_voting_period;
        self.execution_delay = new_execution_delay;
        Ok(())
    }

    // ── Delegation ────────────────────────────────────────────────────────────

    /// Delegate voting power (stub; full implementation would update token state).
    pub fn delegate(
        &mut self,
        _delegator: Address,
        _delegatee: Address,
    ) -> Result<(), String> {
        Ok(())
    }

    /// Get delegation information (stub).
    pub fn get_delegation(&self, _delegator: Address) -> Option<Address> {
        None
    }

    // ─── Issue #219: Delisting Proposal Helper ────────────────────────────────

    /// Create a delisting proposal for `asset_id`.
    ///
    /// This is a convenience wrapper around `create_proposal` that creates a
    /// `DelistAsset` action with a configurable notice period.
    ///
    /// # Arguments
    /// * `proposer`        – Proposer address.
    /// * `asset_id`        – Human-readable asset identifier.
    /// * `reason`          – Reason for delisting.
    /// * `notice_period`   – Notice period in seconds (default: 7 days = 604 800).
    /// * `now`             – Current UNIX timestamp.
    pub fn create_delisting_proposal(
        &mut self,
        proposer: Address,
        asset_id: String,
        reason: String,
        notice_period: u64,
        now: u64,
    ) -> Result<u64, String> {
        let effective_notice = if notice_period == 0 {
            604_800 // default 7 days
        } else {
            notice_period
        };

        let delist_at = now + self.voting_period + self.execution_delay + effective_notice;

        let action = ProposalAction {
            action_type: ActionType::DelistAsset {
                asset_id: asset_id.clone(),
                notice_period: effective_notice,
                delist_at,
            },
            target: "asset_registry".to_string(),
            function: "delist_asset".to_string(),
            parameters: vec![asset_id, effective_notice.to_string(), delist_at.to_string()],
            value: None,
        };

        let title = format!("Delist asset: {}", action.parameters[0]);
        let description = format!(
            "{}\n\nNotice period: {} seconds. Positions can be closed until {}.",
            reason, effective_notice, delist_at
        );

        self.create_proposal(proposer, title, description, vec![action], now)
    }
}

// ─── Types ────────────────────────────────────────────────────────────────────

/// Governance contract information
#[derive(Debug, Clone)]
pub struct GovernanceInfo {
    pub governance_token: String,
    pub quorum_percentage: u32,
    pub voting_period: u64,
    pub execution_delay: u64,
    pub proposal_threshold: u64,
}

/// Proposal structure
#[derive(Debug, Clone)]
pub struct Proposal {
    pub id: u64,
    pub proposer: Address,
    pub title: String,
    pub description: String,
    pub actions: Vec<ProposalAction>,
    pub votes_for: u64,
    pub votes_against: u64,
    pub total_voting_power: u64,
    pub created_at: u64,
    pub voting_deadline: u64,
    pub execution_time: u64,
    pub status: ProposalStatus,
}

/// Proposal action
#[derive(Debug, Clone)]
pub struct ProposalAction {
    pub action_type: ActionType,
    pub target: String,
    pub function: String,
    pub parameters: Vec<String>,
    pub value: Option<u64>,
}

/// Action types for proposals
#[derive(Debug, Clone)]
pub enum ActionType {
    /// Transfer tokens
    Transfer,
    /// Update contract parameters
    UpdateParameters,
    /// Upgrade contract
    UpgradeContract,
    /// Pause contract
    PauseContract,
    /// Unpause contract
    UnpauseContract,
    // ─── Issue #219 ──────────────────────────────────────────────────────────
    /// Initiate an asset delisting with a mandatory notice period.
    ///
    /// * `asset_id`      – Asset being delisted.
    /// * `notice_period` – How long (seconds) existing positions have to close.
    /// * `delist_at`     – Absolute UNIX timestamp when force-close begins.
    DelistAsset {
        asset_id: String,
        notice_period: u64,
        delist_at: u64,
    },
    /// Custom action type
    Custom(String),
}

/// Proposal status
#[derive(Debug, Clone, PartialEq)]
pub enum ProposalStatus {
    Active,
    Succeeded,
    Executed,
    Defeated,
    Cancelled,
    Expired,
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_contract(threshold: u64) -> GovernanceContract {
        GovernanceContract::new(
            "GOV_TOKEN".to_string(),
            5000,   // 50% quorum
            604_800, // 7-day voting period
            86_400,  // 1-day execution delay
            threshold,
        )
    }

    fn default_action() -> ProposalAction {
        ProposalAction {
            action_type: ActionType::Transfer,
            target: "TOKEN_CONTRACT".to_string(),
            function: "transfer".to_string(),
            parameters: vec!["RECIPIENT".to_string(), "1000".to_string()],
            value: None,
        }
    }

    #[test]
    fn test_governance_contract_creation() {
        let contract = make_contract(100_000);
        assert_eq!(contract.governance_token, "GOV_TOKEN");
        assert_eq!(contract.quorum_percentage, 5000);
        assert_eq!(contract.voting_period, 604_800);
        assert_eq!(contract.execution_delay, 86_400);
        assert_eq!(contract.proposal_threshold, 100_000);
    }

    #[test]
    fn test_create_proposal() {
        let mut contract = make_contract(0);
        let proposer = Address::generate(&Env::default());

        let proposal_id = contract
            .create_proposal(
                proposer.clone(),
                "Test Proposal".to_string(),
                "This is a test proposal".to_string(),
                vec![default_action()],
                100,
            )
            .unwrap();

        assert_eq!(proposal_id, 1);
        let proposal = contract.get_proposal(proposal_id).unwrap();
        assert_eq!(proposal.title, "Test Proposal");
        assert_eq!(proposal.proposer, proposer);
        assert_eq!(proposal.created_at, 100);
        assert_eq!(proposal.voting_deadline, 100 + 604_800);
    }

    #[test]
    fn test_create_proposal_ineligible() {
        let mut contract = make_contract(100_000);
        let proposer = Address::generate(&Env::default());

        let result = contract.create_proposal(
            proposer,
            "Test Proposal".to_string(),
            "This is a test proposal".to_string(),
            vec![default_action()],
            100,
        );

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Insufficient voting power to create a proposal"
        );
    }

    #[test]
    fn test_invalid_proposal_title() {
        let mut contract = make_contract(0);
        let proposer = Address::generate(&Env::default());

        let result = contract.create_proposal(
            proposer,
            "".to_string(),
            "desc".to_string(),
            vec![default_action()],
            100,
        );

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Title must be 1-200 characters");
    }

    #[test]
    fn test_vote() {
        let mut contract = make_contract(0);
        let proposer = Address::generate(&Env::default());
        let voter = Address::generate(&Env::default());

        let pid = contract
            .create_proposal(
                proposer,
                "Test Proposal".to_string(),
                "This is a test proposal".to_string(),
                vec![default_action()],
                100,
            )
            .unwrap();

        let result = contract.vote(voter, pid, true, 1000);
        assert!(result.is_ok());
    }

    #[test]
    fn test_invalid_vote_power() {
        let mut contract = make_contract(0);
        let voter = Address::generate(&Env::default());

        let result = contract.vote(voter, 1, true, 0);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Voting power must be greater than 0");
    }

    #[test]
    fn test_update_parameters() {
        let mut contract = make_contract(100_000);
        contract.update_parameters(6000, 1_209_600, 172_800).unwrap();
        assert_eq!(contract.quorum_percentage, 6000);
        assert_eq!(contract.voting_period, 1_209_600);
        assert_eq!(contract.execution_delay, 172_800);
    }

    #[test]
    fn test_invalid_quorum() {
        let mut contract = make_contract(100_000);
        let result = contract.update_parameters(15_000, 604_800, 86_400);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Quorum must be <= 10000 basis points");
    }

    // ─── Issue #219: Delisting Tests ─────────────────────────────────────────

    #[test]
    fn test_create_delisting_proposal() {
        let mut contract = make_contract(0);
        let proposer = Address::generate(&Env::default());
        let now: u64 = 1_000_000;

        let pid = contract
            .create_delisting_proposal(
                proposer,
                "USDC".to_string(),
                "Asset being deprecated from the protocol".to_string(),
                604_800, // 7-day notice
                now,
            )
            .unwrap();

        let proposal = contract.get_proposal(pid).unwrap();
        assert!(proposal.title.contains("USDC"));
        assert_eq!(proposal.actions.len(), 1);

        // Verify the action is DelistAsset
        match &proposal.actions[0].action_type {
            ActionType::DelistAsset { asset_id, notice_period, delist_at } => {
                assert_eq!(asset_id, "USDC");
                assert_eq!(*notice_period, 604_800);
                // delist_at = now + voting_period + execution_delay + notice_period
                let expected = now + 604_800 + 86_400 + 604_800;
                assert_eq!(*delist_at, expected);
            }
            _ => panic!("Expected DelistAsset action"),
        }
    }

    #[test]
    fn test_delisting_proposal_default_notice_period() {
        let mut contract = make_contract(0);
        let proposer = Address::generate(&Env::default());
        let now: u64 = 1_000_000;

        // Pass 0 to trigger the 7-day default
        let pid = contract
            .create_delisting_proposal(
                proposer,
                "XLM".to_string(),
                "Delisting XLM".to_string(),
                0, // use default
                now,
            )
            .unwrap();

        let proposal = contract.get_proposal(pid).unwrap();
        match &proposal.actions[0].action_type {
            ActionType::DelistAsset { notice_period, .. } => {
                assert_eq!(*notice_period, 604_800);
            }
            _ => panic!("Expected DelistAsset action"),
        }
    }

    #[test]
    fn test_vote_on_delisting_proposal() {
        let mut contract = make_contract(0);
        let proposer = Address::generate(&Env::default());
        let voter = Address::generate(&Env::default());
        let now: u64 = 1_000_000;

        let pid = contract
            .create_delisting_proposal(
                proposer,
                "TOKEN".to_string(),
                "Delisting".to_string(),
                604_800,
                now,
            )
            .unwrap();

        let result = contract.vote(voter, pid, true, 5_000);
        assert!(result.is_ok());

        let proposal = contract.get_proposal(pid).unwrap();
        assert_eq!(proposal.votes_for, 5_000);
    }

    #[test]
    fn test_cancel_delisting_proposal() {
        let mut contract = make_contract(0);
        let proposer = Address::generate(&Env::default());
        let now: u64 = 1_000_000;

        let pid = contract
            .create_delisting_proposal(
                proposer.clone(),
                "TOKEN".to_string(),
                "Delisting".to_string(),
                604_800,
                now,
            )
            .unwrap();

        contract.cancel_proposal(proposer, pid).unwrap();

        let proposal = contract.get_proposal(pid).unwrap();
        assert!(matches!(proposal.status, ProposalStatus::Cancelled));
    }
}
