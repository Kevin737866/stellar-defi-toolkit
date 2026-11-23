use soroban_sdk::{testutils::Address as _, Address, Env};
use stellar_defi_toolkit::contracts::governance::{
    ActionType, GovernanceContract, ProposalAction, ProposalCategory, ProposalStatus,
};

fn action() -> ProposalAction {
    ProposalAction {
        action_type: ActionType::Custom("apply".to_string()),
        target: "module".to_string(),
        function: "apply".to_string(),
        parameters: vec![],
        value: None,
    }
}

#[test]
fn categories_set_distinct_quorum_and_voting_periods() {
    let env = Env::default();
    let proposer = Address::generate(&env);
    let mut governance =
        GovernanceContract::new("GOV".to_string(), 1_000, 604_800, 86_400, 0);
    governance
        .update_category_requirements(ProposalCategory::Upgrade, 2_000, 1_209_600)
        .unwrap();

    let parameter_id = governance
        .create_proposal(
            proposer.clone(),
            "Parameter".to_string(),
            "Update a parameter".to_string(),
            vec![action()],
            100,
        )
        .unwrap();
    let upgrade_id = governance
        .create_proposal_with_category(
            proposer,
            "Upgrade".to_string(),
            "Upgrade the module".to_string(),
            vec![action()],
            ProposalCategory::Upgrade,
            None,
            100,
        )
        .unwrap();

    let parameter = governance.get_proposal(parameter_id).unwrap();
    let upgrade = governance.get_proposal(upgrade_id).unwrap();
    assert_eq!(parameter.voting_deadline, 100 + 604_800);
    assert_eq!(parameter.quorum_percentage, 1_000);
    assert_eq!(upgrade.voting_deadline, 100 + 1_209_600);
    assert_eq!(upgrade.quorum_percentage, 2_000);
    assert_eq!(
        governance
            .get_proposals_by_category(ProposalCategory::Upgrade)
            .len(),
        1
    );
}

#[test]
fn module_sub_governance_overrides_requirements_and_filters_proposals() {
    let env = Env::default();
    let proposer = Address::generate(&env);
    let mut governance =
        GovernanceContract::new("GOV".to_string(), 1_000, 604_800, 86_400, 0);
    governance
        .update_module_requirements(
            "lending".to_string(),
            ProposalCategory::Parameter,
            1_500,
            259_200,
        )
        .unwrap();
    governance
        .create_proposal_with_category(
            proposer,
            "Lending parameter".to_string(),
            "Adjust lending parameters".to_string(),
            vec![action()],
            ProposalCategory::Parameter,
            Some("lending".to_string()),
            10,
        )
        .unwrap();

    let scoped = governance.get_proposals_by_module("lending");
    assert_eq!(scoped.len(), 1);
    assert_eq!(scoped[0].voting_deadline, 10 + 259_200);
    assert_eq!(scoped[0].quorum_percentage, 1_500);
    assert!(governance.get_proposals_by_module("treasury").is_empty());
}

#[test]
fn execution_uses_the_proposal_quorum_snapshot() {
    let env = Env::default();
    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    let mut governance =
        GovernanceContract::new("GOV".to_string(), 5_000, 604_800, 86_400, 0);
    let proposal_id = governance
        .create_proposal_with_category(
            proposer.clone(),
            "Emergency".to_string(),
            "Ratify emergency action".to_string(),
            vec![action()],
            ProposalCategory::Emergency,
            None,
            100,
        )
        .unwrap();
    governance.vote(voter, proposal_id, true, 600).unwrap();

    governance
        .execute_proposal(proposer, proposal_id, 100 + 259_200 + 86_400)
        .unwrap();

    assert!(matches!(
        governance.get_proposal(proposal_id).unwrap().status,
        ProposalStatus::Executed
    ));
}
