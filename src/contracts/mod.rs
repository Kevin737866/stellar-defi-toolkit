//! Contract-oriented protocol modules.

pub mod circuit_breaker;
pub mod lending;
pub mod liquidity_pool;
pub mod oracle;
pub mod decentralized_oracle;

pub use circuit_breaker::CircuitBreakerContract;
pub use lending::LendingProtocol;
pub use oracle::{PriceOracle, PriceOracleSim};
pub use decentralized_oracle::{DecentralizedOracle, DecentralizedOracleError};
