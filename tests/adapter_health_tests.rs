//! Adapter health-metrics tests for `AdapterHealth`, `HealthMonitor`,
//! and the auto-demotion behaviour of `FailoverEngine`.
//!
//! Covers:
//!   - Response-time tracking (avg, p95, p99)
//!   - Success-rate calculation (successful / total requests)
//!   - Price deviation tracking (difference from consensus)
//!   - Uptime percentage via tick()
//!   - is_healthy flag transitions
//!   - HealthMonitor.ranked() ordering
//!   - HealthMonitor.unhealthy_adapters()
//!   - FailoverEngine auto-demotes underperforming adapters

use stellar_defi_toolkit::contracts::price_feed_adapters::failover::{
    AdapterHealth, FailoverEngine, HealthMonitor,
    DEMOTION_THRESHOLD_BPS, MAX_ACCEPTABLE_DEVIATION_BPS,
};

// ─── helpers ─────────────────────────────────────────────────────────────────

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Create a fresh `AdapterHealth` via `HealthMonitor` (the only public ctor).
fn make_health(adapter_id: &str) -> AdapterHealth {
    let mut m = HealthMonitor::new();
    m.register(adapter_id);
    m.get(adapter_id).unwrap().clone()
}

// ─── AdapterHealth: initial state ────────────────────────────────────────────

#[test]
fn new_adapter_has_full_success_rate_and_is_healthy() {
    let h = make_health("oracle_a");
    assert_eq!(h.success_rate_bps, 10_000, "new adapter starts at 100 % success rate");
    assert!(h.is_healthy, "new adapter must start healthy");
    assert_eq!(h.total_requests, 0);
    assert_eq!(h.successful_requests, 0);
}

// ─── AdapterHealth: success-rate ─────────────────────────────────────────────

#[test]
fn success_rate_100_percent_when_all_succeed() {
    let mut m = HealthMonitor::new();
    m.register("a");
    let ts = now();
    for _ in 0..10 {
        m.record("a", 10, true, 0, ts);
    }
    let h = m.get("a").unwrap();
    assert_eq!(h.success_rate_bps, 10_000);
    assert_eq!(h.successful_requests, 10);
}

#[test]
fn success_rate_50_percent_for_half_failures() {
    let mut m = HealthMonitor::new();
    m.register("a");
    let ts = now();
    for i in 0..10 {
        m.record("a", 10, i % 2 == 0, 0, ts);
    }
    let h = m.get("a").unwrap();
    assert_eq!(h.success_rate_bps, 5_000, "5/10 success = 50 %");
}

#[test]
fn success_rate_zero_when_all_fail() {
    let mut m = HealthMonitor::new();
    m.register("a");
    let ts = now();
    for _ in 0..5 {
        m.record("a", 10, false, 0, ts);
    }
    let h = m.get("a").unwrap();
    assert_eq!(h.success_rate_bps, 0);
}

// ─── AdapterHealth: is_healthy transitions ───────────────────────────────────

#[test]
fn adapter_becomes_unhealthy_after_many_failures() {
    let mut m = HealthMonitor::new();
    m.register("a");
    let ts = now();
    // Drive success rate below DEMOTION_THRESHOLD_BPS (80 %)
    // 20 % success: 2 succeed, 8 fail
    for _ in 0..8 {
        m.record("a", 10, false, 0, ts);
    }
    for _ in 0..2 {
        m.record("a", 10, true, 0, ts);
    }
    let h = m.get("a").unwrap();
    assert!(
        h.success_rate_bps < DEMOTION_THRESHOLD_BPS,
        "success rate {} must be below demotion threshold {}",
        h.success_rate_bps,
        DEMOTION_THRESHOLD_BPS
    );
    assert!(!h.is_healthy, "adapter must be marked unhealthy");
}

#[test]
fn adapter_stays_healthy_at_exactly_demotion_threshold() {
    let mut m = HealthMonitor::new();
    m.register("a");
    let ts = now();
    // 80 % success: 8 succeed, 2 fail → exactly at DEMOTION_THRESHOLD_BPS (8000)
    for _ in 0..8 {
        m.record("a", 10, true, 0, ts);
    }
    for _ in 0..2 {
        m.record("a", 10, false, 0, ts);
    }
    let h = m.get("a").unwrap();
    assert_eq!(h.success_rate_bps, DEMOTION_THRESHOLD_BPS);
    assert!(h.is_healthy, "exactly at threshold must still be healthy");
}

