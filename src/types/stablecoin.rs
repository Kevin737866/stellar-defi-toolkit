//! Type definitions for the Stablecoin contract

use soroban_sdk::{Address, Map, Symbol, Vec};

/// Collateral type enumeration
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CollateralType {
    /// Native Stellar token (XLM)
    XLM,
    /// Wrapped assets (e.g., wBTC, wETH)
    Wrapped,
    /// Stablecoin collateral (e.g., USDC, DAI)
    Stablecoin,
    /// LP tokens from decentralized exchanges
    LiquidityPool,
    /// Synthetic assets
    Synthetic,
    /// Other custom collateral types
    Other(Symbol),
}

/// Collateral information and configuration
#[derive(Clone, Debug)]
#[contracttype]
pub struct CollateralInfo {
    /// Type of collateral
    pub collateral_type: CollateralType,
    /// Minimum collateral ratio in basis points (e.g., 15000 = 150%)
    pub min_collateral_ratio: u64,
    /// Maximum collateral ratio in basis points
    pub max_collateral_ratio: u64,
    /// Whether this collateral is currently enabled
    pub enabled: bool,
    /// When this collateral was added
    pub added_at: u64,
}

/// Vault position representing user's collateral and debt
#[derive(Clone, Debug)]
#[contracttype]
pub struct VaultPosition {
    /// Owner of the vault
    pub owner: Address,
    /// Collateral deposits by token address
    pub collateral_deposits: Map<Address, u64>,
    /// Amount of stablecoin debt
    pub debt_amount: u64,
    /// Last update timestamp
    pub last_update: u64,
}

/// Stability pool information
#[derive(Clone, Debug)]
#[contracttype]
pub struct StabilityPoolInfo {
    /// Total stablecoins deposited in stability pool
    pub total_deposits: u64,
    /// Reward per share accumulator
    pub reward_per_share: u64,
    /// Last reward update timestamp
    pub last_update: u64,
}

/// Oracle price feed data
#[derive(Clone, Debug)]
#[contracttype]
pub struct OraclePrice {
    /// Address of the asset
    pub asset_address: Address,
    /// Price in USD (with decimals)
    pub price: u64,
    /// Number of decimals in the price
    pub decimals: u32,
    /// Last update timestamp
    pub last_update: u64,
}

/// Liquidation event data
#[derive(Clone, Debug)]
#[contracttype]
pub struct LiquidationEvent {
    /// Address of the liquidated vault
    pub vault_owner: Address,
    /// Address of the liquidator
    pub liquidator: Address,
    /// Collateral token address
    pub collateral_address: Address,
    /// Amount of collateral liquidated
    pub collateral_amount: u64,
    /// Amount of stablecoin debt repaid
    pub debt_repaid: u64,
    /// Liquidation penalty applied
    pub penalty_amount: u64,
}

/// Minting event data
#[derive(Clone, Debug)]
#[contracttype]
pub struct MintingEvent {
    /// Address receiving the minted stablecoins
    pub recipient: Address,
    /// Collateral token address
    pub collateral_address: Address,
    /// Amount of collateral deposited
    pub collateral_amount: u64,
    /// Amount of stablecoins minted
    pub stablecoin_amount: u64,
    /// Minting fee charged
    pub minting_fee: u64,
    /// Collateral ratio after minting
    pub collateral_ratio: u64,
}

/// Redemption event data
#[derive(Clone, Debug)]
#[contracttype]
pub struct RedemptionEvent {
    /// Address redeeming stablecoins
    pub redeemer: Address,
    /// Collateral token address
    pub collateral_address: Address,
    /// Amount of stablecoins burned
    pub stablecoin_amount: u64,
    /// Amount of collateral withdrawn
    pub collateral_amount: u64,
    /// Redemption fee charged
    pub redemption_fee: u64,
    /// Collateral ratio after redemption
    pub collateral_ratio: u64,
}

/// Stability pool deposit event
#[derive(Clone, Debug)]
#[contracttype]
pub struct StabilityPoolDepositEvent {
    /// Address making the deposit
    pub depositor: Address,
    /// Amount deposited
    pub amount: u64,
    /// Total pool size after deposit
    pub new_total: u64,
    /// Reward per share at time of deposit
    pub reward_per_share: u64,
}

/// Stability pool withdrawal event
#[derive(Clone, Debug)]
#[contracttype]
pub struct StabilityPoolWithdrawalEvent {
    /// Address making the withdrawal
    pub withdrawer: Address,
    /// Amount withdrawn
    pub amount: u64,
    /// Rewards earned
    pub rewards_earned: u64,
    /// Total pool size after withdrawal
    pub new_total: u64,
}

