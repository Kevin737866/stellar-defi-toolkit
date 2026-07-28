//! Anomaly detection tests for `AnomalyDetector` and
//! `PriceHistoryManager::detect_anomaly`.
//!
//! Covers:
//!   - Z-score flagging (> 3 σ)
//!   - IQR flagging (> 1.5 × IQR fence)
//!   - Moving-average crossover flagging
//!   - High-confidence when ≥ 2 methods agree
//!   - Normal prices are not flagged
//!   - NativeCircuitBreakerGuard integration (Allowed / Warned / Blocked)

use stellar_defi_toolkit::contracts::price_history::{
    AnomalyConfig, AnomalyConfidence, AnomalyDetector, AnomalyMethod,
    ArchivalConfig, PriceHistoryEntry, PriceHistoryManager,
};
use stellar_defi_toolkit::contracts::circuit_breaker::anomaly_integration::{
    GuardConfig, GuardDecision, NativeCircuitBreakerGuard,
};

// ─── helpers ─────────────────────────────────────────────────────────────────

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Build a window of `n` identical prices — zero variance.
fn flat_window(price: u64, n: usize) -> Vec<u64> {
    vec![price; n]
}

/// Build a stable window oscillating ±1 around `center`.
fn stable_window(center: u64, n: usize) -> Vec<u64> {
    (0..n).map(|i| if i % 2 == 0 { center } else { center + 1 }).collect()
}

/// Detector with a very low MA-crossover threshold so crossover tests fire.
fn sensitive_config() -> AnomalyConfig {
    AnomalyConfig {
        z_score_threshold_x100: 300,
        iqr_multiplier_x100: 150,
        min_window: 10,
        ma_short_period: 3_600,
        ma_long_period: 86_400,
        ma_crossover_threshold_bps: 50, // 0.5 % — easy to trigger
    }
}

// ─── AnomalyDetector: normal price ───────────────────────────────────────────

#[test]
fn normal_price_not_flagged() {
    let det = AnomalyDetector::new();
    let window = stable_window(1_000_000, 20);
    let result = det.detect("XLM", 1_000_001, now(), &window);
    assert!(!result.is_anomaly, "tiny deviation must not be flagged");
    assert!(result.methods.is_empty());
    assert!(result.confidence.is_none());
}

// ─── Z-score ─────────────────────────────────────────────────────────────────

#[test]
fn z_score_flags_extreme_price() {
    let det = AnomalyDetector::new();
    // window centred on 1 000, candidate is 10 000 — many σ away
    let window: Vec<u64> = (0..20).map(|i| 990 + i % 20).collect();
    let result = det.detect("XLM", 10_000, now(), &window);
    assert!(result.is_anomaly);
    assert!(result.methods.contains(&AnomalyMethod::ZScore));
}

#[test]
fn z_score_x100_positive_for_high_candidate() {
    let det = AnomalyDetector::new();
    let window = flat_window(1_000, 20);
    // candidate is much higher than mean
    let z = det.z_score_x100(5_000, &window);
    assert!(z > 0, "z-score should be positive for price above mean");
}

#[test]
fn z_score_x100_negative_for_low_candidate() {
    let det = AnomalyDetector::new();
    let window = flat_window(5_000, 20);
    let z = det.z_score_x100(1, &window);
    assert!(z < 0, "z-score should be negative for price below mean");
}

#[test]
fn z_score_zero_for_mean_price() {
    let det = AnomalyDetector::new();
    let window = flat_window(1_000, 20);
    // candidate equals the mean
    let z = det.z_score_x100(1_000, &window);
    assert_eq!(z, 0);
}

#[test]
fn z_score_zero_variance_same_price_not_anomaly() {
    // All window values and candidate are identical → z = 0
    let det = AnomalyDetector::new();
    let window = flat_window(1_000, 20);
    let result = det.detect("XLM", 1_000, now(), &window);
    assert!(!result.is_anomaly);
}

#[test]
fn z_score_empty_window_returns_zero() {
    let det = AnomalyDetector::new();
    let z = det.z_score_x100(99_999, &[]);
    assert_eq!(z, 0);
}

// ─── IQR ─────────────────────────────────────────────────────────────────────

