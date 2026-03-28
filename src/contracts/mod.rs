//! Smart contract implementations for DeFi protocols on Stellar
//!
//! This module contains various contract implementations including:
//! - Token contracts (ERC-20-like)
//! - Liquidity pool contracts (AMM)
//! - Staking contracts
//! - Governance contracts
//! - Stablecoin contracts
//! - Price oracle contracts
//! - Stability pool contracts
//! - Arbitrage incentives contracts
//! - Synthetic asset protocol contracts

// Core working contracts
pub mod governance;
pub mod liquidity_pool;
pub mod stablecoin;
pub mod staking;
pub mod token;
pub mod vault;

// Temporarily disabled contracts due to Soroban type serialization issues
// These need to be refactored to work with current Soroban SDK
// pub mod price_oracle;
// pub mod stability_pool;
// pub mod governance_v2;
// pub mod arbitrage;
// Synthetic asset protocol contracts
pub mod oracle_manager;
pub mod position_manager;
pub mod synthetic_governance;
pub mod synthetic_protocol;

// Re-export main contract types
pub use governance::GovernanceContract;
pub use liquidity_pool::LiquidityPoolContract;
pub use stablecoin::StablecoinContract;
pub use staking::StakingContract;
pub use oracle_manager::OracleManagerContract;
pub use position_manager::PositionManagerContract;
pub use stablecoin::StablecoinContract;
pub use staking::StakingContract;
pub use synthetic_governance::SyntheticGovernanceContract;
pub use synthetic_protocol::SyntheticProtocolContract;
pub use token::TokenContract;
pub use vault::YieldVaultContract;
