//! Liquidity Pool contract implementation for Stellar DeFi Toolkit
//!
//! Provides automated market maker (AMM) functionality for creating
//! liquidity pools between different tokens on the Stellar blockchain.

use std::collections::BTreeMap;
use soroban_sdk::{contract, contractimpl, contracttype, contracterror, Address, Env, Symbol, log};

// ---------------------------------------------------------------------------
// Soroban on-chain contract (#27)
// ---------------------------------------------------------------------------

/// Storage keys for the liquidity pool Soroban contract.
#[contracttype]
pub enum PoolDataKey {
    TokenA,
    TokenB,
    ReserveA,
    ReserveB,
    TotalLiquidity,
    FeeBps,
    LpBalance(Address),
    CollectedFeesA,
    CollectedFeesB,
    Initialized,
}

/// Error codes for the liquidity pool Soroban contract.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum PoolContractError {
    AlreadyInitialized = 1,
    InvalidAmount = 2,
    InsufficientLiquidity = 3,
    SlippageExceeded = 4,
    NoPosition = 5,
}

/// Liquidity pool Soroban contract implementing AMM with fee collection (#26)
/// and position tracking (#25).
#[contract]
pub struct LiquidityPool;

#[contractimpl]
impl LiquidityPool {
    /// Initialize the pool with token pair and fee.
    pub fn initialize(
        env: Env,
        token_a: Symbol,
        token_b: Symbol,
        fee_bps: u32,
    ) -> Result<(), PoolContractError> {
        if env.storage().instance().has(&PoolDataKey::Initialized) {
            return Err(PoolContractError::AlreadyInitialized);
        }
        env.storage().instance().set(&PoolDataKey::TokenA, &token_a);
        env.storage().instance().set(&PoolDataKey::TokenB, &token_b);
        env.storage().instance().set(&PoolDataKey::FeeBps, &fee_bps);
        env.storage().instance().set(&PoolDataKey::ReserveA, &0u64);
        env.storage().instance().set(&PoolDataKey::ReserveB, &0u64);
        env.storage().instance().set(&PoolDataKey::TotalLiquidity, &0u64);
        env.storage().instance().set(&PoolDataKey::CollectedFeesA, &0u64);
        env.storage().instance().set(&PoolDataKey::CollectedFeesB, &0u64);
        env.storage().instance().set(&PoolDataKey::Initialized, &true);
        log!(&env, "LiquidityPool: initialized token_a={}, token_b={}, fee_bps={}", token_a, token_b, fee_bps);
        Ok(())
    }

    /// Add liquidity and track provider position (#25).
    pub fn add_liquidity(
        env: Env,
        provider: Address,
        amount_a: u64,
        amount_b: u64,
    ) -> Result<u64, PoolContractError> {
        provider.require_auth();
        if amount_a == 0 || amount_b == 0 {
            return Err(PoolContractError::InvalidAmount);
        }

        let reserve_a: u64 = env.storage().instance().get(&PoolDataKey::ReserveA).unwrap_or(0);
        let reserve_b: u64 = env.storage().instance().get(&PoolDataKey::ReserveB).unwrap_or(0);
        let total_liq: u64 = env.storage().instance().get(&PoolDataKey::TotalLiquidity).unwrap_or(0);

        let lp_tokens = if total_liq == 0 {
            (amount_a.checked_mul(amount_b).unwrap_or(0) as f64).sqrt() as u64
        } else {
            let la = amount_a.checked_mul(total_liq).unwrap_or(0) / reserve_a;
            let lb = amount_b.checked_mul(total_liq).unwrap_or(0) / reserve_b;
            la.min(lb)
        };

        if lp_tokens == 0 {
            return Err(PoolContractError::InvalidAmount);
        }

        env.storage().instance().set(&PoolDataKey::ReserveA, &(reserve_a + amount_a));
        env.storage().instance().set(&PoolDataKey::ReserveB, &(reserve_b + amount_b));
        env.storage().instance().set(&PoolDataKey::TotalLiquidity, &(total_liq + lp_tokens));

        // Track provider balance (#25)
        let prev: u64 = env.storage().instance().get(&PoolDataKey::LpBalance(provider.clone())).unwrap_or(0);
        env.storage().instance().set(&PoolDataKey::LpBalance(provider.clone()), &(prev + lp_tokens));

        log!(&env, "LiquidityPool: add_liquidity provider={}, lp_tokens={}", provider, lp_tokens);
        Ok(lp_tokens)
    }