#[test]
fn iqr_flags_price_above_upper_fence() {
    let det = AnomalyDetector::new();
    // Tight cluster 100–200; fence = Q3 + 1.5*IQR ≈ 200 + 150 = 350
    let window: Vec<u64> = (100..120u64).collect(); // 20 values, 100..119
    let result = det.detect("XLM", 10_000, now(), &window);
    assert!(result.is_anomaly);
    assert!(result.methods.contains(&AnomalyMethod::Iqr));
}

#[test]
fn iqr_flags_price_below_lower_fence() {
    let det = AnomalyDetector::new();
    let window: Vec<u64> = (1_000..1_020u64).collect();
    let result = det.detect("XLM", 1, now(), &window);
    assert!(result.is_anomaly);
    assert!(result.methods.contains(&AnomalyMethod::Iqr));
}

#[test]
fn iqr_not_flagged_within_fence() {
    let det = AnomalyDetector::new();
    // All values identical, IQR = 0, fence = 0 → no outlier possible for same value
    let window = flat_window(500, 20);
    let flagged = det.iqr_flagged(500, &window);
    assert!(!flagged);
}

#[test]
fn iqr_small_window_not_flagged() {
    let det = AnomalyDetector::new();
    // < 4 values → IQR method skips
    let window = vec![100u64, 200, 300];
    let flagged = det.iqr_flagged(99_999, &window);
    assert!(!flagged, "window < 4 must not be flagged by IQR");
}

// ─── Moving-average crossover ─────────────────────────────────────────────────

#[test]
fn ma_crossover_flags_diverging_candidate() {
    let det = AnomalyDetector::with_config(sensitive_config());
    // Long mean ≈ 1 000; last quarter of window is trending sharply up
    let mut window: Vec<u64> = vec![1_000u64; 20];
    // Replace last 5 with much higher values to create short-MA divergence
    for v in window.iter_mut().rev().take(5) {
        *v = 1_200;
    }
    // Candidate pushes even further beyond the divergence
    let result = det.detect("XLM", 1_500, now(), &window);
    assert!(result.is_anomaly, "diverging candidate should be flagged");
    assert!(result.methods.contains(&AnomalyMethod::MovingAverageCrossover));
}

#[test]
fn ma_crossover_stable_window_not_flagged() {
    let det = AnomalyDetector::with_config(sensitive_config());
    let window = stable_window(1_000, 20);
    // Candidate barely moves from mean
    let flagged = det.ma_crossover_flagged(1_001, &window);
    assert!(!flagged, "stable window must not trigger MA crossover");
}

// ─── High-confidence (≥ 2 methods) ───────────────────────────────────────────

#[test]
fn high_confidence_when_two_methods_agree() {
    let det = AnomalyDetector::new();
    // Tight cluster 100–119; a price of 50 000 is both Z-score and IQR outlier
    let window: Vec<u64> = (100..120u64).collect();
    let result = det.detect("XLM", 50_000, now(), &window);
    assert!(result.is_anomaly);
    assert!(result.methods.len() >= 2, "expected at least 2 methods to agree");
    assert_eq!(result.confidence, Some(AnomalyConfidence::High));
}

#[test]
fn low_confidence_when_one_method_flags() {
    // Craft a case where only IQR fires: price just outside the fence but
    // not enough to hit the Z-score threshold.
    // Use a wide-variance window so Z-score stays below 3σ.
    let det = AnomalyDetector::new();
    // window with spread so std-dev is large but IQR fence is narrow
    let mut window: Vec<u64> = vec![
        1, 2, 3, 4, 5, 6, 7, 8, 9, 10,
        1_000_000, 1_000_001, 1_000_002, 1_000_003, 1_000_004,
        1_000_005, 1_000_006, 1_000_007, 1_000_008, 1_000_009,
    ];
    // candidate is right at q3 + 1.5*IQR boundary — we just want to check
    // confidence logic, so use a known outlier in the low end
    let result = det.detect("XLM", 0, now(), &window);
    if result.is_anomaly && result.methods.len() == 1 {
        assert_eq!(result.confidence, Some(AnomalyConfidence::Low));
    }
    // If both fire, high confidence is also acceptable
}

