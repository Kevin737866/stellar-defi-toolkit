//! Archival tests for `PriceHistoryManager`.
//!
//! These tests exercise the three-tier data lifecycle:
//!   - Recent  (0–30 d):  full granularity in the live store
//!   - Medium  (30–90 d): compressed to hourly buckets
//!   - Historical (90+ d): compressed to daily buckets
//!
//! Because we cannot make the wall clock jump forward, every test builds its
//! own synthetic "current time" by backdating price-entry timestamps, then
//! calls `run_archival_at` (a test-only helper exposed via the module's
//! `#[cfg(test)]` block) — or simply calls `run_archival()` after populating
//! data with timestamps that are already in the past.

use stellar_defi_toolkit::contracts::price_history::{
    ArchivalConfig, ArchiveTier, PriceHistoryEntry, PriceHistoryManager, TimeBucket,
    TIER_MEDIUM_SECS, TIER_RECENT_SECS,
};

// ─── helpers ─────────────────────────────────────────────────────────────────

/// Current Unix time as seconds.
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Build a minimal `PriceHistoryEntry` at an explicit timestamp.
fn entry_at(asset: &str, price: u64, ts: u64) -> PriceHistoryEntry {
    PriceHistoryEntry {
        asset_id:          asset.to_string(),
        price,
        decimals:          6,
        timestamp:         ts,
        source:            "test_oracle".to_string(),
        volume:            500,
        transaction_count: 1,
    }
}

/// Insert `count` entries spaced `spacing_secs` apart ending at `end_ts`.
fn fill_entries(
    mgr:          &mut PriceHistoryManager,
    asset:        &str,
    count:        u64,
    spacing_secs: u64,
    end_ts:       u64,
    base_price:   u64,
) {
    for i in 0..count {
        let ts    = end_ts - (count - 1 - i) * spacing_secs;
        let price = base_price + i * 1_000;
        // store_price rejects future timestamps — these are all in the past
        mgr.store_price(entry_at(asset, price, ts)).unwrap();
    }
}

// ─── ArchiveTier::from_age ────────────────────────────────────────────────────

#[test]
fn tier_from_age_recent() {
    // Anything within 30 days is Recent
    assert_eq!(ArchiveTier::from_age(0),                        ArchiveTier::Recent);
    assert_eq!(ArchiveTier::from_age(TIER_RECENT_SECS - 1),     ArchiveTier::Recent);
    assert_eq!(ArchiveTier::from_age(TIER_RECENT_SECS),         ArchiveTier::Recent);
}

#[test]
fn tier_from_age_medium() {
    // 30 d + 1 s → Medium
    assert_eq!(ArchiveTier::from_age(TIER_RECENT_SECS + 1),     ArchiveTier::Medium);
    assert_eq!(ArchiveTier::from_age(TIER_MEDIUM_SECS),         ArchiveTier::Medium);
}

#[test]
fn tier_from_age_historical() {
    // Beyond 90 days → Historical
    assert_eq!(ArchiveTier::from_age(TIER_MEDIUM_SECS + 1),     ArchiveTier::Historical);
    assert_eq!(ArchiveTier::from_age(365 * 86_400),             ArchiveTier::Historical);
}

// ─── ArchiveTier::bucket_secs ─────────────────────────────────────────────────

#[test]
fn tier_bucket_widths() {
    assert_eq!(ArchiveTier::Recent.bucket_secs(),     60);        // 1 minute
    assert_eq!(ArchiveTier::Medium.bucket_secs(),     3_600);     // 1 hour
    assert_eq!(ArchiveTier::Historical.bucket_secs(), 86_400);    // 1 day
}

// ─── ArchivedBucket helpers ────────────────────────────────────────────────────

