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

// Re-export main contract types
pub use governance::GovernanceContract;
pub use liquidity_pool::LiquidityPoolContract;
pub use stablecoin::StablecoinContract;
pub use staking::StakingContract;
pub use token::TokenContract;
pub use vault::YieldVaultContract;
