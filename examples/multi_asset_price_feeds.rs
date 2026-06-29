//! Multi-Asset Price Feeds Example
//!
//! Demonstrates the multi-asset price feed system types and structures
//! for supporting a wide range of Stellar assets.

use soroban_sdk::{Address, Env, Symbol, Vec, Map};
use stellar_defi_toolkit::types::asset::{
    StellarAssetId, AssetCategory, AssetPrice, PriceSourceType, AggregationMethod,
    AssetMetadata, PriceFeedConfig, PriceSource, CategoryConfig,
};

fn main() {
    let env = Env::default();
    
    println!("🚀 Multi-Asset Price Feed Types Example\n");
    
    // ─── Asset Identifiers ─────────────────────────────────────────────────────
    println!("📝 Asset Identifiers:");
    
    let xlm = StellarAssetId::Native;
    println!("  XLM (Native): {:?}", xlm);
    
    let usdc_issuer = Address::generate(&env);
    let usdc = StellarAssetId::Token {
        code: Symbol::short("USDC"),
        issuer: usdc_issuer,
    };
    println!("  USDC (Token): {:?}", usdc);
    
    let wbtc_contract = Address::generate(&env);
    let wbtc = StellarAssetId::ContractToken {
        contract_address: wbtc_contract,
    };
    println!("  wBTC (Contract): {:?}", wbtc);
    println!();
    
    // ─── Asset Categories ─────────────────────────────────────────────────────
    println!("📂 Asset Categories:");
    println!("  Native: {:?}", AssetCategory::Native);
    println!("  Cryptocurrency: {:?}", AssetCategory::Cryptocurrency);
    println!("  Stablecoin: {:?}", AssetCategory::Stablecoin);
    println!("  Wrapped: {:?}", AssetCategory::Wrapped);
    println!("  DeFi Token: {:?}", AssetCategory::DeFiToken);
    println!("  Liquidity Pool: {:?}", AssetCategory::LiquidityPool);
    println!("  Synthetic: {:?}", AssetCategory::Synthetic);
    println!("  Real World Asset: {:?}", AssetCategory::RealWorldAsset);
    println!("  Commodity: {:?}", AssetCategory::Commodity);
    println!("  Forex: {:?}", AssetCategory::Forex);
    println!();
    
    // ─── Asset Metadata ───────────────────────────────────────────────────────
    println!("📋 Asset Metadata Example:");
    
    let xlm_metadata = AssetMetadata {
        asset_id: StellarAssetId::Native,
        symbol: Symbol::short("XLM"),
        name: Symbol::short("Stellar Lumens"),
        category: AssetCategory::Native,
        decimals: 7,
        active: true,
        min_update_interval: 300,
        max_price_deviation: 500,
        min_confidence: 7000,
        approved_sources: Vec::new(&env),
        registered_at: env.ledger().timestamp(),
        last_price_update: 0,
        custom_metadata: Map::new(&env),
    };
    
    println!("  Symbol: {:?}", xlm_metadata.symbol);
    println!("  Name: {:?}", xlm_metadata.name);
    println!("  Category: {:?}", xlm_metadata.category);
    println!("  Decimals: {}", xlm_metadata.decimals);
    println!("  Active: {}", xlm_metadata.active);
    println!();
    
    // ─── Price Feed Configuration ─────────────────────────────────────────────
    println!("⚙️  Price Feed Configuration:");
    
    let price_config = PriceFeedConfig {
        asset_id: StellarAssetId::Native,
        aggregation_method: AggregationMethod::WeightedAverage,
        min_sources: 3,
        max_price_age: 3600,
        circuit_breaker_threshold: 1000,
        use_twap: true,
        twap_period: 300,
        heartbeat_interval: 300,
    };
    
    println!("  Aggregation Method: {:?}", price_config.aggregation_method);
    println!("  Min Sources: {}", price_config.min_sources);
    println!("  Max Price Age: {}s", price_config.max_price_age);
    println!("  Use TWAP: {}", price_config.use_twap);
    println!();
    
    // ─── Asset Price ─────────────────────────────────────────────────────────
    println!("💰 Asset Price Example:");
    
    let current_time = env.ledger().timestamp();
    let source = Address::generate(&env);
    
    let xlm_price = AssetPrice {
        asset_id: StellarAssetId::Native,
        price: 15000000, // $0.15 with 7 decimals
        decimals: 7,
        confidence: 8500,
        timestamp: current_time,
        source,
        price_change_24h: 250,
        high_24h: 15500000,
        low_24h: 14500000,
        volume_24h: 1000000000,
    };
    
    println!("  Price: ${}", xlm_price.price as f64 / 10_000_000.0);
    println!("  Confidence: {}%", xlm_price.confidence as f64 / 100.0);
    println!("  24h Change: +{}%", xlm_price.price_change_24h as f64 / 100.0);
    println!("  24h High: ${}", xlm_price.high_24h as f64 / 10_000_000.0);
    println!("  24h Low: ${}", xlm_price.low_24h as f64 / 10_000_000.0);
    println!();
    
    // ─── Price Source ───────────────────────────────────────────────────────
    println!("🔌 Price Source Example:");
    
    let chainlink_address = Address::generate(&env);
    let mut supported_categories = Vec::new(&env);
    supported_categories.push_back(AssetCategory::Cryptocurrency);
    supported_categories.push_back(AssetCategory::Stablecoin);
    
    let chainlink_source = PriceSource {
        address: chainlink_address,
        name: Symbol::short("Chainlink"),
        source_type: PriceSourceType::Chainlink,
        weight: 5000,
        reputation: 9000,
        active: true,
        supported_categories,
        last_update: current_time,
        successful_updates: 1000,
        failed_updates: 5,
    };
    
    println!("  Name: {:?}", chainlink_source.name);
    println!("  Type: {:?}", chainlink_source.source_type);
    println!("  Weight: {}%", chainlink_source.weight as f64 / 100.0);
    println!("  Reputation: {}%", chainlink_source.reputation as f64 / 100.0);
    println!("  Active: {}", chainlink_source.active);
    println!();
    
    // ─── Category Configuration ───────────────────────────────────────────────
    println!("📂 Category Configuration Example:");
    
    let crypto_config = CategoryConfig {
        category: AssetCategory::Cryptocurrency,
        max_price_age: 300,
        min_confidence: 7000,
        preferred_aggregation: AggregationMethod::WeightedAverage,
        min_sources: 3,
        circuit_breaker_threshold: 1000,
        use_twap: true,
        twap_period: 300,
    };
    
    println!("  Category: {:?}", crypto_config.category);
    println!("  Max Price Age: {}s", crypto_config.max_price_age);
    println!("  Min Confidence: {}%", crypto_config.min_confidence as f64 / 100.0);
    println!("  Preferred Aggregation: {:?}", crypto_config.preferred_aggregation);
    println!("  Min Sources: {}", crypto_config.min_sources);
    println!();
    
    // ─── Summary ─────────────────────────────────────────────────────────────
    println!("✨ Multi-Asset Price Feed System Features:");
    println!();
    println!("  📝 Asset Types:");
    println!("     - Native Stellar assets (XLM)");
    println!("     - Custom tokens with issuer");
    println!("     - Soroban contract tokens");
    println!();
    println!("  📂 Asset Categories:");
    println!("     - Native, Cryptocurrency, Stablecoin");
    println!("     - Wrapped, DeFi Token, Liquidity Pool");
    println!("     - Synthetic, Real World Asset");
    println!("     - Commodity, Forex, NFT, Governance, Utility");
    println!();
    println!("  ⚙️  Configuration:");
    println!("     - Category-specific price feed settings");
    println!("     - Multiple aggregation methods");
    println!("     - TWAP support");
    println!("     - Circuit breakers");
    println!();
    println!("  🔌 Price Sources:");
    println!("     - Chainlink, Pyth Network, Band Protocol");
    println!("     - On-chain oracles, DEX price feeds");
    println!("     - Custom adapters");
    println!();
    println!("✨ The system supports a wide range of Stellar assets!");
}
