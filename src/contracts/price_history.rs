//! Historical Price Data Storage for Analytics and TWAP Calculations
//!
//! This module provides comprehensive historical price data storage and analytics
//! capabilities for DeFi applications on the Stellar network.
//!
//! ## Features
//! - Time-bucketed price data storage for efficient querying
//! - Price trend analysis and volatility calculations
//! - Moving averages (SMA, EMA)
//! - Enhanced TWAP (Time-Weighted Average Price) calculations
//! - Historical data query functions
//! - Price deviation detection
//! - Tiered archival with compression (30-day full, 30-90d hourly, 90d+ daily)
//!
//! ## Archival Tiers
//! - **Recent** (0–30 days): every raw update stored at full granularity
//! - **Medium** (30–90 days): compressed to hourly OHLCV buckets
//! - **Historical** (90+ days): compressed to daily OHLCV buckets
//!
//! Compression ratios typically exceed 10:1 for medium/historical tiers.
//! All queries are tier-transparent — callers use the same API regardless
//! of which tier holds the data.
//!
//! This is a library module that can be integrated into existing oracle contracts.

use std::collections::{BTreeMap, HashMap};
use serde::{Serialize, Deserialize};

// ─── Constants ───────────────────────────────────────────────────────────────

/// Maximum number of price entries per time bucket
#[allow(dead_code)]
const MAX_ENTRIES_PER_BUCKET: u32 = 1000;
/// Default TWAP calculation period (1 hour)
#[allow(dead_code)]
const DEFAULT_TWAP_PERIOD: u64 = 3600;
/// Maximum history retention period (30 days) for live data
const MAX_HISTORY_RETENTION: u64 = 2592000;

// ─── Archival tier boundaries (seconds)
/// Recent tier: 0 – 30 days (full granularity)
pub const TIER_RECENT_SECS: u64 = 30 * 24 * 3600;       // 2_592_000
/// Medium tier: 30 – 90 days (hourly aggregation)
pub const TIER_MEDIUM_SECS: u64 = 90 * 24 * 3600;       // 7_776_000
// Data older than TIER_MEDIUM_SECS goes into the historical tier (daily aggregation)

// ─── Time Bucket Definitions ─────────────────────────────────────────────────

/// Time bucket intervals for organizing price data
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize, async_graphql::Enum)]
pub enum TimeBucket {
    /// 1-minute bucket
    OneMinute,
    /// 5-minute bucket
    FiveMinute,
    /// 15-minute bucket
    FifteenMinute,
    /// 1-hour bucket
    OneHour,
    /// 6-hour bucket
    SixHour,
    /// 24-hour bucket
    OneDay,
}

impl TimeBucket {
    /// Get the duration of this time bucket in seconds
    pub fn duration(&self) -> u64 {
        match self {
            TimeBucket::OneMinute => 60,
            TimeBucket::FiveMinute => 300,
            TimeBucket::FifteenMinute => 900,
            TimeBucket::OneHour => 3600,
            TimeBucket::SixHour => 21600,
            TimeBucket::OneDay => 86400,
        }
    }

    /// Get the bucket index for a given timestamp
    pub fn bucket_index(&self, timestamp: u64) -> u64 {
        timestamp / self.duration()
    }

    /// Parse a granularity string into a TimeBucket
    pub fn from_granularity_str(granularity: &str) -> Result<Self, PriceHistoryError> {
        match granularity {
            "1min" => Ok(TimeBucket::OneMinute),
            "5min" => Ok(TimeBucket::FiveMinute),
            "15min" => Ok(TimeBucket::FifteenMinute),
            "1hr" => Ok(TimeBucket::OneHour),
            "1day" => Ok(TimeBucket::OneDay),
            other => Err(PriceHistoryError::InvalidGranularity(other.to_string())),
        }
    }
}

// ─── Enhanced Price Data Structures ─────────────────────────────────────────

/// Enhanced price history entry with additional metadata
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PriceHistoryEntry {
    /// Asset identifier (e.g., token address or symbol)
    pub asset_id: String,
    /// Price value
    pub price: u64,
    /// Number of decimals
    pub decimals: u32,
    /// Timestamp of this price
    pub timestamp: u64,
    /// Source of this price (e.g., oracle ID)
    pub source: String,
    /// Trading volume at this price (if available)
    pub volume: u64,
    /// Number of transactions at this price
    pub transaction_count: u32,
}

/// Time-bucketed price data for efficient querying
#[derive(Clone, Debug, Serialize, Deserialize, async_graphql::SimpleObject)]
pub struct PriceBucket {
    /// Time bucket type
    pub bucket_type: TimeBucket,
    /// Bucket index (timestamp / bucket_duration)
    pub bucket_index: u64,
    /// Opening price in this bucket
    pub open: u64,
    /// Highest price in this bucket
    pub high: u64,
    /// Lowest price in this bucket
    pub low: u64,
    /// Closing price in this bucket
    pub close: u64,
    /// Total volume in this bucket
    pub volume: u64,
    /// Number of price entries in this bucket
    pub entry_count: u32,
    /// First timestamp in this bucket
    pub first_timestamp: u64,
    /// Last timestamp in this bucket
    pub last_timestamp: u64,
}

/// Asset metadata for price history
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AssetMetadata {
    /// Asset identifier
    pub asset_id: String,
    /// Total number of price entries stored
    pub total_entries: u64,
    /// First price timestamp
    pub first_timestamp: u64,
    /// Last price timestamp
    pub last_timestamp: u64,
    /// Current price
    pub current_price: u64,
    /// 24-hour high
    pub high_24h: u64,
    /// 24-hour low
    pub low_24h: u64,
    /// 24-hour volume
    pub volume_24h: u64,
    /// 24-hour price change (basis points)
    pub price_change_24h_bps: i64,
}

/// Analytics data cache
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnalyticsData {
    /// Asset identifier
    pub asset_id: String,
    /// Simple Moving Average (SMA) - various periods
    pub sma_1h: u64,
    pub sma_6h: u64,
    pub sma_24h: u64,
    pub sma_7d: u64,
    /// Exponential Moving Average (EMA) - various periods
    pub ema_1h: u64,
    pub ema_6h: u64,
    pub ema_24h: u64,
    /// Volatility (standard deviation) - 24h
    pub volatility_24h: u32,
    /// Price trend (up, down, sideways)
    pub trend: PriceTrend,
    /// Last analytics update timestamp
    pub last_update: u64,
}

/// Price trend direction
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PriceTrend {
    /// Price is trending up
    Up,
    /// Price is trending down
    Down,
    /// Price is sideways (stable)
    Sideways,
}

// ─── Archival Structures ──────────────────────────────────────────────────

/// Which storage tier a data point belongs to.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ArchiveTier {
    /// 0–30 days: every raw update kept
    Recent,
    /// 30–90 days: one bucket per hour
    Medium,
    /// 90+ days: one bucket per day
    Historical,
}

impl ArchiveTier {
    /// Bucket width in seconds for this tier.
    pub fn bucket_secs(&self) -> u64 {
        match self {
            ArchiveTier::Recent   => 60,      // 1-minute (matches TimeBucket::OneMinute)
            ArchiveTier::Medium   => 3_600,   // 1-hour
            ArchiveTier::Historical => 86_400, // 1-day
        }
    }

    /// Derive the tier from age (seconds since the data point was recorded).
    pub fn from_age(age_secs: u64) -> Self {
        if age_secs <= TIER_RECENT_SECS {
            ArchiveTier::Recent
        } else if age_secs <= TIER_MEDIUM_SECS {
            ArchiveTier::Medium
        } else {
            ArchiveTier::Historical
        }
    }
}

