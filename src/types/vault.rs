//! Vault-related type definitions for the yield farming vault

use serde::{Deserialize, Serialize};
use soroban_sdk::{Address, Symbol};

/// Strategy type classification
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StrategyType {
    /// AMM liquidity provision
    LiquidityPool,
    /// Token staking
    Staking,
    /// Lending protocol
    Lending,
    /// Custom / external protocol
    Custom(Symbol),
}

/// A yield strategy the vault can allocate funds to
#[derive(Debug, Clone)]
pub struct VaultStrategy {
    /// Human-readable strategy name
    pub name: Symbol,
    /// On-chain contract address of the strategy
    pub contract_address: Address,
    /// Strategy classification
    pub strategy_type: StrategyType,
    /// Estimated annual percentage yield (basis points)
    pub estimated_apy: u32,
    /// Amount currently allocated to this strategy
    pub allocated_amount: u64,
    /// Whether this strategy is currently active
    pub active: bool,
}

/// Snapshot of vault state
#[derive(Debug, Clone)]
pub struct VaultInfo {
    /// Underlying asset token address
    pub asset_token: Address,
    /// Vault share token address (SEP-41)
    pub share_token: Address,
    /// Total shares outstanding
    pub total_shares: u64,
    /// Total assets under management
    pub total_assets: u64,
    /// Current share price (assets / shares)
    pub share_price: f64,
    /// Index of the currently active strategy, or -1 when none is registered
    pub active_strategy_index: i32,
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
#[derive(Debug, Clone)]
pub struct DepositResult {
    pub depositor: Address,
    pub amount_deposited: u64,
    pub shares_minted: u64,
    pub share_price: f64,
}

/// Result of a withdrawal operation
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
pub struct PerformanceFeeConfig {
    /// Fee in basis points (e.g., 1000 = 10%)
    pub fee_bps: u32,
    /// Treasury address that receives fees
    pub treasury: Address,
}

/// Reason a vault rebalance was triggered
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RebalanceReason {
    /// A candidate strategy yields more than the active one by at least the threshold
    YieldDifferential,
    /// The active strategy's risk score moved by at least the threshold
    RiskScoreChange,
    /// A candidate strategy's utilization moved by at least the threshold
    UtilizationChange,
    /// An operator forced the rebalance, bypassing thresholds
    ManualOverride,
}

/// Market inputs a rebalance decision is evaluated against
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketConditions {
    /// Yield (APY, in basis points) of each registered strategy
    pub strategy_yields_bps: Vec<u32>,
    /// Risk score (0-10000, higher is riskier) of each registered strategy
    pub strategy_risk_scores: Vec<u32>,
    /// Utilization (0-10000) of each registered strategy
    pub strategy_utilization: Vec<u32>,
}

impl MarketConditions {
    /// Empty market snapshot sized for `strategy_count` strategies
    pub fn empty(strategy_count: usize) -> Self {
        Self {
            strategy_yields_bps: vec![0; strategy_count],
            strategy_risk_scores: vec![0; strategy_count],
            strategy_utilization: vec![0; strategy_count],
        }
    }
}

/// Thresholds that decide when a rebalance fires
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebalanceThresholds {
    /// Minimum yield improvement, in basis points, to trigger a rebalance
    pub yield_differential_bps: u32,
    /// Minimum change in risk score, in basis points, to trigger a rebalance
    pub risk_score_change_bps: u32,
    /// Minimum change in utilization, in basis points, to trigger a rebalance
    pub utilization_change_bps: u32,
    /// Whether yield-based rebalancing is enabled
    pub yield_enabled: bool,
    /// Whether risk-based rebalancing is enabled
    pub risk_enabled: bool,
    /// Whether utilization-based rebalancing is enabled
    pub utilization_enabled: bool,
}

impl Default for RebalanceThresholds {
    fn default() -> Self {
        Self {
            yield_differential_bps: 200,
            risk_score_change_bps: 1_000,
            utilization_change_bps: 2_000,
            yield_enabled: true,
            risk_enabled: true,
            utilization_enabled: true,
        }
    }
}

/// A rebalance trigger that was evaluated, with the rationale behind it
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebalanceTrigger {
    /// Which condition fired
    pub reason: RebalanceReason,
    /// Strategy the vault would move into
    pub target_strategy_index: u32,
    /// Strategy the vault would move out of
    pub current_strategy_index: u32,
    /// Observed value for the active strategy (bps)
    pub current_value: u32,
    /// Observed value for the target strategy (bps)
    pub target_value: u32,
    /// Threshold that was crossed (bps)
    pub threshold_bps: u32,
    /// Human-readable rationale, e.g. `yield 850 -> 1200 exceeds 200bps`
    pub rationale: String,
    /// Whether the trigger actually fired
    pub triggered: bool,
}

/// Result of a triggered rebalance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebalanceOutcome {
    /// Trigger that caused the rebalance
    pub trigger: RebalanceTrigger,
    /// Strategy index the vault moved from
    pub previous_strategy_index: u32,
    /// Strategy index the vault moved into
    pub new_strategy_index: u32,
    /// Assets moved between strategies
    pub assets_moved: u64,
}

