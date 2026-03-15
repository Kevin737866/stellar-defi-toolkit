//! # Stellar DeFi Toolkit
//! 
//! A comprehensive toolkit for building DeFi applications on the Stellar blockchain
//! using Soroban smart contracts.
//! 
//! ## Features
//! 
//! - Token contract implementation
//! - Liquidity pool contracts
//! - Yield farming protocols
//! - Cross-chain bridges
//! - Staking mechanisms
//! - Governance contracts
//! 
//! ## Getting Started
//! 
//! Add this to your `Cargo.toml`:
//! 
//! ```toml
//! [dependencies]
//! stellar-defi-toolkit = "0.1.0"
//! ```

pub mod contracts;
pub mod utils;
pub mod types;

// Re-export commonly used types
pub use contracts::{TokenContract, LiquidityPoolContract, StakingContract};
pub use utils::StellarClient;
pub use types::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_library_imports() {
        // Basic test to ensure library structure is correct
        assert!(true);
    }
}