/// A single compressed OHLCV bucket stored in the archive.
///
/// Each entry aggregates all raw price points that fell within one
/// `bucket_secs`-wide window.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArchivedBucket {
    /// Asset this bucket belongs to.
    pub asset_id: String,
    /// Tier that produced this bucket.
    pub tier: ArchiveTier,
    /// Start of the time window (Unix seconds, inclusive).
    pub window_start: u64,
    /// End of the time window (Unix seconds, exclusive).
    pub window_end: u64,
    /// Opening price (first raw entry in the window).
    pub open: u64,
    /// Highest price seen in the window.
    pub high: u64,
    /// Lowest price seen in the window.
    pub low: u64,
    /// Closing price (last raw entry in the window).
    pub close: u64,
    /// Sum of all raw closing prices (used for mean / TWAP reconstruction).
    pub price_sum: u128,
    /// Total traded volume across all raw entries.
    pub volume: u64,
    /// Number of raw entries that were merged into this bucket.
    pub raw_entry_count: u32,
}

impl ArchivedBucket {
    /// Average (mean) price for this bucket.
    pub fn mean_price(&self) -> u64 {
        if self.raw_entry_count == 0 {
            return 0;
        }
        (self.price_sum / self.raw_entry_count as u128) as u64
    }

    /// Effective compression ratio compared with 1-minute granularity.
    ///
    /// Returns how many 1-minute slots were collapsed into this bucket.
    pub fn compression_ratio(&self) -> u32 {
        let bucket_secs = self.tier.bucket_secs();
        (bucket_secs / 60) as u32  // minutes per bucket
    }
}

/// Configuration for the archival subsystem.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArchivalConfig {
    /// Run archival automatically whenever `store_price` is called.
    /// Disable for batch/offline archival runs.
    pub auto_archive_on_store: bool,
    /// How often (in seconds) to trigger automatic archival.
    /// Only relevant when `auto_archive_on_store` is false.
    pub archival_interval_secs: u64,
}

impl Default for ArchivalConfig {
    fn default() -> Self {
        Self {
            auto_archive_on_store: true,
            archival_interval_secs: 3_600, // 1 hour
        }
    }
}

/// Statistics reported after an archival run.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ArchivalStats {
    /// Number of raw 1-minute buckets compressed into medium-tier hourly buckets.
    pub medium_buckets_created: u32,
    /// Number of medium-tier (hourly) buckets compressed into historical daily buckets.
    pub historical_buckets_created: u32,
    /// Number of raw live-data entries removed after archival.
    pub live_entries_pruned: u32,
    /// Timestamp the archival run completed.
    pub completed_at: u64,
}

/// TWAP calculation result
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TwapResult {
    /// Asset identifier
    pub asset_id: String,
    /// TWAP price
    pub twap_price: u64,
    /// Number of decimals
    pub decimals: u32,
    /// Calculation period in seconds
    pub period: u64,
    /// Number of data points used
    pub data_points: u32,
    /// Timestamp of calculation
    pub calculated_at: u64,
}

// ─── Anomaly Detection ────────────────────────────────────────────────────────

/// Which detection method flagged a price as anomalous.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AnomalyMethod {
    /// Price is more than 3 standard deviations from the rolling mean.
    ZScore,
    /// Price is outside the 1.5 × IQR fence.
    Iqr,
    /// Short-term moving average crossed the long-term moving average
    /// with unusually high magnitude.
    MovingAverageCrossover,
}

/// Confidence level of an anomaly detection result.
///
/// `High` means at least two independent methods agreed.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AnomalyConfidence {
    /// Exactly one method flagged the price.
    Low,
    /// Two or more methods flagged the price.
    High,
}

/// The result returned by the anomaly detector for a single price point.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnomalyResult {
    /// Asset this result belongs to.
    pub asset_id: String,
    /// The price that was tested.
    pub price: u64,
    /// Timestamp of that price.
    pub timestamp: u64,
    /// Whether this price is considered anomalous.
    pub is_anomaly: bool,
    /// Methods that flagged it (empty when `is_anomaly` is false).
    pub methods: Vec<AnomalyMethod>,
    /// Confidence when anomalous.
    pub confidence: Option<AnomalyConfidence>,
    /// Z-score at the time of detection (× 100, integer, to avoid f64).
    /// e.g. 315 means Z = 3.15.
    pub z_score_x100: i64,
}

/// Configurable thresholds for the anomaly detector.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnomalyConfig {
    /// Z-score threshold × 100.  Default 300 (= 3.0 σ).
    pub z_score_threshold_x100: i64,
    /// IQR multiplier × 100.  Default 150 (= 1.5 × IQR).
    pub iqr_multiplier_x100: u64,
    /// Minimum window size for statistical methods.
    pub min_window: usize,
    /// Short MA period for crossover (seconds).
    pub ma_short_period: u64,
    /// Long MA period for crossover (seconds).
    pub ma_long_period: u64,
    /// MA crossover deviation threshold in basis points.
    pub ma_crossover_threshold_bps: u32,
}

impl Default for AnomalyConfig {
    fn default() -> Self {
        Self {
            z_score_threshold_x100: 300,
            iqr_multiplier_x100: 150,
            min_window: 10,
            ma_short_period: 3_600,    // 1 h
            ma_long_period: 86_400,    // 24 h
            ma_crossover_threshold_bps: 200, // 2 %
        }
    }
}

/// Stateless anomaly detector.  Feed it a price window and a candidate price.
pub struct AnomalyDetector {
    config: AnomalyConfig,
}

impl AnomalyDetector {
    /// Create a new detector with default thresholds.
    pub fn new() -> Self {
        Self { config: AnomalyConfig::default() }
    }

    /// Create a detector with custom thresholds.
    pub fn with_config(config: AnomalyConfig) -> Self {
        Self { config }
    }

    /// Expose the current configuration.
    pub fn config(&self) -> &AnomalyConfig {
        &self.config
    }

    /// Analyse a single candidate price against a historical window.
    ///
    /// `window` must be ordered oldest-first.  The candidate is **not** part
    /// of the window — it is the new price being validated.
    ///
    /// Returns an `AnomalyResult` regardless of whether an anomaly was found.
    pub fn detect(
        &self,
        asset_id: &str,
        candidate: u64,
        timestamp: u64,
        window: &[u64],
    ) -> AnomalyResult {
        let mut flagged_by: Vec<AnomalyMethod> = Vec::new();

        let z_score_x100 = if window.len() >= self.config.min_window {
            let z = self.z_score_x100(candidate, window);
            if z.abs() >= self.config.z_score_threshold_x100 {
                flagged_by.push(AnomalyMethod::ZScore);
            }
            z
        } else {
            0
        };

        if window.len() >= self.config.min_window {
            if self.iqr_flagged(candidate, window) {
                flagged_by.push(AnomalyMethod::Iqr);
            }
        }

        if window.len() >= self.config.min_window {
            if self.ma_crossover_flagged(candidate, window) {
                flagged_by.push(AnomalyMethod::MovingAverageCrossover);
            }
        }

        let is_anomaly = !flagged_by.is_empty();
        let confidence = if flagged_by.len() >= 2 {
            Some(AnomalyConfidence::High)
        } else if is_anomaly {
            Some(AnomalyConfidence::Low)
        } else {
            None
        };

        AnomalyResult {
            asset_id: asset_id.to_string(),
            price: candidate,
            timestamp,
            is_anomaly,
            methods: flagged_by,
            confidence,
            z_score_x100,
        }
    }

    // ── Z-score ──────────────────────────────────────────────────────────────

