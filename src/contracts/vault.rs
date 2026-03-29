//! Yield Farming Vault contract for Stellar DeFi Toolkit
//!
//! Implements an automated yield farming vault that optimizes returns across
//! multiple Stellar DeFi protocols using Soroban smart contracts.
//!
//! ## Architecture
//!
//! - Vault token (share representation via SEP-41)
//! - Strategy pattern for pluggable yield sources
//! - Auto-compounding mechanism
//! - Performance fee structure
//! - Emergency pause mechanism
//! - Yield optimization algorithm
//! - Strategy switching logic
//! - Harvest and reinvest functions

use soroban_sdk::{contract, Address, Env, Vec};
use soroban_sdk::testutils::Address as _;
use crate::types::{
    VaultInfo, VaultStrategy, VaultStats, HarvestResult, DepositResult,
    WithdrawResult,
};

// ─── Storage Keys ────────────────────────────────────────────────────────────

// Storage keys are not used as fields are stored directly in the contract struct

/// Default performance fee: 10% (1000 bps)
const DEFAULT_PERFORMANCE_FEE_BPS: u32 = 1000;
/// Maximum performance fee: 30% (3000 bps)
const MAX_PERFORMANCE_FEE_BPS: u32 = 3000;
/// Minimum harvest interval: 1 hour in seconds
const MIN_HARVEST_INTERVAL: u64 = 3600;

// ─── Vault Contract ───────────────────────────────────────────────────────────

/// Automated yield farming vault with multi-strategy optimization
#[contract]
pub struct YieldVaultContract {
    /// Underlying asset token address (what users deposit)
    pub asset_token: Address,
    /// Vault share token address (SEP-41 compliant)
    pub share_token: Address,
    /// Total shares issued
    pub total_shares: u64,
    /// Total assets under management
    pub total_assets: u64,
    /// Active strategies
    pub strategies: Vec<VaultStrategy>,
    /// Active strategy index
    pub active_strategy_index: u32,
    /// Performance fee in basis points
    pub performance_fee_bps: u32,
    /// Treasury address for fee collection
    pub treasury: Option<Address>,
    /// Admin address
    pub admin: Option<Address>,
    /// Whether the vault is paused
    pub paused: bool,
    /// Last harvest timestamp
    pub last_harvest: u64,
    /// Accumulated uncollected fees
    pub accumulated_fees: u64,
    /// Contract address
    pub address: Option<Address>,
}

impl YieldVaultContract {
    /// Create a new yield vault
    pub fn new(env: &Env, asset_token: Address, share_token: Address) -> Self {
        Self {
            asset_token,
            share_token,
            total_shares: 0,
            total_assets: 0,
            strategies: Vec::new(env),
            active_strategy_index: 0,
            performance_fee_bps: DEFAULT_PERFORMANCE_FEE_BPS,
            treasury: None,
            admin: None,
            paused: false,
            last_harvest: 0,
            accumulated_fees: 0,
            address: None,
        }
    }

    /// Create from std string
    pub fn new_std(env: &Env, _asset_token: String, _share_token: String) -> Self {
        Self::new(
            env,
            Address::generate(env),
            Address::generate(env),
        )
    }

    /// Initialize vault with admin and treasury
    pub fn initialize(
        mut self,
        admin: Address,
        treasury: Address,
        performance_fee_bps: u32,
    ) -> Result<Self, String> {
        if performance_fee_bps > MAX_PERFORMANCE_FEE_BPS {
            return Err(format!(
                "Performance fee exceeds maximum of {} bps",
                MAX_PERFORMANCE_FEE_BPS
            ));
        }
        self.admin = Some(admin);
        self.treasury = Some(treasury);
        self.performance_fee_bps = performance_fee_bps;
        Ok(self)
    }

    // ─── Strategy Management ──────────────────────────────────────────────────

    pub fn add_strategy(&mut self, strategy: VaultStrategy) -> Result<u32, String> {
        self.require_admin()?;
        if self.strategies.len() >= 10 {
            return Err("Maximum of 10 strategies allowed".to_string());
        }
        self.strategies.push_back(strategy);
        Ok(self.strategies.len() - 1)
    }

