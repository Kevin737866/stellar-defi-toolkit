//! Type definitions for Synthetic Asset Protocol

use soroban_sdk::{Address, Map, Symbol, Vec};

/// Synthetic asset information and configuration
#[derive(Clone, Debug)]
#[contracttype]
pub struct SyntheticAsset {
    /// Unique identifier for the synthetic asset
    pub asset_id: u32,
    /// Symbol of the synthetic asset (e.g., "sAAPL", "sBTC")
    pub symbol: Symbol,
    /// Name of the synthetic asset
    pub name: Symbol,
    /// Description of the underlying real-world asset
    pub description: Symbol,
    /// Type of underlying asset
    pub asset_type: AssetType,
    /// Oracle providing price feeds for this asset
    pub oracle_address: Address,
    /// Minimum collateralization ratio (basis points)
    pub min_collateral_ratio: u32,
    /// Maximum collateralization ratio (basis points)
    pub max_collateral_ratio: u32,
    /// Minting fee in basis points
    pub minting_fee_bps: u32,
    /// Whether this asset is currently active
    pub active: bool,
    /// When this asset was listed
    pub listed_at: u64,
    /// Total supply of synthetic tokens
    pub total_supply: u64,
    /// Total collateral locked for this asset
    pub total_collateral: u64,
}

/// Types of underlying assets
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum AssetType {
    /// Stock/Equity
    Stock,
    /// Cryptocurrency
    Crypto,
    /// Commodity (gold, oil, etc.)
    Commodity,
    /// Forex pair
    Forex,
    /// Index
    Index,
    /// Other custom type
    Other(Symbol),
}

/// User position in a synthetic asset
#[derive(Clone, Debug)]
#[contracttype]
pub struct SyntheticPosition {
    /// Owner of the position
    pub owner: Address,
    /// Asset ID
    pub asset_id: u32,
    /// Amount of synthetic tokens minted
    pub synthetic_amount: u64,
    /// Collateral locked (by token address)
    pub collateral_deposits: Map<Address, u64>,
    /// Debt amount in USD value
    pub debt_amount: u64,
    /// Collateralization ratio
    pub collateral_ratio: u32,
    /// When position was created
    pub created_at: u64,
    /// Last update timestamp
    pub last_updated: u64,
    /// Whether position is being liquidated
    pub liquidating: bool,
}

/// Liquidation event data
#[derive(Clone, Debug)]
#[contracttype]
pub struct LiquidationEvent {
    /// Unique event ID
    pub event_id: u64,
    /// Position owner
    pub position_owner: Address,
    /// Asset ID being liquidated
    pub asset_id: u32,
    /// Liquidator address
    pub liquidator: Address,
    /// Amount of synthetic tokens burned
    pub synthetic_amount_burned: u64,
    /// Collateral distributed to liquidator
    pub collateral_to_liquidator: Map<Address, u64>,
    /// Collateral returned to position owner
    pub collateral_returned: Map<Address, u64>,
    /// Liquidation penalty in basis points
    pub penalty_bps: u32,
    /// When liquidation occurred
    pub timestamp: u64,
}

/// Oracle price feed data
#[derive(Clone, Debug)]
#[contracttype]
pub struct OraclePrice {
    /// Asset ID this price is for
    pub asset_id: u32,
    /// Price in USD (with decimals)
    pub price: u64,
    /// Number of decimals in price
    pub decimals: u32,
    /// Confidence score (0-10000)
    pub confidence: u32,
    /// When price was last updated
    pub timestamp: u64,
    /// Oracle source address
    pub source_address: Address,
}

/// Fee distribution data
#[derive(Clone, Debug)]
#[contracttype]
pub struct FeeDistribution {
    /// Total fees collected
    pub total_fees: u64,
    /// Fees distributed to stakers
    pub fees_to_stakers: u64,
    /// Fees to treasury
    pub fees_to_treasury: u64,
    /// Last distribution timestamp
    pub last_distribution: u64,
    /// Staking pool share price
    pub staking_share_price: u64,
}

