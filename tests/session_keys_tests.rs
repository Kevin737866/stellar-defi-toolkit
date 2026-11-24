use stellar_defi_toolkit::{
    format_utc_timestamp, validate_public_key, SessionKeyError, SessionKeyRegistration,
    SessionKeyRegistry, SessionScope,
};

fn stellar_pub(fill: char) -> String {
    format!("G{}", fill.to_string().repeat(55))
}

fn stellar_secret(fill: char) -> String {
    format!("S{}", fill.to_string().repeat(55))
}

#[test]
fn end_to_end_active_session_key_lifecycle_and_display() {
    let mut registry = SessionKeyRegistry::new();
    let owner_a = stellar_pub('A');
    let owner_b = stellar_pub('B');
    let key_short = stellar_pub('C');
    let key_long = stellar_pub('D');
    let key_other_owner = stellar_pub('E');

    let now = 1_735_689_600; // 2025-01-01T00:00:00Z

    // Register two session keys for owner_a with different scopes and expiries
    registry
        .register(
            &owner_a,
            SessionKeyRegistration {
                public_key: key_long.clone(),
                owner: owner_a.clone(),
                scopes: vec![
                    SessionScope::Lend,
                    SessionScope::Custom("vault:deposit".to_string()),
                ],
                issued_at: now,
                expires_at: now + 7_200,
            },
            now,
        )
        .expect("long-lived session key registration should succeed");

    registry
        .register(
            &owner_a,
            SessionKeyRegistration {
                public_key: key_short.clone(),
                owner: owner_a.clone(),
                scopes: vec![SessionScope::ReadOnly, SessionScope::Trade],
                issued_at: now,
                expires_at: now + 3_600,
            },
            now,
        )
        .expect("short-lived session key registration should succeed");

    // Register one session key for owner_b
    registry
        .register(
            &owner_b,
            SessionKeyRegistration {
                public_key: key_other_owner.clone(),
                owner: owner_b.clone(),
                scopes: vec![SessionScope::Governance],
                issued_at: now,
                expires_at: now + 1_800,
            },
            now,
        )
        .expect("owner_b session key registration should succeed");

    // Active sessions for owner_a are sorted by soonest expiry first
    let active_a = registry
        .active_sessions(Some(&owner_a), now + 600)
        .expect("querying active sessions should succeed");
    assert_eq!(active_a.len(), 2);
    assert_eq!(active_a[0].public_key, key_short);
    assert_eq!(
        active_a[0].expires_at_utc,
        format_utc_timestamp(now + 3_600).unwrap()
    );
    assert_eq!(active_a[0].remaining_secs, 3_000);
    assert_eq!(active_a[1].public_key, key_long);
    assert_eq!(active_a[1].remaining_secs, 6_600);

    // Display output includes scopes, UTC expiry timestamps, and remaining time
    let rendered = registry
        .display_active_sessions(Some(&owner_a), now + 600)
        .expect("display should succeed");
    assert!(rendered.contains("Active session keys (2):"));
    assert!(rendered.contains("scopes=[read_only, trade]"));
    assert!(rendered.contains("scopes=[lend, vault:deposit]"));
    assert!(rendered.contains("2025-01-01T01:00:00Z"));
    assert!(rendered.contains("2025-01-01T02:00:00Z"));
    assert!(!rendered.contains(&key_other_owner));

    // Secret seed rejected in owner filter without affecting registry
    let secret = stellar_secret('Z');
    assert_eq!(
        validate_public_key(&secret, "owner"),
        Err(SessionKeyError::SecretKeyDetected { field: "owner" })
    );
    assert_eq!(
        registry.display_active_sessions(Some(&secret), now + 600),
        Err(SessionKeyError::SecretKeyDetected { field: "owner" })
    );

    // Unauthorized revocation by owner_b fails without mutating owner_a's keys
    let unauth_err = registry.revoke(&owner_b, &key_long).unwrap_err();
    assert!(matches!(unauth_err, SessionKeyError::Unauthorized { .. }));
    assert_eq!(
        registry
            .active_sessions(Some(&owner_a), now + 600)
            .unwrap()
            .len(),
        2
    );

    // After key_short expires, only key_long remains active for owner_a
    let after_expiry = registry
        .active_sessions(Some(&owner_a), now + 3_600)
        .expect("querying after expiry should succeed");
    assert_eq!(after_expiry.len(), 1);
    assert_eq!(after_expiry[0].public_key, key_long);

    // Revoke key_long and prune expired/revoked entries
    registry
        .revoke(&owner_a, &key_long)
        .expect("owner_a revocation should succeed");
    let empty_display = registry
        .display_active_sessions(Some(&owner_a), now + 3_600)
        .expect("display should succeed");
    assert_eq!(
        empty_display,
        format!("No active session keys for owner {owner_a}.")
    );

    let pruned = registry.prune_expired(now + 3_600);
    assert_eq!(pruned, 3);
    assert!(registry.is_empty());
}
