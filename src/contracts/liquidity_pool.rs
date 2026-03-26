//! Liquidity Pool contract implementation for Stellar DeFi Toolkit
//! 
//! Provides automated market maker (AMM) functionality for creating
//! liquidity pools between different tokens on the Stellar blockchain.

use soroban_sdk::{contract, Address, Env, Vec};
use crate::types::pool::{PoolInfo, LiquidityPosition};
use crate::utils::StellarClient;

/// Liquidity pool contract implementing AMM functionality
#[contract]
pub struct LiquidityPoolContract {
    /// Token A contract address
    token_a: soroban_sdk::String,
    /// Token B contract address
    token_b: soroban_sdk::String,
    /// Reserve of token A
    reserve_a: u64,
    /// Reserve of token B
    reserve_b: u64,
    /// Total liquidity tokens
    total_liquidity: u64,
    /// LP fee percentage (in basis points, e.g., 30 = 0.3%)
    fee_percentage: u32,
}

impl LiquidityPoolContract {
    /// Create a new liquidity pool
    pub fn new(_env: &Env, token_a: soroban_sdk::String, token_b: soroban_sdk::String) -> Self {
        Self {
            token_a,
            token_b,
            reserve_a: 0,
            reserve_b: 0,
            total_liquidity: 0,
            fee_percentage: 30, // 0.3% standard fee
        }
    }

    /// Create from std string
    pub fn new_std(env: &Env, token_a: String, token_b: String) -> Self {
        Self::new(
            env,
            soroban_sdk::String::from_str(env, &token_a),
            soroban_sdk::String::from_str(env, &token_b),
        )
    }

    /// Create a liquidity pool with custom fee
    pub fn new_with_fee(_env: &Env, token_a: soroban_sdk::String, token_b: soroban_sdk::String, fee_percentage: u32) -> Self {
        Self {
            token_a,
            token_b,
            reserve_a: 0,
            reserve_b: 0,
            total_liquidity: 0,
            fee_percentage,
        }
    }

    /// Get pool information
    pub fn get_info(&self, _env: &Env) -> PoolInfo {
        PoolInfo {
            token_a: self.token_a.clone(),
            token_b: self.token_b.clone(),
            reserve_a: self.reserve_a,
            reserve_b: self.reserve_b,
            total_liquidity: self.total_liquidity,
            fee_percentage: self.fee_percentage,
        }
    }

    /// Deploy the liquidity pool contract to Stellar
    pub async fn deploy(self, client: &StellarClient) -> anyhow::Result<String> {
        let contract_id = client.deploy_liquidity_pool_contract(&self).await?;
        // self.address = Some(Address::from_string(&contract_id)); // Address requires Env
        Ok(contract_id)
    }

    pub fn add_liquidity(
        &mut self,
        _env: &Env,
        _provider: Address,
        amount_a: u64,
        amount_b: u64,
        min_amount_a: u64,
        min_amount_b: u64,
    ) -> Result<u64, String> {
        if amount_a == 0 || amount_b == 0 {
            return Err("Amounts must be greater than 0".to_string());
        }

        // Check if amounts meet minimum requirements
        if amount_a < min_amount_a || amount_b < min_amount_b {
            return Err("Amounts below minimum threshold".to_string());
        }

        let liquidity_tokens = if self.total_liquidity == 0 {
            // First liquidity provider - calculate based on geometric mean
            ((amount_a.checked_mul(amount_b).unwrap() as f64).sqrt()) as u64
        } else {
            // Calculate liquidity tokens based on the ratio of amounts to reserves
            let liquidity_a = amount_a.checked_mul(self.total_liquidity).unwrap() / self.reserve_a;
            let liquidity_b = amount_b.checked_mul(self.total_liquidity).unwrap() / self.reserve_b;
            std::cmp::min(liquidity_a, liquidity_b)
        };

        if liquidity_tokens == 0 {
            return Err("Insufficient liquidity amount".to_string());
        }

        // Update reserves and total liquidity
        self.reserve_a += amount_a;
        self.reserve_b += amount_b;
        self.total_liquidity += liquidity_tokens;

        Ok(liquidity_tokens)
    }