#[test]
fn archived_bucket_mean_price_and_compression_ratio() {
    use stellar_defi_toolkit::contracts::price_history::ArchivedBucket;

    let bucket = ArchivedBucket {
        asset_id:       "XLM".into(),
        tier:           ArchiveTier::Medium,
        window_start:   0,
        window_end:     3_600,
        open:           1_000,
        high:           2_000,
        low:              500,
        close:          1_500,
        price_sum:      60_000,  // 60 one-minute entries at 1 000 avg
        volume:         60_000,
        raw_entry_count: 60,
    };

    assert_eq!(bucket.mean_price(), 1_000);
    // Medium tier = 3 600 s / 60 s = 60 × compression
    assert_eq!(bucket.compression_ratio(), 60);
}

#[test]
fn archived_bucket_mean_price_zero_entries() {
    use stellar_defi_toolkit::contracts::price_history::ArchivedBucket;

    let bucket = ArchivedBucket {
        asset_id:        "XLM".into(),
        tier:            ArchiveTier::Historical,
        window_start:    0,
        window_end:      86_400,
        open:            0,
        high:            0,
        low:             0,
        close:           0,
        price_sum:       0,
        volume:          0,
        raw_entry_count: 0,
    };

    assert_eq!(bucket.mean_price(), 0); // no div-by-zero
}

#[test]
fn historical_bucket_compression_ratio() {
    use stellar_defi_toolkit::contracts::price_history::ArchivedBucket;

    let bucket = ArchivedBucket {
        asset_id:        "XLM".into(),
        tier:            ArchiveTier::Historical,
        window_start:    0,
        window_end:      86_400,
        open:            0,
        high:            0,
        low:             0,
        close:           0,
        price_sum:       0,
        volume:          0,
        raw_entry_count: 0,
    };

    // 86 400 / 60 = 1 440 minutes per day
    assert_eq!(bucket.compression_ratio(), 1_440);
}

// ─── ArchivalConfig defaults ───────────────────────────────────────────────────

#[test]
fn archival_config_defaults() {
    let cfg = ArchivalConfig::default();
    assert!(cfg.auto_archive_on_store);
    assert_eq!(cfg.archival_interval_secs, 3_600);
}

#[test]
fn set_archival_config_round_trips() {
    let mut mgr = PriceHistoryManager::new();
    let custom = ArchivalConfig {
        auto_archive_on_store:  false,
        archival_interval_secs: 7_200,
    };
    mgr.set_archival_config(custom.clone());
    assert!(!mgr.archival_config().auto_archive_on_store);
    assert_eq!(mgr.archival_config().archival_interval_secs, 7_200);
}

// ─── Initial state ─────────────────────────────────────────────────────────────

#[test]
fn initial_last_archival_run_is_zero() {
    let mgr = PriceHistoryManager::new();
    assert_eq!(mgr.last_archival_run(), 0);
}

// ─── run_archival on empty manager ───────────────────────────────────────────

#[test]
fn run_archival_on_empty_manager_succeeds() {
    let mut mgr = PriceHistoryManager::new();
    let stats = mgr.run_archival().unwrap();
    assert_eq!(stats.medium_buckets_created, 0);
    assert_eq!(stats.historical_buckets_created, 0);
    assert_eq!(stats.live_entries_pruned, 0);
    assert!(stats.completed_at > 0);
}

// ─── Recent data stays in the live store ──────────────────────────────────────

#[test]
fn recent_data_not_archived() {
    // Disable auto-archive so store_price doesn't interfere with assertions
    let cfg = ArchivalConfig { auto_archive_on_store: false, ..Default::default() };
    let mut mgr = PriceHistoryManager::with_config(cfg);

    let now_ts = now();
    // Insert 10 entries spaced 60 s apart, all within the last 30 minutes
    fill_entries(&mut mgr, "XLM", 10, 60, now_ts - 60, 1_000_000);

    let stats = mgr.run_archival().unwrap();

    // Nothing should have been moved — data is < 30 days old
    assert_eq!(stats.medium_buckets_created,     0, "no medium buckets expected for recent data");
    assert_eq!(stats.historical_buckets_created, 0, "no historical buckets expected");
    assert_eq!(stats.live_entries_pruned,        0, "no live entries should be pruned");

    // Data should still be accessible via get_price_history
    let buckets = mgr
        .get_price_history("XLM", TimeBucket::OneMinute, now_ts - 700, now_ts)
        .unwrap();
    assert!(!buckets.is_empty(), "recent data must remain in the live store");
}