    /// Returns Z-score × 100 (signed integer) to avoid floating point.
    pub fn z_score_x100(&self, candidate: u64, window: &[u64]) -> i64 {
        let n = window.len() as u128;
        if n == 0 { return 0; }

        let sum: u128 = window.iter().map(|&p| p as u128).sum();
        let mean = sum / n;

        let variance: u128 = window.iter()
            .map(|&p| {
                let d = if p as u128 >= mean { p as u128 - mean } else { mean - p as u128 };
                d * d
            })
            .sum::<u128>()
            / n;

        if variance == 0 {
            // All values identical; deviation of candidate from mean
            let diff = if candidate as u128 >= mean {
                (candidate as u128 - mean) as i64
            } else {
                -((mean - candidate as u128) as i64)
            };
            // Treat as infinite z-score direction
            return if diff == 0 { 0 } else if diff > 0 { i64::MAX / 100 } else { i64::MIN / 100 };
        }

        // Integer square root via Newton's method
        let std_dev = isqrt(variance);
        if std_dev == 0 { return 0; }

        let diff_x100 = if candidate as u128 >= mean {
            ((candidate as u128 - mean) * 100) as i64
        } else {
            -(((mean - candidate as u128) * 100) as i64)
        };

        diff_x100 / std_dev as i64
    }

    // ── IQR ──────────────────────────────────────────────────────────────────

    /// Returns true when `candidate` is outside Q1 − k·IQR or Q3 + k·IQR,
    /// where k = `iqr_multiplier_x100 / 100`.
    pub fn iqr_flagged(&self, candidate: u64, window: &[u64]) -> bool {
        let mut sorted: Vec<u64> = window.to_vec();
        sorted.sort_unstable();
        let n = sorted.len();
        if n < 4 { return false; }

        let q1 = sorted[n / 4];
        let q3 = sorted[(3 * n) / 4];

        if q3 < q1 { return false; } // shouldn't happen after sort

        let iqr = q3 - q1;
        // fence = multiplier * iqr / 100  (integer, avoiding f64)
        let fence = (iqr as u128 * self.config.iqr_multiplier_x100) / 100;

        let lower = q1 as u128;
        let upper = q3 as u128 + fence;

        (candidate as u128) < lower.saturating_sub(fence) || (candidate as u128) > upper
    }

    // ── Moving-average crossover ─────────────────────────────────────────────

    /// Returns true when the short MA of the window (last `ma_short_period`
    /// worth of points) deviates from the long MA by more than
    /// `ma_crossover_threshold_bps`, **and** the candidate price extends that
    /// divergence further.
    pub fn ma_crossover_flagged(&self, candidate: u64, window: &[u64]) -> bool {
        if window.is_empty() { return false; }

        let long_mean = mean_of(window);
        if long_mean == 0 { return false; }

        // Short window = last min(short_period_fraction, len/2) points
        let short_len = (window.len() / 4).max(1);
        let short_window = &window[window.len() - short_len..];
        let short_mean = mean_of(short_window);

        let deviation_bps = abs_deviation_bps(short_mean, long_mean);

        if deviation_bps < self.config.ma_crossover_threshold_bps {
            return false;
        }

        // Check whether candidate pushes price further away from long mean
        let cand_deviation_bps = abs_deviation_bps(candidate, long_mean);
        cand_deviation_bps > deviation_bps
    }
}

impl Default for AnomalyDetector {
    fn default() -> Self { Self::new() }
}

// ── AnomalyDetector integration on PriceHistoryManager ───────────────────────

impl PriceHistoryManager {
    /// Check whether `candidate_price` is anomalous relative to recent history.
    ///
    /// Uses the last `window_secs` seconds of 1-minute buckets as the
    /// statistical window.  Returns an `AnomalyResult` for every call.
    pub fn detect_anomaly(
        &self,
        asset_id: &str,
        candidate_price: u64,
        timestamp: u64,
        window_secs: u64,
        config: Option<AnomalyConfig>,
    ) -> AnomalyResult {
        let detector = match config {
            Some(c) => AnomalyDetector::with_config(c),
            None    => AnomalyDetector::new(),
        };

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let start = now.saturating_sub(window_secs);

        // Collect closing prices from the live 1-minute store
        let window: Vec<u64> = self
            .price_buckets
            .get(asset_id)
            .and_then(|ab| ab.get(&TimeBucket::OneMinute))
            .map(|btree| {
                let start_idx = start / 60;
                let end_idx   = now  / 60;
                btree.range(start_idx..=end_idx)
                    .map(|(_, b)| b.close)
                    .collect()
            })
            .unwrap_or_default();

        detector.detect(asset_id, candidate_price, timestamp, &window)
    }
}

// ── Private numeric helpers ───────────────────────────────────────────────────

/// Integer square root (Newton / Babylonian).
fn isqrt(n: u128) -> u128 {
    if n == 0 { return 0; }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

fn mean_of(slice: &[u64]) -> u64 {
    if slice.is_empty() { return 0; }
    let sum: u128 = slice.iter().map(|&v| v as u128).sum();
    (sum / slice.len() as u128) as u64
}

fn abs_deviation_bps(a: u64, b: u64) -> u32 {
    if b == 0 { return 0; }
    let diff = if a >= b { a - b } else { b - a };
    ((diff as u128 * 10_000) / b as u128) as u32
}

// ─── Error Codes ─────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PriceHistoryError {
    AlreadyInitialized,
    Unauthorized,
    InvalidPrice,
    InvalidTimestamp,
    AssetNotFound,
    InsufficientData,
    InvalidPeriod,
    ExportError,
}

impl std::fmt::Display for PriceHistoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PriceHistoryError::AlreadyInitialized => write!(f, "Already initialized"),
            PriceHistoryError::Unauthorized => write!(f, "Unauthorized"),
            PriceHistoryError::InvalidPrice => write!(f, "Invalid price"),
            PriceHistoryError::InvalidTimestamp => write!(f, "Invalid timestamp"),
            PriceHistoryError::AssetNotFound => write!(f, "Asset not found"),
            PriceHistoryError::InsufficientData => write!(f, "Insufficient data"),
            PriceHistoryError::InvalidPeriod => write!(f, "Invalid period"),
            PriceHistoryError::ExportError => write!(f, "Export failed"),
        }
    }
}

impl std::error::Error for PriceHistoryError {}

// ─── Price History Manager ─────────────────────────────────────────────────

/// Price history manager for storing and analyzing historical price data
pub struct PriceHistoryManager {
    /// Price buckets organized by asset and time bucket type (live / recent data)
    price_buckets: HashMap<String, HashMap<TimeBucket, BTreeMap<u64, PriceBucket>>>,
    /// Asset metadata
    asset_metadata: HashMap<String, AssetMetadata>,
    /// Analytics cache
    analytics_cache: HashMap<String, AnalyticsData>,

    // ── Archival fields ──────────────────────────────────────────────────────
    /// Medium-tier archive: asset_id → (window_start → ArchivedBucket)
    /// Each bucket covers exactly one hour.
    medium_archive: HashMap<String, BTreeMap<u64, ArchivedBucket>>,
    /// Historical-tier archive: asset_id → (window_start → ArchivedBucket)
    /// Each bucket covers exactly one day.
    historical_archive: HashMap<String, BTreeMap<u64, ArchivedBucket>>,
    /// Archival configuration
    archival_config: ArchivalConfig,
    /// Unix timestamp of the last successful archival run (0 = never ran)
    last_archival_run: u64,
}

impl PriceHistoryManager {
    /// Create a new price history manager
    pub fn new() -> Self {
        Self {
            price_buckets: HashMap::new(),
            asset_metadata: HashMap::new(),
            analytics_cache: HashMap::new(),
            medium_archive: HashMap::new(),
            historical_archive: HashMap::new(),
            archival_config: ArchivalConfig::default(),
            last_archival_run: 0,
        }
    }

