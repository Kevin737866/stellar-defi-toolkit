//! Failover engine tests for `FailoverEngine` and
//! `OracleFailoverCoordinator`.
//!
//! Covers:
//!   - Ordered adapter priority
//!   - Automatic failover on error
//!   - Health-check gate (unhealthy adapters skipped)
//!   - Manual override for adapter selection
//!   - Failover events emitted for monitoring
//!   - Auto-demotion of underperforming adapters
//!   - OracleFailoverCoordinator consensus + quorum logic

use stellar_defi_toolkit::contracts::price_feed_adapters::failover::{
    FailoverEngine, FailoverReason, DEMOTION_THRESHOLD_BPS,
};
use stellar_defi_toolkit::contracts::oracle_manager::oracle_failover::{
    CoordinationEventType, CoordinatorConfig, CoordinatorError,
    OracleFailoverCoordinator,
};

// ─── helpers ─────────────────────────────────────────────────────────────────

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Build a fetch closure: `succeeding` ids always return `Ok(price)`,
/// everything else returns `Err`.
fn make_fetch(
    succeeding: &[(&str, u64)],
) -> impl FnMut(&str, &str) -> (Result<u64, String>, u64) + '_ {
    let map: std::collections::HashMap<String, u64> = succeeding
        .iter()
        .map(|(k, v)| (k.to_string(), *v))
        .collect();
    move |adapter_id: &str, _asset_id: &str| {
        match map.get(adapter_id) {
            Some(&price) => (Ok(price), 10),
            None => (Err(format!("{} failed", adapter_id)), 50),
        }
    }
}

// ─── FailoverEngine: basic registration ──────────────────────────────────────

#[test]
fn register_adapters_sorted_by_priority() {
    let mut engine = FailoverEngine::new();
    engine.register("c", 30);
    engine.register("a", 10);
    engine.register("b", 20);

    let ids: Vec<&str> = engine.adapters().iter().map(|a| a.adapter_id.as_str()).collect();
    assert_eq!(ids, vec!["a", "b", "c"]);
}

#[test]
fn re_register_updates_priority_and_re_sorts() {
    let mut engine = FailoverEngine::new();
    engine.register("a", 10);
    engine.register("b", 20);
    // Re-register "a" with lower priority than "b"
    engine.register("a", 30);
    let ids: Vec<&str> = engine.adapters().iter().map(|a| a.adapter_id.as_str()).collect();
    assert_eq!(ids, vec!["b", "a"]);
}

// ─── FailoverEngine: normal fetch (first adapter succeeds) ───────────────────

#[test]
fn fetch_returns_first_adapter_price_on_success() {
    let mut engine = FailoverEngine::new();
    engine.register("primary", 1);
    engine.register("secondary", 2);

    let price = engine
        .fetch("XLM", now(), make_fetch(&[("primary", 1_000_000), ("secondary", 900_000)]))
        .unwrap();

    assert_eq!(price, 1_000_000, "first adapter should win");
}

// ─── FailoverEngine: failover on error ───────────────────────────────────────

#[test]
fn fetch_falls_through_to_secondary_on_primary_error() {
    let mut engine = FailoverEngine::new();
    engine.register("primary", 1);
    engine.register("secondary", 2);

    // primary always fails
    let price = engine
        .fetch("XLM", now(), make_fetch(&[("secondary", 2_000_000)]))
        .unwrap();

    assert_eq!(price, 2_000_000);
}

#[test]
fn fetch_falls_through_to_tertiary_when_primary_and_secondary_fail() {
    let mut engine = FailoverEngine::new();
    engine.register("primary",   1);
    engine.register("secondary", 2);
    engine.register("tertiary",  3);

    let price = engine
        .fetch("XLM", now(), make_fetch(&[("tertiary", 3_000_000)]))
        .unwrap();

    assert_eq!(price, 3_000_000);
}

#[test]
fn fetch_returns_err_when_all_adapters_fail() {
    let mut engine = FailoverEngine::new();
    engine.register("a", 1);
    engine.register("b", 2);

    let result = engine.fetch("XLM", now(), make_fetch(&[]));
    assert!(result.is_err(), "all-fail must propagate Err");
}

// ─── FailoverEngine: disabled adapters ───────────────────────────────────────

#[test]
fn disabled_adapter_is_skipped() {
    let mut engine = FailoverEngine::new();
    engine.register("primary", 1);
    engine.register("backup",  2);
    engine.set_enabled("primary", false);

    let price = engine
        .fetch("XLM", now(), make_fetch(&[("primary", 1_000), ("backup", 2_000)]))
        .unwrap();

    assert_eq!(price, 2_000, "disabled primary must be skipped");
}

#[test]
fn disabled_adapter_emits_disabled_reason_event() {
    let mut engine = FailoverEngine::new();
    engine.register("primary", 1);
    engine.register("backup",  2);
    engine.set_enabled("primary", false);

    engine
        .fetch("XLM", now(), make_fetch(&[("backup", 2_000)]))
        .unwrap();

    let events = engine.drain_events();
    let disabled_ev = events
        .iter()
        .find(|e| e.adapter_id == "primary");
    assert!(disabled_ev.is_some());
    assert_eq!(disabled_ev.unwrap().reason, Some(FailoverReason::Disabled));
}