    /// Remove liquidity and update provider position (#25).
    pub fn remove_liquidity(
        env: Env,
        provider: Address,
        lp_tokens: u64,
    ) -> Result<(u64, u64), PoolContractError> {
        provider.require_auth();
        if lp_tokens == 0 {
            return Err(PoolContractError::InvalidAmount);
        }

        let reserve_a: u64 = env.storage().instance().get(&PoolDataKey::ReserveA).unwrap_or(0);
        let reserve_b: u64 = env.storage().instance().get(&PoolDataKey::ReserveB).unwrap_or(0);
        let total_liq: u64 = env.storage().instance().get(&PoolDataKey::TotalLiquidity).unwrap_or(0);

        if lp_tokens > total_liq {
            return Err(PoolContractError::InsufficientLiquidity);
        }

        let amount_a = lp_tokens.checked_mul(reserve_a).unwrap_or(0) / total_liq;
        let amount_b = lp_tokens.checked_mul(reserve_b).unwrap_or(0) / total_liq;

        env.storage().instance().set(&PoolDataKey::ReserveA, &(reserve_a - amount_a));
        env.storage().instance().set(&PoolDataKey::ReserveB, &(reserve_b - amount_b));
        env.storage().instance().set(&PoolDataKey::TotalLiquidity, &(total_liq - lp_tokens));

        // Update provider balance (#25)
        let prev: u64 = env.storage().instance().get(&PoolDataKey::LpBalance(provider.clone())).unwrap_or(0);
        let new_bal = prev.saturating_sub(lp_tokens);
        env.storage().instance().set(&PoolDataKey::LpBalance(provider.clone()), &new_bal);

        log!(&env, "LiquidityPool: remove_liquidity provider={}, amount_a={}, amount_b={}", provider, amount_a, amount_b);
        Ok((amount_a, amount_b))
    }

    /// Swap token A for token B with fee collection (#26).
    pub fn swap_a_for_b(
        env: Env,
        user: Address,
        amount_in: u64,
        min_out: u64,
    ) -> Result<u64, PoolContractError> {
        user.require_auth();
        if amount_in == 0 {
            return Err(PoolContractError::InvalidAmount);
        }

        let reserve_a: u64 = env.storage().instance().get(&PoolDataKey::ReserveA).unwrap_or(0);
        let reserve_b: u64 = env.storage().instance().get(&PoolDataKey::ReserveB).unwrap_or(0);
        let fee_bps: u32 = env.storage().instance().get(&PoolDataKey::FeeBps).unwrap_or(30);

        let (amount_out, fee) = Self::compute_swap(amount_in, reserve_a, reserve_b, fee_bps);
        if amount_out < min_out {
            return Err(PoolContractError::SlippageExceeded);
        }

        env.storage().instance().set(&PoolDataKey::ReserveA, &(reserve_a + amount_in));
        env.storage().instance().set(&PoolDataKey::ReserveB, &(reserve_b - amount_out));

        // Collect fee (#26)
        let fees: u64 = env.storage().instance().get(&PoolDataKey::CollectedFeesA).unwrap_or(0);
        env.storage().instance().set(&PoolDataKey::CollectedFeesA, &(fees + fee));

        Ok(amount_out)
    }

    /// Swap token B for token A with fee collection (#26).
    pub fn swap_b_for_a(
        env: Env,
        user: Address,
        amount_in: u64,
        min_out: u64,
    ) -> Result<u64, PoolContractError> {
        user.require_auth();
        if amount_in == 0 {
            return Err(PoolContractError::InvalidAmount);
        }

        let reserve_a: u64 = env.storage().instance().get(&PoolDataKey::ReserveA).unwrap_or(0);
        let reserve_b: u64 = env.storage().instance().get(&PoolDataKey::ReserveB).unwrap_or(0);
        let fee_bps: u32 = env.storage().instance().get(&PoolDataKey::FeeBps).unwrap_or(30);

        let (amount_out, fee) = Self::compute_swap(amount_in, reserve_b, reserve_a, fee_bps);
        if amount_out < min_out {
            return Err(PoolContractError::SlippageExceeded);
        }

        env.storage().instance().set(&PoolDataKey::ReserveB, &(reserve_b + amount_in));
        env.storage().instance().set(&PoolDataKey::ReserveA, &(reserve_a - amount_out));

        // Collect fee (#26)
        let fees: u64 = env.storage().instance().get(&PoolDataKey::CollectedFeesB).unwrap_or(0);
        env.storage().instance().set(&PoolDataKey::CollectedFeesB, &(fees + fee));

        Ok(amount_out)
    }