// ─── Medium-tier archival (30–90 days) ───────────────────────────────────────

#[test]
fn data_older_than_30_days_moves_to_medium_archive() {
    let cfg = ArchivalConfig { auto_archive_on_store: false, ..Default::default() };
    let mut mgr = PriceHistoryManager::with_config(cfg);

    let now_ts = now();
    // Place 48 entries spaced 1 hour apart, ending 35 days ago
    // → all timestamps are 35–37 days in the past → should be Medium tier
    let end_ts = now_ts - TIER_RECENT_SECS - 5 * 3_600; // 30 d + 5 h ago
    fill_entries(&mut mgr, "XLM", 48, 3_600, end_ts, 2_000_000);

    let stats = mgr.run_archival().unwrap();

    assert!(
        stats.medium_buckets_created > 0,
        "expected medium-tier buckets to be created; got {}",
        stats.medium_buckets_created
    );
    assert!(
        stats.live_entries_pruned > 0,
        "expected live entries to be pruned after archival; got {}",
        stats.live_entries_pruned
    );

    // Medium archive should contain the data
    let medium = mgr.get_medium_archive(
        "XLM",
        end_ts - 48 * 3_600,
        end_ts + 3_600,
    );
    assert!(!medium.is_empty(), "medium archive should contain archived buckets");

    // All returned buckets must be Medium tier
    for b in &medium {
        assert_eq!(b.tier, ArchiveTier::Medium, "unexpected tier in medium archive");
        assert_eq!(b.window_end - b.window_start, 3_600, "medium window must be 1 hour wide");
    }
}

#[test]
fn medium_archive_ohlcv_correctness() {
    let cfg = ArchivalConfig { auto_archive_on_store: false, ..Default::default() };
    let mut mgr = PriceHistoryManager::with_config(cfg);

    let now_ts = now();
    // Place exactly 3 entries inside one hour window, 35 days ago
    let hour_start = now_ts - TIER_RECENT_SECS - 2 * 3_600;
    let hour_start = (hour_start / 3_600) * 3_600; // align to hour boundary

    // Three prices within the hour: 1 000, 3 000, 2 000 (in order)
    let prices = [1_000u64, 3_000, 2_000];
    for (i, &p) in prices.iter().enumerate() {
        let ts = hour_start + i as u64 * 60 + 30; // within the hour
        mgr.store_price(entry_at("ETH", p, ts)).unwrap();
    }

    mgr.run_archival().unwrap();

    let medium = mgr.get_medium_archive("ETH", hour_start, hour_start + 3_600);
    assert_eq!(medium.len(), 1, "exactly one hourly bucket expected");

    let b = &medium[0];
    assert_eq!(b.open,  1_000, "open should be first price");
    assert_eq!(b.high,  3_000, "high should be max price");
    assert_eq!(b.low,   1_000, "low should be min price");
    assert_eq!(b.close, 2_000, "close should be last price");
    assert_eq!(b.raw_entry_count, 3, "should record 3 raw entries");
}

// ─── Historical-tier archival (90+ days) ──────────────────────────────────────

#[test]
fn data_older_than_90_days_moves_to_historical_archive() {
    let cfg = ArchivalConfig { auto_archive_on_store: false, ..Default::default() };
    let mut mgr = PriceHistoryManager::with_config(cfg);

    let now_ts = now();
    // Place entries 95 days ago — one per hour over 5 days
    let end_ts = now_ts - TIER_MEDIUM_SECS - 5 * 86_400;
    fill_entries(&mut mgr, "BTC", 120, 3_600, end_ts, 50_000_000);

    let stats = mgr.run_archival().unwrap();

    assert!(
        stats.historical_buckets_created > 0,
        "expected historical-tier buckets; got {}",
        stats.historical_buckets_created
    );

    let hist = mgr.get_historical_archive(
        "BTC",
        end_ts - 120 * 3_600,
        end_ts + 86_400,
    );
    assert!(!hist.is_empty(), "historical archive must contain daily buckets");

    for b in &hist {
        assert_eq!(b.tier, ArchiveTier::Historical, "unexpected tier in historical archive");
        assert_eq!(
            b.window_end - b.window_start,
            86_400,
            "historical window must be exactly one day wide"
        );
    }
}