    /// Switch to a different strategy (admin only)
    ///
    /// Withdraws all funds from the current strategy, then deposits into the new one.
    pub fn switch_strategy(&mut self, new_index: u32) -> Result<(), String> {
        self.require_admin()?;
        self.require_not_paused()?;
 
        if new_index >= self.strategies.len() {
            return Err("Strategy index out of bounds".to_string());
        }
 
        if new_index == self.active_strategy_index {
            return Err("Already using this strategy".to_string());
        }
 
        // 1. Harvest pending rewards from current strategy before switching
        let _ = self.harvest_from_strategy(self.active_strategy_index);
 
        // 2. Withdraw all assets from current strategy
        let withdrawn = self.withdraw_from_strategy(self.active_strategy_index, self.total_assets)?;
 
        // 3. Deposit into new strategy
        self.deposit_into_strategy(new_index, withdrawn)?;
 
        self.active_strategy_index = new_index;
        Ok(())
    }

    /// Get the best strategy by estimated APY
    pub fn get_optimal_strategy_index(&self) -> u32 {
        if self.strategies.is_empty() {
            return 0;
        }

        let mut best_index = 0;
        let mut best_apy = 0;

        for (i, strategy) in self.strategies.iter().enumerate() {
            if strategy.estimated_apy > best_apy {
                best_apy = strategy.estimated_apy;
                best_index = i as u32;
            }
        }

        best_index
    }

    /// Auto-optimize: switch to the highest-yield strategy if it's significantly better
    ///
    /// Only switches if the new strategy APY exceeds current by at least `threshold_bps`.
    pub fn optimize_strategy(&mut self, threshold_bps: u32) -> Result<bool, String> {
        self.require_admin()?;
        self.require_not_paused()?;

        let optimal = self.get_optimal_strategy_index();
        if optimal == self.active_strategy_index {
            return Ok(false);
        }

        let current_apy = self
            .strategies
            .get(self.active_strategy_index)
            .map(|s| s.estimated_apy)
            .unwrap_or(0);

        let optimal_apy = self
            .strategies
            .get(optimal)
            .map(|s| s.estimated_apy)
            .unwrap_or(0);

        let improvement_bps = if current_apy > 0 {
            (optimal_apy.saturating_sub(current_apy) as u64 * 10000 / current_apy as u64) as u32
        } else if optimal_apy > 0 {
            10000 // Infinite improvement
        } else {
            0
        };

        if improvement_bps >= threshold_bps {
            self.switch_strategy(optimal)?;
            return Ok(true);
        }

        Ok(false)
    }

    // ─── Deposit / Withdrawal ─────────────────────────────────────────────────

    /// Deposit assets into the vault and receive share tokens
    ///
    /// Shares are calculated as: `shares = (amount * total_shares) / total_assets`
    /// For the first deposit: `shares = amount`
    pub fn deposit(&mut self, depositor: Address, amount: u64) -> Result<DepositResult, String> {
        self.require_not_paused()?;

        if amount == 0 {
            return Err("Deposit amount must be greater than 0".to_string());
        }

        // Calculate shares to mint
        let shares_to_mint = self.calculate_shares_for_amount(amount);

        // Deploy assets into active strategy
        if !self.strategies.is_empty() {
            self.deposit_into_strategy(self.active_strategy_index, amount)?;
        }

        // Update state
        self.total_assets += amount;
        self.total_shares += shares_to_mint;

        Ok(DepositResult {
            depositor,
            amount_deposited: amount,
            shares_minted: shares_to_mint,
            share_price: self.get_share_price(),
        })
    }

    /// Withdraw assets from the vault by burning share tokens
    ///
    /// Assets returned: `amount = (shares * total_assets) / total_shares`
    pub fn withdraw(&mut self, withdrawer: Address, shares: u64) -> Result<WithdrawResult, String> {
        self.require_not_paused()?;

        if shares == 0 {
            return Err("Shares amount must be greater than 0".to_string());
        }

        if shares > self.total_shares {
            return Err("Insufficient shares".to_string());
        }

        // Calculate assets to return
        let assets_to_return = self.calculate_assets_for_shares(shares);

        // Withdraw from active strategy
        if !self.strategies.is_empty() {
            self.withdraw_from_strategy(self.active_strategy_index, assets_to_return)?;
        }

        // Update state
        self.total_assets = self.total_assets.saturating_sub(assets_to_return);
        self.total_shares -= shares;

        Ok(WithdrawResult {
            withdrawer,
            shares_burned: shares,
            amount_withdrawn: assets_to_return,
            share_price: self.get_share_price(),
        })
    }

    // ─── Harvest & Compound ───────────────────────────────────────────────────