    /// Claim accumulated fees proportional to LP share (#26).
    pub fn claim_fees(env: Env, provider: Address) -> Result<(u64, u64), PoolContractError> {
        provider.require_auth();
        let balance: u64 = env.storage().instance().get(&PoolDataKey::LpBalance(provider.clone())).unwrap_or(0);
        if balance == 0 {
            return Err(PoolContractError::NoPosition);
        }
        let total_liq: u64 = env.storage().instance().get(&PoolDataKey::TotalLiquidity).unwrap_or(0);
        if total_liq == 0 {
            return Err(PoolContractError::InsufficientLiquidity);
        }

        let fees_a: u64 = env.storage().instance().get(&PoolDataKey::CollectedFeesA).unwrap_or(0);
        let fees_b: u64 = env.storage().instance().get(&PoolDataKey::CollectedFeesB).unwrap_or(0);

        let share_a = fees_a.checked_mul(balance).unwrap_or(0) / total_liq;
        let share_b = fees_b.checked_mul(balance).unwrap_or(0) / total_liq;

        env.storage().instance().set(&PoolDataKey::CollectedFeesA, &(fees_a.saturating_sub(share_a)));
        env.storage().instance().set(&PoolDataKey::CollectedFeesB, &(fees_b.saturating_sub(share_b)));

        log!(&env, "LiquidityPool: claim_fees provider={}, share_a={}, share_b={}", provider, share_a, share_b);
        Ok((share_a, share_b))
    }

    /// Get LP balance for a provider (#25).
    pub fn get_position(env: Env, provider: Address) -> u64 {
        env.storage().instance().get(&PoolDataKey::LpBalance(provider)).unwrap_or(0)
    }

    /// Get total collected fees (#26).
    pub fn get_collected_fees(env: Env) -> (u64, u64) {
        let a: u64 = env.storage().instance().get(&PoolDataKey::CollectedFeesA).unwrap_or(0);
        let b: u64 = env.storage().instance().get(&PoolDataKey::CollectedFeesB).unwrap_or(0);
        (a, b)
    }

    // --- Internal helpers ---

    fn compute_swap(amount_in: u64, reserve_in: u64, reserve_out: u64, fee_bps: u32) -> (u64, u64) {
        if reserve_in == 0 || reserve_out == 0 {
            return (0, 0);
        }
        let fee = amount_in.checked_mul(fee_bps as u64).unwrap_or(0) / 10000;
        let after_fee = amount_in - fee;
        let out = reserve_out.checked_mul(after_fee).unwrap_or(0) / (reserve_in + after_fee);
        (out, fee)
    }
}

// ---------------------------------------------------------------------------
// Library / simulation implementation (preserves existing API + #25 + #26)
// ---------------------------------------------------------------------------

/// Liquidity position for a user.
#[derive(Debug, Clone)]
pub struct LiquidityPosition {
    pub provider: String,
    pub liquidity_tokens: u64,
    pub share_percentage: f64,
}

/// Pool information snapshot.
#[derive(Debug, Clone)]
pub struct PoolInfo {
    pub token_a: String,
    pub token_b: String,
    pub reserve_a: u64,
    pub reserve_b: u64,
    pub total_liquidity: u64,
    pub fee_percentage: u32,
}

/// Liquidity pool library implementation for off-chain simulation and testing.
pub struct LiquidityPoolContract {
    /// Token A contract address
    pub token_a: String,
    /// Token B contract address
    pub token_b: String,
    /// Reserve of token A
    pub reserve_a: u64,
    /// Reserve of token B
    pub reserve_b: u64,
    /// Total liquidity tokens
    pub total_liquidity: u64,
    /// LP fee percentage (in basis points, e.g., 30 = 0.3%)
    pub fee_percentage: u32,
    /// Tracked LP token balances per provider (#25)
    lp_balances: BTreeMap<String, u64>,
    /// Accumulated fees for token A available for LP distribution (#26)
    collected_fees_a: u64,
    /// Accumulated fees for token B available for LP distribution (#26)
    collected_fees_b: u64,
}

