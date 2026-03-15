//! Smart contract implementations for DeFi protocols on Stellar
//! 
//! This module contains various contract implementations including:
//! - Token contracts (ERC-20-like)
//! - Liquidity pool contracts (AMM)
//! - Staking contracts
//! - Governance contracts

pub mod token;
pub mod liquidity_pool;
pub mod staking;
pub mod governance;

// Re-export main contract types
pub use token::TokenContract;
pub use liquidity_pool::LiquidityPoolContract;
pub use staking::StakingContract;
pub use governance::GovernanceContract;