    /// Harvest rewards from the active strategy and auto-compound
    ///
    /// 1. Collect rewards from the strategy
    /// 2. Deduct performance fee
    /// 3. Reinvest remaining rewards back into the strategy
    pub fn harvest(&mut self) -> Result<HarvestResult, String> {
        self.require_not_paused()?;

        let current_time = self.get_current_timestamp();
        if current_time < self.last_harvest + MIN_HARVEST_INTERVAL {
            return Err("Harvest too soon: minimum interval not elapsed".to_string());
        }

        let raw_rewards = self.harvest_from_strategy(self.active_strategy_index)?;

        if raw_rewards == 0 {
            return Ok(HarvestResult {
                raw_rewards: 0,
                performance_fee: 0,
                net_rewards: 0,
                compounded_amount: 0,
                new_total_assets: self.total_assets,
            });
        }

        // Deduct performance fee
        let performance_fee = self.calculate_performance_fee(raw_rewards);
        let net_rewards = raw_rewards - performance_fee;

        // Accumulate fees for treasury collection
        self.accumulated_fees += performance_fee;

        // Reinvest net rewards (auto-compound)
        if net_rewards > 0 && !self.strategies.is_empty() {
            self.deposit_into_strategy(self.active_strategy_index, net_rewards)?;
            self.total_assets += net_rewards;
        }

        self.last_harvest = current_time;

        Ok(HarvestResult {
            raw_rewards,
            performance_fee,
            net_rewards,
            compounded_amount: net_rewards,
            new_total_assets: self.total_assets,
        })
    }

    /// Collect accumulated performance fees to treasury (admin only)
    pub fn collect_fees(&mut self) -> Result<u64, String> {
        self.require_admin()?;

        if self.accumulated_fees == 0 {
            return Err("No fees to collect".to_string());
        }

        if self.treasury.is_none() {
            return Err("Treasury address not set".to_string());
        }

        let fees = self.accumulated_fees;
        self.accumulated_fees = 0;

        // In a real implementation: transfer `fees` to self.treasury
        Ok(fees)
    }

    // ─── Emergency Controls ───────────────────────────────────────────────────

    /// Pause the vault (admin only) — blocks deposits, withdrawals, and harvests
    pub fn pause(&mut self) -> Result<(), String> {
        self.require_admin()?;
        if self.paused {
            return Err("Vault is already paused".to_string());
        }
        self.paused = true;
        Ok(())
    }

    /// Unpause the vault (admin only)
    pub fn unpause(&mut self) -> Result<(), String> {
        self.require_admin()?;
        if !self.paused {
            return Err("Vault is not paused".to_string());
        }
        self.paused = false;
        Ok(())
    }

    /// Emergency withdraw all assets from the active strategy (admin only)
    ///
    /// Pulls all funds back to the vault without harvesting rewards.
    /// Useful when a strategy is compromised.
    pub fn emergency_exit(&mut self) -> Result<u64, String> {
        self.require_admin()?;

        if self.strategies.is_empty() {
            return Err("No active strategy".to_string());
        }

        let withdrawn = self.withdraw_from_strategy(self.active_strategy_index, self.total_assets)?;
        // Keep total_assets accurate; strategy balance is now 0
        Ok(withdrawn)
    }

    // ─── View Functions ───────────────────────────────────────────────────────

    /// Get current share price: `total_assets / total_shares`
    pub fn get_share_price(&self) -> f64 {
        if self.total_shares == 0 {
            return 1.0; // Initial price is 1:1
        }
        self.total_assets as f64 / self.total_shares as f64
    }

    /// Get vault info snapshot
    pub fn get_info(&self, _env: &Env) -> VaultInfo {
        VaultInfo {
            asset_token: self.asset_token.clone(),
            share_token: self.share_token.clone(),
            total_shares: self.total_shares,
            total_assets: self.total_assets,
            share_price: (self.get_share_price() * 10000.0) as u32,
            active_strategy_index: if self.strategies.len() > 0 { self.active_strategy_index as i32 } else { -1 },
            strategy_count: self.strategies.len(),
            performance_fee_bps: self.performance_fee_bps,
            paused: self.paused,
            last_harvest: self.last_harvest,
        }
    }

    /// Get vault statistics
    pub fn get_stats(&self) -> VaultStats {
        let current_apy = self
            .strategies
            .get(self.active_strategy_index)
            .map(|s| s.estimated_apy)
            .unwrap_or(0);

        VaultStats {
            total_assets: self.total_assets,
            total_shares: self.total_shares,
            share_price: self.get_share_price(),
            current_apy: current_apy as f64,
            accumulated_fees: self.accumulated_fees,
            last_harvest: self.last_harvest,
            paused: self.paused,
        }
    }

