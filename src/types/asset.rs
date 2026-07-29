//! Comprehensive Stellar Asset Type Definitions
//!
//! This module provides type definitions for a wide range of Stellar assets,
//! including native assets, custom tokens, stablecoins, wrapped assets, and more.
//!
//! ## Issue #215 – Allowlist / Blocklist
//! [`RegistryListEntry`] and [`ListChangeEvent`] support allow/block management.
//!
//! ## Issue #216 – Asset Metadata Registry
//! [`ProtocolAssetMetadata`] stores name, symbol, decimals, token standard and
//! contract address.  [`TokenStandard`] enumerates the supported standards.
//!
//! ## Issue #217 – Risk Parameters
//! [`AssetRiskParams`] holds LTV, liquidation threshold, liquidation bonus and
//! oracle source per collateral asset.
//!
//! ## Issue #218 – Asset Migration
//! [`MigrationState`], [`MigrationStatus`] and [`MigrationEvent`] define the
//! upgrade/migration path for token contracts.

use soroban_sdk::{Address, Symbol, Map, Vec};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ─── Issue #215 – Allowlist / Blocklist ──────────────────────────────────────

/// Describes why an asset was added to (or removed from) a registry list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ListChangeReason {
    /// Added through normal governance approval.
    GovernanceApproval,
    /// Removed after governance vote.
    GovernanceRemoval,
    /// Emergency block by an admin (e.g. exploit risk).
    EmergencyBlock,
    /// Administrative action without a full governance vote.
    AdminAction,
    /// Free-form reason provided at registration time.
    Custom(String),
}

/// A single entry in either the allowlist or the blocklist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryListEntry {
    /// Canonical asset identifier (contract address string or ticker).
    pub asset_id: String,
    /// The admin / governance address that made the change.
    pub changed_by: String,
    /// Human-readable reason.
    pub reason: ListChangeReason,
    /// Ledger / unix timestamp at which the entry was recorded.
    pub recorded_at: u64,
    /// Whether the entry is currently active (can be soft-deleted).
    pub active: bool,
}

/// Events emitted when the allowlist or blocklist changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ListChangeEvent {
    /// Asset was added to the allowlist.
    AllowlistAdded {
        asset_id: String,
        changed_by: String,
        reason: ListChangeReason,
    },
    /// Asset was removed from the allowlist.
    AllowlistRemoved {
        asset_id: String,
        changed_by: String,
    },
    /// Asset was added to the blocklist.
    BlocklistAdded {
        asset_id: String,
        changed_by: String,
        reason: ListChangeReason,
    },
    /// Asset was removed from the blocklist.
    BlocklistRemoved {
        asset_id: String,
        changed_by: String,
    },
}

// ─── Issue #216 – Asset Metadata & Token Standards ───────────────────────────

/// Recognised token standards on Stellar / Soroban.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TokenStandard {
    /// Stellar native asset (XLM).
    StellarNative,
    /// SEP-41 fungible token interface.
    Sep41,
    /// Classic Stellar asset (SEP-0011 / TOML-described).
    ClassicStellarAsset,
    /// Wrapped asset bridged from another chain.
    Wrapped,
    /// Custom / unknown standard.
    Custom(String),
}

impl std::fmt::Display for TokenStandard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenStandard::StellarNative => write!(f, "StellarNative"),
            TokenStandard::Sep41 => write!(f, "SEP-41"),
            TokenStandard::ClassicStellarAsset => write!(f, "ClassicStellarAsset"),
            TokenStandard::Wrapped => write!(f, "Wrapped"),
            TokenStandard::Custom(s) => write!(f, "Custom({})", s),
        }
    }
}

/// Full metadata for a protocol-registered asset (issue #216).
///
/// Queryable by any contract via [`AssetRegistry::get_asset_metadata`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolAssetMetadata {
    /// Canonical asset identifier (contract address string or ticker).
    pub asset_id: String,
    /// Human-readable asset name (e.g. "USD Coin").
    pub name: String,
    /// Ticker symbol (e.g. "USDC").
    pub symbol: String,
    /// Number of decimal places (e.g. 7 for Stellar-native, 6 for USDC).
    pub decimals: u8,
    /// On-chain contract address (empty string for native XLM).
    pub contract_address: String,
    /// Token standard this asset conforms to.
    pub standard: TokenStandard,
    /// Whether the metadata record is currently valid / active.
    pub active: bool,
    /// Ledger timestamp when this metadata was first registered.
    pub registered_at: u64,
    /// Ledger timestamp of the most recent admin metadata update.
    pub last_updated_at: u64,
}