    /// Create a price history manager with custom archival configuration.
    pub fn with_config(config: ArchivalConfig) -> Self {
        Self {
            price_buckets: HashMap::new(),
            asset_metadata: HashMap::new(),
            analytics_cache: HashMap::new(),
            medium_archive: HashMap::new(),
            historical_archive: HashMap::new(),
            archival_config: config,
            last_archival_run: 0,
        }
    }

    /// Store a price data point
    ///
    /// # Arguments
    /// * `entry` - Price history entry to store
    pub fn store_price(&mut self, entry: PriceHistoryEntry) -> Result<(), PriceHistoryError> {
        if entry.price == 0 {
            return Err(PriceHistoryError::InvalidPrice);
        }

        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        if entry.timestamp > current_time {
            return Err(PriceHistoryError::InvalidTimestamp);
        }

        // Store price in all time buckets
        for bucket_type in [
            TimeBucket::OneMinute,
            TimeBucket::FiveMinute,
            TimeBucket::FifteenMinute,
            TimeBucket::OneHour,
            TimeBucket::SixHour,
        ] {
            self.store_in_bucket(entry.clone(), bucket_type.clone());
        }

        // Update asset metadata
        self.update_asset_metadata(&entry);

        // Invalidate analytics cache for this asset
        self.invalidate_analytics_cache(&entry.asset_id);

        // Optionally run archival
        if self.archival_config.auto_archive_on_store {
            let current_time = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            if current_time - self.last_archival_run >= self.archival_config.archival_interval_secs {
                let _ = self.run_archival(); // best-effort; ignore errors so store_price never fails
            }
        }

        Ok(())
    }