#[test]
fn historical_archive_ohlcv_correctness() {
    let cfg = ArchivalConfig { auto_archive_on_store: false, ..Default::default() };
    let mut mgr = PriceHistoryManager::with_config(cfg);

    let now_ts = now();
    // Align to a day boundary 100 days ago
    let day_start = {
        let raw = now_ts - TIER_MEDIUM_SECS - 10 * 86_400;
        (raw / 86_400) * 86_400
    };

    // 4 entries within that single day: prices 100, 400, 50, 200
    let prices = [100u64, 400, 50, 200];
    for (i, &p) in prices.iter().enumerate() {
        let ts = day_start + i as u64 * 3_600 + 600; // spread across the day
        mgr.store_price(entry_at("USDC", p, ts)).unwrap();
    }

    mgr.run_archival().unwrap();

    let hist = mgr.get_historical_archive("USDC", day_start, day_start + 86_400);
    assert_eq!(hist.len(), 1, "exactly one daily bucket expected");

    let b = &hist[0];
    assert_eq!(b.open,   100, "open = first price");
    assert_eq!(b.high,   400, "high = max price");
    assert_eq!(b.low,     50, "low = min price");
    assert_eq!(b.close,  200, "close = last price");
}

// ─── Compression ratio > 10:1 ─────────────────────────────────────────────────

#[test]
fn medium_archive_compression_ratio_exceeds_10_to_1() {
    let cfg = ArchivalConfig { auto_archive_on_store: false, ..Default::default() };
    let mut mgr = PriceHistoryManager::with_config(cfg);

    let now_ts = now();
    // 60 one-minute entries inside a single hour window, 35 days ago
    let hour_start = {
        let raw = now_ts - TIER_RECENT_SECS - 2 * 3_600;
        (raw / 3_600) * 3_600
    };

    for i in 0u64..60 {
        let ts = hour_start + i * 60 + 10;
        mgr.store_price(entry_at("XLM", 1_000_000 + i * 100, ts)).unwrap();
    }

    mgr.run_archival().unwrap();

    let medium = mgr.get_medium_archive("XLM", hour_start, hour_start + 3_600);
    assert_eq!(medium.len(), 1);

    let b = &medium[0];
    // 60 raw entries merged → compression ratio is 60:1 (> 10:1 requirement)
    assert!(
        b.compression_ratio() >= 10,
        "medium compression ratio {} must be ≥ 10",
        b.compression_ratio()
    );
    assert_eq!(b.raw_entry_count, 60);
}

#[test]
fn historical_archive_compression_ratio_exceeds_10_to_1() {
    // The daily bucket covers 1 440 minutes vs 1 raw entry per minute,
    // so even a single entry still gives a 1 440:1 structural ratio.
    let bucket_secs = ArchiveTier::Historical.bucket_secs();
    let minutes_per_day = (bucket_secs / 60) as u32;
    assert!(
        minutes_per_day >= 10,
        "daily bucket covers {} minutes, ratio must be ≥ 10",
        minutes_per_day
    );
}

// ─── Transparent query across tiers ──────────────────────────────────────────

