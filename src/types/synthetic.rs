//! Type definitions for Synthetic Asset Protocol

use soroban_sdk::{contracttype, Address, Map, Symbol, Vec};

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

/// Request to mint synthetic tokens against deposited collateral
#[derive(Clone, Debug)]
#[contracttype]
pub struct MintRequest {
    /// User minting the synthetic asset
    pub user: Address,
    /// Synthetic asset to mint
    pub asset_id: u32,
    /// Collateral token deposited
    pub collateral_token: Address,
    /// Amount of collateral deposited (token units)
    pub collateral_amount: u64,
    /// Amount of synthetic tokens requested (token units)
    pub synthetic_amount: u64,
}

/// Receipt emitted for a completed mint
#[derive(Clone, Debug)]
#[contracttype]
pub struct MintReceipt {
    /// User that minted
    pub user: Address,
    /// Synthetic asset minted
    pub asset_id: u32,
    /// Collateral token deposited
    pub collateral_token: Address,
    /// Collateral amount deposited
    pub collateral_amount: u64,
    /// Synthetic tokens credited (net of fee)
    pub synthetic_minted: u64,
    /// Minting fee charged (token units)
    pub minting_fee: u64,
    /// Collateral value in USD (6 decimals)
    pub collateral_value_usd: u64,
    /// Debt value in USD (6 decimals)
    pub debt_value_usd: u64,
    /// Resulting collateral ratio (basis points)
    pub collateral_ratio: u32,
    /// Oracle price used for the mint (6 decimals)
    pub price: u64,
    /// Timestamp of the mint
    pub timestamp: u64,
}

/// Request to redeem synthetic tokens for the underlying collateral
#[derive(Clone, Debug)]
#[contracttype]
pub struct RedemptionRequest {
    /// User redeeming
    pub user: Address,
    /// Synthetic asset to burn
    pub asset_id: u32,
    /// Amount of synthetic tokens to burn (token units)
    pub synthetic_amount: u64,
    /// Collateral token to receive
    pub collateral_token: Address,
}

/// Receipt emitted for a completed redemption
#[derive(Clone, Debug)]
#[contracttype]
pub struct RedemptionReceipt {
    /// User that redeemed
    pub user: Address,
    /// Synthetic asset burned
    pub asset_id: u32,
    /// Collateral token released
    pub collateral_token: Address,
    /// Synthetic tokens burned
    pub synthetic_burned: u64,
    /// Collateral released to the user (net of fee)
    pub collateral_released: u64,
    /// Redemption fee charged, denominated in collateral
    pub redemption_fee: u64,
    /// Oracle price used for the redemption (6 decimals)
    pub price: u64,
    /// Collateral ratio after redemption (basis points)
    pub collateral_ratio: u32,
    /// Timestamp of the redemption
    pub timestamp: u64,
}

/// Request to exchange one synthetic asset for another
#[derive(Clone, Debug)]
#[contracttype]
pub struct ExchangeRequest {
    /// User exchanging
    pub user: Address,
    /// Synthetic asset sold
    pub from_asset_id: u32,
    /// Synthetic asset bought
    pub to_asset_id: u32,
    /// Amount of the source synthetic to sell (token units)
    pub synthetic_amount: u64,
}

/// Receipt emitted for a completed exchange
#[derive(Clone, Debug)]
#[contracttype]
pub struct ExchangeReceipt {
    /// User that exchanged
    pub user: Address,
    /// Synthetic asset sold
    pub from_asset_id: u32,
    /// Synthetic asset bought
    pub to_asset_id: u32,
    /// Amount of source synthetic sold
    pub amount_in: u64,
    /// Amount of destination synthetic received (net of fee)
    pub amount_out: u64,
    /// Exchange fee charged, denominated in the destination synthetic
    pub exchange_fee: u64,
    /// Oracle price of the source synthetic (6 decimals)
    pub from_price: u64,
    /// Oracle price of the destination synthetic (6 decimals)
    pub to_price: u64,
    /// Number of hops executed
    pub hops: u32,
    /// Timestamp of the exchange
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
#[derive(Clone, Debug)]
#[contracttype]
pub enum AssetProposalType {
    /// List new synthetic asset
    ListAsset(SyntheticAsset),
    /// Update asset parameters: (asset_id, new_params)
    UpdateAsset(u32, AssetUpdateParams),
    /// Delist asset
    DelistAsset(u32),
    /// Update risk parameters
    UpdateRiskParams(RiskParameters),
    /// Emergency pause
    EmergencyPause(Symbol),
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