// ─── Issue #217 – Risk Parameters ────────────────────────────────────────────

/// Risk parameters for an asset used as collateral (issue #217).
///
/// All ratio fields are expressed in *basis points* (1 bps = 0.01 %).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetRiskParams {
    /// Canonical asset identifier.
    pub asset_id: String,
    /// Loan-to-value ratio in bps (e.g. 7500 = 75 %).
    /// The maximum a borrower can draw against this collateral.
    pub ltv_bps: u32,
    /// Liquidation threshold in bps (e.g. 8000 = 80 %).
    /// Below this health-factor the position becomes liquidatable.
    pub liquidation_threshold_bps: u32,
    /// Liquidation bonus in bps (e.g. 500 = 5 %).
    /// Extra collateral reward paid to the liquidator.
    pub liquidation_bonus_bps: u32,
    /// Identifier of the oracle / price-source assigned to this asset.
    pub oracle_source: String,
    /// Ledger timestamp when these parameters were last set.
    pub last_updated_at: u64,
}

impl AssetRiskParams {
    /// Basic invariant: LTV ≤ liquidation threshold.
    pub fn is_valid(&self) -> bool {
        self.ltv_bps <= self.liquidation_threshold_bps
            && self.liquidation_threshold_bps <= 10_000
            && self.liquidation_bonus_bps <= 5_000
    }
}

// ─── Issue #218 – Migration / Upgrade Path ────────────────────────────────────

/// Current phase of a token-contract migration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MigrationStatus {
    /// Migration has been requested but not yet started.
    Pending,
    /// Balances are being migrated.
    InProgress,
    /// All state has been transferred; old contract is paused.
    Completed,
    /// Migration was cancelled before completion.
    Cancelled,
}

/// Tracks the full state of a token-contract upgrade/migration (issue #218).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationState {
    /// Asset being migrated.
    pub asset_id: String,
    /// Address of the old (source) contract.
    pub old_contract: String,
    /// Address of the new (destination) contract.
    pub new_contract: String,
    /// Snapshot of all balances at migration initiation: address → amount.
    pub balance_snapshot: HashMap<String, u64>,
    /// Snapshot of all allowances: owner → (spender → amount).
    pub allowance_snapshot: HashMap<String, HashMap<String, u64>>,
    /// Total supply at the time the migration was initiated.
    pub total_supply_snapshot: u64,
    /// Current migration phase.
    pub status: MigrationStatus,
    /// Address that initiated the migration.
    pub initiated_by: String,
    /// Ledger timestamp when migration was initiated.
    pub initiated_at: u64,
    /// Ledger timestamp when migration completed (0 if not yet done).
    pub completed_at: u64,
    /// Whether the old contract has been paused post-migration.
    pub old_contract_paused: bool,
}

/// Audit-trail events emitted during a migration (issue #218).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MigrationEvent {
    /// Migration was initiated.
    Initiated {
        asset_id: String,
        old_contract: String,
        new_contract: String,
        initiated_by: String,
    },
    /// Balances were successfully transferred to the new contract.
    BalancesMigrated {
        asset_id: String,
        accounts_migrated: u64,
        total_supply: u64,
    },
    /// Allowances were successfully transferred to the new contract.
    AllowancesMigrated {
        asset_id: String,
        allowances_migrated: u64,
    },
    /// The old contract was paused to prevent further operations.
    OldContractPaused {
        asset_id: String,
        old_contract: String,
    },
    /// Migration completed successfully.
    Completed {
        asset_id: String,
        old_contract: String,
        new_contract: String,
    },
    /// Migration was cancelled (old contract remains active).
    Cancelled {
        asset_id: String,
        cancelled_by: String,
    },
}

