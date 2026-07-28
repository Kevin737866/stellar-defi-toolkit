//! Circuit Breaker Tests
//!
//! Exercises the oracle's built-in circuit breaker (price-deviation
//! rejection) and staleness protection using `MockOracle`, so every scenario
//! is deterministic and doesn't depend on wall-clock time.

mod common;

use stellar_defi_toolkit::{MockOracle, OracleSanityConfig, ProtocolError, WAD};

fn oracle_with_deviation_threshold(max_price_deviation_bps: u32) -> MockOracle {
    let sanity = OracleSanityConfig {
        max_price_deviation_bps,
        ..OracleSanityConfig::default()
    };
    MockOracle::with_sanity("admin", sanity)
}

#[test]
fn test_circuit_breaker_initialization() {
    // A freshly constructed oracle has no prices and starts at time zero.
    let oracle = MockOracle::new("admin");
    assert_eq!(oracle.now(), 0);
    assert_eq!(
        oracle.get_price("XLM").unwrap_err(),
        ProtocolError::MissingPrice("XLM".to_string())
    );
}

#[test]
fn test_single_deviation_trip() {
    // Circuit breaker trips on a single update that deviates > 10%.
    let mut oracle = oracle_with_deviation_threshold(1_000); // 10%
    oracle.set_price("admin", "XLM", WAD).unwrap();

    // 20% jump is rejected.
    let err = oracle
        .set_price("admin", "XLM", WAD + WAD * 20 / 100)
        .unwrap_err();
    assert_eq!(
        err,
        ProtocolError::OracleSanityCheckFailed(
            "XLM".to_string(),
            "price deviation 2000bps exceeds circuit-breaker threshold 1000bps".to_string()
        )
    );

    // The last accepted price is unchanged.
    assert_eq!(oracle.get_price("XLM").unwrap(), WAD);
}

#[test]
fn test_consecutive_small_deviations_each_pass_within_threshold() {
    // The sim oracle evaluates deviation per-update against the last
    // *accepted* price, so a sequence of updates that each individually stay
    // within the threshold succeeds even though the cumulative drift is
    // large.
    let mut oracle = oracle_with_deviation_threshold(500); // 5%
    oracle.set_price("admin", "XLM", 1_000).unwrap();

    for _ in 0..3 {
        oracle.set_price("admin", "XLM", oracle.get_price("XLM").unwrap() * 104 / 100)
            .unwrap();
    }

    // ~12.5% cumulative drift, but every individual step was ~4%.
    assert!(oracle.get_price("XLM").unwrap() > 1_120);
}

#[test]
fn test_circuit_breaker_reset_via_sanity_config_update() {
    // "Resetting" the breaker for an asset means allowing the next update
    // through — admins do this by relaxing (or momentarily disabling) the
    // deviation threshold.
    let mut oracle = oracle_with_deviation_threshold(1_000);
    oracle.set_price("admin", "XLM", WAD).unwrap();

    let tripped = oracle.set_price("admin", "XLM", WAD * 2).unwrap_err();
    assert_eq!(
        tripped,
        ProtocolError::OracleSanityCheckFailed(
            "XLM".to_string(),
            "price deviation 10000bps exceeds circuit-breaker threshold 1000bps".to_string()
        )
    );

    // Admin disables the breaker (max_price_deviation_bps = 0) to push the
    // large move through deliberately.
    oracle
        .set_sanity_config(
            "admin",
            OracleSanityConfig {
                max_price_deviation_bps: 0,
                ..OracleSanityConfig::default()
            },
        )
        .unwrap();
    oracle.set_price("admin", "XLM", WAD * 2).unwrap();
    assert_eq!(oracle.get_price("XLM").unwrap(), WAD * 2);
}

