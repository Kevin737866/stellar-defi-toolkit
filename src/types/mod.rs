//! Shared types used by the lending protocol and multi-asset price feeds.
//!
//! NOTE: `asset`, `pool`, `stablecoin`, `synthetic`, `token`, and `vault` are
//! temporarily excluded — see `contracts/mod.rs` for why.

pub mod asset;
pub mod lending;
pub mod pool;
pub mod stablecoin;
pub mod synthetic;
pub mod token;
pub mod vault;

pub use lending::*;
