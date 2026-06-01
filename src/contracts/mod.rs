//! Contract-oriented protocol modules.

pub mod lending;
pub mod liquidity_pool;
pub mod oracle;
pub mod staking;

pub use lending::LendingProtocol;
pub use oracle::PriceOracle;
pub use staking::{StakingContract, StakingInfo};
