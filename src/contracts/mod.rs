//! Contract-oriented protocol modules.

pub mod circuit_breaker;
pub mod lending;
pub mod liquidity_pool;
pub mod oracle;
pub mod price_history;

pub use circuit_breaker::CircuitBreakerContract;
pub use lending::LendingProtocol;
pub use oracle::{PriceOracle, PriceOracleSim};
pub use price_history::{
    PriceHistoryManager, TimeBucket, PriceHistoryEntry, TwapResult, AnalyticsData,
    PriceTrend, AssetMetadata, PriceBucket, PriceHistoryError,
};