/// Stellar asset identifier - can be native XLM or a custom token
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum StellarAssetId {
    /// Native Stellar Lumens (XLM)
    Native,
    /// Custom token with issuer address and asset code
    Token {
        code: Symbol,
        issuer: Address,
    },
    /// Soroban smart contract token
    ContractToken {
        contract_address: Address,
    },
}

/// Asset category for classification and price feed routing
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum AssetCategory {
    /// Native Stellar asset (XLM)
    Native,
    /// Major cryptocurrencies (BTC, ETH, etc.)
    Cryptocurrency,
    /// Stablecoins (USDC, USDT, DAI, etc.)
    Stablecoin,
    /// Wrapped assets from other chains (wBTC, wETH, etc.)
    Wrapped,
    /// DeFi tokens (UNI, AAVE, COMP, etc.)
    DeFiToken,
    /// Liquidity pool tokens
    LiquidityPool,
    /// Synthetic assets
    Synthetic,
    /// Real-world assets (tokenized stocks, bonds, etc.)
    RealWorldAsset,
    /// Commodities (gold, oil, etc.)
    Commodity,
    /// Forex pairs
    Forex,
    /// NFTs and collectibles
    NFT,
    /// Governance tokens
    Governance,
    /// Utility tokens
    Utility,
    /// Other custom category
    Other(Symbol),
}

/// Asset metadata for price feed configuration
#[derive(Clone, Debug)]
#[contracttype]
pub struct AssetMetadata {
    /// Unique asset identifier
    pub asset_id: StellarAssetId,
    /// Asset symbol (e.g., "XLM", "BTC", "USDC")
    pub symbol: Symbol,
    /// Asset name
    pub name: Symbol,
    /// Asset category
    pub category: AssetCategory,
    /// Number of decimals for the asset
    pub decimals: u32,
    /// Whether the asset is currently active for price feeds
    pub active: bool,
    /// Minimum price update interval (seconds)
    pub min_update_interval: u64,
    /// Maximum price deviation allowed (basis points)
    pub max_price_deviation: u32,
    /// Required confidence threshold (basis points)
    pub min_confidence: u32,
    /// List of approved price feed sources
    pub approved_sources: Vec<Address>,
    /// When this asset was registered
    pub registered_at: u64,
    /// Last price update timestamp
    pub last_price_update: u64,
    /// Additional custom metadata
    pub custom_metadata: Map<Symbol, Symbol>,
}

/// Price feed configuration for a specific asset
#[derive(Clone, Debug)]
#[contracttype]
pub struct PriceFeedConfig {
    /// Asset this configuration applies to
    pub asset_id: StellarAssetId,
    /// Preferred aggregation method
    pub aggregation_method: AggregationMethod,
    /// Minimum number of sources required
    pub min_sources: u32,
    /// Maximum age of price data (seconds)
    pub max_price_age: u64,
    /// Circuit breaker threshold (basis points)
    pub circuit_breaker_threshold: u32,
    /// Whether to use TWAP
    pub use_twap: bool,
    /// TWAP period (seconds)
    pub twap_period: u64,
    /// Heartbeat interval (seconds)
    pub heartbeat_interval: u64,
}

/// Price aggregation methods
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum AggregationMethod {
    /// Simple average of all sources
    SimpleAverage,
    /// Weighted average based on source reputation
    WeightedAverage,
    /// Median price
    Median,
    /// Time-weighted average price
    TimeWeightedAverage,
    /// Confidence-weighted average
    ConfidenceWeighted,
}

/// Asset price data with metadata
#[derive(Clone, Debug)]
#[contracttype]
pub struct AssetPrice {
    /// Asset identifier
    pub asset_id: StellarAssetId,
    /// Current price in USD (scaled by decimals)
    pub price: u64,
    /// Number of decimals in price
    pub decimals: u32,
    /// Price confidence score (0-10000)
    pub confidence: u32,
    /// Timestamp of price
    pub timestamp: u64,
    /// Source of this price
    pub source: Address,
    /// 24h price change (basis points)
    pub price_change_24h: i32,
    /// 24h high price
    pub high_24h: u64,
    /// 24h low price
    pub low_24h: u64,
    /// 24h volume
    pub volume_24h: u64,
}

