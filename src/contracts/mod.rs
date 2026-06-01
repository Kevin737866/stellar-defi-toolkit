//! Contract-oriented protocol modules.

pub mod lending;
pub mod liquidity_pool;
pub mod oracle;

pub use lending::{LendingContract, LendingProtocol};
pub use liquidity_pool::{LiquidityPool, LiquidityPoolContract};
pub use oracle::{PriceOracle, PriceOracleSim};
