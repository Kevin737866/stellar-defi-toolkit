//! Liquidity Pool contract implementation for Stellar DeFi Toolkit
//!
//! Provides automated market maker (AMM) functionality for creating
//! liquidity pools between different tokens on the Stellar blockchain.

use soroban_sdk::{contract, contractimpl, Address, Env, Symbol, Vec, Map};
use crate::types::pool::{PoolInfo, LiquidityPosition, SwapParams};
use crate::types::stablecoin::OraclePrice;
use crate::utils::StellarClient;

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
    fee_percentage: u32,
    /// Flash loan fee percentage (in basis points, e.g., 9 = 0.09%)
    flash_loan_fee_bps: u32,
    /// Price oracle address
    oracle_address: Option<Address>,
    /// Whether the pool is in emergency mode
    is_emergency_mode: bool,
    /// Admin address
    admin: Option<Address>,
    /// Contract address
    address: Option<Address>,
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
            fee_percentage: 30, // 0.3% standard fee
            flash_loan_fee_bps: 9, // 0.09% default flash loan fee
            oracle_address: None,
            is_emergency_mode: false,
            admin: None,
            address: None,
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
            flash_loan_fee_bps: 9,
            oracle_address: None,
            is_emergency_mode: false,
            admin: None,
            address: None,
        }
    }

    /// Set the admin address
    pub fn set_admin(&mut self, admin: Address) {
        self.admin = Some(admin);
    }

    /// Set the price oracle address
    pub fn set_oracle(&mut self, caller: Address, oracle: Address) -> Result<(), String> {
        if let Some(admin) = &self.admin {
            if caller != *admin {
                return Err("Only admin can set oracle address".to_string());
            }
        }
        self.oracle_address = Some(oracle);
        Ok(())
    }

    /// Toggle emergency mode (only admin)
    pub fn set_emergency_mode(&mut self, caller: Address, enabled: bool) -> Result<(), String> {
        if let Some(admin) = &self.admin {
            if caller != *admin {
                return Err("Only admin can set emergency mode".to_string());
            }
        } else {
            // If no admin is set, we allow the first caller to set it or just allow it for now
            // In a real contract, the admin would be set during initialization
            self.admin = Some(caller);
        }
        
        self.is_emergency_mode = enabled;
        Ok(())
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
            flash_loan_fee_bps: self.flash_loan_fee_bps,
            oracle_address: self.oracle_address.clone(),
            is_emergency_mode: self.is_emergency_mode,
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
        if self.is_emergency_mode {
            return Err("Emergency mode active: adding liquidity is disabled".to_string());
        }

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

    /// Emergency withdrawal for LP providers
    /// Only works when is_emergency_mode is true
    /// Bypasses minimum amount checks to allow users to exit as quickly as possible
    pub fn emergency_withdraw(
        &mut self,
        provider: Address,
        liquidity_tokens: u64,
    ) -> Result<(u64, u64), String> {
        if !self.is_emergency_mode {
            return Err("Emergency mode is not active. Use regular remove_liquidity.".to_string());
        }

        if liquidity_tokens == 0 {
            return Err("Liquidity tokens must be greater than 0".to_string());
        }

        if liquidity_tokens > self.total_liquidity {
            return Err("Insufficient liquidity tokens".to_string());
        }

        // Calculate amounts to return based on liquidity token ratio
        let amount_a = liquidity_tokens.checked_mul(self.reserve_a).unwrap() / self.total_liquidity;
        let amount_b = liquidity_tokens.checked_mul(self.reserve_b).unwrap() / self.total_liquidity;

        // Update reserves and total liquidity
        self.reserve_a -= amount_a;
        self.reserve_b -= amount_b;
        self.total_liquidity -= liquidity_tokens;

        Ok((amount_a, amount_b))
    }

    /// Swap token A for token B
    pub fn swap_a_for_b(
        &mut self,
        user: Address,
        amount_in: u64,
        min_amount_out: u64,
    ) -> Result<u64, String> {
        if self.is_emergency_mode {
            return Err("Emergency mode active: swaps are disabled".to_string());
        }

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

    /// Get oracle price for a token
    /// In a real Soroban contract, this would call the PriceOracleContract
    pub fn get_oracle_price(&self, token: String) -> Result<u64, String> {
        if let Some(oracle_addr) = &self.oracle_address {
            // Simulated oracle call
            // In reality: let price: OraclePrice = env.invoke_contract(oracle_addr, "get_price", vec![&env, token]);
            
            // For simulation, we return the spot price with some potential variance
            // but we'll use a fixed value or based on reserves if oracle is not "real"
            if token == self.token_a {
                Ok(1_000_000) // $1.00
            } else if token == self.token_b {
                let price = self.get_price_a_to_b();
                Ok((price * 1_000_000.0) as u64)
            } else {
                Err("Invalid token for oracle price".to_string())
            }
        } else {
            Err("Oracle address not set".to_string())
        }
    }

    /// Check for price divergence between spot and oracle
    pub fn check_price_divergence(&self, max_divergence_bps: u32) -> Result<bool, String> {
        let oracle_price_a = self.get_oracle_price(self.token_a.clone())?;
        let oracle_price_b = self.get_oracle_price(self.token_b.clone())?;
        
        if oracle_price_a == 0 || oracle_price_b == 0 {
            return Err("Invalid oracle prices".to_string());
        }

        let oracle_ratio = (oracle_price_b as f64) / (oracle_price_a as f64);
        let spot_ratio = self.get_price_a_to_b();

        if spot_ratio == 0.0 {
            return Ok(false);
        }

        let diff = (oracle_ratio - spot_ratio).abs();
        let divergence_bps = ((diff / spot_ratio) * 10000.0) as u32;

        Ok(divergence_bps <= max_divergence_bps)
    }

    /// Swap token B for token A
    pub fn swap_b_for_a(
        &mut self,
        user: Address,
        amount_in: u64,
        min_amount_out: u64,
    ) -> Result<u64, String> {
        if self.is_emergency_mode {
            return Err("Emergency mode active: swaps are disabled".to_string());
        }

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

    /// Flash loan functionality
    pub fn flash_loan(
        &mut self,
        receiver: Address,
        token: String,
        amount: u64,
    ) -> Result<u64, String> {
        if self.is_emergency_mode {
            return Err("Emergency mode active: flash loans are disabled".to_string());
        }

        if amount == 0 {
            return Err("Amount must be greater than 0".to_string());
        }

        // Identify which reserve to use
        let (reserve_ptr, token_name) = if token == self.token_a {
            (&mut self.reserve_a, "Token A")
        } else if token == self.token_b {
            (&mut self.reserve_b, "Token B")
        } else {
            return Err("Invalid token for flash loan".to_string());
        };

        if amount > *reserve_ptr {
            return Err(format!("Insufficient {} liquidity", token_name));
        }

        // Calculate fee
        let fee = amount.checked_mul(self.flash_loan_fee_bps as u64).unwrap() / 10000;
        
        // In a real Soroban contract, we would:
        // 1. Transfer 'amount' to 'receiver'
        // 2. Call 'on_flash_loan' on 'receiver'
        // 3. Verify that 'amount + fee' was returned
        
        // For this implementation, we simulate the fee accrual to the reserves
        *reserve_ptr += fee;

        Ok(fee)
    }

    /// Get liquidity position for a user
    pub fn get_liquidity_position(&self, user: Address) -> LiquidityPosition {
        // In a real implementation, this would query the contract state
        // For now, return a placeholder
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

    #[test]
    fn test_emergency_mode_behavior() {
        let mut pool = LiquidityPoolContract::new(
            "TOKEN_A_CONTRACT".to_string(),
            "TOKEN_B_CONTRACT".to_string(),
        );
        let admin = Address::generate(&Env::default());
        let provider = Address::generate(&Env::default());
        
        pool.set_admin(admin.clone());

        // Add some initial liquidity
        pool.add_liquidity(provider.clone(), 1000, 2000, 1000, 2000).unwrap();
        let liquidity_tokens = pool.total_liquidity;

        // Enable emergency mode
        pool.set_emergency_mode(admin.clone(), true).unwrap();
        assert!(pool.is_emergency_mode);

        // Try adding more liquidity - should fail
        let result = pool.add_liquidity(provider.clone(), 500, 1000, 500, 1000);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Emergency mode active: adding liquidity is disabled");

        // Try swapping - should fail
        let result = pool.swap_a_for_b(provider.clone(), 100, 0);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Emergency mode active: swaps are disabled");

        // Emergency withdraw - should work and bypass min_amount checks (implicitly)
        let (amount_a, amount_b) = pool.emergency_withdraw(provider.clone(), liquidity_tokens).unwrap();
        assert_eq!(amount_a, 1000);
        assert_eq!(amount_b, 2000);
        assert_eq!(pool.total_liquidity, 0);
    }

    #[test]
    fn test_flash_loan_behavior() {
        let mut pool = LiquidityPoolContract::new(
            "TOKEN_A_CONTRACT".to_string(),
            "TOKEN_B_CONTRACT".to_string(),
        );
        let receiver = Address::generate(&Env::default());

        // Add liquidity first
        pool.reserve_a = 1000000; // 1M
        pool.reserve_b = 2000000; // 2M

        // Flash loan token A
        let fee = pool.flash_loan(receiver.clone(), "TOKEN_A_CONTRACT".to_string(), 100000).unwrap();
        
        // Default fee is 0.09% (9 bps)
        // 100,000 * 9 / 10,000 = 90
        assert_eq!(fee, 90);
        assert_eq!(pool.reserve_a, 1000000 + 90);

        // Flash loan too much
        let result = pool.flash_loan(receiver.clone(), "TOKEN_B_CONTRACT".to_string(), 3000000);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Insufficient Token B liquidity");

        // Flash loan invalid token
        let result = pool.flash_loan(receiver.clone(), "INVALID_TOKEN".to_string(), 100);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Invalid token for flash loan");
    }

    #[test]
    fn test_oracle_integration() {
        let mut pool = LiquidityPoolContract::new(
            "TOKEN_A_CONTRACT".to_string(),
            "TOKEN_B_CONTRACT".to_string(),
        );
        let admin = Address::generate(&Env::default());
        let oracle = Address::generate(&Env::default());

        pool.set_admin(admin.clone());
        
        // Oracle initially not set
        assert!(pool.get_oracle_price("TOKEN_A_CONTRACT".to_string()).is_err());

        // Set oracle
        pool.set_oracle(admin.clone(), oracle.clone()).unwrap();
        assert_eq!(pool.oracle_address, Some(oracle));

        // Set reserves to establish a spot price
        pool.reserve_a = 1000;
        pool.reserve_b = 2000; // Spot price A:B = 2.0

        let price_a = pool.get_oracle_price("TOKEN_A_CONTRACT".to_string()).unwrap();
        let price_b = pool.get_oracle_price("TOKEN_B_CONTRACT".to_string()).unwrap();

        assert_eq!(price_a, 1_000_000);
        assert_eq!(price_b, 2_000_000);

        // Check divergence (should be 0 since simulated oracle matches spot)
        let within_limits = pool.check_price_divergence(100).unwrap(); // 1% limit
        assert!(within_limits);
    }
}