/// Price source information
#[derive(Clone, Debug)]
#[contracttype]
pub struct PriceSource {
    /// Source address
    pub address: Address,
    /// Source name
    pub name: Symbol,
    /// Source type
    pub source_type: PriceSourceType,
    /// Weight in aggregation (basis points)
    pub weight: u32,
    /// Reputation score (0-10000)
    pub reputation: u32,
    /// Whether source is active
    pub active: bool,
    /// Supported asset categories
    pub supported_categories: Vec<AssetCategory>,
    /// Last successful update
    pub last_update: u64,
    /// Number of successful updates
    pub successful_updates: u64,
    /// Number of failed updates
    pub failed_updates: u64,
}

/// Types of price feed sources
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum PriceSourceType {
    /// On-chain oracle
    OnChainOracle,
    /// Off-chain API
    OffChainAPI,
    /// DEX price feed
    DEXPriceFeed,
    /// AMM price calculation
    AMMCalculation,
    /// Chainlink price feed
    Chainlink,
    /// Band Protocol oracle
    BandProtocol,
    /// Pyth Network
    PythNetwork,
    /// Custom source
    Custom(Symbol),
}

/// Asset registry entry
#[derive(Clone, Debug)]
#[contracttype]
pub struct AssetRegistryEntry {
    /// Asset metadata
    pub metadata: AssetMetadata,
    /// Price feed configuration
    pub price_config: PriceFeedConfig,
    /// Current price data
    pub current_price: Option<AssetPrice>,
    /// Price history (last N entries)
    pub price_history: Vec<AssetPrice>,
}

/// Price deviation alert
#[derive(Clone, Debug)]
#[contracttype]
pub struct PriceDeviationAlert {
    /// Asset identifier
    pub asset_id: StellarAssetId,
    /// Expected price
    pub expected_price: u64,
    /// Reported price
    pub reported_price: u64,
    /// Deviation in basis points
    pub deviation_bps: u32,
    /// Source reporting deviation
    pub source: Address,
    /// Alert severity
    pub severity: AlertSeverity,
    /// Timestamp
    pub timestamp: u64,
}

/// Alert severity levels
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum AlertSeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// Asset statistics
#[derive(Clone, Debug)]
#[contracttype]
pub struct AssetStats {
    /// Asset identifier
    pub asset_id: StellarAssetId,
    /// Total number of price updates
    pub total_updates: u64,
    /// Average update interval (seconds)
    pub avg_update_interval: u64,
    /// Number of price deviation alerts
    pub deviation_alerts: u64,
    /// Current confidence score
    pub current_confidence: u32,
    /// Average confidence score
    pub avg_confidence: u32,
    /// Last update timestamp
    pub last_update: u64,
}

/// Batch price update request
#[derive(Clone, Debug)]
#[contracttype]
pub struct BatchPriceUpdate {
    /// List of asset prices to update
    pub prices: Vec<AssetPrice>,
    /// Source submitting the update
    pub source: Address,
    /// Signature for verification
    pub signature: Vec<u8>,
}

/// Cross-chain asset information
#[derive(Clone, Debug)]
#[contracttype]
pub struct CrossChainAsset {
    /// Native chain ID
    pub native_chain_id: u32,
    /// Native asset address/symbol
    pub native_asset: Symbol,
    /// Stellar asset identifier
    pub stellar_asset: StellarAssetId,
    /// Bridge contract address
    pub bridge_address: Address,
    /// Whether this is a wrapped asset
    pub is_wrapped: bool,
    /// Last bridge update timestamp
    pub last_bridge_update: u64,
}

/// Asset whitelist entry
#[derive(Clone, Debug)]
#[contracttype]
pub struct WhitelistEntry {
    /// Asset identifier
    pub asset_id: StellarAssetId,
    /// Added by
    pub added_by: Address,
    /// Reason for whitelisting
    pub reason: Symbol,
    /// When added
    pub added_at: u64,
    /// Whether entry is active
    pub active: bool,
}
