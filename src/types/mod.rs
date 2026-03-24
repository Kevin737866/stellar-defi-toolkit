//! Type definitions for Stellar DeFi Toolkit

pub mod token;
pub mod pool;
pub mod vault;

// Re-export commonly used types
pub use token::{TokenInfo, TokenMetadata};
pub use pool::{PoolInfo, LiquidityPosition, SwapParams};
pub use vault::{VaultInfo, VaultStrategy, VaultStats, StrategyType, HarvestResult, DepositResult, WithdrawResult, PerformanceFeeConfig};