impl LiquidityPoolContract {
    /// Create a new liquidity pool
    pub fn new(token_a: String, token_b: String) -> Self {
        Self {
            token_a,
            token_b,
            reserve_a: 0,
            reserve_b: 0,
            total_liquidity: 0,
            fee_percentage: 30,
            lp_balances: BTreeMap::new(),
            collected_fees_a: 0,
            collected_fees_b: 0,
        }
    }

    /// Create a liquidity pool with custom fee
    pub fn new_with_fee(token_a: String, token_b: String, fee_percentage: u32) -> Self {
        Self {
            token_a,
            token_b,
            reserve_a: 0,
            reserve_b: 0,
            total_liquidity: 0,
            fee_percentage,
            lp_balances: BTreeMap::new(),
            collected_fees_a: 0,
            collected_fees_b: 0,
        }
    }

    /// Get pool information
    pub fn get_info(&self) -> PoolInfo {
        PoolInfo {
            token_a: self.token_a.clone(),
            token_b: self.token_b.clone(),
            reserve_a: self.reserve_a,
            reserve_b: self.reserve_b,
            total_liquidity: self.total_liquidity,
            fee_percentage: self.fee_percentage,
        }
    }

    /// Add liquidity to the pool
    pub fn add_liquidity(
        &mut self,
        provider: &str,
        amount_a: u64,
        amount_b: u64,
        min_amount_a: u64,
        min_amount_b: u64,
    ) -> Result<u64, String> {
        if amount_a == 0 || amount_b == 0 {
            return Err("Amounts must be greater than 0".to_string());
        }

        if amount_a < min_amount_a || amount_b < min_amount_b {
            return Err("Amounts below minimum threshold".to_string());
        }

        let liquidity_tokens = if self.total_liquidity == 0 {
            (amount_a.checked_mul(amount_b).unwrap() as f64).sqrt() as u64
        } else {
            let liquidity_a = amount_a.checked_mul(self.total_liquidity).unwrap() / self.reserve_a;
            let liquidity_b = amount_b.checked_mul(self.total_liquidity).unwrap() / self.reserve_b;
            std::cmp::min(liquidity_a, liquidity_b)
        };

        if liquidity_tokens == 0 {
            return Err("Insufficient liquidity amount".to_string());
        }

        self.reserve_a += amount_a;
        self.reserve_b += amount_b;
        self.total_liquidity += liquidity_tokens;

        // Track provider position (#25)
        *self.lp_balances.entry(provider.to_string()).or_insert(0) += liquidity_tokens;

        Ok(liquidity_tokens)
    }

    /// Remove liquidity from the pool
    pub fn remove_liquidity(
        &mut self,
        provider: &str,
        liquidity_tokens: u64,
        min_amount_a: u64,
        min_amount_b: u64,
    ) -> Result<(u64, u64), String> {
        if liquidity_tokens == 0 {
            return Err("Liquidity tokens must be greater than 0".to_string());
        }

        if liquidity_tokens > self.total_liquidity {
            return Err("Insufficient liquidity tokens".to_string());
        }

        let amount_a = liquidity_tokens.checked_mul(self.reserve_a).unwrap() / self.total_liquidity;
        let amount_b = liquidity_tokens.checked_mul(self.reserve_b).unwrap() / self.total_liquidity;

        if amount_a < min_amount_a || amount_b < min_amount_b {
            return Err("Amounts below minimum threshold".to_string());
        }

        self.reserve_a -= amount_a;
        self.reserve_b -= amount_b;
        self.total_liquidity -= liquidity_tokens;

        // Update provider position (#25)
        if let Some(balance) = self.lp_balances.get_mut(provider) {
            *balance = balance.saturating_sub(liquidity_tokens);
            if *balance == 0 {
                self.lp_balances.remove(provider);
            }
        }

        Ok((amount_a, amount_b))
    }

    /// Swap token A for token B
    pub fn swap_a_for_b(&mut self, amount_in: u64, min_amount_out: u64) -> Result<u64, String> {
        if amount_in == 0 {
            return Err("Input amount must be greater than 0".to_string());
        }

        let (amount_out, fee) = self.calculate_swap_output_with_fee(amount_in, self.reserve_a, self.reserve_b);

        if amount_out < min_amount_out {
            return Err("Insufficient output amount".to_string());
        }

        self.reserve_a += amount_in;
        self.reserve_b -= amount_out;
        self.collected_fees_a += fee; // #26

        Ok(amount_out)
    }