/// Governance proposal for parameter changes
#[derive(Clone, Debug)]
#[contracttype]
pub struct GovernanceProposal {
    /// Unique proposal ID
    pub proposal_id: u64,
    /// Address that created the proposal
    pub proposer: Address,
    /// Type of proposal
    pub proposal_type: ProposalType,
    /// Description of the proposal
    pub description: Symbol,
    /// When the proposal was created
    pub created_at: u64,
    /// Voting deadline
    pub voting_deadline: u64,
    /// Votes in favor
    pub votes_for: u64,
    /// Votes against
    pub votes_against: u64,
    /// Whether the proposal has been executed
    pub executed: bool,
}

/// Types of governance proposals
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum ProposalType {
    /// Update collateral parameters
    UpdateCollateralParameters {
        collateral_address: Address,
        min_ratio: u64,
        max_ratio: u64,
    },
    /// Update fee parameters
    UpdateFees {
        minting_fee_bps: u32,
        redemption_fee_bps: u32,
    },
    /// Add new collateral type
    AddCollateral {
        collateral_address: Address,
        collateral_type: CollateralType,
        min_ratio: u64,
        max_ratio: u64,
    },
    /// Remove collateral type
    RemoveCollateral {
        collateral_address: Address,
    },
    /// Update oracle address
    UpdateOracle {
        new_oracle: Address,
    },
    /// Emergency shutdown
    EmergencyShutdown,
    /// Other custom proposal
    Custom(Symbol),
}

/// Arbitrage opportunity data
#[derive(Clone, Debug)]
#[contracttype]
pub struct ArbitrageOpportunity {
    /// Unique opportunity ID
    pub opportunity_id: u64,
    /// Source token address
    pub source_token: Address,
    /// Target token address
    pub target_token: Address,
    /// Price difference in basis points
    pub price_diff_bps: u32,
    /// Potential profit amount
    pub potential_profit: u64,
    /// Required capital
    pub required_capital: u64,
    /// When this opportunity was discovered
    pub discovered_at: u64,
    /// Expiration time
    pub expires_at: u64,
    /// Whether this opportunity is still valid
    pub valid: bool,
}

/// Estimated MEV cost for an arbitrage opportunity
///
/// Represents the expected cost of MEV extraction (sandwich attacks,
/// frontrunning) that must be subtracted from gross profit to determine
/// net expected profit.
#[derive(Clone, Debug)]
#[contracttype]
pub struct MEVCost {
    /// Estimated sandwich attack cost in base units
    pub sandwich_cost: u64,
    /// Estimated frontrunning cost in base units
    pub frontrun_cost: u64,
    /// Total MEV cost (sum of sandwich + frontrun)
    pub total_mev_cost: u64,
    /// MEV buffer percentage (basis points) applied on top of estimated cost
    pub buffer_bps: u32,
    /// Gas cost included in MEV calculation
    pub gas_cost: u64,
    /// Block number this estimate was derived from
    pub estimated_at_block: u64,
}

/// MEV risk level for market monitoring
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum MEVRiskLevel {
    /// Low MEV activity, safe to execute
    Low,
    /// Moderate MEV activity, execute with caution
    Medium,
    /// High MEV activity, significant risk of extraction
    High,
    /// Extreme MEV activity, recommend postponing
    Critical,
}

/// MEV event for monitoring market conditions
#[derive(Clone, Debug)]
#[contracttype]
pub struct MEVEvent {
    /// Type of MEV activity detected
    pub event_type: Symbol,
    /// Estimated cost in base units
    pub cost_estimate: u64,
    /// Risk level
    pub risk_level: MEVRiskLevel,
    /// Block number
    pub block_number: u64,
    /// Timestamp
    pub timestamp: u64,
}

/// Arbitrage simulation result (read-only, no state changes)
#[derive(Clone, Debug)]
#[contracttype]
pub struct ArbitrageSimulation {
    /// Whether the simulation succeeded
    pub success: bool,
    /// Expected gross profit before fees
    pub gross_profit: u64,
    /// Pool fees across all hops
    pub pool_fees: u64,
    /// Gas cost for execution
    pub gas_cost: u64,
    /// Protocol fees
    pub protocol_fees: u64,
    /// Estimated slippage for the trade size
    pub slippage_estimate: u64,
    /// Estimated MEV cost
    pub mev_cost: u64,
    /// Net profit after all deductions
    pub net_profit: i128,
    /// Route description
    pub route_summary: Symbol,
    /// Simulation timestamp
    pub simulated_at: u64,
}

