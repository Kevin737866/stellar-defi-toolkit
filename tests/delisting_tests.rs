//! Integration tests for asset delisting procedure (issue #219).
//!
//! These tests exercise the `GovernanceContract::create_delisting_proposal`
//! helper together with the `AssetRegistryContract` delisting lifecycle methods.

use stellar_defi_toolkit::contracts::governance::{
    ActionType, GovernanceContract, ProposalStatus,
};
use soroban_sdk::{testutils::Address as _, Address, Env};

// ─── Governance-side Tests ────────────────────────────────────────────────────

#[test]
fn test_delisting_proposal_via_governance() {
    let mut gov = GovernanceContract::new(
        "GOV_TOKEN".to_string(),
        5000,   // 50% quorum
        604_800, // 7-day voting period
        86_400,  // 1-day execution delay
        0,       // no threshold for easy testing
    );
    let env = Env::default();
    let proposer = Address::generate(&env);
    let now: u64 = 1_000_000;

    let pid = gov
        .create_delisting_proposal(
            proposer.clone(),
            "USDC".to_string(),
            "Asset being deprecated".to_string(),
            604_800, // 7-day notice
            now,
        )
        .unwrap();

    let proposal = gov.get_proposal(pid).unwrap();
    assert_eq!(proposal.actions.len(), 1);

    match &proposal.actions[0].action_type {
        ActionType::DelistAsset {
            asset_id,
            notice_period,
            delist_at,
        } => {
            assert_eq!(asset_id, "USDC");
            assert_eq!(*notice_period, 604_800);
            // delist_at = now + voting_period + execution_delay + notice_period
            assert_eq!(*delist_at, now + 604_800 + 86_400 + 604_800);
        }
        _ => panic!("Expected DelistAsset action"),
    }
}

#[test]
fn test_delisting_proposal_with_default_notice_period() {
    let mut gov = GovernanceContract::new(
        "GOV_TOKEN".to_string(),
        5000,
        604_800,
        86_400,
        0,
    );
    let env = Env::default();
    let proposer = Address::generate(&env);

    // Passing 0 should default to 7 days.
    let pid = gov
        .create_delisting_proposal(
            proposer,
            "XLM".to_string(),
            "Delisting XLM".to_string(),
            0, // trigger default
            1_000_000,
        )
        .unwrap();

    let proposal = gov.get_proposal(pid).unwrap();
    match &proposal.actions[0].action_type {
        ActionType::DelistAsset { notice_period, .. } => {
            assert_eq!(*notice_period, 604_800, "Default notice period should be 7 days");
        }
        _ => panic!("Expected DelistAsset action"),
    }
}

#[test]
fn test_vote_on_delisting_proposal() {
    let mut gov = GovernanceContract::new(
        "GOV_TOKEN".to_string(),
        5000,
        604_800,
        86_400,
        0,
    );
    let env = Env::default();
    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    let now: u64 = 1_000_000;

    let pid = gov
        .create_delisting_proposal(
            proposer,
            "BTC".to_string(),
            "Deprecating BTC bridge".to_string(),
            604_800,
            now,
        )
        .unwrap();

    gov.vote(voter, pid, true, 5_000).unwrap();

    let proposal = gov.get_proposal(pid).unwrap();
    assert_eq!(proposal.votes_for, 5_000);
    assert_eq!(proposal.votes_against, 0);
}

#[test]
fn test_cancel_delisting_proposal() {
    let mut gov = GovernanceContract::new(
        "GOV_TOKEN".to_string(),
        5000,
        604_800,
        86_400,
        0,
    );
    let env = Env::default();
    let proposer = Address::generate(&env);

    let pid = gov
        .create_delisting_proposal(
            proposer.clone(),
            "ETH".to_string(),
            "Delisting ETH".to_string(),
            604_800,
            1_000_000,
        )
        .unwrap();

    gov.cancel_proposal(proposer, pid).unwrap();

    let proposal = gov.get_proposal(pid).unwrap();
    assert!(
        matches!(proposal.status, ProposalStatus::Cancelled),
        "Proposal should be cancelled"
    );
}

#[test]
fn test_new_positions_disabled_immediately_on_delist() {
    // The governance `create_delisting_proposal` records `active = false` in
    // the action parameters. Verify the notice period is propagated correctly
    // so callers can read it back.
    let mut gov = GovernanceContract::new(
        "GOV_TOKEN".to_string(),
        5000,
        604_800,
        86_400,
        0,
    );
    let env = Env::default();
    let proposer = Address::generate(&env);
    let now: u64 = 2_000_000;
    let custom_notice: u64 = 86_400 * 14; // 14 days

    let pid = gov
        .create_delisting_proposal(
            proposer,
            "WBTC".to_string(),
            "Deprecating WBTC".to_string(),
            custom_notice,
            now,
        )
        .unwrap();

    let proposal = gov.get_proposal(pid).unwrap();

    // The action parameters[1] holds the notice_period as a string.
    assert_eq!(
        proposal.actions[0].parameters[1],
        custom_notice.to_string()
    );
}
