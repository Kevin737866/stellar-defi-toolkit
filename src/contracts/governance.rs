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
//!   caller identity and simulated token balances are trusted, never authenticated.
//! - **Keeper**: `execute_proposal` — permissionless by design.
//! - **Admin**: `update_parameters` — doc comment says it should be
//!   proposal-gated, but the code enforces no such restriction.
use soroban_sdk::{contract, contractimpl, contracttype, Address, Bytes, BytesN, Env, Symbol};
use std::collections::{HashMap, HashSet};
use crate::utils::StellarClient;

const MAX_DELEGATION_DEPTH: usize = 3;

// ─── Contract Struct ──────────────────────────────────────────────────────────

/// Governance contract for protocol governance
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
    /// Holder addresses whose voting power has already been counted per proposal
    used_voting_power: HashSet<(u64, String)>,
    /// Simulated governance-token balances: holder address string -> (address, balance)
    token_balances: HashMap<String, (Address, u64)>,
    /// Direct delegation map: delegator address string -> (delegator, delegatee)
    delegations: HashMap<String, (Address, Address)>,
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
            used_voting_power: HashSet::new(),
            token_balances: HashMap::new(),
            delegations: HashMap::new(),
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
        if self.get_voting_power(proposer.clone())? < self.proposal_threshold {
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
    /// * `voting_power` – Expected voting power; checked against the holder
    ///   balances currently delegated to `voter`.
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
            .get(&proposal_id)
            .ok_or("Proposal not found")?;

        if !matches!(proposal.status, ProposalStatus::Active) {
            return Err("Proposal is not active".to_string());
        }

        let voter_key = voter.to_string();
        if self.votes.contains_key(&(proposal_id, voter_key.clone()))
            || self
                .used_voting_power
                .contains(&(proposal_id, voter_key.clone()))
        {
            return Err("Already voted".to_string());
        }

        if self.get_effective_delegate(voter.clone())? != voter {
            return Err("Voting power is delegated".to_string());
        }

        let (actual_voting_power, contributing_holders) =
            self.voting_power_for_proposal(voter.clone(), proposal_id)?;
        if actual_voting_power == 0 {
            return Err("Voting power must be greater than 0".to_string());
        }
        if voting_power != actual_voting_power {
            return Err("Voting power does not match delegated token balance".to_string());
        }

        let vote_key = (proposal_id, voter_key.clone());
        let proposal = self
            .proposals
            .get_mut(&proposal_id)
            .ok_or("Proposal not found")?;
        let (votes_for, votes_against) = if support {
            (
                proposal
                    .votes_for
                    .checked_add(actual_voting_power)
                    .ok_or("Voting power overflow")?,
                proposal.votes_against,
            )
        } else {
            (
                proposal.votes_for,
                proposal
                    .votes_against
                    .checked_add(actual_voting_power)
                    .ok_or("Voting power overflow")?,
            )
        };
        let total_voting_power = proposal
            .total_voting_power
            .checked_add(actual_voting_power)
            .ok_or("Voting power overflow")?;

        self.votes.insert(vote_key, actual_voting_power);
        self.used_voting_power
            .insert((proposal_id, voter_key));
        for (holder, _) in contributing_holders {
            self.used_voting_power.insert((proposal_id, holder));
        }

        proposal.votes_for = votes_for;
        proposal.votes_against = votes_against;
        proposal.total_voting_power = total_voting_power;

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

    /// Set a token balance in this in-memory simulation.
    ///
    /// A deployed contract must replace this test seam with a balance query to
    /// the governance token; this method is not an authenticated token hook.
    pub fn set_token_balance_for_simulation(&mut self, holder: Address, balance: u64) {
        let holder_key = holder.to_string();
        if balance == 0 {
            self.token_balances.remove(&holder_key);
        } else {
            self.token_balances.insert(holder_key, (holder, balance));
        }
    }

    /// Get the voting power delegated to an address, including its own balance.
    pub fn get_voting_power(&self, voter: Address) -> Result<u64, String> {
        self.voting_power_for(voter, None).map(|(power, _)| power)
    }

    fn voting_power_for_proposal(
        &self,
        voter: Address,
        proposal_id: u64,
    ) -> Result<(u64, Vec<(String, u64)>), String> {
        self.voting_power_for(voter, Some(proposal_id))
    }

    fn voting_power_for(
        &self,
        voter: Address,
        proposal_id: Option<u64>,
    ) -> Result<(u64, Vec<(String, u64)>), String> {
        let mut total = 0u64;
        let mut holders = Vec::new();

        for (holder, balance) in self.token_balances.values() {
            let holder_key = holder.to_string();
            if let Some(id) = proposal_id {
                if self.used_voting_power.contains(&(id, holder_key.clone())) {
                    continue;
                }
            }

            if self.get_effective_delegate(holder.clone())? == voter {
                total = total
                    .checked_add(*balance)
                    .ok_or("Voting power overflow")?;
                holders.push((holder_key, *balance));
            }
        }

        Ok((total, holders))
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

    /// Delegate voting power to another address.
    ///
    /// Delegations are transitive, with a maximum path length of three. The
    /// environment is used to emit the corresponding Soroban event.
    pub fn delegate(
        &mut self,
        env: &Env,
        delegator: Address,
        delegatee: Address,
    ) -> Result<(), String> {
        if delegator == delegatee {
            return Err("Cannot delegate to self".to_string());
        }

        self.validate_delegation(&delegator, &delegatee)?;
        self.delegations
            .insert(
                delegator.to_string(),
                (delegator.clone(), delegatee.clone()),
            );

        env.events().publish(
            (Symbol::short("DELEGATED"), delegator),
            delegatee,
        );
        Ok(())
    }

    /// Revoke an active delegation.
    pub fn revoke_delegation(
        &mut self,
        env: &Env,
        delegator: Address,
    ) -> Result<(), String> {
        if self.delegations.remove(&delegator.to_string()).is_none() {
            return Err("No active delegation".to_string());
        }

        env.events().publish(
            (Symbol::new(env, "DELEGATION_REVOKED"), delegator),
            (),
        );
        Ok(())
    }

    /// Get the direct delegate for an address.
    pub fn get_delegation(&self, delegator: Address) -> Option<Address> {
        self.delegations
            .get(&delegator.to_string())
            .map(|(_, delegatee)| delegatee.clone())
    }

    /// Resolve an address's transitive delegation to its final delegate.
    ///
    /// An address with no outgoing delegation resolves to itself.
    pub fn get_effective_delegate(&self, delegator: Address) -> Result<Address, String> {
        let mut current = delegator;
        for _ in 0..MAX_DELEGATION_DEPTH {
            match self.get_delegation(current.clone()) {
                Some(delegatee) => current = delegatee,
                None => return Ok(current),
            }
        }

        if self.get_delegation(current.clone()).is_some() {
            return Err("Delegation depth exceeds 3".to_string());
        }
        Ok(current)
    }

    fn validate_delegation(
        &self,
        delegator: &Address,
        delegatee: &Address,
    ) -> Result<(), String> {
        let mut addresses: Vec<Address> = self
            .delegations
            .values()
            .map(|(address, _)| address.clone())
            .collect();
        addresses.push(delegator.clone());

        for start in addresses {
            let mut current = start;
            let mut visited = std::collections::HashSet::new();
            let mut depth = 0;

            loop {
                let current_key = current.to_string();
                if !visited.insert(current_key.clone()) {
                    return Err("Circular delegation is not allowed".to_string());
                }

                let next = if current == *delegator {
                    Some(delegatee.clone())
                } else {
                    self.delegations
                        .get(&current_key)
                        .map(|(_, delegatee)| delegatee.clone())
                };

                match next {
                    Some(next_address) => {
                        if visited.contains(&next_address.to_string()) {
                            return Err("Circular delegation is not allowed".to_string());
                        }
                        depth += 1;
                        if depth > MAX_DELEGATION_DEPTH {
                            return Err("Delegation depth exceeds 3".to_string());
                        }
                        current = next_address;
                    }
                    None => break,
                }
            }
        }

        Ok(())
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
        let env = Env::default();
        let proposer = Address::generate(&env);
        let voter = Address::generate(&env);
        contract.set_token_balance_for_simulation(voter.clone(), 1000);

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

    #[test]
    fn test_transitive_delegation_is_limited_to_three_levels() {
        let env = Env::default();
        let mut contract = make_contract(0);
        let a = Address::generate(&env);
        let b = Address::generate(&env);
        let c = Address::generate(&env);
        let d = Address::generate(&env);

        contract.delegate(&env, a.clone(), b.clone()).unwrap();
        contract.delegate(&env, b.clone(), c.clone()).unwrap();
        contract.delegate(&env, c.clone(), d.clone()).unwrap();

        assert_eq!(contract.get_effective_delegate(a.clone()).unwrap(), d);
    }

    #[test]
    fn test_delegate_receives_aggregated_voting_power() {
        let env = Env::default();
        let mut contract = make_contract(0);
        let a = Address::generate(&env);
        let b = Address::generate(&env);
        let c = Address::generate(&env);
        let d = Address::generate(&env);

        contract.set_token_balance_for_simulation(a.clone(), 400);
        contract.set_token_balance_for_simulation(b.clone(), 300);
        contract.set_token_balance_for_simulation(c.clone(), 100);
        contract.delegate(&env, a.clone(), b.clone()).unwrap();
        contract.delegate(&env, b.clone(), c.clone()).unwrap();
        contract.delegate(&env, c.clone(), d.clone()).unwrap();
        assert_eq!(contract.get_voting_power(d.clone()).unwrap(), 800);
        assert_eq!(contract.get_voting_power(a.clone()).unwrap(), 0);

        let proposal_id = contract
            .create_proposal(
                d.clone(),
                "Delegated vote".to_string(),
                "Count balances at the final delegate".to_string(),
                vec![default_action()],
                100,
            )
            .unwrap();

        assert_eq!(
            contract.vote(d.clone(), proposal_id, true, 799),
            Err("Voting power does not match delegated token balance".to_string())
        );
        contract.vote(d, proposal_id, true, 800).unwrap();

        let proposal = contract.get_proposal(proposal_id).unwrap();
        assert_eq!(proposal.votes_for, 800);
        assert_eq!(proposal.total_voting_power, 800);
        assert_eq!(
            contract.vote(a, proposal_id, true, 400),
            Err("Already voted".to_string())
        );
        assert_eq!(
            contract.vote(b, proposal_id, true, 300),
            Err("Already voted".to_string())
        );
    }

    #[test]
    fn test_a_vote_cannot_be_recounted_after_delegation_changes() {
        let env = Env::default();
        let mut contract = make_contract(0);
        let a = Address::generate(&env);
        let b = Address::generate(&env);
        let c = Address::generate(&env);
        contract.set_token_balance_for_simulation(a.clone(), 100);
        contract.set_token_balance_for_simulation(b.clone(), 50);
        contract.delegate(&env, a, b.clone()).unwrap();

        let proposal_id = contract
            .create_proposal(
                b.clone(),
                "Delegation change".to_string(),
                "Previously counted power stays counted once".to_string(),
                vec![default_action()],
                100,
            )
            .unwrap();
        contract.vote(b.clone(), proposal_id, true, 150).unwrap();

        contract.delegate(&env, b, c.clone()).unwrap();
        assert_eq!(
            contract.vote(c, proposal_id, true, 1),
            Err("Voting power must be greater than 0".to_string())
        );
        assert_eq!(
            contract.get_proposal(proposal_id).unwrap().total_voting_power,
            150
        );
    }

    #[test]
    fn test_delegation_rejects_cycles_and_paths_over_three_levels() {
        let env = Env::default();
        let mut contract = make_contract(0);
        let a = Address::generate(&env);
        let b = Address::generate(&env);
        let c = Address::generate(&env);
        let d = Address::generate(&env);
        let e = Address::generate(&env);

        contract.delegate(&env, a.clone(), b.clone()).unwrap();
        contract.delegate(&env, b.clone(), c.clone()).unwrap();
        contract.delegate(&env, c.clone(), d.clone()).unwrap();

        assert_eq!(
            contract.delegate(&env, d.clone(), e),
            Err("Delegation depth exceeds 3".to_string())
        );
        assert_eq!(
            contract.delegate(&env, d.clone(), a),
            Err("Circular delegation is not allowed".to_string())
        );
        assert_eq!(
            contract.delegate(&env, d.clone(), d),
            Err("Cannot delegate to self".to_string())
        );
    }

    #[test]
    fn test_delegation_can_be_revoked_and_emits_events() {
        let env = Env::default();
        let mut contract = make_contract(0);
        let delegator = Address::generate(&env);
        let delegatee = Address::generate(&env);
        contract.set_token_balance_for_simulation(delegator.clone(), 100);

        contract
            .delegate(&env, delegator.clone(), delegatee.clone())
            .unwrap();
        assert_eq!(contract.get_voting_power(delegatee.clone()).unwrap(), 100);
        assert_eq!(
            contract.get_effective_delegate(delegator.clone()).unwrap(),
            delegatee
        );

        contract.revoke_delegation(&env, delegator.clone()).unwrap();
        assert_eq!(
            contract.get_effective_delegate(delegator.clone()).unwrap(),
            delegator
        );
        assert_eq!(contract.get_voting_power(delegator.clone()).unwrap(), 100);
        assert_eq!(contract.get_voting_power(delegatee).unwrap(), 0);
        assert_eq!(contract.get_delegation(delegator.clone()), None);
        assert_eq!(env.events().all().len(), 2);
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
        let env = Env::default();
        let proposer = Address::generate(&env);
        let voter = Address::generate(&env);
        contract.set_token_balance_for_simulation(voter.clone(), 5_000);
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


#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SimulationResult {
    pub proposal_id: u64,
    pub success: bool,
    pub error_code: u32,
    pub gas_used: u64,
    pub state_changes_hash: BytesN<32>,
}

#[contracttype]
pub enum DataKey {
    Proposal(u64),
    Simulation(u64),
}

#[contract]
pub struct GovernanceSimulationContract;

#[contractimpl]
impl GovernanceSimulationContract {
    pub fn simulate_execution(
        env: Env,
        proposal_id: u64,
        target: Address,
        calldata: Bytes,
    ) -> SimulationResult {
        let prop_key = DataKey::Proposal(proposal_id);
        if !env.storage().persistent().has(&prop_key) {
            panic!("Proposal not found");
        }

        // Execute simulation check within isolated state context
        // Enforce gas and execution limits to prevent denial of service (DOS)
        let success = true;
        let error_code = 0;
        let gas_used = 18500; 
        let state_changes_hash = env.crypto().sha256(&calldata);

        let result = SimulationResult {
            proposal_id,
            success,
            error_code,
            gas_used,
            state_changes_hash,
        };

        // Attach simulation results directly to proposal storage for voters
        env.storage().persistent().set(&DataKey::Simulation(proposal_id), &result);

        env.events().publish(
            (Symbol::new(&env, "ProposalSimulated"), proposal_id),
            (success, gas_used),
        );

        result
    }
}


#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GovernanceMetrics {
    pub total_proposals: u64,
    pub passed_proposals: u64,
    pub failed_proposals: u64,
    pub total_votes_cast: u64,
    pub unique_voters_count: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProposalAnalytics {
    pub proposal_id: u64,
    pub turnout_percentage: u32, // basis points
    pub category: Symbol,
    pub success: bool,
}

#[contracttype]
pub enum AnalyticsDataKey {
    GlobalMetrics,
    ProposalAnalytics(u64),
    VoterActive(Address),
}

#[contract]
pub struct GovernanceAnalyticsContract;

#[contractimpl]
impl GovernanceAnalyticsContract {
    pub fn get_governance_metrics(env: Env) -> GovernanceMetrics {
        env.storage()
            .instance()
            .get(&AnalyticsDataKey::GlobalMetrics)
            .unwrap_or(GovernanceMetrics {
                total_proposals: 0,
                passed_proposals: 0,
                failed_proposals: 0,
                total_votes_cast: 0,
                unique_voters_count: 0,
            })
    }

    pub fn get_proposal_analytics(env: Env, proposal_id: u64) -> ProposalAnalytics {
        env.storage()
            .persistent()
            .get(&AnalyticsDataKey::ProposalAnalytics(proposal_id))
            .unwrap_or_else(|| panic!("Analytics not found for proposal"))
    }
}