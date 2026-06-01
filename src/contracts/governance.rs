//! Governance contract implementation for Stellar DeFi Toolkit
//! 
//! Provides decentralized governance functionality for protocol
//! management and decision-making on the Stellar blockchain.

use soroban_sdk::{contract, contractimpl, Address, Env, Symbol};
use std::collections::HashMap;
use crate::utils::StellarClient;

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
    /// Next proposal ID
    next_proposal_id: u64,
    /// Proposals stored by ID
    proposals: std::collections::BTreeMap<u64, Proposal>,
    /// Contract address
    address: Option<Address>,
    /// Internal proposal map
    proposals: HashMap<u64, Proposal>,
    /// Internal vote map (proposal_id, voter_string) -> voting_power
    votes: HashMap<(u64, String), u64>,
    /// Next proposal ID
    next_proposal_id: u64,
}

impl GovernanceContract {
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
            next_proposal_id: 1,
            proposals: std::collections::BTreeMap::new(),
            address: None,
            proposals: HashMap::new(),
            votes: HashMap::new(),
            next_proposal_id: 1,
        }
    }

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

    /// Create a new proposal
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

        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let proposal = Proposal {
            id: proposal_id,
            proposer,
            title,
            description,
            actions,
            votes_for: 0,
            votes_against: 0,
            total_voting_power: 0,
            created_at: current_time,
            voting_deadline: current_time + self.voting_period,
            execution_time: current_time + self.voting_period + self.execution_delay,
            status: ProposalStatus::Active,
        };

        self.proposals.insert(proposal_id, proposal);
        Ok(proposal_id)
    }

    /// Vote on a proposal
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

        let proposal = self.proposals.get_mut(&proposal_id).ok_or("Proposal not found")?;

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

    /// Execute a proposal
    pub fn execute_proposal(
        &mut self,
        executor: Address,
        proposal_id: u64,
    ) -> Result<(), String> {
        let proposal = self.proposals.get_mut(&proposal_id).ok_or("Proposal not found")?;

        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if current_time < proposal.voting_deadline {
            return Err("Voting period has not ended".to_string());
        }

        if !matches!(proposal.status, ProposalStatus::Active | ProposalStatus::Succeeded) {
            return Err("Proposal cannot be executed".to_string());
        }

        let total_possible_votes = 10000; // Mock total supply
        let quorum_votes = (total_possible_votes * self.quorum_percentage as u64) / 10000;

        if proposal.total_voting_power < quorum_votes || proposal.votes_for <= proposal.votes_against {
            proposal.status = ProposalStatus::Defeated;
            return Err("Proposal did not pass".to_string());
        }

        if current_time < proposal.execution_time {
            proposal.status = ProposalStatus::Succeeded;
            return Err("Execution delay has not passed".to_string());
        }

        proposal.status = ProposalStatus::Executed;

        Ok(())
    }

    /// Cancel a proposal (only by proposer)
    pub fn cancel_proposal(
        &mut self,
        proposer: Address,
        proposal_id: u64,
    ) -> Result<(), String> {
        let proposal = self.proposals.get_mut(&proposal_id).ok_or("Proposal not found")?;

        if proposal.proposer != proposer {
            return Err("Only proposer can cancel".to_string());
        }

        if matches!(proposal.status, ProposalStatus::Executed) {
            return Err("Cannot cancel executed proposal".to_string());
        }

        proposal.status = ProposalStatus::Cancelled;

        Ok(())
    }

    /// Get proposal details
    pub fn get_proposal(&self, proposal_id: u64) -> Option<Proposal> {
        self.proposals.get(&proposal_id).cloned()
    }

    /// Get all proposals
    pub fn get_all_proposals(&self) -> Vec<Proposal> {
        self.proposals.values().cloned().collect()
    }

    /// Get active proposals
    pub fn get_active_proposals(&self) -> Vec<Proposal> {
        self.proposals.values()
            .filter(|p| matches!(p.status, ProposalStatus::Active))
            .cloned()
            .collect()
    }

    /// Check if a proposal has passed
    pub fn has_proposal_passed(&self, proposal_id: u64) -> bool {
        if let Some(proposal) = self.get_proposal(proposal_id) {
            let total_possible_votes = 10000;
            let quorum_votes = (total_possible_votes * self.quorum_percentage as u64) / 10000;
            proposal.total_voting_power >= quorum_votes && proposal.votes_for > proposal.votes_against
        } else {
            false
        }
    }

    /// Get voting power for an address
    pub fn get_voting_power(&self, voter: Address) -> u64 {
        // In a real implementation, this would:
        // 1. Query governance token balance
        // 2. Apply any voting power multipliers
        // 3. Return voting power

        // For now, return 0
        0
    }

    /// Update governance parameters (only through proposal)
    pub fn update_parameters(
        &mut self,
        new_quorum: u32,
        new_voting_period: u64,
        new_execution_delay: u64,
    ) -> Result<(), String> {
        if new_quorum > 10000 {
            return Err("Quorum must be <= 10000 basis points".to_string());
        }

        self.quorum_percentage = new_quorum;
        self.voting_period = new_voting_period;
        self.execution_delay = new_execution_delay;

        Ok(())
    }

    /// Delegate voting power
    pub fn delegate(
        &mut self,
        delegator: Address,
        delegatee: Address,
    ) -> Result<(), String> {
        // In a real implementation, this would:
        // 1. Check if delegator has voting power
        // 2. Remove delegator's direct voting power
        // 3. Add to delegatee's delegated voting power
        // 4. Emit delegation event

        Ok(())
    }

    /// Get delegation information
    pub fn get_delegation(&self, delegator: Address) -> Option<Address> {
        // In a real implementation, this would query the contract state
        // For now, return None
        None
    }
}

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
    /// Proposal ID
    pub id: u64,
    /// Proposer address
    pub proposer: Address,
    /// Proposal title
    pub title: String,
    /// Proposal description
    pub description: String,
    /// List of actions to execute
    pub actions: Vec<ProposalAction>,
    /// Number of votes for
    pub votes_for: u64,
    /// Number of votes against
    pub votes_against: u64,
    /// Total voting power that has voted
    pub total_voting_power: u64,
    /// Creation timestamp
    pub created_at: u64,
    /// Voting deadline timestamp
    pub voting_deadline: u64,
    /// Execution timestamp (when it can be executed)
    pub execution_time: u64,
    /// Proposal status
    pub status: ProposalStatus,
}

