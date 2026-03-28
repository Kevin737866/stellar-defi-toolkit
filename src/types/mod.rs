//! Type definitions for Stellar DeFi Toolkit

pub mod pool;
pub mod stablecoin;
pub mod synthetic;
pub mod token;
pub mod vault;

// Re-export commonly used types
pub use pool::{LiquidityPosition, PoolInfo, SwapParams};
pub use stablecoin::{
    AlertSeverity, ArbitrageOpportunity, CollateralInfo, CollateralType, FeeConfig,
    GovernanceProposal, LiquidationEvent, MintingEvent, OraclePrice, PriceDeviationAlert,
    ProposalType, RedemptionEvent, RiskParameters, StabilityPoolDepositEvent, StabilityPoolInfo,
    StabilityPoolWithdrawalEvent, SystemStats, TreasuryInfo, VaultPosition,
};
pub use synthetic::{
    AggregationParams, AlertType, AssetProposal, AssetProposalType, AssetType, AssetUpdateParams,
    BatchOperation, BatchOperationItem, BatchOperationType, BatchStatus, FeeDistribution,
    GovernanceParams, LiquidationEvent, MarketData, MultiSigRequirement, MultiSigRequirement,
    OracleInfo, OraclePrice, PositionAlert, PositionMetadata, PositionPerformance, PositionStatus,
    ProtocolStats, RiskParameters, StakingPosition, SyntheticAsset, SyntheticPosition, Vote,
};
pub use token::{TokenInfo, TokenMetadata};
pub use vault::{
    DepositResult, HarvestResult, PerformanceFeeConfig, StrategyType, VaultInfo, VaultStats,
    VaultStrategy, WithdrawResult,
};
