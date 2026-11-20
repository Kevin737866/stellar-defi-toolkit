//! Vault-related type definitions for the yield farming vault.
//!
//! All types are `#[contracttype]` so they can be stored in Soroban storage and
//! returned from contract entry points.

use soroban_sdk::{contracttype, Address, Symbol};

/// Strategy type classification
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
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
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct VaultStrategy {
    /// Human-readable strategy name
    pub name: Symbol,
    /// On-chain contract address of the strategy
    pub contract_address: Address,
    /// Strategy classification
    pub strategy_type: StrategyType,
    /// Estimated annual percentage yield, in basis points (1200 = 12%)
    pub estimated_apy_bps: u32,
    /// Amount currently allocated to this strategy
    pub allocated_amount: u64,
    /// Whether this strategy is currently active
    pub active: bool,
}

/// Kind of request held in the deposit/withdrawal queue
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum QueueKind {
    Deposit,
    Withdrawal,
}

/// A pending deposit or withdrawal larger than the configured queue threshold
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct QueueEntry {
    /// Monotonically increasing request identifier
    pub id: u64,
    /// Address that owns the request
    pub owner: Address,
    /// Whether this is a deposit or a withdrawal
    pub kind: QueueKind,
    /// Deposit: asset units to deposit. Withdrawal: shares to burn.
    pub amount: u64,
    /// Asset value of the request when it was created. Shares queued for
    /// withdrawal are priced at request time for display purposes only.
    pub requested_assets: u64,
    /// Timestamp the request was created
    pub created_at: u64,
    /// Earliest timestamp at which the request may be processed
    pub processable_at: u64,
}

/// Aggregate view of both queues
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct VaultQueueSummary {
    /// Number of pending deposits
    pub pending_deposits: u32,
    /// Number of pending withdrawals
    pub pending_withdrawals: u32,
    /// Total assets awaiting deposit
    pub queued_deposit_amount: u64,
    /// Total shares awaiting withdrawal
    pub queued_withdrawal_shares: u64,
    /// Timestamp of the next queue processing window
    pub next_processing_at: u64,
    /// Minimum interval between processing windows
    pub processing_interval: u64,
}

/// Result of a queue processing run
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct QueueProcessResult {
    /// Deposits executed in this run
    pub deposits_processed: u32,
    /// Withdrawals executed in this run
    pub withdrawals_processed: u32,
    /// Assets added to the vault by processed deposits
    pub deposited_amount: u64,
    /// Assets paid out by processed withdrawals
    pub withdrawn_amount: u64,
    /// Timestamp the run executed at
    pub processed_at: u64,
}

/// Snapshot of vault state
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct VaultInfo {
    /// Underlying asset token address
    pub asset_token: Address,
    /// Vault share token address (SEP-41)
    pub share_token: Address,
    /// Total shares outstanding
    pub total_shares: u64,
    /// Total assets under management
    pub total_assets: u64,
    /// Current share price in basis points (10000 = 1.0)
    pub share_price_bps: u32,
    /// Currently active strategy index, `None` when no strategy is registered
    pub active_strategy_index: Option<u32>,
    /// Total number of registered strategies
    pub strategy_count: u32,
    /// Performance fee in basis points
    pub performance_fee_bps: u32,
    /// Whether the vault is paused
    pub paused: bool,
    /// Timestamp of last harvest
    pub last_harvest: u64,
    /// Number of pending deposits
    pub pending_deposits: u32,
    /// Number of pending withdrawals
    pub pending_withdrawals: u32,
}

/// Vault runtime statistics
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct VaultStats {
    pub total_assets: u64,
    pub total_shares: u64,
    /// Current share price in basis points (10000 = 1.0)
    pub share_price_bps: u32,
    /// Active strategy APY in basis points
    pub current_apy_bps: u32,
    pub accumulated_fees: u64,
    pub last_harvest: u64,
    pub paused: bool,
    pub pending_deposits: u32,
    pub pending_withdrawals: u32,
}