/// Proposal action
#[derive(Debug, Clone)]
pub struct ProposalAction {
    /// Action type
    pub action_type: ActionType,
    /// Target contract address
    pub target: String,
    /// Function to call
    pub function: String,
    /// Function parameters
    pub parameters: Vec<String>,
    /// Value to send (if applicable)
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
    Custom(String),
}

/// Proposal status
#[derive(Debug, Clone)]
pub enum ProposalStatus {
    /// Proposal is active for voting
    Active,
    /// Proposal has passed but not executed
    Succeeded,
    /// Proposal has been executed
    Executed,
    /// Proposal was defeated
    Defeated,
    /// Proposal was cancelled
    Cancelled,
    /// Proposal has expired
    Expired,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_governance_contract_creation() {
        let contract = GovernanceContract::new(
            "GOV_TOKEN".to_string(),
            5000, // 50% quorum
            604800, // 7 days voting period
            86400, // 1 day execution delay
            100_000, // 100k tokens threshold
        );

        assert_eq!(contract.governance_token, "GOV_TOKEN");
        assert_eq!(contract.quorum_percentage, 5000);
        assert_eq!(contract.voting_period, 604800);
        assert_eq!(contract.execution_delay, 86400);
        assert_eq!(contract.proposal_threshold, 100_000);
    }