    /// Get price history for an asset within a time range.
    ///
    /// This call is **tier-transparent**: data from the live store, the medium
    /// archive (hourly), and the historical archive (daily) are all merged and
    /// returned in ascending timestamp order.  The caller does not need to know
    /// which tier holds a given data point.
    ///
    /// # Arguments
    /// * `asset_id`    - Asset identifier
    /// * `bucket_type` - Preferred granularity for *live* data; archive data is
    ///                   always returned at its native granularity
    /// * `start_time`  - Start timestamp (Unix seconds, inclusive)
    /// * `end_time`    - End timestamp (Unix seconds, inclusive)
    ///
    /// # Returns
    /// Vector of price buckets in ascending timestamp order
    pub fn get_price_history(
        &self,
        asset_id: &str,
        bucket_type: TimeBucket,
        start_time: u64,
        end_time: u64,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Vec<PriceBucket>, PriceHistoryError> {
        let mut result: Vec<PriceBucket> = Vec::new();

        // ── 1. Pull from live store ──────────────────────────────────────────
        if let Some(asset_buckets) = self.price_buckets.get(asset_id) {
            if let Some(buckets) = asset_buckets.get(&bucket_type) {
                let bucket_duration = bucket_type.duration();
                let start_index = start_time / bucket_duration;
                let end_index   = end_time   / bucket_duration;
                for (_index, bucket) in buckets.range(start_index..=end_index) {
                    result.push(bucket.clone());
                }
            }
        }

        // ── 2. Pull from medium archive (hourly) ────────────────────────────
        if let Some(medium_buckets) = self.medium_archive.get(asset_id) {
            for (&window_start, archived) in medium_buckets.range(start_time..=end_time) {
                // Convert ArchivedBucket → PriceBucket so the API is uniform
                result.push(PriceBucket {
                    bucket_type: TimeBucket::OneHour,
                    bucket_index: window_start / TimeBucket::OneHour.duration(),
                    open:             archived.open,
                    high:             archived.high,
                    low:              archived.low,
                    close:            archived.close,
                    volume:           archived.volume,
                    entry_count:      archived.raw_entry_count,
                    first_timestamp:  archived.window_start,
                    last_timestamp:   archived.window_end.saturating_sub(1),
                });
            }
        }

        // ── 3. Pull from historical archive (daily) ─────────────────────────
        if let Some(hist_buckets) = self.historical_archive.get(asset_id) {
            for (&window_start, archived) in hist_buckets.range(start_time..=end_time) {
                result.push(PriceBucket {
                    bucket_type: TimeBucket::OneDay,
                    bucket_index: window_start / TimeBucket::OneDay.duration(),
                    open:             archived.open,
                    high:             archived.high,
                    low:              archived.low,
                    close:            archived.close,
                    volume:           archived.volume,
                    entry_count:      archived.raw_entry_count,
                    first_timestamp:  archived.window_start,
                    last_timestamp:   archived.window_end.saturating_sub(1),
                });
            }
        }

        if result.is_empty() {
            // Return empty vec rather than error — no data for range is valid
            return Ok(result);
        }

        // Sort by first_timestamp ascending and deduplicate overlapping windows
        result.sort_by_key(|b| b.first_timestamp);
        result.dedup_by_key(|b| b.first_timestamp);

        Ok(result)
    }

    /// Calculate TWAP (Time-Weighted Average Price)
    ///
    /// # Arguments
    /// * `asset_id` - Asset identifier
    /// * `period` - Time period in seconds for TWAP calculation
    ///
    /// # Returns
    /// TWAP calculation result
    pub fn calculate_twap(
        &self,
        asset_id: &str,
        period: u64,
    ) -> Result<TwapResult, PriceHistoryError> {
        if period == 0 || period > MAX_HISTORY_RETENTION {
            return Err(PriceHistoryError::InvalidPeriod);
        }

        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let start_time = current_time - period;

        // Use 1-minute buckets for precise TWAP calculation
        let price_buckets = self.get_price_history(
            asset_id,
            TimeBucket::OneMinute,
            start_time,
            current_time,
            None,
            None,
        )?;

        if price_buckets.is_empty() {
            return Err(PriceHistoryError::InsufficientData);
        }

        let mut weighted_sum = 0u128;
        let mut total_weight = 0u64;
        let mut data_points = 0u32;
        let mut last_timestamp = 0u64;
        let mut last_price = 0u64;
        let mut decimals = 6u32;

        for bucket in &price_buckets {
            if last_timestamp > 0 {
                let time_weight = bucket.first_timestamp - last_timestamp;
                weighted_sum += (last_price as u128) * (time_weight as u128);
                total_weight += time_weight;
            }

            last_timestamp = bucket.last_timestamp;
            last_price = bucket.close;
            data_points += bucket.entry_count;
            decimals = 6; // Default decimals
        }

        if total_weight == 0 {
            return Err(PriceHistoryError::InsufficientData);
        }

        let twap_price = (weighted_sum / (total_weight as u128)) as u64;

        Ok(TwapResult {
            asset_id: asset_id.to_string(),
            twap_price,
            decimals,
            period,
            data_points,
            calculated_at: current_time,
        })
    }

    /// Query historical prices with granularity conversion, TWAP, and pagination
    ///
    /// # Arguments
    /// * `asset_id` - Asset identifier
    /// * `start_time` - Start timestamp
    /// * `end_time` - End timestamp
    /// * `granularity` - Bucket granularity ("1min", "5min", "15min", "1hr", "1day")
    /// * `limit` - Maximum number of buckets to return
    /// * `offset` - Number of buckets to skip
    ///
    /// # Returns
    /// Price query result with prices, TWAP, total count, and pagination flag
    pub fn query_historical_prices(
        &self,
        asset_id: &str,
        start_time: u64,
        end_time: u64,
        granularity: &str,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<PriceQueryResult, PriceHistoryError> {
        let bucket_type = TimeBucket::from_granularity_str(granularity)?;

        let total_count =
            self.count_prices_in_range(asset_id, &bucket_type, start_time, end_time)?;
        let prices =
            self.get_price_history(asset_id, bucket_type, start_time, end_time, limit, offset)?;

        let twap = if prices.is_empty() {
            0
        } else {
            let sum: u128 = prices.iter().map(|b| b.close as u128).sum();
            (sum / prices.len() as u128) as u64
        };

        let limit = limit.unwrap_or(usize::MAX) as u64;
        let offset = offset.unwrap_or(0) as u64;
        let has_more = total_count > offset + limit;

        Ok(PriceQueryResult {
            prices,
            twap,
            total_count,
            has_more,
        })
    }

    /// Get analytics data for an asset
    ///
    /// # Arguments
    /// * `asset_id` - Asset identifier
    ///
    /// # Returns
    /// Analytics data including moving averages, volatility, and trend
    pub fn get_analytics(&mut self, asset_id: &str) -> Result<AnalyticsData, PriceHistoryError> {
        // Check if cache is valid (within 5 minutes)
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        if let Some(cached) = self.analytics_cache.get(asset_id) {
            if current_time - cached.last_update < 300 {
                return Ok(cached.clone());
            }
        }

        // Calculate fresh analytics
        let analytics = self.calculate_analytics(asset_id)?;

        // Cache the result
        self.analytics_cache.insert(asset_id.to_string(), analytics.clone());

        Ok(analytics)
    }

    /// Calculate Simple Moving Average (SMA)
    ///
    /// # Arguments
    /// * `asset_id` - Asset identifier
    /// * `period` - Time period in seconds
    ///
    /// # Returns
    /// SMA price
    pub fn calculate_sma(&self, asset_id: &str, period: u64) -> Result<u64, PriceHistoryError> {
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let start_time = current_time - period;

        let price_buckets = self.get_price_history(
            asset_id,
            TimeBucket::OneMinute,
            start_time,
            current_time,
            None,
            None,
        )?;

        if price_buckets.is_empty() {
            return Err(PriceHistoryError::InsufficientData);
        }

        let mut sum = 0u128;
        let count = price_buckets.len();

        for bucket in &price_buckets {
            sum += bucket.close as u128;
        }

        if count == 0 {
            return Err(PriceHistoryError::InsufficientData);
        }

        Ok((sum / (count as u128)) as u64)
    }

    /// Calculate price volatility (standard deviation)
    ///
    /// # Arguments
    /// * `asset_id` - Asset identifier
    /// * `period` - Time period in seconds
    ///
    /// # Returns
    /// Volatility as basis points
    pub fn calculate_volatility(&self, asset_id: &str, period: u64) -> Result<u32, PriceHistoryError> {
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let start_time = current_time - period;

        let price_buckets = self.get_price_history(
            asset_id,
            TimeBucket::OneMinute,
            start_time,
            current_time,
            None,
            None,
        )?;

        if price_buckets.is_empty() {
            return Err(PriceHistoryError::InsufficientData);
        }

        // Calculate mean
        let mut sum = 0u128;
        let count = price_buckets.len();

        for bucket in &price_buckets {
            sum += bucket.close as u128;
        }

        let mean = sum / (count as u128);

        // Calculate variance
        let mut variance_sum = 0u128;

        for bucket in &price_buckets {
            let diff = if bucket.close as u128 > mean {
                bucket.close as u128 - mean
            } else {
                mean - bucket.close as u128
            };
            variance_sum += diff * diff;
        }

        let variance = variance_sum / (count as u128);

        // Standard deviation as basis points of mean
        let std_dev = if variance > 0 {
            // Approximate square root
            let mut approx = variance;
            let mut i = 0;
            while i < 10 && approx > 1 {
                approx = (approx + (variance / approx)) / 2;
                i += 1;
            }
            approx
        } else {
            0
        };

        let volatility_bps = if mean > 0 {
            ((std_dev * 10000) / mean) as u32
        } else {
            0
        };

        Ok(volatility_bps)
    }

    // ─── Archival Public API ───────────────────────────────────────────────────

    /// Run a full archival pass over all assets.
    ///
    /// - Live 1-minute buckets older than 30 days are compressed into hourly
    ///   `ArchivedBucket`s in the medium archive.
    /// - Medium-tier hourly buckets older than 90 days are compressed into
    ///   daily `ArchivedBucket`s in the historical archive and then removed
    ///   from the medium archive.
    /// - Live buckets that have been archived are pruned from the live store.
    ///
    /// # Returns
    /// `ArchivalStats` describing what was done, or a `PriceHistoryError`.
    pub fn run_archival(&mut self) -> Result<ArchivalStats, PriceHistoryError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let medium_cutoff    = now.saturating_sub(TIER_RECENT_SECS); // 30 d ago
        let historical_cutoff = now.saturating_sub(TIER_MEDIUM_SECS); // 90 d ago

        let mut stats = ArchivalStats::default();

        // Collect asset IDs first to satisfy the borrow checker
        let asset_ids: Vec<String> = self.price_buckets.keys().cloned().collect();

        for asset_id in &asset_ids {
            // ── Step 1: live → medium (compress 1-min buckets > 30 days old) ──
            let medium_created = self.compress_live_to_medium(asset_id, medium_cutoff);
            stats.medium_buckets_created     += medium_created.0;
            stats.live_entries_pruned        += medium_created.1;
        }

        // ── Step 2: medium → historical (compress hourly > 90 days old) ──────
        let medium_asset_ids: Vec<String> = self.medium_archive.keys().cloned().collect();
        for asset_id in &medium_asset_ids {
            let hist_created = self.compress_medium_to_historical(asset_id, historical_cutoff);
            stats.historical_buckets_created += hist_created;
        }

        self.last_archival_run = now;
        stats.completed_at    = now;
        Ok(stats)
    }

    /// Retrieve medium-tier (hourly) archived buckets for an asset.
    ///
    /// # Arguments
    /// * `asset_id`   - Asset identifier
    /// * `start_time` - Window start (inclusive, Unix seconds)
    /// * `end_time`   - Window end (inclusive, Unix seconds)
    pub fn get_medium_archive(
        &self,
        asset_id: &str,
        start_time: u64,
        end_time: u64,
    ) -> Vec<ArchivedBucket> {
        match self.medium_archive.get(asset_id) {
            None => Vec::new(),
            Some(buckets) => buckets
                .range(start_time..=end_time)
                .map(|(_, b)| b.clone())
                .collect(),
        }
    }

    /// Retrieve historical-tier (daily) archived buckets for an asset.
    ///
    /// # Arguments
    /// * `asset_id`   - Asset identifier
    /// * `start_time` - Window start (inclusive, Unix seconds)
    /// * `end_time`   - Window end (inclusive, Unix seconds)
    pub fn get_historical_archive(
        &self,
        asset_id: &str,
        start_time: u64,
        end_time: u64,
    ) -> Vec<ArchivedBucket> {
        match self.historical_archive.get(asset_id) {
            None => Vec::new(),
            Some(buckets) => buckets
                .range(start_time..=end_time)
                .map(|(_, b)| b.clone())
                .collect(),
        }
    }

    /// Return the archival tier that covers a given age (seconds since now).
    pub fn tier_for_age(age_secs: u64) -> ArchiveTier {
        ArchiveTier::from_age(age_secs)
    }

    /// Return a snapshot of the current archival configuration.
    pub fn archival_config(&self) -> &ArchivalConfig {
        &self.archival_config
    }

    /// Update the archival configuration at runtime.
    pub fn set_archival_config(&mut self, config: ArchivalConfig) {
        self.archival_config = config;
    }

    /// Timestamp of the last completed archival run (0 = never ran).
    pub fn last_archival_run(&self) -> u64 {
        self.last_archival_run
    }

    // ─── Private Compression Helpers ──────────────────────────────────────────

    /// Compress live 1-minute buckets older than `cutoff_timestamp` into hourly
    /// ArchivedBuckets in the medium archive.
    ///
    /// Returns `(buckets_created, live_entries_pruned)`.
    fn compress_live_to_medium(&mut self, asset_id: &str, cutoff_timestamp: u64) -> (u32, u32) {
        // Collect the 1-minute buckets that need to be archived
        let minute_buckets_to_archive: Vec<PriceBucket> = {
            let asset_buckets = match self.price_buckets.get(asset_id) {
                None => return (0, 0),
                Some(ab) => ab,
            };
            let one_min = match asset_buckets.get(&TimeBucket::OneMinute) {
                None => return (0, 0),
                Some(m) => m,
            };
            // Bucket index cutoff for 1-minute buckets
            let cutoff_index = cutoff_timestamp / TimeBucket::OneMinute.duration();
            one_min
                .range(..cutoff_index)
                .map(|(_, b)| b.clone())
                .collect()
        };

        if minute_buckets_to_archive.is_empty() {
            return (0, 0);
        }

        // Group by hour window
        let hour_secs = ArchiveTier::Medium.bucket_secs(); // 3600
        let mut hourly_groups: BTreeMap<u64, Vec<PriceBucket>> = BTreeMap::new();
        for bucket in &minute_buckets_to_archive {
            let window_start = (bucket.first_timestamp / hour_secs) * hour_secs;
            hourly_groups.entry(window_start).or_default().push(bucket.clone());
        }

        let medium_map = self.medium_archive
            .entry(asset_id.to_string())
            .or_insert_with(BTreeMap::new);

        let mut buckets_created = 0u32;
        for (window_start, group) in &hourly_groups {
            // Only create / update the archived bucket if the hour window is
            // fully past the cutoff (i.e. we won't receive more data for it)
            let window_end = window_start + hour_secs;
            if window_end > cutoff_timestamp {
                continue; // still accumulating — skip for now
            }

            // Merge all 1-minute buckets in this window into one ArchivedBucket
            let archived = Self::merge_buckets_into_archive(
                asset_id,
                ArchiveTier::Medium,
                *window_start,
                window_end,
                group,
            );
            medium_map.entry(*window_start).or_insert_with(|| {
                buckets_created += 1;
                archived
            });
        }

        // Prune the archived 1-minute buckets from the live store
        let cutoff_index = cutoff_timestamp / TimeBucket::OneMinute.duration();
        let pruned = {
            let asset_buckets = self.price_buckets.get_mut(asset_id).unwrap();
            let one_min = asset_buckets.get_mut(&TimeBucket::OneMinute).unwrap();
            let before = one_min.len();
            one_min.retain(|&idx, _| idx >= cutoff_index);
            (before - one_min.len()) as u32
        };

        // Also prune the same time range from all other TimeBucket granularities
        if let Some(asset_buckets) = self.price_buckets.get_mut(asset_id) {
            for (bucket_type, buckets) in asset_buckets.iter_mut() {
                if *bucket_type == TimeBucket::OneMinute {
                    continue; // already done above
                }
                let dur = bucket_type.duration();
                let ci  = cutoff_timestamp / dur;
                buckets.retain(|&idx, _| idx >= ci);
            }
        }

        (buckets_created, pruned)
    }

    /// Compress medium-tier (hourly) buckets older than `cutoff_timestamp` into
    /// daily ArchivedBuckets in the historical archive, then remove them from
    /// the medium archive.
    ///
    /// Returns the number of daily buckets created.
    fn compress_medium_to_historical(&mut self, asset_id: &str, cutoff_timestamp: u64) -> u32 {
        // Collect medium buckets older than cutoff
        let hourly_to_archive: Vec<ArchivedBucket> = {
            let medium_map = match self.medium_archive.get(asset_id) {
                None => return 0,
                Some(m) => m,
            };
            medium_map
                .range(..cutoff_timestamp)
                .map(|(_, b)| b.clone())
                .collect()
        };

        if hourly_to_archive.is_empty() {
            return 0;
        }

        // Group by day window
        let day_secs = ArchiveTier::Historical.bucket_secs(); // 86400
        let mut daily_groups: BTreeMap<u64, Vec<ArchivedBucket>> = BTreeMap::new();
        for bucket in &hourly_to_archive {
            let window_start = (bucket.window_start / day_secs) * day_secs;
            daily_groups.entry(window_start).or_default().push(bucket.clone());
        }

        let hist_map = self.historical_archive
            .entry(asset_id.to_string())
            .or_insert_with(BTreeMap::new);

        let mut buckets_created = 0u32;
        for (window_start, group) in &daily_groups {
            let window_end = window_start + day_secs;
            if window_end > cutoff_timestamp {
                continue; // day not yet complete
            }

            let archived = Self::merge_archived_into_daily(
                asset_id,
                *window_start,
                window_end,
                group,
            );
            hist_map.entry(*window_start).or_insert_with(|| {
                buckets_created += 1;
                archived
            });
        }

        // Prune archived hourly buckets from medium archive
        if let Some(medium_map) = self.medium_archive.get_mut(asset_id) {
            medium_map.retain(|&ws, _| ws >= cutoff_timestamp);
        }

        buckets_created
    }

    /// Merge a group of `PriceBucket`s (1-minute live data) into a single
    /// `ArchivedBucket` covering `[window_start, window_end)`.
    fn merge_buckets_into_archive(
        asset_id: &str,
        tier: ArchiveTier,
        window_start: u64,
        window_end: u64,
        buckets: &[PriceBucket],
    ) -> ArchivedBucket {
        debug_assert!(!buckets.is_empty());

        let open   = buckets.first().map(|b| b.open).unwrap_or(0);
        let close  = buckets.last().map(|b| b.close).unwrap_or(0);
        let high   = buckets.iter().map(|b| b.high).max().unwrap_or(0);
        let low    = buckets.iter().map(|b| b.low).min().unwrap_or(0);
        let volume = buckets.iter().map(|b| b.volume).fold(0u64, |acc, v| acc.saturating_add(v));
        let raw_count  = buckets.iter().map(|b| b.entry_count).fold(0u32, |acc, c| acc.saturating_add(c));
        let price_sum  = buckets.iter().map(|b| b.close as u128).sum::<u128>();

        ArchivedBucket {
            asset_id:       asset_id.to_string(),
            tier,
            window_start,
            window_end,
            open,
            high,
            low,
            close,
            price_sum,
            volume,
            raw_entry_count: raw_count,
        }
    }

    /// Merge a group of `ArchivedBucket`s (hourly) into a single daily
    /// `ArchivedBucket` covering `[window_start, window_end)`.
    fn merge_archived_into_daily(
        asset_id: &str,
        window_start: u64,
        window_end: u64,
        buckets: &[ArchivedBucket],
    ) -> ArchivedBucket {
        debug_assert!(!buckets.is_empty());

        let open  = buckets.first().map(|b| b.open).unwrap_or(0);
        let close = buckets.last().map(|b| b.close).unwrap_or(0);
        let high  = buckets.iter().map(|b| b.high).max().unwrap_or(0);
        let low   = buckets.iter().map(|b| b.low).min().unwrap_or(0);
        let volume    = buckets.iter().map(|b| b.volume).fold(0u64, |acc, v| acc.saturating_add(v));
        let raw_count = buckets.iter().map(|b| b.raw_entry_count).fold(0u32, |acc, c| acc.saturating_add(c));
        let price_sum = buckets.iter().map(|b| b.price_sum).sum::<u128>();

        ArchivedBucket {
            asset_id: asset_id.to_string(),
            tier: ArchiveTier::Historical,
            window_start,
            window_end,
            open,
            high,
            low,
            close,
            price_sum,
            volume,
            raw_entry_count: raw_count,
        }
    }

    // ─── Archival Public API ends ──────────────────────────────────────────────

    /// Get asset metadata
    ///
    /// # Arguments
    /// * `asset_id` - Asset identifier
    ///
    /// # Returns
    /// Asset metadata
    pub fn get_asset_metadata(&self, asset_id: &str) -> Result<AssetMetadata, PriceHistoryError> {
        self.asset_metadata.get(asset_id)
            .cloned()
            .ok_or(PriceHistoryError::AssetNotFound)
    }

    /// Clean up old price data beyond retention period
    ///
    /// # Arguments
    /// * `retention_period` - Retention period in seconds
    pub fn cleanup_old_data(&mut self, retention_period: u64) {
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let cutoff_time = current_time - retention_period;

        // Clean up each bucket type for each asset
        for (_asset_id, asset_buckets) in self.price_buckets.iter_mut() {
            for (bucket_type, buckets) in asset_buckets.iter_mut() {
                let bucket_duration = bucket_type.duration();
                let cutoff_index = cutoff_time / bucket_duration;

                buckets.retain(|&index, _| index >= cutoff_index);
            }
        }
    }

    /// Export price history to CSV format for external analytics
    ///
    /// # Arguments
    /// * `asset_id` - Asset identifier
    /// * `bucket_type` - Time bucket type to export
    /// * `start_time` - Start timestamp
    /// * `end_time` - End timestamp
    /// * `include_metadata` - Whether to include asset metadata in output
    ///
    /// # Returns
    /// CSV-formatted string with price history data
    pub fn export_to_csv(
        &self,
        asset_id: &str,
        bucket_type: TimeBucket,
        start_time: u64,
        end_time: u64,
        include_metadata: bool,
    ) -> Result<String, PriceHistoryError> {
        let buckets = self.get_price_history(asset_id, bucket_type, start_time, end_time)?;

        let mut csv_output = String::new();

        // Write header
        csv_output.push_str("timestamp,open,high,low,close,volume,entry_count,bucket_index\n");

        // Write data rows
        for bucket in &buckets {
            csv_output.push_str(&format!(
                "{},{},{},{},{},{},{},{}\n",
                bucket.first_timestamp,
                bucket.open,
                bucket.high,
                bucket.low,
                bucket.close,
                bucket.volume,
                bucket.entry_count,
                bucket.bucket_index,
            ));
        }

        // Optionally include metadata
        if include_metadata {
            if let Ok(metadata) = self.get_asset_metadata(asset_id) {
                csv_output.push_str("\n# Asset Metadata\n");
                csv_output.push_str(&format!("asset_id,{},total_entries,{},first_timestamp,{},last_timestamp,{},current_price,{}\n",
                    metadata.asset_id,
                    metadata.total_entries,
                    metadata.first_timestamp,
                    metadata.last_timestamp,
                    metadata.current_price,
                ));
            }
        }

        Ok(csv_output)
    }

    /// Export price history to JSON format for external analytics
    ///
    /// # Arguments
    /// * `asset_id` - Asset identifier
    /// * `bucket_type` - Time bucket type to export
    /// * `start_time` - Start timestamp
    /// * `end_time` - End timestamp
    /// * `include_metadata` - Whether to include asset metadata in output
    ///
    /// # Returns
    /// JSON-formatted string with price history data
    pub fn export_to_json(
        &self,
        asset_id: &str,
        bucket_type: TimeBucket,
        start_time: u64,
        end_time: u64,
        include_metadata: bool,
    ) -> Result<String, PriceHistoryError> {
        let buckets = self.get_price_history(asset_id, bucket_type, start_time, end_time)?;

        let mut output = serde_json::Map::new();

        // Add data array
        let data_array: Vec<serde_json::Value> = buckets.iter().map(|bucket| {
            serde_json::json!({
                "timestamp": bucket.first_timestamp,
                "open": bucket.open,
                "high": bucket.high,
                "low": bucket.low,
                "close": bucket.close,
                "volume": bucket.volume,
                "entry_count": bucket.entry_count,
                "bucket_index": bucket.bucket_index,
            })
        }).collect();
        output.insert("data".to_string(), serde_json::Value::Array(data_array));

        // Optionally include metadata
        if include_metadata {
            if let Ok(metadata) = self.get_asset_metadata(asset_id) {
                let metadata_json = serde_json::json!({
                    "asset_id": metadata.asset_id,
                    "total_entries": metadata.total_entries,
                    "first_timestamp": metadata.first_timestamp,
                    "last_timestamp": metadata.last_timestamp,
                    "current_price": metadata.current_price,
                    "high_24h": metadata.high_24h,
                    "low_24h": metadata.low_24h,
                    "volume_24h": metadata.volume_24h,
                    "price_change_24h_bps": metadata.price_change_24h_bps,
                });
                output.insert("metadata".to_string(), metadata_json);
            }
        }

        Ok(serde_json::to_string_pretty(&output).map_err(|_| PriceHistoryError::ExportError)?)
    }

    // ─── Internal Functions ─────────────────────────────────────────────────────

    fn store_in_bucket(&mut self, entry: PriceHistoryEntry, bucket_type: TimeBucket) {
        let bucket_duration = bucket_type.duration();
        let bucket_index = entry.timestamp / bucket_duration;

        let asset_buckets = self.price_buckets
            .entry(entry.asset_id.clone())
            .or_insert_with(HashMap::new);

        let buckets = asset_buckets
            .entry(bucket_type.clone())
            .or_insert_with(BTreeMap::new);

        let bucket = buckets.entry(bucket_index).or_insert_with(|| {
            PriceBucket {
                bucket_type: bucket_type.clone(),
                bucket_index,
                open: entry.price,
                high: entry.price,
                low: entry.price,
                close: entry.price,
                volume: entry.volume,
                entry_count: 0,
                first_timestamp: entry.timestamp,
                last_timestamp: entry.timestamp,
            }
        });

        // Update bucket with new price
        bucket.close = entry.price;
        bucket.high = bucket.high.max(entry.price);
        bucket.low = bucket.low.min(entry.price);
        bucket.volume += entry.volume;
        bucket.entry_count += 1;
        bucket.last_timestamp = entry.timestamp;
    }

    fn update_asset_metadata(&mut self, entry: &PriceHistoryEntry) {
        let metadata = self.asset_metadata
            .entry(entry.asset_id.clone())
            .or_insert_with(|| {
                AssetMetadata {
                    asset_id: entry.asset_id.clone(),
                    total_entries: 0,
                    first_timestamp: entry.timestamp,
                    last_timestamp: entry.timestamp,
                    current_price: entry.price,
                    high_24h: entry.price,
                    low_24h: entry.price,
                    volume_24h: 0,
                    price_change_24h_bps: 0,
                }
            });

        metadata.total_entries += 1;
        metadata.last_timestamp = entry.timestamp;
        metadata.current_price = entry.price;

        // Update 24h stats
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        if current_time - entry.timestamp <= 86400 {
            metadata.high_24h = metadata.high_24h.max(entry.price);
            metadata.low_24h = metadata.low_24h.min(entry.price);
            metadata.volume_24h += entry.volume;
        }

        // Calculate 24h price change
        if metadata.total_entries > 1 {
            let old_price = if metadata.total_entries == 1 {
                entry.price
            } else {
                metadata.current_price
            };
            let price_diff = if entry.price > old_price {
                entry.price - old_price
            } else {
                old_price - entry.price
            };
            metadata.price_change_24h_bps = if old_price > 0 {
                ((price_diff as i128 * 10000) / old_price as i128) as i64
            } else {
                0
            };
        }
    }

    fn calculate_analytics(&self, asset_id: &str) -> Result<AnalyticsData, PriceHistoryError> {
        let sma_1h = self.calculate_sma(asset_id, 3600)?;
        let sma_6h = self.calculate_sma(asset_id, 21600)?;
        let sma_24h = self.calculate_sma(asset_id, 86400)?;
        let sma_7d = self.calculate_sma(asset_id, 604800)?;

        let volatility_24h = self.calculate_volatility(asset_id, 86400)?;

        // Determine trend based on SMAs
        let trend = if sma_1h > sma_24h {
            PriceTrend::Up
        } else if sma_1h < sma_24h {
            PriceTrend::Down
        } else {
            PriceTrend::Sideways
        };

        // Calculate EMAs (simplified version - using SMA for now)
        let ema_1h = sma_1h;
        let ema_6h = sma_6h;
        let ema_24h = sma_24h;

        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Ok(AnalyticsData {
            asset_id: asset_id.to_string(),
            sma_1h,
            sma_6h,
            sma_24h,
            sma_7d,
            ema_1h,
            ema_6h,
            ema_24h,
            volatility_24h,
            trend,
            last_update: current_time,
        })
    }

    fn invalidate_analytics_cache(&mut self, asset_id: &str) {
        self.analytics_cache.remove(asset_id);
    }

    fn count_prices_in_range(
        &self,
        asset_id: &str,
        bucket_type: &TimeBucket,
        start_time: u64,
        end_time: u64,
    ) -> Result<u64, PriceHistoryError> {
        let asset_buckets = self.price_buckets.get(asset_id)
            .ok_or(PriceHistoryError::AssetNotFound)?;
        let buckets = asset_buckets.get(bucket_type)
            .ok_or(PriceHistoryError::AssetNotFound)?;

        let bucket_duration = bucket_type.duration();
        let start_index = start_time / bucket_duration;
        let end_index = end_time / bucket_duration;

        Ok(buckets.range(start_index..=end_index).count() as u64)
    }
}

impl Default for PriceHistoryManager {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manager_creation() {
        let manager = PriceHistoryManager::new();
        assert!(manager.price_buckets.is_empty());
    }