    pub fn remove_liquidity(
        &mut self,
        _env: &Env,
        _provider: Address,
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

        // Calculate amounts to return based on liquidity token ratio
        let amount_a = liquidity_tokens.checked_mul(self.reserve_a).unwrap() / self.total_liquidity;
        let amount_b = liquidity_tokens.checked_mul(self.reserve_b).unwrap() / self.total_liquidity;

        // Check minimum amounts
        if amount_a < min_amount_a || amount_b < min_amount_b {
            return Err("Amounts below minimum threshold".to_string());
        }

        // Update reserves and total liquidity
        self.reserve_a -= amount_a;
        self.reserve_b -= amount_b;
        self.total_liquidity -= liquidity_tokens;

        Ok((amount_a, amount_b))
    }

    /// Swap token A for token B
    pub fn swap_a_for_b(
        &mut self,
        _user: Address,
        amount_in: u64,
        min_amount_out: u64,
    ) -> Result<u64, String> {
        if amount_in == 0 {
            return Err("Input amount must be greater than 0".to_string());
        }

        // Calculate output amount using constant product formula
        let amount_out = self.calculate_swap_output(amount_in, self.reserve_a, self.reserve_b);

        if amount_out < min_amount_out {
            return Err("Insufficient output amount".to_string());
        }

        // Update reserves
        self.reserve_a += amount_in;
        self.reserve_b -= amount_out;

        Ok(amount_out)
    }

    /// Swap token B for token A
    pub fn swap_b_for_a(
        &mut self,
        _user: Address,
        amount_in: u64,
        min_amount_out: u64,
    ) -> Result<u64, String> {
        if amount_in == 0 {
            return Err("Input amount must be greater than 0".to_string());
        }

        // Calculate output amount using constant product formula
        let amount_out = self.calculate_swap_output(amount_in, self.reserve_b, self.reserve_a);

        if amount_out < min_amount_out {
            return Err("Insufficient output amount".to_string());
        }

        // Update reserves
        self.reserve_b += amount_in;
        self.reserve_a -= amount_out;

        Ok(amount_out)
    }

    /// Calculate swap output amount using constant product formula
    fn calculate_swap_output(&self, amount_in: u64, reserve_in: u64, reserve_out: u64) -> u64 {
        if reserve_in == 0 || reserve_out == 0 {
            return 0;
        }

        // Apply fee
        let fee_amount = amount_in.checked_mul(self.fee_percentage as u64).unwrap() / 10000;
        let amount_in_after_fee = amount_in - fee_amount;

        // Constant product formula: (x * y = k)
        // amount_out = (reserve_out * amount_in_after_fee) / (reserve_in + amount_in_after_fee)
        reserve_out.checked_mul(amount_in_after_fee).unwrap() / (reserve_in + amount_in_after_fee)
    }

    /// Get current price of token A in terms of token B
    pub fn get_price_a_to_b(&self) -> f64 {
        if self.reserve_a == 0 {
            return 0.0;
        }
        self.reserve_b as f64 / self.reserve_a as f64
    }

    /// Get current price of token B in terms of token A
    pub fn get_price_b_to_a(&self) -> f64 {
        if self.reserve_b == 0 {
            return 0.0;
        }
        self.reserve_a as f64 / self.reserve_b as f64
    }

    /// Get liquidity position for a user
    pub fn get_liquidity_position(&self, _env: &Env, user: Address) -> LiquidityPosition {
        // In a real implementation, this would query the contract state
        // For now, return a placeholder
        LiquidityPosition {
            user,
            liquidity_tokens: 0,
            share_percentage: 0,
        }
    }

    /// Get all liquidity positions
    pub fn get_all_liquidity_positions(&self, _env: &Env) -> Vec<LiquidityPosition> {
        // In a real implementation, this would query the contract state
        // For now, return an empty vector
        Vec::new(&Env::default())
    }