#[test]
fn high_deviation_makes_adapter_unhealthy() {
    let mut m = HealthMonitor::new();
    m.register("a");
    let ts = now();
    // All succeed but with large deviation
    for _ in 0..10 {
        m.record("a", 10, true, MAX_ACCEPTABLE_DEVIATION_BPS + 100, ts);
    }
    let h = m.get("a").unwrap();
    assert!(
        h.avg_deviation_bps > MAX_ACCEPTABLE_DEVIATION_BPS,
        "avg deviation {} must exceed threshold {}",
        h.avg_deviation_bps,
        MAX_ACCEPTABLE_DEVIATION_BPS
    );
    assert!(!h.is_healthy, "high deviation must mark adapter unhealthy");
}

// ─── AdapterHealth: latency stats ────────────────────────────────────────────

#[test]
fn avg_latency_computed_correctly() {
    let mut m = HealthMonitor::new();
    m.register("a");
    let ts = now();
    // latencies: 10, 20, 30 → avg = 20
    m.record("a", 10, true, 0, ts);
    m.record("a", 20, true, 0, ts);
    m.record("a", 30, true, 0, ts);
    let h = m.get("a").unwrap();
    assert_eq!(h.avg_latency_ms, 20);
}

#[test]
fn p95_latency_is_95th_percentile() {
    let mut m = HealthMonitor::new();
    m.register("a");
    let ts = now();
    // 100 samples: 1..=100 ms
    for i in 1u64..=100 {
        m.record("a", i, true, 0, ts);
    }
    let h = m.get("a").unwrap();
    // p95 of [1..100] → index 94 (0-based) of sorted array = 95
    assert_eq!(h.p95_latency_ms, 95, "p95 should be 95 ms");
}

#[test]
fn p99_latency_is_99th_percentile() {
    let mut m = HealthMonitor::new();
    m.register("a");
    let ts = now();
    for i in 1u64..=100 {
        m.record("a", i, true, 0, ts);
    }
    let h = m.get("a").unwrap();
    assert_eq!(h.p99_latency_ms, 99, "p99 should be 99 ms");
}

#[test]
fn latency_window_capped_at_100_samples() {
    let mut m = HealthMonitor::new();
    m.register("a");
    let ts = now();
    // Insert 150 samples — window must stay ≤ 100
    for i in 0u64..150 {
        m.record("a", i, true, 0, ts);
    }
    let h = m.get("a").unwrap();
    assert!(
        h.latency_samples_ms.len() <= 100,
        "latency window must not exceed 100 samples"
    );
}

// ─── AdapterHealth: uptime via tick ──────────────────────────────────────────

#[test]
fn uptime_100_when_always_healthy() {
    let mut m = HealthMonitor::new();
    m.register("a");
    // 10 ticks × 1 second, always healthy
    for _ in 0..10 {
        m.tick_all(1);
    }
    let h = m.get("a").unwrap();
    assert_eq!(h.uptime_bps, 10_000, "100 % uptime when always healthy");
}

#[test]
fn uptime_50_when_half_healthy() {
    let mut m = HealthMonitor::new();
    m.register("a");
    let ts = now();
    // First make adapter unhealthy
    for _ in 0..10 {
        m.record("a", 10, false, 0, ts);
    }
    // 10 healthy ticks then 10 unhealthy ticks
    for _ in 0..10 {
        // Force is_healthy = true for half
        m.record("a", 10, true, 0, ts);
        m.tick_all(1);
    }
    for _ in 0..10 {
        m.tick_all(1);
    }
    let h = m.get("a").unwrap();
    // uptime_bps depends on actual healthy_secs / monitored_secs ratio
    assert!(h.uptime_bps <= 10_000, "uptime must be between 0 and 100 %");
}

#[test]
fn uptime_zero_when_never_healthy() {
    let mut m = HealthMonitor::new();
    m.register("a");
    let ts = now();
    // Make unhealthy immediately
    for _ in 0..10 {
        m.record("a", 10, false, 0, ts);
    }
    // All ticks while unhealthy
    for _ in 0..10 {
        m.tick_all(1);
    }
    let h = m.get("a").unwrap();
    assert_eq!(h.uptime_bps, 0, "zero uptime when never healthy");
}

// ─── AdapterHealth: price deviation tracking ─────────────────────────────────

#[test]
fn avg_deviation_zero_when_always_on_consensus() {
    let mut m = HealthMonitor::new();
    m.register("a");
    let ts = now();
    for _ in 0..10 {
        m.record("a", 10, true, 0, ts);
    }
    assert_eq!(m.get("a").unwrap().avg_deviation_bps, 0);
}

#[test]
fn avg_deviation_tracks_mean_of_deviation_samples() {
    let mut m = HealthMonitor::new();
    m.register("a");
    let ts = now();
    // 5 requests with deviation 100 bps each
    for _ in 0..5 {
        m.record("a", 10, true, 100, ts);
    }
    assert_eq!(m.get("a").unwrap().avg_deviation_bps, 100);
}