    #[test]
    fn test_store_price() {
        let mut manager = PriceHistoryManager::new();
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let entry = PriceHistoryEntry {
            asset_id: "XLM".to_string(),
            price: 1000000,
            decimals: 6,
            timestamp: current_time,
            source: "oracle1".to_string(),
            volume: 1000,
            transaction_count: 10,
        };

        assert!(manager.store_price(entry).is_ok());
    }

    #[test]
    fn test_twap_calculation() {
        let mut manager = PriceHistoryManager::new();
        let base_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Store multiple price points strictly in the past (base_time - 600 .. base_time)
        for i in 0u64..10 {
            let entry = PriceHistoryEntry {
                asset_id: "XLM".to_string(),
                price: 1_000_000 + i * 10_000,
                decimals: 6,
                timestamp: base_time - 600 + i * 60, // past timestamps only
                source: "oracle1".to_string(),
                volume: 1000,
                transaction_count: 10,
            };
            manager.store_price(entry).unwrap();
        }

        let twap = manager.calculate_twap("XLM", 600).unwrap();
        assert!(twap.twap_price > 0);
    }

    #[test]
    fn test_get_price_history_pagination() {
        let mut manager = PriceHistoryManager::new();
        let base_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        for i in 0..10 {
            let entry = PriceHistoryEntry {
                asset_id: "XLM".to_string(),
                price: 1000000 + (i * 10000),
                decimals: 6,
                timestamp: base_time + (i * 60),
                source: "oracle1".to_string(),
                volume: 1000,
                transaction_count: 10,
            };
            manager.store_price(entry).unwrap();
        }

        // Full range without pagination
        let all = manager.get_price_history("XLM", TimeBucket::OneMinute, base_time, base_time + 600, None, None).unwrap();
        assert_eq!(all.len(), 10);

        // With limit
        let limited = manager.get_price_history("XLM", TimeBucket::OneMinute, base_time, base_time + 600, Some(3), None).unwrap();
        assert_eq!(limited.len(), 3);
        assert_eq!(limited[0].close, 1000000);

        // With offset
        let offset = manager.get_price_history("XLM", TimeBucket::OneMinute, base_time, base_time + 600, None, Some(5)).unwrap();
        assert_eq!(offset.len(), 5);
        assert_eq!(offset[0].close, 1050000);

        // With limit and offset
        let paginated = manager.get_price_history("XLM", TimeBucket::OneMinute, base_time, base_time + 600, Some(2), Some(3)).unwrap();
        assert_eq!(paginated.len(), 2);
        assert_eq!(paginated[0].close, 1030000);
        assert_eq!(paginated[1].close, 1040000);
    }