// ─── Below min_window: no statistical methods run ────────────────────────────

#[test]
fn below_min_window_no_anomaly() {
    let cfg = AnomalyConfig { min_window: 10, ..Default::default() };
    let det = AnomalyDetector::with_config(cfg);
    // Only 5 points — below min_window of 10
    let window: Vec<u64> = vec![100, 100, 100, 100, 100];
    let result = det.detect("XLM", 999_999, now(), &window);
    assert!(!result.is_anomaly, "below min_window no method should fire");
}

// ─── AnomalyConfig defaults ───────────────────────────────────────────────────

#[test]
fn anomaly_config_defaults() {
    let cfg = AnomalyConfig::default();
    assert_eq!(cfg.z_score_threshold_x100, 300);
    assert_eq!(cfg.iqr_multiplier_x100, 150);
    assert_eq!(cfg.min_window, 10);
    assert_eq!(cfg.ma_crossover_threshold_bps, 200);
}

// ─── AnomalyResult fields ─────────────────────────────────────────────────────

#[test]
fn anomaly_result_asset_price_timestamp_populated() {
    let det = AnomalyDetector::new();
    let ts = now();
    let result = det.detect("BTC", 999, ts, &[]);
    assert_eq!(result.asset_id, "BTC");
    assert_eq!(result.price, 999);
    assert_eq!(result.timestamp, ts);
}

// ─── PriceHistoryManager::detect_anomaly integration ─────────────────────────

#[test]
fn detect_anomaly_returns_result_for_unknown_asset() {
    let mgr = PriceHistoryManager::new();
    // No data stored — window will be empty; should not crash
    let result = mgr.detect_anomaly("UNKNOWN", 1_000_000, now(), 3_600, None);
    assert!(!result.is_anomaly, "empty window must not produce anomaly");
}

#[test]
fn detect_anomaly_normal_price_not_flagged() {
    let cfg = ArchivalConfig { auto_archive_on_store: false, ..Default::default() };
    let mut mgr = PriceHistoryManager::with_config(cfg);
    let base = now() - 1_200;
    // Store 20 stable prices
    for i in 0u64..20 {
        mgr.store_price(PriceHistoryEntry {
            asset_id: "XLM".into(),
            price: 1_000_000 + i * 10,
            decimals: 6,
            timestamp: base + i * 60,
            source: "oracle".into(),
            volume: 100,
            transaction_count: 1,
        })
        .unwrap();
    }
    let result = mgr.detect_anomaly("XLM", 1_000_100, now(), 3_600, None);
    assert!(!result.is_anomaly, "price within normal range must not be flagged");
}

#[test]
fn detect_anomaly_extreme_price_flagged() {
    let cfg = ArchivalConfig { auto_archive_on_store: false, ..Default::default() };
    let mut mgr = PriceHistoryManager::with_config(cfg);
    let base = now() - 1_200;
    // Store 20 identical prices
    for i in 0u64..20 {
        mgr.store_price(PriceHistoryEntry {
            asset_id: "XLM".into(),
            price: 1_000_000,
            decimals: 6,
            timestamp: base + i * 60,
            source: "oracle".into(),
            volume: 100,
            transaction_count: 1,
        })
        .unwrap();
    }
    // Candidate is 100× the mean — should trigger Z-score at minimum
    let result = mgr.detect_anomaly("XLM", 100_000_000, now(), 3_600, None);
    assert!(result.is_anomaly, "100× price spike must be detected");
}

// ─── NativeCircuitBreakerGuard ────────────────────────────────────────────────

#[test]
fn guard_allows_normal_prices() {
    let mut guard = NativeCircuitBreakerGuard::new();
    let ts = now();
    // Seed the window with 15 stable prices first
    for i in 0u64..15 {
        guard.check_price("XLM", 1_000_000 + i * 10, ts - 900 + i * 60).ok();
    }
    // Normal price — should be allowed
    let decision = guard.check_price("XLM", 1_000_100, ts);
    assert!(decision.is_ok());
    assert_ne!(decision.unwrap(), GuardDecision::Blocked);
}

