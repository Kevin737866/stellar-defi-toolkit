//! Type definitions for Stellar DeFi Toolkit

pub mod token;
pub mod pool;
pub mod strategy;
pub mod vault;
pub mod flash_loan;

// Re-export commonly used types
pub use token::{TokenInfo, TokenMetadata};
pub use pool::{PoolInfo, LiquidityPosition, SwapParams};
pub use strategy::{VaultStrategy, StrategyType};
pub use vault::{VaultInfo, VaultStats, HarvestResult, DepositResult, WithdrawResult, PerformanceFeeConfig};
pub use flash_loan::{FlashLoanInfo, FlashLoanParams, FlashLoanResult, FlashLoanEvent};
