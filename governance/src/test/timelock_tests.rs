#[cfg(test)]
mod timelock_tests {
    use crate::contracts::governance_v2::{
        EmergencyAction, EmergencyRatificationStatus, GovernanceV2, SecurityCouncil,
    };
    use crate::contracts::circuit_breaker::CircuitBreaker;

    const RATIFICATION_WINDOW_SECS: u64 = 7 * 24 * 60 * 60;

    fn council_with_members(n: usize, threshold: usize) -> SecurityCouncil {
        let members: Vec<String> = (0..n).map(|i| format!("member_{}", i)).collect();
        SecurityCouncil::new(members, threshold).expect("valid council")
    }

    #[test]
    fn council_rejects_invalid_size() {
        assert!(SecurityCouncil::new(vec!["a".into(), "b".into()], 2).is_err());
        let nine: Vec<String> = (0..10).map(|i| format!("m{}", i)).collect();
        assert!(SecurityCouncil::new(nine, 5).is_err());
    }

    #[test]
    fn council_rejects_invalid_threshold() {
        let members: Vec<String> = (0..5).map(|i| format!("m{}", i)).collect();
        assert!(SecurityCouncil::new(members.clone(), 0).is_err());
        assert!(SecurityCouncil::new(members, 6).is_err());
    }

    #[test]
    fn emergency_pause_requires_threshold_signatures() {
        let council = council_with_members(5, 3);
        let mut gov = GovernanceV2::new(council);
        let mut cb = CircuitBreaker::new();

        let action = EmergencyAction::PauseContracts;
        let sigs = vec!["member_0".to_string(), "member_1".to_string()];
        assert!(gov.execute_emergency(action.clone(), &sigs, 1_000, &mut cb).is_err());

        let sigs = vec![
            "member_0".to_string(),
            "member_1".to_string(),
            "member_2".to_string(),
        ];
        let id = gov
            .execute_emergency(action, &sigs, 1_000, &mut cb)
            .expect("threshold met");
        assert!(cb.is_paused());
        assert_eq!(
            gov.ratification_status(id),
            Some(EmergencyRatificationStatus::Pending {
                deadline: 1_000 + RATIFICATION_WINDOW_SECS
            })
        );
    }

    #[test]
    fn non_member_signatures_are_ignored() {
        let council = council_with_members(3, 2);
        let mut gov = GovernanceV2::new(council);
        let mut cb = CircuitBreaker::new();
        let sigs = vec!["member_0".to_string(), "intruder".to_string()];
        assert!(gov
            .execute_emergency(EmergencyAction::FreezeFunds, &sigs, 0, &mut cb)
            .is_err());
    }

    #[test]
    fn emergency_action_expires_without_ratification() {
        let council = council_with_members(3, 2);
        let mut gov = GovernanceV2::new(council);
        let mut cb = CircuitBreaker::new();
        let sigs = vec!["member_0".to_string(), "member_1".to_string()];
        let id = gov
            .execute_emergency(EmergencyAction::PauseContracts, &sigs, 100, &mut cb)
            .unwrap();
        gov.enforce_deadlines(100 + RATIFICATION_WINDOW_SECS + 1, &mut cb);
        assert_eq!(
            gov.ratification_status(id),
            Some(EmergencyRatificationStatus::Expired)
        );
        assert!(!cb.is_paused());
    }

    #[test]
    fn ratification_vote_finalizes_action() {
        let council = council_with_members(3, 2);
        let mut gov = GovernanceV2::new(council);
        let mut cb = CircuitBreaker::new();
        let sigs = vec!["member_0".to_string(), "member_1".to_string()];
        let id = gov
            .execute_emergency(EmergencyAction::PauseContracts, &sigs, 0, &mut cb)
            .unwrap();
        gov.ratify(id, true, 10).expect("ratified");
        assert_eq!(
            gov.ratification_status(id),
            Some(EmergencyRatificationStatus::Ratified)
        );
        gov.enforce_deadlines(RATIFICATION_WINDOW_SECS * 2, &mut cb);
        assert!(cb.is_paused());
    }

    #[test]
    fn rejected_ratification_reverts_action() {
        let council = council_with_members(3, 2);
        let mut gov = GovernanceV2::new(council);
        let mut cb = CircuitBreaker::new();
        let sigs = vec!["member_0".to_string(), "member_1".to_string()];
        let id = gov
            .execute_emergency(EmergencyAction::PauseContracts, &sigs, 0, &mut cb)
            .unwrap();
        gov.ratify(id, false, 10).expect("vote recorded");
        assert_eq!(
            gov.ratification_status(id),
            Some(EmergencyRatificationStatus::Rejected)
        );
        assert!(!cb.is_paused());
    }

    #[test]
    fn ratification_after_deadline_fails() {
        let council = council_with_members(3, 2);
        let mut gov = GovernanceV2::new(council);
        let mut cb = CircuitBreaker::new();
        let sigs = vec!["member_0".to_string(), "member_1".to_string()];
        let id = gov
            .execute_emergency(EmergencyAction::PauseContracts, &sigs, 0, &mut cb)
            .unwrap();
        assert!(gov.ratify(id, true, RATIFICATION_WINDOW_SECS + 1).is_err());
    }

    #[test]
    fn council_rotation_requires_governance() {
        let council = council_with_members(3, 2);
        let mut gov = GovernanceV2::new(council);
        let new_members: Vec<String> = (0..4).map(|i| format!("new_{}", i)).collect();
        assert!(gov.rotate_council_external(new_members.clone(), 3).is_err());
        gov.rotate_council_by_governance(new_members, 3)
            .expect("governance rotation");
        assert!(gov.council().is_member("new_0"));
        assert!(!gov.council().is_member("member_0"));
    }

    #[test]
    fn emergency_events_record_council_signatures() {
        let council = council_with_members(3, 2);
        let mut gov = GovernanceV2::new(council);
        let mut cb = CircuitBreaker::new();
        let sigs = vec!["member_0".to_string(), "member_2".to_string()];
        let id = gov
            .execute_emergency(EmergencyAction::FreezeFunds, &sigs, 42, &mut cb)
            .unwrap();
        let event = gov.emergency_event(id).expect("event logged");
        assert_eq!(event.signers, sigs);
        assert_eq!(event.timestamp, 42);
    }
}