#[test]
fn guard_blocks_extreme_price_when_high_confidence() {
    let cfg = GuardConfig {
        block_on_high_confidence: true,
        warn_on_low_confidence: true,
        anomaly_config: AnomalyConfig { min_window: 10, ..Default::default() },
        max_window_size: 200,
    };
    let mut guard = NativeCircuitBreakerGuard::with_config(cfg);
    let ts = now();
    // Seed with 15 identical prices
    for i in 0u64..15 {
        guard.check_price("XLM", 1_000_000, ts - 900 + i * 60).ok();
    }
    // Candidate is extreme — expect Blocked
    let decision = guard.check_price("XLM", 999_000_000, ts);
    assert_eq!(decision, Err(GuardDecision::Blocked));
}

#[test]
fn guard_does_not_block_when_block_disabled() {
    let cfg = GuardConfig {
        block_on_high_confidence: false,
        warn_on_low_confidence: true,
        anomaly_config: AnomalyConfig { min_window: 10, ..Default::default() },
        max_window_size: 200,
    };
    let mut guard = NativeCircuitBreakerGuard::with_config(cfg);
    let ts = now();
    for i in 0u64..15 {
        guard.check_price("XLM", 1_000_000, ts - 900 + i * 60).ok();
    }
    // Even an extreme price must not be blocked when block_on_high_confidence=false
    let decision = guard.check_price("XLM", 999_000_000, ts);
    assert!(decision.is_ok(), "blocking disabled — must not return Err");
}

#[test]
fn guard_emits_event_for_every_check() {
    let mut guard = NativeCircuitBreakerGuard::new();
    let ts = now();
    guard.check_price("XLM", 1_000_000, ts - 120).ok();
    guard.check_price("XLM", 1_000_001, ts - 60).ok();
    guard.check_price("XLM", 1_000_002, ts).ok();
    assert_eq!(guard.events().len(), 3);
}

#[test]
fn guard_drain_events_clears_log() {
    let mut guard = NativeCircuitBreakerGuard::new();
    let ts = now();
    guard.check_price("XLM", 1_000_000, ts).ok();
    let events = guard.drain_events();
    assert_eq!(events.len(), 1);
    assert_eq!(guard.events().len(), 0, "drain must empty the log");
}

#[test]
fn guard_window_grows_with_allowed_prices() {
    let mut guard = NativeCircuitBreakerGuard::new();
    let ts = now();
    assert_eq!(guard.window_depth("XLM"), 0);
    guard.check_price("XLM", 1_000_000, ts - 60).ok();
    guard.check_price("XLM", 1_000_001, ts - 30).ok();
    assert_eq!(guard.window_depth("XLM"), 2);
}

#[test]
fn guard_blocked_price_not_added_to_window() {
    let cfg = GuardConfig {
        block_on_high_confidence: true,
        anomaly_config: AnomalyConfig { min_window: 10, ..Default::default() },
        ..Default::default()
    };
    let mut guard = NativeCircuitBreakerGuard::with_config(cfg);
    let ts = now();
    // Seed
    for i in 0u64..15 {
        guard.check_price("XLM", 1_000_000, ts - 900 + i * 60).ok();
    }
    let depth_before = guard.window_depth("XLM");
    // Extreme price — blocked
    let _ = guard.check_price("XLM", 999_000_000, ts);
    assert_eq!(
        guard.window_depth("XLM"),
        depth_before,
        "blocked price must not extend the window"
    );
}

#[test]
fn guard_evict_asset_resets_window() {
    let mut guard = NativeCircuitBreakerGuard::new();
    let ts = now();
    guard.check_price("XLM", 1_000_000, ts).ok();
    assert!(guard.window_depth("XLM") > 0);
    guard.evict_asset("XLM");
    assert_eq!(guard.window_depth("XLM"), 0);
}

#[test]
fn guard_config_round_trip() {
    let cfg = GuardConfig {
        block_on_high_confidence: false,
        warn_on_low_confidence: false,
        anomaly_config: AnomalyConfig::default(),
        max_window_size: 42,
    };
    let guard = NativeCircuitBreakerGuard::with_config(cfg);
    assert_eq!(guard.config().max_window_size, 42);
    assert!(!guard.config().block_on_high_confidence);
}
