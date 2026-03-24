//! Vault-related type definitions for the yield farming vault

use serde::{Deserialize, Serialize};
use soroban_sdk::Address;

/// Strategy type classification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StrategyType {
    /// AMM liquidity provision
    LiquidityPool,
    /// Token staking
    Staking,
    /// Lending protocol
    Lending,
    /// Custom / external protocol
    Custom(String),
}

/// A yield strategy the vault can allocate funds to
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultStrategy {
    /// Human-readable strategy name
    pub name: String,
    /// On-chain contract address of the strategy
    pub contract_address: String,
    /// Strategy classification
    pub strategy_type: StrategyType,
    /// Estimated annual percentage yield (%)
    pub estimated_apy: f64,
    /// Amount currently allocated to this strategy
    pub allocated_amount: u64,
    /// Whether this strategy is currently active
    pub active: bool,
}

/// Snapshot of vault state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultInfo {
    /// Underlying asset token address
    pub asset_token: String,
    /// Vault share token address (SEP-41)
    pub share_token: String,
    /// Total shares outstanding
    pub total_shares: u64,
    /// Total assets under management
    pub total_assets: u64,
    /// Current share price (assets / shares)
    pub share_price: f64,
    /// Currently active strategy
    pub active_strategy: Option<VaultStrategy>,
    /// Total number of registered strategies
    pub strategy_count: usize,
    /// Performance fee in basis points
    pub performance_fee_bps: u32,
    /// Whether the vault is paused
    pub paused: bool,
    /// Timestamp of last harvest
    pub last_harvest: u64,
}

/// Vault runtime statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultStats {
    pub total_assets: u64,
    pub total_shares: u64,
    pub share_price: f64,
    pub current_apy: f64,
    pub accumulated_fees: u64,
    pub last_harvest: u64,
    pub paused: bool,
}

/// Result of a deposit operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepositResult {
    pub depositor: Address,
    pub amount_deposited: u64,
    pub shares_minted: u64,
    pub share_price: f64,
}

/// Result of a withdrawal operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WithdrawResult {
    pub withdrawer: Address,
    pub shares_burned: u64,
    pub amount_withdrawn: u64,
    pub share_price: f64,
}

/// Result of a harvest operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarvestResult {
    /// Total rewards collected from the strategy
    pub raw_rewards: u64,
    /// Performance fee deducted
    pub performance_fee: u64,
    /// Net rewards after fee
    pub net_rewards: u64,
    /// Amount reinvested (auto-compounded)
    pub compounded_amount: u64,
    /// New total assets after compounding
    pub new_total_assets: u64,
}

/// Performance fee configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceFeeConfig {
    /// Fee in basis points (e.g., 1000 = 10%)
    pub fee_bps: u32,
    /// Treasury address that receives fees
    pub treasury: Address,
}