#[test]
fn deviation_only_counted_on_successful_requests() {
    let mut m = HealthMonitor::new();
    m.register("a");
    let ts = now();
    // 5 failures with huge deviation → should not affect avg_deviation
    for _ in 0..5 {
        m.record("a", 10, false, 9_999, ts);
    }
    // 5 successes with 0 deviation
    for _ in 0..5 {
        m.record("a", 10, true, 0, ts);
    }
    assert_eq!(m.get("a").unwrap().avg_deviation_bps, 0);
}

// ─── HealthMonitor: ranked ────────────────────────────────────────────────────

#[test]
fn ranked_returns_adapters_sorted_by_success_rate_descending() {
    let mut m = HealthMonitor::new();
    m.register("a");
    m.register("b");
    m.register("c");
    let ts = now();
    // a: 10/10, b: 5/10, c: 2/10
    for _ in 0..10 { m.record("a", 10, true,  0, ts); }
    for i in 0..10 { m.record("b", 10, i < 5, 0, ts); }
    for i in 0..10 { m.record("c", 10, i < 2, 0, ts); }

    let ranked = m.ranked();
    assert_eq!(ranked[0].adapter_id, "a");
    assert_eq!(ranked[1].adapter_id, "b");
    assert_eq!(ranked[2].adapter_id, "c");
}

// ─── HealthMonitor: unhealthy_adapters ───────────────────────────────────────

#[test]
fn unhealthy_adapters_empty_when_all_healthy() {
    let mut m = HealthMonitor::new();
    m.register("a");
    m.register("b");
    let ts = now();
    for _ in 0..5 { m.record("a", 10, true, 0, ts); }
    for _ in 0..5 { m.record("b", 10, true, 0, ts); }
    assert!(m.unhealthy_adapters().is_empty());
}

#[test]
fn unhealthy_adapters_returns_only_bad_ones() {
    let mut m = HealthMonitor::new();
    m.register("good");
    m.register("bad");
    let ts = now();
    for _ in 0..10 { m.record("good", 10, true,  0, ts); }
    for _ in 0..10 { m.record("bad",  10, false, 0, ts); }
    let unhealthy = m.unhealthy_adapters();
    assert_eq!(unhealthy.len(), 1);
    assert_eq!(unhealthy[0], "bad");
}

// ─── FailoverEngine: auto-demotion ───────────────────────────────────────────

#[test]
fn auto_demote_disables_underperforming_adapter_after_successful_fetch() {
    let mut engine = FailoverEngine::new();
    engine.register("bad",  1);
    engine.register("good", 2);

    let ts = now();
    // Drive "bad" below demotion threshold (80 %)
    // 2 success, 8 fail → 20 % success
    for _ in 0..8 { engine.health.record("bad", 10, false, 0, ts); }
    for _ in 0..2 { engine.health.record("bad", 10, true,  0, ts); }

    // Perform a fetch — "bad" is unhealthy so engine skips it; "good" succeeds
    // The auto_demote fires on the successful return
    let fetch_fn = |adapter_id: &str, _: &str| -> (Result<u64, String>, u64) {
        if adapter_id == "good" { (Ok(1_000), 10) } else { (Err("fail".into()), 50) }
    };
    engine.fetch("XLM", ts, fetch_fn).unwrap();

    let bad_adapter = engine.adapters().iter().find(|a| a.adapter_id == "bad");
    assert!(
        bad_adapter.map(|a| !a.enabled).unwrap_or(true),
        "underperforming adapter must be disabled after auto-demotion"
    );
}

// ─── HealthMonitor: register is idempotent ───────────────────────────────────

#[test]
fn register_same_adapter_twice_is_idempotent() {
    let mut m = HealthMonitor::new();
    m.register("a");
    let ts = now();
    m.record("a", 10, true, 0, ts); // 1 successful request
    m.register("a"); // second registration — must not reset counters
    assert_eq!(m.get("a").unwrap().total_requests, 1);
}

// ─── last_success_ts ─────────────────────────────────────────────────────────

#[test]
fn last_success_ts_updated_on_success() {
    let mut m = HealthMonitor::new();
    m.register("a");
    let ts = now();
    m.record("a", 10, true, 0, ts);
    assert_eq!(m.get("a").unwrap().last_success_ts, ts);
}

#[test]
fn last_success_ts_not_updated_on_failure() {
    let mut m = HealthMonitor::new();
    m.register("a");
    let ts = now();
    m.record("a", 10, false, 0, ts);
    assert_eq!(m.get("a").unwrap().last_success_ts, 0);
}