    /// Calculate how many shares a given asset amount would mint
    pub fn preview_deposit(&self, amount: u64) -> u64 {
        self.calculate_shares_for_amount(amount)
    }

    /// Calculate how many assets a given share amount would return
    pub fn preview_withdraw(&self, shares: u64) -> u64 {
        self.calculate_assets_for_shares(shares)
    }

    /// Update performance fee configuration (admin only)
    pub fn set_performance_fee(&mut self, fee_bps: u32) -> Result<(), String> {
        self.require_admin()?;
        if fee_bps > MAX_PERFORMANCE_FEE_BPS {
            return Err(format!(
                "Fee exceeds maximum of {} bps",
                MAX_PERFORMANCE_FEE_BPS
            ));
        }
        self.performance_fee_bps = fee_bps;
        Ok(())
    }

    /// Update treasury address (admin only)
    pub fn set_treasury(&mut self, treasury: Address) -> Result<(), String> {
        self.require_admin()?;
        self.treasury = Some(treasury);
        Ok(())
    }

    // ─── Internal Helpers ─────────────────────────────────────────────────────

    fn calculate_shares_for_amount(&self, amount: u64) -> u64 {
        if self.total_shares == 0 || self.total_assets == 0 {
            return amount; // 1:1 for first deposit
        }
        amount
            .checked_mul(self.total_shares)
            .unwrap_or(u64::MAX)
            / self.total_assets
    }

    fn calculate_assets_for_shares(&self, shares: u64) -> u64 {
        if self.total_shares == 0 {
            return 0;
        }
        shares
            .checked_mul(self.total_assets)
            .unwrap_or(u64::MAX)
            / self.total_shares
    }

    fn calculate_performance_fee(&self, rewards: u64) -> u64 {
        rewards
            .checked_mul(self.performance_fee_bps as u64)
            .unwrap_or(0)
            / 10000
    }

    /// Simulate depositing into a strategy (cross-contract call in production)
    fn deposit_into_strategy(&self, index: u32, _amount: u64) -> Result<(), String> {
        if index >= self.strategies.len() {
            return Err("Strategy index out of bounds".to_string());
        }
        // In production: invoke strategy contract via cross-contract call
        // soroban_sdk::invoke_contract(&strategy.contract_address, "deposit", (amount,))
        Ok(())
    }

    /// Simulate withdrawing from a strategy (cross-contract call in production)
    fn withdraw_from_strategy(&self, index: u32, amount: u64) -> Result<u64, String> {
        if index >= self.strategies.len() {
            return Err("Strategy index out of bounds".to_string());
        }
        // In production: invoke strategy contract via cross-contract call
        // soroban_sdk::invoke_contract(&strategy.contract_address, "withdraw", (amount,))
        Ok(amount)
    }

    /// Simulate harvesting rewards from a strategy (cross-contract call in production)
    fn harvest_from_strategy(&self, index: u32) -> Result<u64, String> {
        if index >= self.strategies.len() {
            return Ok(0);
        }
        let strategy = self.strategies.get(index).unwrap();
        // Estimate rewards based on APY and time since last harvest
        let elapsed = self
            .get_current_timestamp()
            .saturating_sub(self.last_harvest);
        let annual_rewards =
            (self.total_assets as f64 * (strategy.estimated_apy as f64 / 10000.0)) as u64;
        let rewards = annual_rewards
            .checked_mul(elapsed)
            .unwrap_or(0)
            / (365 * 24 * 3600);
        Ok(rewards)
    }

    fn get_current_timestamp(&self) -> u64 {
        // In production: use Env::ledger().timestamp()
        0
    }

    fn require_admin(&self) -> Result<(), String> {
        // In production: verify env.invoker() == self.admin
        if self.admin.is_none() {
            return Err("Admin not set".to_string());
        }
        Ok(())
    }