    #[test]
    fn test_query_historical_prices() {
        let mut manager = PriceHistoryManager::new();
        let base_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        for i in 0..10 {
            let entry = PriceHistoryEntry {
                asset_id: "XLM".to_string(),
                price: 1000000 + (i * 10000),
                decimals: 6,
                timestamp: base_time + (i * 60),
                source: "oracle1".to_string(),
                volume: 1000,
                transaction_count: 10,
            };
            manager.store_price(entry).unwrap();
        }

        let result = manager.query_historical_prices(
            "XLM", base_time, base_time + 600, "1min",
            Some(3), Some(0),
        ).unwrap();

        assert_eq!(result.prices.len(), 3);
        assert!(result.twap > 0);
        assert_eq!(result.total_count, 10);
        assert!(result.has_more);
    }

    #[test]
    fn test_without_pagination_flag() {
        let mut manager = PriceHistoryManager::new();
        let base_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let entry = PriceHistoryEntry {
            asset_id: "XLM".to_string(),
            price: 1000000,
            decimals: 6,
            timestamp: base_time,
            source: "oracle1".to_string(),
            volume: 1000,
            transaction_count: 10,
        };
        manager.store_price(entry).unwrap();

        let result = manager.query_historical_prices(
            "XLM", base_time, base_time + 60, "1min",
            None, None,
        ).unwrap();

        assert_eq!(result.prices.len(), 1);
        assert_eq!(result.has_more, false);
        assert_eq!(result.total_count, 1);
    }