    /// Simulate a swap without executing it
    pub fn simulate_swap(&self, _env: &Env, token_in: soroban_sdk::String, amount_in: u64) -> Result<u64, String> {
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
    use soroban_sdk::{Env, Address};
    use soroban_sdk::testutils::Address as _;

    #[test]
    fn test_pool_creation() {
        let env = Env::default();
        let pool = LiquidityPoolContract::new_std(
            &env,
            "TOKEN_A_CONTRACT".to_string(),
            "TOKEN_B_CONTRACT".to_string(),
        );

        assert_eq!(pool.token_a, soroban_sdk::String::from_str(&env, "TOKEN_A_CONTRACT"));
        assert_eq!(pool.token_b, soroban_sdk::String::from_str(&env, "TOKEN_B_CONTRACT"));
        assert_eq!(pool.reserve_a, 0);
        assert_eq!(pool.reserve_b, 0);
        assert_eq!(pool.total_liquidity, 0);
        assert_eq!(pool.fee_percentage, 30);
    }

    #[test]
    fn test_pool_creation_with_custom_fee() {
        let env = Env::default();
        let pool = LiquidityPoolContract::new_with_fee(
            &env,
            soroban_sdk::String::from_str(&env, "TOKEN_A_CONTRACT"),
            soroban_sdk::String::from_str(&env, "TOKEN_B_CONTRACT"),
            50, // 0.5%
        );

        assert_eq!(pool.fee_percentage, 50);
    }

    #[test]
    fn test_add_initial_liquidity() {
        let env = Env::default();
        let mut pool = LiquidityPoolContract::new_std(
            &env,
            "TOKEN_A_CONTRACT".to_string(),
            "TOKEN_B_CONTRACT".to_string(),
        );
        let provider = Address::generate(&env);

        let liquidity = pool
            .add_liquidity(&env, provider, 1000, 2000, 1000, 2000)
            .unwrap();

        assert_eq!(pool.reserve_b, 2000);
        assert_eq!(pool.total_liquidity, liquidity);
        assert_eq!(liquidity, ((1000 * 2000) as f64).sqrt() as u64);
    }

    #[test]
    fn test_swap_calculation() {
        let env = Env::default();
        let mut pool = LiquidityPoolContract::new_std(
            &env,
            "TOKEN_A_CONTRACT".to_string(),
            "TOKEN_B_CONTRACT".to_string(),
        );
        
        // Set initial reserves
        pool.reserve_a = 1000;
        pool.reserve_b = 2000;

        let output = pool.calculate_swap_output(100, pool.reserve_a, pool.reserve_b);
        
        // With 30 bps fee (0.3%), amount_in_after_fee = 100 * (10000 - 30) / 10000 = 99.7
        // output = (2000 * 99.7) / (1000 + 99.7) = 199400 / 1099.7 ≈ 181.35
        assert!(output > 180 && output < 182);
    }

    #[test]
    fn test_price_calculation() {
        let env = Env::default();
        let mut pool = LiquidityPoolContract::new_std(
            &env,
            "TOKEN_A_CONTRACT".to_string(),
            "TOKEN_B_CONTRACT".to_string(),
        );
        
        pool.reserve_a = 1000;
        pool.reserve_b = 2000;

        let price_a_to_b = pool.get_price_a_to_b();
        let price_b_to_a = pool.get_price_b_to_a();

        assert_eq!(price_a_to_b, 2.0); // 2000 / 1000
        assert_eq!(price_b_to_a, 0.5); // 1000 / 2000
    }

    #[test]
    fn test_invalid_add_liquidity() {
        let env = Env::default();
        let mut pool = LiquidityPoolContract::new_std(
            &env,
            "TOKEN_A_CONTRACT".to_string(),
            "TOKEN_B_CONTRACT".to_string(),
        );
        let provider = Address::generate(&env);

        let result = pool.add_liquidity(&env, provider, 0, 1000, 0, 1000);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Amounts must be greater than 0");
    }
}