#[test]
fn test_get_price_stale_after_max_age() {
    let sanity = OracleSanityConfig {
        max_price_age_secs: 3_600,
        ..OracleSanityConfig::default()
    };
    let mut oracle = MockOracle::with_sanity("admin", sanity);
    oracle.set_price("admin", "XLM", WAD).unwrap();

    // Still fresh just before the cutoff.
    oracle.set_time(3_599);
    assert_eq!(oracle.get_price("XLM").unwrap(), WAD);

    // Stale once max_price_age_secs is exceeded.
    oracle.simulate_staleness(2);
    let err = oracle.get_price("XLM").unwrap_err();
    assert_eq!(err, ProtocolError::OraclePriceStale("XLM".to_string()));
}

#[test]
fn test_is_operational_reports_min_max_price_bounds() {
    let sanity = OracleSanityConfig {
        min_price: 100,
        max_price: 10_000,
        ..OracleSanityConfig::default()
    };
    let mut oracle = MockOracle::with_sanity("admin", sanity);

    assert_eq!(
        oracle.set_price("admin", "XLM", 50).unwrap_err(),
        ProtocolError::OracleSanityCheckFailed(
            "XLM".to_string(),
            "price 50 is below minimum 100".to_string()
        )
    );
    assert_eq!(
        oracle.set_price("admin", "XLM", 20_000).unwrap_err(),
        ProtocolError::OracleSanityCheckFailed(
            "XLM".to_string(),
            "price 20000 exceeds maximum 10000".to_string()
        )
    );

    // A price inside the bounds is operational (accepted).
    oracle.set_price("admin", "XLM", 5_000).unwrap();
    assert_eq!(oracle.get_price("XLM").unwrap(), 5_000);
}

#[test]
fn test_disable_circuit_breaker() {
    let mut oracle = oracle_with_deviation_threshold(0); // disabled
    oracle.set_price("admin", "XLM", WAD).unwrap();

    // Even a 1000% jump is accepted when the breaker is disabled.
    oracle.set_price("admin", "XLM", WAD * 11).unwrap();
    assert_eq!(oracle.get_price("XLM").unwrap(), WAD * 11);
}

#[test]
fn test_normal_price_updates_do_not_trip_breaker() {
    let mut oracle = oracle_with_deviation_threshold(2_000); // 20% (protocol default)
    oracle.set_price("admin", "XLM", WAD).unwrap();

    // 5% moves are well within the default threshold.
    oracle.set_price("admin", "XLM", WAD * 105 / 100).unwrap();
    oracle.set_price("admin", "XLM", WAD * 100 / 100).unwrap();
    assert_eq!(oracle.get_price("XLM").unwrap(), WAD);
}

#[test]
fn test_only_admin_can_update_price() {
    let mut oracle = MockOracle::new("admin");
    let err = oracle.set_price("mallory", "XLM", WAD).unwrap_err();
    assert_eq!(err, ProtocolError::Unauthorized);
}

#[test]
fn test_market_crash_scenario_trips_breaker_without_bypass() {
    // A sudden 50% single-update crash trips the default protocol breaker
    // (20% max single-update deviation) unless the admin explicitly relaxes
    // it — this is the behavior stress tests in #251 rely on.
    let mut oracle = oracle_with_deviation_threshold(2_000);
    oracle.set_price("admin", "XLM", WAD).unwrap();

    let err = oracle.set_price("admin", "XLM", WAD / 2).unwrap_err();
    assert_eq!(
        err,
        ProtocolError::OracleSanityCheckFailed(
            "XLM".to_string(),
            "price deviation 5000bps exceeds circuit-breaker threshold 2000bps".to_string()
        )
    );

    // Spreading the same 50% crash over several smaller steps (as
    // `MockOracle::simulate_crash` does) succeeds because each individual
    // step stays under the threshold.
    let mut oracle = oracle_with_deviation_threshold(2_000);
    oracle
        .simulate_crash("admin", "XLM", WAD, 5_000, 6, 3_600)
        .unwrap();
    assert_eq!(oracle.get_price("XLM").unwrap(), WAD / 2);
}
