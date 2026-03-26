//! Liquidity pool related type definitions

use serde::{Deserialize, Serialize};
use soroban_sdk::{Address, Env};

/// Liquidity pool information
#[soroban_sdk::contracttype]
#[derive(Debug, Clone)]
pub struct PoolInfo {
    /// Token A contract address
    pub token_a: soroban_sdk::String,
    /// Token B contract address
    pub token_b: soroban_sdk::String,
    /// Reserve of token A
    pub reserve_a: u64,
    /// Reserve of token B
    pub reserve_b: u64,
    /// Total liquidity tokens
    pub total_liquidity: u64,
    /// Fee percentage in basis points (e.g., 30 = 0.3%)
    pub fee_percentage: u32,
}

/// Liquidity position for a user
#[soroban_sdk::contracttype]
#[derive(Debug, Clone)]
pub struct LiquidityPosition {
    /// User address
    pub user: Address,
    /// Amount of liquidity tokens
    pub liquidity_tokens: u64,
    /// Share percentage of the pool (in basis points)
    pub share_percentage: u32,
}

/// Swap parameters
#[soroban_sdk::contracttype]
#[derive(Debug, Clone)]
pub struct SwapParams {
    /// Input token contract address
    pub token_in: soroban_sdk::String,
    /// Output token contract address
    pub token_out: soroban_sdk::String,
    /// Amount to swap in
    pub amount_in: u64,
    /// Minimum amount out (slippage protection)
    pub min_amount_out: u64,
    /// Recipient address
    pub recipient: Address,
    /// Deadline timestamp
    pub deadline: u64,
}

/// Swap result
#[derive(Debug, Clone)]
pub struct SwapResult {
    /// Amount received
    pub amount_out: u64,
    /// Fee paid
    pub fee_amount: u64,
    /// Price impact percentage
    pub price_impact: f64,
    /// Transaction hash
    pub tx_hash: String,
}

/// Add liquidity parameters
#[derive(Debug, Clone)]
pub struct AddLiquidityParams {
    /// Token A contract address
    pub token_a: String,
    /// Token B contract address
    pub token_b: String,
    /// Amount of token A to add
    pub amount_a: u64,
    /// Amount of token B to add
    pub amount_b: u64,
    /// Minimum amount of token A (slippage protection)
    pub min_amount_a: u64,
    /// Minimum amount of token B (slippage protection)
    pub min_amount_b: u64,
    /// Recipient address
    pub recipient: Address,
    /// Deadline timestamp
    pub deadline: u64,
}

/// Add liquidity result
#[derive(Debug, Clone)]
pub struct AddLiquidityResult {
    /// Amount of liquidity tokens received
    pub liquidity_tokens: u64,
    /// Amount of token A actually used
    pub amount_a_used: u64,
    /// Amount of token B actually used
    pub amount_b_used: u64,
    /// Transaction hash
    pub tx_hash: String,
}

/// Remove liquidity parameters
#[derive(Debug, Clone)]
pub struct RemoveLiquidityParams {
    /// Token A contract address
    pub token_a: String,
    /// Token B contract address
    pub token_b: String,
    /// Amount of liquidity tokens to remove
    pub liquidity_tokens: u64,
    /// Minimum amount of token A (slippage protection)
    pub min_amount_a: u64,
    /// Minimum amount of token B (slippage protection)
    pub min_amount_b: u64,
    /// Recipient address
    pub recipient: Address,
    /// Deadline timestamp
    pub deadline: u64,
}

/// Remove liquidity result
#[derive(Debug, Clone)]
pub struct RemoveLiquidityResult {
    /// Amount of token A received
    pub amount_a: u64,
    /// Amount of token B received
    pub amount_b: u64,
    /// Transaction hash
    pub tx_hash: String,
}

/// Pool statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolStats {
    /// Pool contract address
    pub pool_address: String,
    /// Total volume in the last 24 hours
    pub volume_24h: u64,
    /// Total fees collected in the last 24 hours
    pub fees_24h: u64,
    /// Annual percentage yield for liquidity providers
    pub apr: f64,
    /// Total value locked in the pool
    pub tvl: u64,
    /// Number of liquidity providers
    pub lp_count: u32,
    /// Current price of token A in terms of token B
    pub price_a_to_b: f64,
    /// Current price of token B in terms of token A
    pub price_b_to_a: f64,
}

/// Pool event types
#[derive(Debug, Clone)]
pub enum PoolEvent {
    /// Liquidity added event
    LiquidityAdded(AddLiquidityEvent),
    /// Liquidity removed event
    LiquidityRemoved(RemoveLiquidityEvent),
    /// Swap executed event
    SwapExecuted(SwapEvent),
}

/// Liquidity added event
#[derive(Debug, Clone)]
pub struct AddLiquidityEvent {
    /// Provider address
    pub provider: Address,
    /// Amount of token A added
    pub amount_a: u64,
    /// Amount of token B added
    pub amount_b: u64,
    /// Liquidity tokens received
    pub liquidity_tokens: u64,
    /// Transaction hash
    pub tx_hash: String,
    /// Block number
    pub block_number: u64,
    /// Timestamp
    pub timestamp: u64,
}

/// Liquidity removed event
#[derive(Debug, Clone)]
pub struct RemoveLiquidityEvent {
    /// Provider address
    pub provider: Address,
    /// Amount of token A received
    pub amount_a: u64,
    /// Amount of token B received
    pub amount_b: u64,
    /// Liquidity tokens burned
    pub liquidity_tokens: u64,
    /// Transaction hash
    pub tx_hash: String,
    /// Block number
    pub block_number: u64,
    /// Timestamp
    pub timestamp: u64,
}

/// Swap executed event
#[derive(Debug, Clone)]
pub struct SwapEvent {
    /// Swapper address
    pub swapper: Address,
    /// Token in contract address
    pub token_in: String,
    /// Token out contract address
    pub token_out: String,
    /// Amount in
    pub amount_in: u64,
    /// Amount out
    pub amount_out: u64,
    /// Fee paid
    pub fee_amount: u64,
    /// Transaction hash
    pub tx_hash: String,
    /// Block number
    pub block_number: u64,
    /// Timestamp
    pub timestamp: u64,
}

impl PoolInfo {
    /// Create new pool info
    pub fn new(env: &Env, token_a: String, token_b: String, fee_percentage: u32) -> Self {
        Self {
            token_a: soroban_sdk::String::from_str(env, &token_a),
            token_b: soroban_sdk::String::from_str(env, &token_b),
            reserve_a: 0,
            reserve_b: 0,
            total_liquidity: 0,
            fee_percentage,
        }
    }

    /// Get the current price of token A in terms of token B
    pub fn get_price_a_to_b(&self) -> f64 {
        if self.reserve_a == 0 {
            return 0.0;
        }
        self.reserve_b as f64 / self.reserve_a as f64
    }

    /// Get the current price of token B in terms of token A
    pub fn get_price_b_to_a(&self) -> f64 {
        if self.reserve_b == 0 {
            return 0.0;
        }
        self.reserve_a as f64 / self.reserve_b as f64
    }

    /// Check if the pool has liquidity
    pub fn has_liquidity(&self) -> bool {
        self.reserve_a > 0 && self.reserve_b > 0
    }

    /// Get total value locked (TVL) - simplified calculation
    pub fn get_tvl(&self, price_a_usd: f64, price_b_usd: f64) -> f64 {
        let value_a = self.reserve_a as f64 * price_a_usd;
        let value_b = self.reserve_b as f64 * price_b_usd;
        value_a + value_b
    }
}