    fn require_not_paused(&self) -> Result<(), String> {
        if self.paused {
            return Err("Vault is paused".to_string());
        }
        Ok(())
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{VaultStrategy, StrategyType};
    use soroban_sdk::{Address, Env};
    use soroban_sdk::testutils::Address as _;

    fn make_vault(env: &Env) -> YieldVaultContract {
        let admin = Address::generate(env);
        let treasury = Address::generate(env);
        YieldVaultContract::new_std(env, "ASSET_TOKEN".to_string(), "VAULT_SHARE".to_string())
            .initialize(admin, treasury, DEFAULT_PERFORMANCE_FEE_BPS)
            .unwrap()
    }

    fn make_strategy(env: &Env, name: &str, apy: f64) -> VaultStrategy {
        VaultStrategy {
            name: soroban_sdk::Symbol::new(env, name),
            contract_address: Address::generate(env),
            strategy_type: StrategyType::LiquidityPool,
            estimated_apy: (apy * 100.0) as u32,
            allocated_amount: 0,
            active: true,
        }
    }

    #[test]
    fn test_vault_creation() {
        let env = Env::default();
        let vault = make_vault(&env);
        assert_eq!(vault.total_shares, 0);
        assert_eq!(vault.total_assets, 0);
        assert_eq!(vault.performance_fee_bps, DEFAULT_PERFORMANCE_FEE_BPS);
        assert!(!vault.paused);
    }

    #[test]
    fn test_deposit_first() {
        let env = Env::default();
        let mut vault = make_vault(&env);
        let user = Address::generate(&env);

        let result = vault.deposit(user, 1_000_000).unwrap();
        assert_eq!(result.amount_deposited, 1_000_000);
        assert_eq!(result.shares_minted, 1_000_000); // 1:1 on first deposit
        assert_eq!(vault.total_assets, 1_000_000);
        assert_eq!(vault.total_shares, 1_000_000);
    }

    #[test]
    fn test_deposit_subsequent_share_price() {
        let env = Env::default();
        let mut vault = make_vault(&env);
        let user = Address::generate(&env);

        vault.deposit(user.clone(), 1_000_000).unwrap();
        // Simulate yield: total_assets grows without new shares
        vault.total_assets = 1_100_000; // 10% gain

        let result = vault.deposit(user, 1_100_000).unwrap();
        // share price = 1_100_000 / 1_000_000 = 1.1
        // new shares = 1_100_000 / 1.1 = 1_000_000
        assert_eq!(result.shares_minted, 1_000_000);
    }

    #[test]
    fn test_withdraw() {
        let env = Env::default();
        let mut vault = make_vault(&env);
        let user = Address::generate(&env);

        vault.deposit(user.clone(), 1_000_000).unwrap();
        let result = vault.withdraw(user, 500_000).unwrap();

        assert_eq!(result.shares_burned, 500_000);
        assert_eq!(result.amount_withdrawn, 500_000);
        assert_eq!(vault.total_shares, 500_000);
        assert_eq!(vault.total_assets, 500_000);
    }

    #[test]
    fn test_withdraw_zero_fails() {
        let env = Env::default();
        let mut vault = make_vault(&env);
        let user = Address::generate(&env);
        vault.deposit(user.clone(), 1_000_000).unwrap();
        assert!(vault.withdraw(user, 0).is_err());
    }

    #[test]
    fn test_withdraw_excess_fails() {
        let env = Env::default();
        let mut vault = make_vault(&env);
        let user = Address::generate(&env);
        vault.deposit(user.clone(), 1_000_000).unwrap();
        assert!(vault.withdraw(user, 2_000_000).is_err());
    }

    #[test]
    fn test_add_strategy() {
        let env = Env::default();
        let mut vault = make_vault(&env);
        let idx = vault.add_strategy(make_strategy(&env, "LP_STRATEGY", 12.5)).unwrap();
        assert_eq!(idx, 0);
        assert_eq!(vault.strategies.len(), 1);
    }

    #[test]
    fn test_switch_strategy() {
        let env = Env::default();
        let mut vault = make_vault(&env);
        vault.add_strategy(make_strategy(&env, "STAKING", 8.0)).unwrap();
        vault.add_strategy(make_strategy(&env, "LP", 15.0)).unwrap();

        vault.switch_strategy(1).unwrap();
        assert_eq!(vault.active_strategy_index, 1);
    }

    #[test]
    fn test_switch_same_strategy_fails() {
        let env = Env::default();
        let mut vault = make_vault(&env);
        vault.add_strategy(make_strategy(&env, "STAKING", 8.0)).unwrap();
        assert!(vault.switch_strategy(0).is_err());
    }

    #[test]
    fn test_get_optimal_strategy() {
        let env = Env::default();
        let mut vault = make_vault(&env);
        vault.add_strategy(make_strategy(&env, "LOW", 5.0)).unwrap();
        vault.add_strategy(make_strategy(&env, "MID", 10.0)).unwrap();
        vault.add_strategy(make_strategy(&env, "HIGH", 20.0)).unwrap();

        assert_eq!(vault.get_optimal_strategy_index(), 2);
    }

    #[test]
    fn test_performance_fee_calculation() {
        let env = Env::default();
        let vault = make_vault(&env); // 1000 bps = 10%
        let fee = vault.calculate_performance_fee(100_000);
        assert_eq!(fee, 10_000); // 10% of 100_000
    }

    #[test]
    fn test_set_performance_fee() {
        let env = Env::default();
        let mut vault = make_vault(&env);
        vault.set_performance_fee(500).unwrap(); // 5%
        assert_eq!(vault.performance_fee_bps, 500);
    }

    #[test]
    fn test_set_performance_fee_exceeds_max() {
        let env = Env::default();
        let mut vault = make_vault(&env);
        assert!(vault.set_performance_fee(5000).is_err()); // 50% > 30% max
    }

    #[test]
    fn test_pause_unpause() {
        let env = Env::default();
        let mut vault = make_vault(&env);

        vault.pause().unwrap();
        assert!(vault.paused);

        // Operations should fail while paused
        let user = Address::generate(&env);
        assert!(vault.deposit(user.clone(), 1000).is_err());
        assert!(vault.withdraw(user, 1000).is_err());
        assert!(vault.harvest().is_err());

        vault.unpause().unwrap();
        assert!(!vault.paused);
    }

    #[test]
    fn test_double_pause_fails() {
        let env = Env::default();
        let mut vault = make_vault(&env);
        vault.pause().unwrap();
        assert!(vault.pause().is_err());
    }

    #[test]
    fn test_unpause_when_not_paused_fails() {
        let env = Env::default();
        let mut vault = make_vault(&env);
        assert!(vault.unpause().is_err());
    }

    #[test]
    fn test_share_price_initial() {
        let env = Env::default();
        let vault = make_vault(&env);
        assert_eq!(vault.get_share_price(), 1.0);
    }

    #[test]
    fn test_share_price_after_yield() {
        let env = Env::default();
        let mut vault = make_vault(&env);
        let user = Address::generate(&env);
        vault.deposit(user, 1_000_000).unwrap();
        vault.total_assets = 1_200_000; // 20% yield
        assert!((vault.get_share_price() - 1.2).abs() < f64::EPSILON);
    }

    #[test]
    fn test_preview_deposit() {
        let env = Env::default();
        let mut vault = make_vault(&env);
        let user = Address::generate(&env);
        vault.deposit(user, 1_000_000).unwrap();
        vault.total_assets = 1_100_000;

        let shares = vault.preview_deposit(1_100_000);
        assert_eq!(shares, 1_000_000);
    }

    #[test]
    fn test_preview_withdraw() {
        let env = Env::default();
        let mut vault = make_vault(&env);
        let user = Address::generate(&env);
        vault.deposit(user, 1_000_000).unwrap();

        let assets = vault.preview_withdraw(500_000);
        assert_eq!(assets, 500_000);
    }

    #[test]
    fn test_collect_fees_no_fees() {
        let env = Env::default();
        let mut vault = make_vault(&env);
        assert!(vault.collect_fees().is_err());
    }

    #[test]
    fn test_emergency_exit_no_strategy() {
        let env = Env::default();
        let mut vault = make_vault(&env);
        assert!(vault.emergency_exit().is_err());
    }

    #[test]
    fn test_emergency_exit_with_strategy() {
        let env = Env::default();
        let mut vault = make_vault(&env);
        let user = Address::generate(&env);
        vault.add_strategy(make_strategy(&env, "STAKING", 10.0)).unwrap();
        vault.deposit(user, 1_000_000).unwrap();

        let withdrawn = vault.emergency_exit().unwrap();
        assert_eq!(withdrawn, 1_000_000);
    }

    #[test]
    fn test_max_strategies() {
        let env = Env::default();
        let mut vault = make_vault(&env);
        for i in 0..10 {
            vault
                .add_strategy(make_strategy(&env, &format!("S{}", i), i as f64))
                .unwrap();
        }
        assert!(vault.add_strategy(make_strategy(&env, "S11", 99.0)).is_err());
    }

    #[test]
    fn test_initialize_fee_too_high() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        let result = YieldVaultContract::new_std(&env, "A".to_string(), "B".to_string())
            .initialize(admin, treasury, 5000); // 50% > 30% max
        assert!(result.is_err());
    }
}