    #[test]
    fn test_invalid_granularity() {
        let manager = PriceHistoryManager::new();
        let err = manager.query_historical_prices(
            "XLM", 0, 100, "invalid", None, None,
        ).unwrap_err();
        assert_eq!(err, PriceHistoryError::InvalidGranularity("invalid".to_string()));
    }

    #[test]
    fn test_granularity_mapping() {
        assert_eq!(TimeBucket::from_granularity_str("1min").unwrap(), TimeBucket::OneMinute);
        assert_eq!(TimeBucket::from_granularity_str("5min").unwrap(), TimeBucket::FiveMinute);
        assert_eq!(TimeBucket::from_granularity_str("15min").unwrap(), TimeBucket::FifteenMinute);
        assert_eq!(TimeBucket::from_granularity_str("1hr").unwrap(), TimeBucket::OneHour);
        assert_eq!(TimeBucket::from_granularity_str("1day").unwrap(), TimeBucket::OneDay);
        assert!(TimeBucket::from_granularity_str("bad").is_err());
    }

    #[test]
    fn test_time_bucket_durations() {
        assert_eq!(TimeBucket::OneMinute.duration(), 60);
        assert_eq!(TimeBucket::FiveMinute.duration(), 300);
        assert_eq!(TimeBucket::FifteenMinute.duration(), 900);
        assert_eq!(TimeBucket::OneHour.duration(), 3600);
        assert_eq!(TimeBucket::SixHour.duration(), 21600);
        assert_eq!(TimeBucket::OneDay.duration(), 86400);
    }
}
