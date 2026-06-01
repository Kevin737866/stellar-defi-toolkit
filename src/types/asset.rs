//! Comprehensive Stellar Asset Type Definitions
//!
//! This module provides type definitions for a wide range of Stellar assets,
//! including native assets, custom tokens, stablecoins, wrapped assets, and more.

use soroban_sdk::{Address, Symbol, Map, Vec};

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