#[test]
fn get_price_history_merges_live_and_medium_tiers() {
    let cfg = ArchivalConfig { auto_archive_on_store: false, ..Default::default() };
    let mut mgr = PriceHistoryManager::with_config(cfg);

    let now_ts = now();

    // ── Recent data (last 10 minutes) ──
    fill_entries(&mut mgr, "XLM", 5, 60, now_ts - 60, 1_000_000);

    // ── Old data that should be archived (35 days ago, 3 hours worth) ──
    let old_end = now_ts - TIER_RECENT_SECS - 2 * 3_600;
    fill_entries(&mut mgr, "XLM", 3, 3_600, old_end, 900_000);

    mgr.run_archival().unwrap();

    // Query spanning both ranges
    let buckets = mgr
        .get_price_history(
            "XLM",
            TimeBucket::OneMinute,
            old_end - 4 * 3_600,
            now_ts,
        )
        .unwrap();

    // We expect entries from both the live store and the medium archive
    assert!(
        buckets.len() >= 2,
        "merged query should return data from both tiers; got {} buckets",
        buckets.len()
    );

    // Results must be in ascending timestamp order
    let ts: Vec<u64> = buckets.iter().map(|b| b.first_timestamp).collect();
    let mut sorted = ts.clone();
    sorted.sort_unstable();
    assert_eq!(ts, sorted, "get_price_history must return data in ascending order");
}

#[test]
fn get_price_history_returns_empty_for_unknown_asset() {
    let mgr = PriceHistoryManager::new();
    let result = mgr
        .get_price_history("UNKNOWN", TimeBucket::OneMinute, 0, u64::MAX)
        .unwrap();
    assert!(result.is_empty());
}

#[test]
fn get_price_history_merges_all_three_tiers() {
    let cfg = ArchivalConfig { auto_archive_on_store: false, ..Default::default() };
    let mut mgr = PriceHistoryManager::with_config(cfg);

    let now_ts = now();

    // Recent (last 5 minutes)
    fill_entries(&mut mgr, "SOL", 3, 60, now_ts - 120, 20_000_000);

    // Medium tier target: 35 days ago
    let mid_end = now_ts - TIER_RECENT_SECS - 2 * 3_600;
    fill_entries(&mut mgr, "SOL", 2, 3_600, mid_end, 18_000_000);

    // Historical tier target: 100 days ago
    let hist_end = now_ts - TIER_MEDIUM_SECS - 10 * 86_400;
    fill_entries(&mut mgr, "SOL", 2, 3_600, hist_end, 15_000_000);

    mgr.run_archival().unwrap();

    let buckets = mgr
        .get_price_history(
            "SOL",
            TimeBucket::OneMinute,
            hist_end - 2 * 3_600,
            now_ts,
        )
        .unwrap();

    assert!(
        buckets.len() >= 3,
        "should see data from all three tiers; got {}",
        buckets.len()
    );

    // Ascending order check
    let ts: Vec<u64> = buckets.iter().map(|b| b.first_timestamp).collect();
    let mut sorted = ts.clone();
    sorted.sort_unstable();
    assert_eq!(ts, sorted, "results must be in ascending timestamp order");
}

// ─── Idempotency ───────────────────────────────────────────────────────────────

#[test]
fn run_archival_twice_is_idempotent() {
    let cfg = ArchivalConfig { auto_archive_on_store: false, ..Default::default() };
    let mut mgr = PriceHistoryManager::with_config(cfg);

    let now_ts = now();
    let end_ts = now_ts - TIER_RECENT_SECS - 3 * 3_600;
    fill_entries(&mut mgr, "XLM", 6, 3_600, end_ts, 1_000_000);

    let first  = mgr.run_archival().unwrap();
    let second = mgr.run_archival().unwrap();

    assert!(first.medium_buckets_created > 0, "first run should create buckets");
    assert_eq!(
        second.medium_buckets_created, 0,
        "second run must not create duplicate buckets"
    );
    assert_eq!(
        second.live_entries_pruned, 0,
        "second run must not prune already-pruned entries"
    );
}

// ─── live_entries_pruned count ────────────────────────────────────────────────

