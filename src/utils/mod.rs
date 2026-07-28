//! Utility helpers.
//!
//! NOTE: `client` (StellarClient) is temporarily excluded — it depends on the
//! `reqwest` crate (not a declared dependency) and references contract types
//! that are currently excluded from `contracts/mod.rs`. See that file for why.

pub mod fixed_point;

pub use fixed_point::*;