/// Flash bundle for atomic multi-hop arbitrage execution
#[derive(Clone, Debug)]
#[contracttype]
pub struct FlashBundle {
    /// Unique bundle ID
    pub bundle_id: u64,
    /// Flash loan amount (principal)
    pub loan_amount: u64,
    /// Flash loan fee
    pub loan_fee: u64,
    /// List of swap hops in execution order
    pub hops: Vec<FlashBundleHop>,
    /// Expected net profit after all costs
    pub expected_profit: i128,
    /// Maximum acceptable gas cost
    pub max_gas_cost: u64,
    /// Whether execution was successful (atomic)
    pub executed: bool,
    /// Execution timestamp
    pub executed_at: u64,
}

/// A single swap hop within a flash bundle
#[derive(Clone, Debug)]
#[contracttype]
pub struct FlashBundleHop {
    /// Pool address for this swap
    pub pool: Address,
    /// Token being swapped in
    pub token_in: Address,
    /// Token being swapped out
    pub token_out: Address,
    /// Amount of token_in
    pub amount_in: u64,
    /// Minimum amount of token_out (slippage protection)
    pub min_amount_out: u64,
    /// Pool fee in basis points
    pub fee_bps: u32,
}

/// System statistics for monitoring
#[derive(Clone, Debug)]
#[contracttype]
pub struct SystemStats {
    /// Total value locked in USD
    pub total_value_locked: u64,
    /// Total stablecoin supply
    pub total_supply: u64,
    /// Number of active vaults
    pub active_vaults: u32,
    /// Average collateral ratio across all vaults
    pub average_collateral_ratio: u64,
    /// Stability pool size
    pub stability_pool_size: u64,
    /// Total liquidations in the last 24 hours
    pub daily_liquidations: u32,
    /// Total minting volume in the last 24 hours
    pub daily_minting_volume: u64,
    /// Total redemption volume in the last 24 hours
    pub daily_redemption_volume: u64,
    /// Current system health score (0-10000)
    pub health_score: u32,
}

/// Risk parameters for the system
#[derive(Clone, Debug)]
#[contracttype]
pub struct RiskParameters {
    /// Global minimum collateral ratio
    pub global_min_ratio: u64,
    /// Maximum total debt system can support
    pub max_total_debt: u64,
    /// Maximum percentage of any single collateral type
    pub max_collateral_concentration: u32,
    /// Liquidation threshold for system-wide alerts
    pub system_liquidation_threshold: u32,
    /// Emergency pause threshold
    pub emergency_pause_threshold: u32,
    /// Minimum reserve ratio for stability pool
    pub min_stability_reserve_ratio: u32,
}

/// Fee configuration
#[derive(Clone, Debug)]
#[contracttype]
pub struct FeeConfig {
    /// Minting fee in basis points
    pub minting_fee_bps: u32,
    /// Redemption fee in basis points
    pub redemption_fee_bps: u32,
    /// Liquidation penalty in basis points
    pub liquidation_penalty_bps: u32,
    /// Stability pool reward rate in basis points
    pub stability_reward_rate_bps: u32,
    /// Governance fee (portion of liquidation penalties)
    pub governance_fee_bps: u32,
}

/// Price deviation alert
#[derive(Clone, Debug)]
#[contracttype]
pub struct PriceDeviationAlert {
    /// Token address with price deviation
    pub token_address: Address,
    /// Expected price
    pub expected_price: u64,
    /// Actual price
    pub actual_price: u64,
    /// Deviation percentage in basis points
    pub deviation_bps: u32,
    /// When the alert was triggered
    pub triggered_at: u64,
    /// Alert severity level
    pub severity: AlertSeverity,
}

/// Alert severity levels
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum AlertSeverity {
    /// Low severity - informational
    Low,
    /// Medium severity - requires attention
    Medium,
    /// High severity - requires immediate action
    High,
    /// Critical severity - system at risk
    Critical,
}

/// Treasury information
#[derive(Clone, Debug)]
#[contracttype]
pub struct TreasuryInfo {
    /// Treasury address
    pub treasury_address: Address,
    /// Total fees collected
    pub total_fees_collected: u64,
    /// Fees available for withdrawal
    pub available_fees: u64,
    /// Last fee collection timestamp
    pub last_fee_collection: u64,
    /// Treasury balance by token
    pub token_balances: Map<Address, u64>,
}
