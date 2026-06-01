//! Contract-oriented protocol modules.

pub mod arbitrage;
pub mod asset_registry;
pub mod flash_loan;
pub mod governance;
pub mod governance_v2;
pub mod lending;
pub mod liquidity_pool;
pub mod multi_asset_oracle;
pub mod oracle;
pub mod oracle_manager;
pub mod position_manager;
pub mod price_feed_adapters;
pub mod price_oracle;
pub mod stability_pool;
pub mod stablecoin;
pub mod staking;
pub mod synthetic_governance;
pub mod synthetic_protocol;
pub mod token;
pub mod vault;

pub use asset_registry::AssetRegistryContract;
pub use lending::LendingProtocol;
pub use multi_asset_oracle::MultiAssetOracleContract;
pub use oracle::{PriceOracle, PriceOracleSim};
pub use price_feed_adapters::PriceFeedAdaptersContract;