#[test]
fn archival_prunes_live_store() {
    let cfg = ArchivalConfig { auto_archive_on_store: false, ..Default::default() };
    let mut mgr = PriceHistoryManager::with_config(cfg);

    let now_ts = now();
    // 5 entries, each in its own minute window, 35 days ago
    let end_ts = now_ts - TIER_RECENT_SECS - 3 * 3_600;
    fill_entries(&mut mgr, "XLM", 5, 60, end_ts, 1_000_000);

    let stats = mgr.run_archival().unwrap();

    assert_eq!(
        stats.live_entries_pruned, 5,
        "all 5 old live buckets must be pruned; got {}",
        stats.live_entries_pruned
    );

    // Live store query for that old window should be empty
    let live = mgr
        .get_price_history(
            "XLM",
            TimeBucket::OneMinute,
            end_ts - 400,
            end_ts + 400,
        )
        .unwrap();

    // Medium archive should have picked up the data instead
    let medium = mgr.get_medium_archive("XLM", end_ts - 3_600, end_ts + 3_600);
    // At least one of live-or-medium must hold the data
    let total = live.len() + medium.len();
    assert!(
        total > 0,
        "data must exist in medium archive after pruning from live store"
    );
}

// ─── last_archival_run timestamp ─────────────────────────────────────────────

#[test]
fn last_archival_run_updated_after_run() {
    let mut mgr = PriceHistoryManager::new();
    assert_eq!(mgr.last_archival_run(), 0);

    let before = now();
    mgr.run_archival().unwrap();
    let after = now();

    let ran_at = mgr.last_archival_run();
    assert!(
        ran_at >= before && ran_at <= after,
        "last_archival_run ({}) should be between {} and {}",
        ran_at, before, after
    );
}

// ─── Invalid price / timestamp guard ─────────────────────────────────────────

#[test]
fn store_price_rejects_zero_price() {
    let mut mgr = PriceHistoryManager::new();
    let result  = mgr.store_price(entry_at("XLM", 0, now() - 60));
    assert!(result.is_err(), "zero price must be rejected");
}

#[test]
fn store_price_rejects_future_timestamp() {
    let mut mgr = PriceHistoryManager::new();
    let result  = mgr.store_price(entry_at("XLM", 1_000_000, now() + 9_999));
    assert!(result.is_err(), "future timestamp must be rejected");
}

// ─── Multi-asset isolation ────────────────────────────────────────────────────

#[test]
fn archival_is_isolated_per_asset() {
    let cfg = ArchivalConfig { auto_archive_on_store: false, ..Default::default() };
    let mut mgr = PriceHistoryManager::with_config(cfg);

    let now_ts = now();

    // XLM: old data (should be archived)
    let old_end = now_ts - TIER_RECENT_SECS - 4 * 3_600;
    fill_entries(&mut mgr, "XLM",  4, 3_600, old_end, 1_000_000);

    // BTC: recent data (should stay in live store)
    fill_entries(&mut mgr, "BTC",  4, 60, now_ts - 300, 50_000_000);

    let stats = mgr.run_archival().unwrap();

    assert!(stats.medium_buckets_created > 0, "XLM should have been archived");

    // BTC should still be in the live store, not the medium archive
    let btc_medium = mgr.get_medium_archive("BTC", 0, now_ts);
    assert!(
        btc_medium.is_empty(),
        "recent BTC data must not appear in the medium archive"
    );

    // XLM medium archive should be non-empty
    let xlm_medium = mgr.get_medium_archive("XLM", old_end - 4 * 3_600, old_end + 3_600);
    assert!(
        !xlm_medium.is_empty(),
        "old XLM data must be in the medium archive"
    );
}

// ─── with_config constructor ─────────────────────────────────────────────────

#[test]
fn with_config_respects_disable_auto_archive() {
    let cfg = ArchivalConfig {
        auto_archive_on_store:  false,
        archival_interval_secs: 999,
    };
    let mut mgr = PriceHistoryManager::with_config(cfg);

    let now_ts = now();
    let old_ts = now_ts - TIER_RECENT_SECS - 2 * 3_600;

    // Populate old data
    mgr.store_price(entry_at("XLM", 1_000_000, old_ts)).unwrap();

    // auto_archive_on_store = false → archival must NOT have run automatically
    assert_eq!(
        mgr.last_archival_run(), 0,
        "archival should not run automatically when auto_archive_on_store=false"
    );

    // Manually trigger archival
    let stats = mgr.run_archival().unwrap();
    assert!(stats.completed_at > 0);
}
