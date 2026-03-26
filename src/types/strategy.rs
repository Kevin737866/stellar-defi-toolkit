//! Strategy-related type definitions for the yield farming vault

use soroban_sdk::{contracttype, Address, Symbol};

/// Strategy type classification
#[contracttype]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum StrategyType {
    /// AMM liquidity provision
    LiquidityPool = 0,
    /// Token staking
    Staking = 1,
    /// Lending protocol
    Lending = 2,
    /// Custom / external protocol
    Custom = 3,
}

/// A yield strategy the vault can allocate funds to
#[contracttype]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct VaultStrategy {
    pub name: Symbol,
    pub contract_address: Address,
    pub strategy_type: StrategyType,
    pub estimated_apy: u32,
    pub allocated_amount: u64,
    pub active: bool,
}