/// Result of a deposit operation
///
/// When the deposit is queued, `amount_deposited` and `shares_minted` are `0`
/// and `queued` is `true`; the request can be tracked with `request_id`.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct DepositResult {
    pub depositor: Address,
    pub amount_deposited: u64,
    pub shares_minted: u64,
    pub share_price_bps: u32,
    /// Whether the deposit was placed in the queue instead of executed
    pub queued: bool,
    /// Queue request id when `queued`
    pub request_id: u64,
    /// 1-based position in the deposit queue when `queued`
    pub queue_position: u32,
    /// Estimated processing timestamp when `queued`
    pub estimated_processing_at: u64,
}

/// Result of a withdrawal operation
///
/// When the withdrawal is queued, `shares_burned` and `amount_withdrawn` are
/// `0` and `queued` is `true`.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct WithdrawResult {
    pub withdrawer: Address,
    pub shares_burned: u64,
    pub amount_withdrawn: u64,
    pub share_price_bps: u32,
    /// Whether the withdrawal was placed in the queue instead of executed
    pub queued: bool,
    /// Queue request id when `queued`
    pub request_id: u64,
    /// 1-based position in the withdrawal queue when `queued`
    pub queue_position: u32,
    /// Estimated processing timestamp when `queued`
    pub estimated_processing_at: u64,
}

/// Receipt for an emergency withdrawal
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct EmergencyWithdrawReceipt {
    /// Depositor that exited
    pub owner: Address,
    /// Shares burned to exit
    pub shares_burned: u64,
    /// Assets transferred to the depositor (after penalty)
    pub assets_withdrawn: u64,
    /// Penalty retained by the vault and redistributed to remaining depositors
    pub penalty_amount: u64,
    /// Penalty applied, in basis points
    pub penalty_bps: u32,
    /// Timestamp of the withdrawal
    pub timestamp: u64,
}

/// Receipt for a keeper-driven compounding run
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct CompoundReceipt {
    /// Keeper that triggered the compound call
    pub keeper: Address,
    /// Strategy indices that were compounded
    pub strategies: soroban_sdk::Vec<u32>,
    /// Total rewards harvested before any deductions
    pub gross_rewards: u64,
    /// Assets paid to the keeper for covering gas costs
    pub keeper_reward: u64,
    /// Vault shares minted to the keeper for `keeper_reward`
    pub keeper_reward_shares: u64,
    /// Performance fee deducted for the treasury
    pub performance_fee: u64,
    /// Rewards reinvested into the vault
    pub net_compounded: u64,
    /// Total assets after compounding
    pub new_total_assets: u64,
    /// Timestamp of the compound call
    pub timestamp: u64,
}

/// Result of a harvest operation
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
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
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PerformanceFeeConfig {
    /// Fee in basis points (e.g., 1000 = 10%)
    pub fee_bps: u32,
    /// Treasury address that receives fees
    pub treasury: Address,
}

/// Keys for per-owner persistent entries
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum VaultDataKey {
    /// Share balance owned by an address
    Shares(Address),
}

/// Events emitted by vault state-changing operations
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum VaultEvent {
    /// Deposited: owner, assets, shares minted
    DepositExecuted(Address, u64, u64),
    /// Queued: owner, assets, request id
    DepositQueued(Address, u64, u64),
    /// Withdrawn: owner, shares burned, assets returned
    WithdrawExecuted(Address, u64, u64),
    /// Queued: owner, shares, request id
    WithdrawQueued(Address, u64, u64),
    /// Cancelled: owner, request id
    QueueRequestCancelled(Address, u64),
    /// Processed: deposits executed, withdrawals executed
    QueueProcessed(u32, u32),
    /// Emergency exit: owner, shares burned, assets paid, penalty, penalty bps
    EmergencyWithdrawn(Address, u64, u64, u64, u32),
    /// Compounded: keeper, gross rewards, keeper reward, net compounded
    Compounded(Address, u64, u64, u64),
    /// Pause state changed
    PauseChanged(bool),
}
