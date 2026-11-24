use soroban_sdk::{testutils::Address as _, Address, Env};
use stellar_defi_toolkit::contracts::governance::{
    GovernanceContract, ProposalEvent, ProposalStatus,
};

fn setup_governance(quorum_bps: u32, threshold: u64, circulating_supply: u64) -> GovernanceContract {
    let mut gov = GovernanceContract::new(
        "GOV_TOKEN".to_string(),
        quorum_bps,
        604_800, // 7-day voting period
        86_400,  // 1-day execution delay
        threshold,
    );
    gov.set_circulating_supply(circulating_supply);
    gov
}

#[test]
fn test_proposal_creation_requires_minimum_tokens_configurable() {
    let env = Env::default();
    let proposer = Address::generate(&env);
    let mut gov = setup_governance(2_000, 1_000, 100_000);

    // Below threshold -> rejected
    gov.set_token_balance(proposer.clone(), 999);
    let err = gov
        .create_proposal_with_calldata(
            proposer.clone(),
            "Upgrade Fee".to_string(),
            "Lower protocol fee to 10 bps".to_string(),
            "FEE_CONTROLLER".to_string(),
            vec![0x01, 0x02, 0x03],
            1_000,
        )
        .unwrap_err();
    assert_eq!(err, "Insufficient voting power to create a proposal");

    // At threshold -> succeeds
    gov.set_token_balance(proposer.clone(), 1_000);
    let id = gov
        .create_proposal_with_calldata(
            proposer.clone(),
            "Upgrade Fee".to_string(),
            "Lower protocol fee to 10 bps".to_string(),
            "FEE_CONTROLLER".to_string(),
            vec![0x01, 0x02, 0x03],
            1_000,
        )
        .unwrap();
    assert_eq!(id, 1);

    // Raising configurable threshold blocks the same proposer
    gov.set_proposal_threshold(5_000);
    let err = gov
        .create_proposal_with_calldata(
            proposer,
            "Second Proposal".to_string(),
            "Should fail after threshold increase".to_string(),
            "FEE_CONTROLLER".to_string(),
            vec![0x04],
            2_000,
        )
        .unwrap_err();
    assert_eq!(err, "Insufficient voting power to create a proposal");
}

#[test]
fn test_quorum_calculated_as_percentage_of_circulating_supply() {
    let env = Env::default();
    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);

    // 25% quorum (2500 bps) of 1,000,000 circulating supply = 250,000 votes
    let mut gov = setup_governance(2_500, 1_000, 1_000_000);
    assert_eq!(gov.get_circulating_supply(), 1_000_000);
    assert_eq!(gov.calculate_quorum(), 250_000);

    gov.set_token_balance(proposer.clone(), 10_000);
    let pid = gov
        .create_proposal_with_calldata(
            proposer,
            "Treasury Allocation".to_string(),
            "Allocate funds for audits".to_string(),
            "TREASURY".to_string(),
            vec![0xAA, 0xBB],
            10_000,
        )
        .unwrap();

    let proposal = gov.get_proposal(pid).unwrap();
    assert_eq!(proposal.quorum_required, 250_000);

    // Below quorum -> has_proposal_passed is false
    gov.vote(voter.clone(), pid, true, 249_999).unwrap();
    assert!(!gov.has_proposal_passed(pid));

    // Another voter pushes total voting power to exact quorum
    let voter2 = Address::generate(&env);
    gov.vote(voter2, pid, true, 1).unwrap();
    assert!(gov.has_proposal_passed(pid));
}

#[test]
fn test_proposal_fields_title_description_target_calldata() {
    let env = Env::default();
    let proposer = Address::generate(&env);
    let mut gov = setup_governance(1_000, 500, 500_000);
    gov.set_token_balance(proposer.clone(), 500);

    let calldata = vec![0xDE, 0xAD, 0xBE, 0xEF];
    let pid = gov
        .create_proposal_with_calldata(
            proposer.clone(),
            "Update Risk Params".to_string(),
            "Adjust collateral factor for USDC".to_string(),
            "LENDING_POOL_V1".to_string(),
            calldata.clone(),
            5_000,
        )
        .unwrap();

    let proposal = gov.get_proposal(pid).unwrap();
    assert_eq!(proposal.id, 1);
    assert_eq!(proposal.proposer, proposer);
    assert_eq!(proposal.title, "Update Risk Params");
    assert_eq!(proposal.description, "Adjust collateral factor for USDC");
    assert_eq!(proposal.target_contract, "LENDING_POOL_V1");
    assert_eq!(proposal.calldata, calldata);
    assert_eq!(proposal.status, ProposalStatus::Active);

    // Empty target contract is rejected
    let err = gov
        .create_proposal_with_calldata(
            proposer,
            "Invalid Target".to_string(),
            "Missing target contract".to_string(),
            "".to_string(),
            vec![],
            5_000,
        )
        .unwrap_err();
    assert_eq!(err, "Target contract is required");
}

#[test]
fn test_proposal_id_auto_incremented() {
    let env = Env::default();
    let proposer = Address::generate(&env);
    let mut gov = setup_governance(1_000, 100, 100_000);
    gov.set_token_balance(proposer.clone(), 100);

    let id1 = gov
        .create_proposal_with_calldata(
            proposer.clone(),
            "Proposal 1".to_string(),
            "First proposal".to_string(),
            "TARGET_A".to_string(),
            vec![1],
            100,
        )
        .unwrap();
    let id2 = gov
        .create_proposal_with_calldata(
            proposer.clone(),
            "Proposal 2".to_string(),
            "Second proposal".to_string(),
            "TARGET_B".to_string(),
            vec![2],
            200,
        )
        .unwrap();
    let id3 = gov
        .create_proposal_with_calldata(
            proposer,
            "Proposal 3".to_string(),
            "Third proposal".to_string(),
            "TARGET_C".to_string(),
            vec![3],
            300,
        )
        .unwrap();

    assert_eq!(id1, 1);
    assert_eq!(id2, 2);
    assert_eq!(id3, 3);
}

#[test]
fn test_proposal_events_emitted() {
    let env = Env::default();
    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    let executor = Address::generate(&env);
    let mut gov = setup_governance(1_000, 100, 100_000);
    gov.set_token_balance(proposer.clone(), 500);

    let now = 1_000;
    let pid = gov
        .create_proposal_with_calldata(
            proposer.clone(),
            "Emit Event Test".to_string(),
            "Verify ProposalCreated event".to_string(),
            "TARGET_CONTRACT".to_string(),
            vec![0x42],
            now,
        )
        .unwrap();

    assert_eq!(gov.get_events().len(), 1);
    assert_eq!(
        gov.get_events()[0],
        ProposalEvent::ProposalCreated {
            proposal_id: pid,
            proposer: proposer.clone(),
            title: "Emit Event Test".to_string(),
            target_contract: "TARGET_CONTRACT".to_string(),
            quorum_required: 10_000,
            created_at: now,
            voting_deadline: now + 604_800,
        }
    );

    // Vote and execute -> ProposalExecuted event emitted
    gov.vote(voter, pid, true, 15_000).unwrap();
    gov.execute_proposal(executor.clone(), pid, now + 604_800 + 86_400)
        .unwrap();

    assert_eq!(gov.get_events().len(), 2);
    assert_eq!(
        gov.get_events()[1],
        ProposalEvent::ProposalExecuted {
            proposal_id: pid,
            executor,
        }
    );
}