    /// Swap token B for token A
    pub fn swap_b_for_a(&mut self, amount_in: u64, min_amount_out: u64) -> Result<u64, String> {
        if amount_in == 0 {
            return Err("Input amount must be greater than 0".to_string());
        }

        let (amount_out, fee) = self.calculate_swap_output_with_fee(amount_in, self.reserve_b, self.reserve_a);

        if amount_out < min_amount_out {
            return Err("Insufficient output amount".to_string());
        }

        self.reserve_b += amount_in;
        self.reserve_a -= amount_out;
        self.collected_fees_b += fee; // #26

        Ok(amount_out)
    }

    /// Calculate swap output with fee amount (#26).
    fn calculate_swap_output_with_fee(&self, amount_in: u64, reserve_in: u64, reserve_out: u64) -> (u64, u64) {
        if reserve_in == 0 || reserve_out == 0 {
            return (0, 0);
        }
        let fee_amount = amount_in.checked_mul(self.fee_percentage as u64).unwrap() / 10000;
        let amount_in_after_fee = amount_in - fee_amount;
        let output = reserve_out.checked_mul(amount_in_after_fee).unwrap() / (reserve_in + amount_in_after_fee);
        (output, fee_amount)
    }

    /// Calculate swap output (legacy helper).
    pub fn calculate_swap_output(&self, amount_in: u64, reserve_in: u64, reserve_out: u64) -> u64 {
        self.calculate_swap_output_with_fee(amount_in, reserve_in, reserve_out).0
    }

    /// Get current price of token A in terms of token B
    pub fn get_price_a_to_b(&self) -> f64 {
        if self.reserve_a == 0 { return 0.0; }
        self.reserve_b as f64 / self.reserve_a as f64
    }

    /// Get current price of token B in terms of token A
    pub fn get_price_b_to_a(&self) -> f64 {
        if self.reserve_b == 0 { return 0.0; }
        self.reserve_a as f64 / self.reserve_b as f64
    }

    /// Get liquidity position for a provider (#25)
    pub fn get_liquidity_position(&self, provider: &str) -> LiquidityPosition {
        let liquidity_tokens = self.lp_balances.get(provider).copied().unwrap_or(0);
        let share_percentage = if self.total_liquidity > 0 {
            liquidity_tokens as f64 / self.total_liquidity as f64 * 100.0
        } else {
            0.0
        };
        LiquidityPosition {
            provider: provider.to_string(),
            liquidity_tokens,
            share_percentage,
        }
    }

    /// Get all liquidity positions (#25)
    pub fn get_all_liquidity_positions(&self) -> Vec<LiquidityPosition> {
        self.lp_balances
            .iter()
            .map(|(provider, tokens)| {
                let share_percentage = if self.total_liquidity > 0 {
                    *tokens as f64 / self.total_liquidity as f64 * 100.0
                } else {
                    0.0
                };
                LiquidityPosition {
                    provider: provider.clone(),
                    liquidity_tokens: *tokens,
                    share_percentage,
                }
            })
            .collect()
    }

    /// Claim accumulated fees proportional to LP share (#26)
    pub fn claim_fees(&mut self, provider: &str) -> Result<(u64, u64), String> {
        let balance = self.lp_balances.get(provider).copied().unwrap_or(0);
        if balance == 0 {
            return Err("No liquidity position found".to_string());
        }
        if self.total_liquidity == 0 {
            return Err("Pool has no liquidity".to_string());
        }

        let fee_share_a = self.collected_fees_a.checked_mul(balance).unwrap_or(0) / self.total_liquidity;
        let fee_share_b = self.collected_fees_b.checked_mul(balance).unwrap_or(0) / self.total_liquidity;

        self.collected_fees_a = self.collected_fees_a.saturating_sub(fee_share_a);
        self.collected_fees_b = self.collected_fees_b.saturating_sub(fee_share_b);

        Ok((fee_share_a, fee_share_b))
    }

    /// View total collected fees pending distribution (#26)
    pub fn get_collected_fees(&self) -> (u64, u64) {
        (self.collected_fees_a, self.collected_fees_b)
    }