/// Risk parameters for the protocol
#[derive(Clone, Debug)]
#[contracttype]
pub struct RiskParameters {
    /// Global minimum collateral ratio
    pub global_min_ratio: u32,
    /// Maximum debt per user
    pub max_debt_per_user: u64,
    /// Maximum total protocol debt
    pub max_total_debt: u64,
    /// Liquidation threshold
    pub liquidation_threshold: u32,
    /// Emergency pause threshold
    pub emergency_pause_threshold: u32,
    /// Minimum oracle confidence
    pub min_oracle_confidence: u32,
}

/// Protocol statistics
#[derive(Clone, Debug)]
#[contracttype]
pub struct ProtocolStats {
    /// Total value locked (USD)
    pub total_value_locked: u64,
    /// Total synthetic supply (USD equivalent)
    pub total_synthetic_supply: u64,
    /// Number of active positions
    pub active_positions: u32,
    /// Average collateral ratio
    pub avg_collateral_ratio: u32,
    /// Total fees collected
    pub total_fees_collected: u64,
    /// Number of liquidations (24h)
    pub daily_liquidations: u32,
    /// Protocol health score (0-10000)
    pub health_score: u32,
}

/// Governance proposal for synthetic assets
#[derive(Clone, Debug)]
#[contracttype]
pub struct AssetProposal {
    /// Unique proposal ID
    pub proposal_id: u64,
    /// Proposer address
    pub proposer: Address,
    /// Type of proposal
    pub proposal_type: AssetProposalType,
    /// Asset ID (if applicable)
    pub asset_id: Option<u32>,
    /// Proposal details
    pub details: Symbol,
    /// When created
    pub created_at: u64,
    /// Voting deadline
    pub voting_deadline: u64,
    /// Votes in favor
    pub votes_for: u64,
    /// Votes against
    pub votes_against: u64,
    /// Whether executed
    pub executed: bool,
}

/// Types of asset governance proposals
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum AssetProposalType {
    /// List new synthetic asset
    ListAsset {
        asset: SyntheticAsset,
    },
    /// Update asset parameters
    UpdateAsset {
        asset_id: u32,
        new_params: AssetUpdateParams,
    },
    /// Delist asset
    DelistAsset {
        asset_id: u32,
    },
    /// Update risk parameters
    UpdateRiskParams {
        new_params: RiskParameters,
    },
    /// Emergency pause
    EmergencyPause {
        reason: Symbol,
    },
}

/// Asset update parameters
#[derive(Clone, Debug)]
#[contracttype]
pub struct AssetUpdateParams {
    /// New minimum collateral ratio
    pub min_collateral_ratio: Option<u32>,
    /// New maximum collateral ratio
    pub max_collateral_ratio: Option<u32>,
    /// New minting fee
    pub minting_fee_bps: Option<u32>,
    /// New oracle address
    pub oracle_address: Option<Address>,
}

/// Staking position for fee sharing
#[derive(Clone, Debug)]
#[contracttype]
pub struct StakingPosition {
    /// Staker address
    pub staker: Address,
    /// Amount staked
    pub staked_amount: u64,
    /// Reward accumulator
    pub reward_index: u64,
    /// When staking started
    pub staked_at: u64,
    /// Last claim timestamp
    pub last_claim: u64,
    /// Total rewards claimed
    pub total_rewards_claimed: u64,
}

/// Market data for an asset
#[derive(Clone, Debug)]
#[contracttype]
pub struct MarketData {
    /// Asset ID
    pub asset_id: u32,
    /// 24h trading volume
    pub volume_24h: u64,
    /// 24h price change (basis points)
    pub price_change_24h: i32,
    /// Current price
    pub current_price: u64,
    /// 24h high price
    pub high_24h: u64,
    /// 24h low price
    pub low_24h: u64,
    /// Market cap
    pub market_cap: u64,
    /// Last updated
    pub last_updated: u64,
}