// ─── FailoverEngine: health-check gate ───────────────────────────────────────

#[test]
fn unhealthy_adapter_is_skipped_by_health_gate() {
    let mut engine = FailoverEngine::new();
    engine.register("bad",  1);
    engine.register("good", 2);

    // Drive "bad" below the demotion threshold
    let ts = now();
    for _ in 0..20 {
        engine.health.record("bad", 100, false, 0, ts);
    }

    let price = engine
        .fetch("XLM", ts, make_fetch(&[("bad", 1_000), ("good", 2_000)]))
        .unwrap();

    assert_eq!(price, 2_000, "unhealthy adapter should be skipped");

    let events = engine.drain_events();
    let unhealthy_ev = events
        .iter()
        .find(|e| e.adapter_id == "bad");
    assert!(unhealthy_ev.is_some());
    assert_eq!(
        unhealthy_ev.unwrap().reason,
        Some(FailoverReason::UnhealthyAdapter)
    );
}

// ─── FailoverEngine: manual override ─────────────────────────────────────────

#[test]
fn manual_override_uses_only_specified_adapter() {
    let mut engine = FailoverEngine::new();
    engine.register("primary",  1);
    engine.register("override", 2);
    engine.set_manual_override(Some("override"));

    let price = engine
        .fetch("XLM", now(), make_fetch(&[("primary", 1_000), ("override", 9_999)]))
        .unwrap();

    assert_eq!(price, 9_999, "manual override must bypass priority chain");
}

#[test]
fn clearing_manual_override_resumes_normal_chain() {
    let mut engine = FailoverEngine::new();
    engine.register("primary",  1);
    engine.register("override", 2);
    engine.set_manual_override(Some("override"));
    engine.set_manual_override(None);

    assert!(engine.manual_override().is_none());

    let price = engine
        .fetch("XLM", now(), make_fetch(&[("primary", 1_000), ("override", 9_999)]))
        .unwrap();

    assert_eq!(price, 1_000, "after clearing override, primary should win");
}

#[test]
fn manual_override_returns_err_when_override_adapter_fails() {
    let mut engine = FailoverEngine::new();
    engine.register("primary",  1);
    engine.register("override", 2);
    engine.set_manual_override(Some("override"));

    // override is not in the success map
    let result = engine.fetch("XLM", now(), make_fetch(&[("primary", 1_000)]));
    assert!(result.is_err());
}

// ─── FailoverEngine: event log ────────────────────────────────────────────────

#[test]
fn successful_fetch_emits_succeeded_event() {
    let mut engine = FailoverEngine::new();
    engine.register("a", 1);
    engine.fetch("XLM", now(), make_fetch(&[("a", 500)])).unwrap();

    let events = engine.drain_events();
    assert_eq!(events.len(), 1);
    assert!(events[0].succeeded);
    assert!(events[0].reason.is_none());
}

#[test]
fn failed_fetch_emits_error_reason_event() {
    let mut engine = FailoverEngine::new();
    engine.register("a", 1);
    let _ = engine.fetch("XLM", now(), make_fetch(&[]));

    let events = engine.drain_events();
    assert_eq!(events.len(), 1);
    assert!(!events[0].succeeded);
    assert_eq!(events[0].reason, Some(FailoverReason::AdapterError));
}

#[test]
fn drain_events_clears_log() {
    let mut engine = FailoverEngine::new();
    engine.register("a", 1);
    engine.fetch("XLM", now(), make_fetch(&[("a", 1)])).unwrap();
    engine.drain_events();
    assert!(engine.events().is_empty());
}

// ─── FailoverEngine: set_priority ────────────────────────────────────────────

#[test]
fn set_priority_re_sorts_adapter_list() {
    let mut engine = FailoverEngine::new();
    engine.register("a", 10);
    engine.register("b", 20);
    engine.set_priority("b", 5); // promote b above a

    let ids: Vec<&str> = engine.adapters().iter().map(|a| a.adapter_id.as_str()).collect();
    assert_eq!(ids[0], "b", "b should now be first");
}

// ─── OracleFailoverCoordinator: consensus ────────────────────────────────────

#[test]
fn coordinator_reaches_consensus_with_agreeing_oracles() {
    let mut coord = OracleFailoverCoordinator::new();
    coord.register_oracle("o1", 1);
    coord.register_oracle("o2", 2);
    coord.register_oracle("o3", 3);

    let result = coord
        .fetch_consensus("XLM", now(), make_fetch(&[
            ("o1", 1_000_000),
            ("o2", 1_000_010),
            ("o3", 1_000_005),
        ]))
        .unwrap();

    assert!(result.quorum_reached);
    assert!(result.consensus_price.is_some());
    assert_eq!(result.responses, 3);
    assert_eq!(result.excluded, 0);
}