    /// Simulate a swap without executing it
    pub fn simulate_swap(&self, token_in: &str, amount_in: u64) -> Result<u64, String> {
        if amount_in == 0 {
            return Err("Input amount must be greater than 0".to_string());
        }

        let amount_out = if token_in == self.token_a {
            self.calculate_swap_output(amount_in, self.reserve_a, self.reserve_b)
        } else if token_in == self.token_b {
            self.calculate_swap_output(amount_in, self.reserve_b, self.reserve_a)
        } else {
            return Err("Invalid token".to_string());
        };

        Ok(amount_out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_creation() {
        let pool = LiquidityPoolContract::new(
            "TOKEN_A_CONTRACT".to_string(),
            "TOKEN_B_CONTRACT".to_string(),
        );

        assert_eq!(pool.token_a, "TOKEN_A_CONTRACT");
        assert_eq!(pool.token_b, "TOKEN_B_CONTRACT");
        assert_eq!(pool.reserve_a, 0);
        assert_eq!(pool.reserve_b, 0);
        assert_eq!(pool.total_liquidity, 0);
        assert_eq!(pool.fee_percentage, 30);
    }

    #[test]
    fn test_pool_creation_with_custom_fee() {
        let pool = LiquidityPoolContract::new_with_fee(
            "TOKEN_A_CONTRACT".to_string(),
            "TOKEN_B_CONTRACT".to_string(),
            50,
        );
        assert_eq!(pool.fee_percentage, 50);
    }

    #[test]
    fn test_add_initial_liquidity() {
        let mut pool = LiquidityPoolContract::new(
            "TOKEN_A_CONTRACT".to_string(),
            "TOKEN_B_CONTRACT".to_string(),
        );

        let liquidity = pool
            .add_liquidity("provider1", 1000, 2000, 1000, 2000)
            .unwrap();

        assert_eq!(pool.reserve_a, 1000);
        assert_eq!(pool.reserve_b, 2000);
        assert_eq!(pool.total_liquidity, liquidity);
        assert!(liquidity > 0);
    }

    #[test]
    fn test_position_tracking() {
        let mut pool = LiquidityPoolContract::new(
            "TOKEN_A".to_string(),
            "TOKEN_B".to_string(),
        );

        let lp = pool.add_liquidity("alice", 1000, 1000, 0, 0).unwrap();
        let pos = pool.get_liquidity_position("alice");
        assert_eq!(pos.liquidity_tokens, lp);
        assert_eq!(pos.share_percentage, 100.0);

        let positions = pool.get_all_liquidity_positions();
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].provider, "alice");
    }

    #[test]
    fn test_fee_collection_and_claim() {
        let mut pool = LiquidityPoolContract::new(
            "TOKEN_A".to_string(),
            "TOKEN_B".to_string(),
        );

        pool.add_liquidity("alice", 10000, 10000, 0, 0).unwrap();

        // Execute a swap to generate fees
        pool.swap_a_for_b(1000, 0).unwrap();
        let (fees_a, fees_b) = pool.get_collected_fees();
        assert!(fees_a > 0); // Fee collected on token A input
        assert_eq!(fees_b, 0);

        // Claim fees
        let (claimed_a, claimed_b) = pool.claim_fees("alice").unwrap();
        assert_eq!(claimed_a, fees_a); // Alice has 100% of pool
        assert_eq!(claimed_b, 0);

        // Fees should be zeroed after claim
        let (remaining_a, _) = pool.get_collected_fees();
        assert_eq!(remaining_a, 0);
    }

    #[test]
    fn test_swap_calculation() {
        let mut pool = LiquidityPoolContract::new(
            "TOKEN_A_CONTRACT".to_string(),
            "TOKEN_B_CONTRACT".to_string(),
        );

        pool.reserve_a = 1000;
        pool.reserve_b = 2000;

        let output = pool.calculate_swap_output(100, pool.reserve_a, pool.reserve_b);
        // With 30 bps fee: amount_in_after_fee ≈ 99.7
        // output ≈ (2000 * 99.7) / (1000 + 99.7) ≈ 181
        assert!(output > 180 && output < 182);
    }

    #[test]
    fn test_price_calculation() {
        let mut pool = LiquidityPoolContract::new(
            "TOKEN_A_CONTRACT".to_string(),
            "TOKEN_B_CONTRACT".to_string(),
        );

        pool.reserve_a = 1000;
        pool.reserve_b = 2000;

        assert_eq!(pool.get_price_a_to_b(), 2.0);
        assert_eq!(pool.get_price_b_to_a(), 0.5);
    }

    #[test]
    fn test_invalid_add_liquidity() {
        let mut pool = LiquidityPoolContract::new(
            "TOKEN_A_CONTRACT".to_string(),
            "TOKEN_B_CONTRACT".to_string(),
        );

        let result = pool.add_liquidity("alice", 0, 1000, 0, 1000);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Amounts must be greater than 0");
    }
}
