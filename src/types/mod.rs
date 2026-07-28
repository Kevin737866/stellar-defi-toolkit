//! Shared types used by the lending protocol and multi-asset price feeds.
//!
//! NOTE: `asset`, `pool`, `stablecoin`, `synthetic`, `token`, and `vault` are
//! temporarily excluded — see `contracts/mod.rs` for why.

pub mod lending;

pub use lending::*;
