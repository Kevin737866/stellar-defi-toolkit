use std::collections::BTreeMap;

use crate::types::ProtocolError;

/// Maximum allowed single-update price deviation (50% = 5000 bps).
/// A price update that moves more than this from the last known price is
/// rejected as a potential manipulation attempt.
const MAX_PRICE_DEVIATION_BPS: i128 = 5_000;

/// Maximum age of a price entry in seconds before it is considered stale.
/// Consumers that pass a `now` timestamp will receive an error for stale data.
const MAX_PRICE_AGE_SECS: u64 = 3_600; // 1 hour

/// Per-asset price bounds used to reject obviously invalid prices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriceBounds {
    /// Minimum acceptable price (WAD-scaled, i.e. 1e18 = $1.00 with 18 decimals).
    pub min_price: i128,
    /// Maximum acceptable price.
    pub max_price: i128,
}

/// A price entry stored inside the oracle, carrying the value and the
/// timestamp at which it was last set.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PriceEntry {
    price: i128,
    /// Unix timestamp (seconds) when this price was recorded.
    updated_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriceOracle {
    admin: String,
    entries: BTreeMap<String, PriceEntry>,
    /// Optional per-asset absolute price bounds.
    bounds: BTreeMap<String, PriceBounds>,
}

impl PriceOracle {
    pub fn new(admin: impl Into<String>) -> Self {
        Self {
            admin: admin.into(),
            entries: BTreeMap::new(),
            bounds: BTreeMap::new(),
        }
    }

    pub fn admin(&self) -> &str {
        &self.admin
    }

    // ── Admin helpers ────────────────────────────────────────────────────────

    /// Register per-asset absolute price bounds (admin only).
    ///
    /// Once set, any `set_price` call whose value falls outside
    /// `[min_price, max_price]` will be rejected.
    pub fn set_price_bounds(
        &mut self,
        caller: &str,
        asset: impl Into<String>,
        bounds: PriceBounds,
    ) -> Result<(), ProtocolError> {
        if caller != self.admin {
            return Err(ProtocolError::Unauthorized);
        }
        if bounds.min_price <= 0 || bounds.max_price <= 0 || bounds.min_price >= bounds.max_price {
            return Err(ProtocolError::InvalidAmount);
        }
        self.bounds.insert(asset.into(), bounds);
        Ok(())
    }

    /// Update the oracle price for `asset` (admin only).
    ///
    /// # Sanity checks performed
    /// 1. Caller must be the admin.
    /// 2. `price` must be strictly positive.
    /// 3. If absolute bounds are registered for the asset, `price` must lie
    ///    within `[min_price, max_price]`.
    /// 4. If a previous price exists, the relative deviation from it must not
    ///    exceed [`MAX_PRICE_DEVIATION_BPS`] (50 %).  This acts as a circuit
    ///    breaker against sudden manipulation.
    pub fn set_price(
        &mut self,
        caller: &str,
        asset: impl Into<String>,
        price: i128,
        now: u64,
    ) -> Result<(), ProtocolError> {
        if caller != self.admin {
            return Err(ProtocolError::Unauthorized);
        }

        // ── Check 1: price must be positive ──────────────────────────────────
        if price <= 0 {
            log::warn!(
                "oracle: rejected non-positive price {} for asset",
                price
            );
            return Err(ProtocolError::InvalidAmount);
        }

        let asset: String = asset.into();

        // ── Check 2: absolute bounds ──────────────────────────────────────────
        if let Some(b) = self.bounds.get(&asset) {
            if price < b.min_price || price > b.max_price {
                log::warn!(
                    "oracle: price {} for '{}' is outside bounds [{}, {}]",
                    price,
                    asset,
                    b.min_price,
                    b.max_price
                );
                return Err(ProtocolError::OraclePriceOutOfBounds);
            }
        }

        // ── Check 3: deviation circuit-breaker ────────────────────────────────
        if let Some(prev) = self.entries.get(&asset) {
            let deviation_bps = price_deviation_bps(prev.price, price);
            if deviation_bps > MAX_PRICE_DEVIATION_BPS {
                log::warn!(
                    "oracle: price update for '{}' rejected — deviation {} bps exceeds limit {} bps \
                     (prev={}, new={})",
                    asset,
                    deviation_bps,
                    MAX_PRICE_DEVIATION_BPS,
                    prev.price,
                    price
                );
                return Err(ProtocolError::OraclePriceDeviationTooHigh);
            }
        }

        log::info!(
            "oracle: price set for '{}' → {} (timestamp={})",
            asset,
            price,
            now
        );

        self.entries.insert(asset, PriceEntry { price, updated_at: now });
        Ok(())
    }

    /// Retrieve the current price for `asset`.
    ///
    /// Returns [`ProtocolError::MissingPrice`] if no price has been set yet.
    pub fn get_price(&self, asset: &str) -> Result<i128, ProtocolError> {
        self.entries
            .get(asset)
            .map(|e| e.price)
            .ok_or_else(|| ProtocolError::MissingPrice(asset.to_string()))
    }

    /// Retrieve the price for `asset` and verify it is not stale.
    ///
    /// A price is considered stale when `now - updated_at > MAX_PRICE_AGE_SECS`.
    /// Use this variant in any code path that is sensitive to stale data
    /// (e.g. collateral valuation, liquidation checks).
    pub fn get_price_checked(&self, asset: &str, now: u64) -> Result<i128, ProtocolError> {
        let entry = self
            .entries
            .get(asset)
            .ok_or_else(|| ProtocolError::MissingPrice(asset.to_string()))?;

        if now.saturating_sub(entry.updated_at) > MAX_PRICE_AGE_SECS {
            log::warn!(
                "oracle: stale price for '{}' (last updated {}s ago, limit {}s)",
                asset,
                now.saturating_sub(entry.updated_at),
                MAX_PRICE_AGE_SECS
            );
            return Err(ProtocolError::OraclePriceStale);
        }

        Ok(entry.price)
    }

    /// Return the timestamp at which the price for `asset` was last updated,
    /// or `None` if no price has been set.
    pub fn last_updated(&self, asset: &str) -> Option<u64> {
        self.entries.get(asset).map(|e| e.updated_at)
    }
}

// ── Internal helpers ─────────────────────────────────────────────────────────

/// Compute the absolute deviation between `old` and `new` in basis points.
///
/// Returns 0 if `old` is zero to avoid division by zero.
fn price_deviation_bps(old: i128, new: i128) -> i128 {
    if old == 0 {
        return 0;
    }
    let diff = (new - old).abs();
    diff * 10_000 / old
}