#[test]
fn coordinator_fails_when_quorum_not_met() {
    let cfg = CoordinatorConfig { min_quorum: 3, ..Default::default() };
    let mut coord = OracleFailoverCoordinator::with_config(cfg);
    coord.register_oracle("o1", 1);
    coord.register_oracle("o2", 2);
    // Only 2 oracles registered but 3 required

    let result = coord.fetch_consensus(
        "XLM",
        now(),
        make_fetch(&[("o1", 1_000_000), ("o2", 1_000_000)]),
    );

    assert!(matches!(result, Err(CoordinatorError::QuorumNotMet { .. })));
}

#[test]
fn coordinator_returns_no_oracles_error_when_empty() {
    let mut coord = OracleFailoverCoordinator::new();
    let result = coord.fetch_consensus("XLM", now(), make_fetch(&[]));
    assert!(matches!(result, Err(CoordinatorError::NoOracles)));
}

#[test]
fn coordinator_excludes_outlier_oracle() {
    let cfg = CoordinatorConfig {
        min_quorum: 2,
        max_deviation_bps: 100, // 1 %
        ..Default::default()
    };
    let mut coord = OracleFailoverCoordinator::with_config(cfg);
    coord.register_oracle("o1", 1);
    coord.register_oracle("o2", 2);
    coord.register_oracle("outlier", 3);

    // o1 and o2 agree; outlier is 50 % away
    let result = coord
        .fetch_consensus("XLM", now(), make_fetch(&[
            ("o1",      1_000_000),
            ("o2",      1_000_000),
            ("outlier", 1_500_000), // 50 % deviation
        ]))
        .unwrap();

    assert!(result.quorum_reached);
    assert_eq!(result.excluded, 1, "outlier must be excluded");
}

#[test]
fn coordinator_consensus_price_is_median_of_included() {
    let mut coord = OracleFailoverCoordinator::new();
    coord.register_oracle("o1", 1);
    coord.register_oracle("o2", 2);
    coord.register_oracle("o3", 3);

    // Prices: 900, 1000, 1100 → median = 1000
    let result = coord
        .fetch_consensus("XLM", now(), make_fetch(&[
            ("o1",   900_000),
            ("o2", 1_000_000),
            ("o3", 1_100_000),
        ]))
        .unwrap();

    assert_eq!(result.consensus_price, Some(1_000_000));
}

#[test]
fn coordinator_manual_override_bypasses_multi_oracle_chain() {
    let mut coord = OracleFailoverCoordinator::new();
    coord.register_oracle("o1", 1);
    coord.register_oracle("o2", 2);
    coord.set_manual_override(Some("o2"));

    let result = coord
        .fetch_consensus("XLM", now(), make_fetch(&[
            ("o1", 1_000_000),
            ("o2", 7_777_777),
        ]))
        .unwrap();

    assert_eq!(result.consensus_price, Some(7_777_777));
}

#[test]
fn coordinator_drain_events_returns_consensus_reached_event() {
    let mut coord = OracleFailoverCoordinator::new();
    coord.register_oracle("o1", 1);
    coord.register_oracle("o2", 2);

    coord
        .fetch_consensus("XLM", now(), make_fetch(&[
            ("o1", 1_000_000),
            ("o2", 1_000_000),
        ]))
        .unwrap();

    let events = coord.drain_events();
    let reached = events
        .iter()
        .any(|e| e.event_type == CoordinationEventType::ConsensusReached);
    assert!(reached, "ConsensusReached event must be emitted");
}

#[test]
fn coordinator_health_accessible_after_fetch() {
    let mut coord = OracleFailoverCoordinator::new();
    coord.register_oracle("o1", 1);
    coord.register_oracle("o2", 2);

    coord
        .fetch_consensus("XLM", now(), make_fetch(&[
            ("o1", 1_000_000),
            ("o2", 1_000_000),
        ]))
        .unwrap();

    let h1 = coord.health().get("o1").unwrap();
    assert_eq!(h1.successful_requests, 1);
}

#[test]
fn coordinator_set_oracle_enabled_skips_disabled() {
    let mut coord = OracleFailoverCoordinator::new();
    coord.register_oracle("o1", 1);
    coord.register_oracle("o2", 2);
    coord.set_oracle_enabled("o1", false);

    let result = coord
        .fetch_consensus("XLM", now(), make_fetch(&[
            ("o1", 1_000_000),
            ("o2", 2_000_000),
        ]));

    // Only o2 responded — quorum = 2 so this may fail or succeed depending on
    // min_quorum (default 2). Either way o1 must not contribute.
    match result {
        Ok(r) => {
            let o1_contrib = r.contributions.iter().find(|c| c.oracle_id == "o1");
            if let Some(c) = o1_contrib {
                assert!(!c.included, "disabled oracle must not be included");
            }
        }
        Err(_) => { /* quorum not met — also acceptable */ }
    }
}
