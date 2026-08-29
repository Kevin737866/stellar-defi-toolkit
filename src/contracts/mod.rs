//! Contract-oriented protocol modules.
//!
//! NOTE: several contract modules currently fail to compile on `main` due to
//! pre-existing structural bugs unrelated to the lending protocol (missing
//! types such as `PoolContractError`/`PoolDataKey`, duplicate type
//! definitions, broken SDK imports, etc.). They are temporarily excluded here
//! so the crate — and the lending protocol tests/CLI work that depend on it —
//! can compile. Restoring them requires a separate repair pass on each module:
//! arbitrage, flash_loan, governance_v2,
//! liquidity_pool, multi_asset_oracle,
//! price_feed_adapters, price_oracle, stability_pool, stablecoin, staking,
//! synthetic_governance, synthetic_protocol, token, vault.
pub mod asset_registry_protocol;
/// synthetic_governance, synthetic_protocol, vault.
pub mod asset_registry;
pub mod governance;
pub mod lending;
pub mod oracle;
pub mod oracle_manager;
pub mod position_manager;
pub mod price_feed_adapters;
pub mod price_history;
pub mod price_oracle;
pub mod stability_pool;
pub mod stablecoin;
pub mod staking;
pub mod synthetic_governance;
pub mod synthetic_protocol;
pub mod token;
pub mod token_metadata_support;
pub mod pausable_token;
pub mod vault;

pub use lending::LendingProtocol;
pub use oracle::{PriceOracle, PriceOracleSim};
pub use price_feed_adapters::{PriceFeedAdaptersContract, StellarDexAdapter};
