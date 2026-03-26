//! Contract-oriented protocol modules.

pub mod lending;
pub mod oracle;

pub use lending::LendingProtocol;
pub use oracle::PriceOracle;
