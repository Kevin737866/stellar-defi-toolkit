//! Lending and borrowing protocol primitives for Soroban-based Stellar applications.
//!
//! The crate exposes a complete protocol simulation core that mirrors the moving
//! parts a Soroban lending market needs:
//! - liquidity pools for supplier deposits
//! - utilization-based interest rates
//! - collateralized borrowing
//! - liquidations
//! - flash loans
//! - protocol fee accounting
//! - oracle-driven pricing
//! - multi-asset price feeds for a wide range of Stellar assets

pub mod contracts;
pub mod types;
pub mod utils;

pub use contracts::{
    AssetRegistryContract,
    LendingProtocol,
    MultiAssetOracleContract,
    PriceFeedAdaptersContract,
    PriceOracle,
    PriceOracleSim,
};
pub use types::asset::*;
pub use types::lending::*;
pub use utils::fixed_point::{
    bps_mul, mul_div, wad_div, wad_mul, BPS_DENOMINATOR, WAD, YEAR_IN_SECONDS,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exports_are_available() {
        let _ = InterestRateModel::default();
    }
}