    #[test]
    fn test_create_proposal() {
        let mut contract = GovernanceContract::new(
            "GOV_TOKEN".to_string(),
            5000,
            604800,
            86400,
            0, // 0 threshold for easy testing
        );
        let proposer = Address::generate(&Env::default());

        let actions = vec![ProposalAction {
            action_type: ActionType::Transfer,
            target: "TOKEN_CONTRACT".to_string(),
            function: "transfer".to_string(),
            parameters: vec!["RECIPIENT".to_string(), "1000".to_string()],
            value: None,
        }];

        let proposal_id = contract
            .create_proposal(
                proposer.clone(),
                "Test Proposal".to_string(),
                "This is a test proposal".to_string(),
                actions,
                100, // now
            )
            .unwrap();

        assert_eq!(proposal_id, 1);
        let proposal = contract.get_proposal(proposal_id).unwrap();
        assert_eq!(proposal.title, "Test Proposal");
        assert_eq!(proposal.proposer, proposer);
        assert_eq!(proposal.created_at, 100);
        assert_eq!(proposal.voting_deadline, 100 + 604800);
    }

    #[test]
    fn test_create_proposal_ineligible() {
        let mut contract = GovernanceContract::new(
            "GOV_TOKEN".to_string(),
            5000,
            604800,
            86400,
            100_000, // High threshold
        );
        let proposer = Address::generate(&Env::default());
        let actions = vec![ProposalAction {
            action_type: ActionType::Transfer,
            target: "TOKEN_CONTRACT".to_string(),
            function: "transfer".to_string(),
            parameters: vec![],
            value: None,
        }];

        let result = contract.create_proposal(
            proposer,
            "Test Proposal".to_string(),
            "This is a test proposal".to_string(),
            actions,
            100,
        );

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Insufficient voting power to create a proposal");
    }

    #[test]
    fn test_invalid_proposal_title() {
        let mut contract = GovernanceContract::new(
            "GOV_TOKEN".to_string(),
            5000,
            604800,
            86400,
            0,
        );
        let proposer = Address::generate(&Env::default());
        let actions = vec![];

        let result = contract.create_proposal(
            proposer,
            "".to_string(), // Empty title
            "This is a test proposal".to_string(),
            actions,
            100,
        );

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Title must be 1-200 characters");
    }

    #[test]
    fn test_vote() {
        let mut contract = GovernanceContract::new(
            "GOV_TOKEN".to_string(),
            5000,
            604800,
            86400,
            0,
        );
        let voter = Address::generate(&Env::default());

        let proposer = Address::generate(&Env::default());
        let actions = vec![ProposalAction {
            action_type: ActionType::Transfer,
            target: "TOKEN_CONTRACT".to_string(),
            function: "transfer".to_string(),
            parameters: vec!["RECIPIENT".to_string(), "1000".to_string()],
            value: None,
        }];
        
        contract.create_proposal(
            proposer,
            "Test Proposal".to_string(),
            "This is a test proposal".to_string(),
            actions,
        ).unwrap();

        let result = contract.vote(voter, 1, true, 1000);
        assert!(result.is_ok());
    }

    #[test]
    fn test_invalid_vote_power() {
        let mut contract = GovernanceContract::new(
            "GOV_TOKEN".to_string(),
            5000,
            604800,
            86400,
            0,
        );
        let voter = Address::generate(&Env::default());

        let result = contract.vote(voter, 1, true, 0);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Voting power must be greater than 0");
    }

    #[test]
    fn test_update_parameters() {
        let mut contract = GovernanceContract::new(
            "GOV_TOKEN".to_string(),
            5000,
            604800,
            86400,
            100_000,
        );

        contract
            .update_parameters(6000, 1209600, 172800)
            .unwrap();

        assert_eq!(contract.quorum_percentage, 6000);
        assert_eq!(contract.voting_period, 1209600);
        assert_eq!(contract.execution_delay, 172800);
    }

    #[test]
    fn test_invalid_quorum() {
        let mut contract = GovernanceContract::new(
            "GOV_TOKEN".to_string(),
            5000,
            604800,
            86400,
            100_000,
        );

        let result = contract.update_parameters(15000, 604800, 86400); // 150% quorum
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Quorum must be <= 10000 basis points");
    }
}
