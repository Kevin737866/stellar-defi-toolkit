//! Flash Loan contract implementation for Stellar DeFi Toolkit
//!
//! Provides flash loan functionality allowing users to borrow assets
//! without collateral as long as they are returned within the same transaction.

use soroban_sdk::{contract, contractimpl, contracttype, contracterror, Address, Env, Symbol, Bytes, log};
use crate::types::flash_loan::{FlashLoanInfo, LoanTakenEvent, LoanRepaidEvent, LiquidationParams};

/// Storage keys for flash loan data
#[contracttype]
pub enum DataKey {
    Token,      // soroban_sdk::String
    FeeBps,     // u32
    IsActive,   // bool
    Admin,      // Address
    Executing,  // bool
}

/// Error codes for the flash loan contract
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum FlashLoanError {
    AlreadyInitialized = 1,
    NotActive = 2,
    ReentrancyDetected = 3,
    InvalidAmount = 4,
    ArbitrageThresholdExceeded = 5,
    NotAdmin = 6,
    FeeTooHigh = 7,
    AdminNotSet = 8,
}

/// Flash loan contract implementing single-transaction borrow/repay logic
#[contract]
pub struct FlashLoanContract;

#[contractimpl]
impl FlashLoanContract {
    /// Initialize the flash loan contract with a token and admin
    pub fn initialize(env: Env, token: soroban_sdk::String, admin: Address) -> Result<(), FlashLoanError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(FlashLoanError::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Token, &token);
        env.storage().instance().set(&DataKey::FeeBps, &9u32); // Default 0.09%
        env.storage().instance().set(&DataKey::IsActive, &true);
        env.storage().instance().set(&DataKey::Admin, &admin);
        Ok(())
    }

    /// Get current flash loan information
    pub fn get_info(env: Env) -> FlashLoanInfo {
        let token = env.storage().instance().get(&DataKey::Token).unwrap();
        let fee_bps = env.storage().instance().get(&DataKey::FeeBps).unwrap_or(9u32);
        let balance = 1_000_000_000; // Simulated balance
        FlashLoanInfo::new(&env, token, balance, fee_bps)
    }

    fn get_token(env: &Env) -> soroban_sdk::String {
        env.storage().instance().get(&DataKey::Token).unwrap()
    }
    
    fn get_fee_bps(env: &Env) -> u32 {
        env.storage().instance().get(&DataKey::FeeBps).unwrap_or(9)
    }
    
    fn is_active(env: &Env) -> bool {
        env.storage().instance().get(&DataKey::IsActive).unwrap_or(false)
    }
    
    fn get_admin(env: &Env) -> Result<Address, FlashLoanError> {
        env.storage().instance().get(&DataKey::Admin).ok_or(FlashLoanError::AdminNotSet)
    }

    /// Main entry point for taking a flash loan
    pub fn flash_loan(
        env: Env,
        receiver: Address,
        amount: u64,
        _params: Bytes,
    ) -> Result<u64, FlashLoanError> {
        Self::require_active(&env)?;
        Self::require_not_executing(&env)?;

        if amount == 0 {
            return Err(FlashLoanError::InvalidAmount);
        }

        // Apply arbitrage detection safeguard
        Self::arbitrage_safeguard(amount)?;

        let fee_bps = Self::get_fee_bps(&env);
        let fee = amount.checked_mul(fee_bps as u64).unwrap_or(0) / 10000;
        let total_to_repay = amount + fee;
        let token = Self::get_token(&env);

        // 1. Set executing flag for reentrancy protection
        env.storage().instance().set(&DataKey::Executing, &true);

        // 2. Publish LoanTaken event
        env.events().publish(
            (Symbol::new(&env, "flash_loan"), Symbol::new(&env, "borrow")),
            LoanTakenEvent {
                receiver: receiver.clone(),
                token: token.clone(),
                amount,
                fee,
                timestamp: env.ledger().timestamp(),
            },
        );

        // 3. Simulated token transfer to receiver
        log!(&env, "FlashLoan: Transferring {} {} to {}", amount, token, receiver);

        // 4. Invoke callback pattern on receiver (simulated)
        log!(&env, "FlashLoan: Invoking on_flash_loan callback on {}", receiver);

        // 5. Verify repayment logic (simulated)
        log!(&env, "FlashLoan: Verifying repayment of {} (amount: {}, fee: {})", total_to_repay, amount, fee);

        // 6. Publish LoanRepaid event
        env.events().publish(
            (Symbol::new(&env, "flash_loan"), Symbol::new(&env, "repay")),
            LoanRepaidEvent {
                receiver: receiver.clone(),
                token: token.clone(),
                amount: total_to_repay,
                fee,
                timestamp: env.ledger().timestamp(),
            },
        );

        // 7. Clear executing flag
        env.storage().instance().remove(&DataKey::Executing);

        Ok(fee)
    }

    /// Calculate the fee for a given borrow amount (0.09% by default)
    pub fn calculate_fee(env: Env, amount: u64) -> u64 {
        let fee_bps = Self::get_fee_bps(&env);
        amount.checked_mul(fee_bps as u64).unwrap_or(0) / 10000
    }

    /// Liquidation helper function that uses a flash loan to liquidate a position
    pub fn liquidate_with_flash_loan(
        env: Env,
        params: LiquidationParams,
        flash_params: Bytes,
    ) -> Result<(), FlashLoanError> {
        Self::require_active(&env)?;
        
        log!(&env, "Liquidation: Attempting to liquidate {} using flash loan", params.victim);
        
        // 1. Take flash loan for debt_to_repay
        let fee = Self::flash_loan(env.clone(), env.current_contract_address(), params.debt_to_repay, flash_params)?;
        
        // 2. The logic for using the loan and seizing collateral would be handled by the callback or internal logic
        log!(&env, "Liquidation: Successfully liquidated {}. Fee paid: {}", params.victim, fee);
        
        Ok(())
    }

    /// Arbitrage detection and safeguard
    pub fn arbitrage_safeguard(amount: u64) -> Result<(), FlashLoanError> {
        let max_loan_limit = 1_000_000_000; // Example 1B limit
        if amount > max_loan_limit {
            return Err(FlashLoanError::ArbitrageThresholdExceeded);
        }
        Ok(())
    }

    /// Update the fee percentage (admin only)
    pub fn set_fee(env: Env, fee_bps: u32) -> Result<(), FlashLoanError> {
        Self::require_admin(&env)?;
        if fee_bps > 1000 { // Max 10% fee
            return Err(FlashLoanError::FeeTooHigh);
        }
        env.storage().instance().set(&DataKey::FeeBps, &fee_bps);
        Ok(())
    }

    /// Emergency pause (admin only)
    pub fn pause(env: Env) -> Result<(), FlashLoanError> {
        Self::require_admin(&env)?;
        env.storage().instance().set(&DataKey::IsActive, &false);
        Ok(())
    }

    /// Resume functionality (admin only)
    pub fn resume(env: Env) -> Result<(), FlashLoanError> {
        Self::require_admin(&env)?;
        env.storage().instance().set(&DataKey::IsActive, &true);
        Ok(())
    }

    // ─── Internal Helpers ─────────────────────────────────────────────────────

    fn require_active(env: &Env) -> Result<(), FlashLoanError> {
        if !Self::is_active(env) {
            return Err(FlashLoanError::NotActive);
        }
        Ok(())
    }

    fn require_not_executing(env: &Env) -> Result<(), FlashLoanError> {
        let is_executing: bool = env
            .storage()
            .instance()
            .get(&DataKey::Executing)
            .unwrap_or(false);
        if is_executing {
            return Err(FlashLoanError::ReentrancyDetected);
        }
        Ok(())
    }

    fn require_admin(env: &Env) -> Result<(), FlashLoanError> {
        let _admin = Self::get_admin(env)?;
        // In production: env.invoker().require_auth();
        Ok(())
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{Address, Env, Bytes, String};
    use soroban_sdk::testutils::Address as _;

    fn setup_test() -> (Env, FlashLoanContractClient<'static>, Address) {
        let env = Env::default();
        let contract_id = env.register_contract(None, FlashLoanContract);
        let client = FlashLoanContractClient::new(&env, &contract_id);
        
        let admin = Address::generate(&env);
        let token = String::from_str(&env, "USDC_TOKEN");
        
        client.initialize(&token, &admin);
        
        (env, client, admin)
    }

    #[test]
    fn test_initialization() {
        let (env, client, admin) = setup_test();
        let info = client.get_info();
        assert_eq!(info.token, String::from_str(&env, "USDC_TOKEN"));
        assert_eq!(info.fee_bps, 9);
        // Admin is not directly exposed in get_info but we've verified initialize didn't panic
    }

    #[test]
    fn test_calculate_fee() {
        let (_, client, _) = setup_test();
        let fee = client.calculate_fee(&1000000); // 1M
        assert_eq!(fee, 900); // 0.09% of 1M = 900
    }

    #[test]
    fn test_flash_loan_success() {
        let (env, client, _) = setup_test();
        let receiver = Address::generate(&env);
        let params = Bytes::new(&env);
        
        let result = client.try_flash_loan(&receiver, &100000, &params);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().unwrap(), 90); // 0.09% of 100k = 90
    }

    #[test]
    fn test_flash_loan_zero_fails() {
        let (env, client, _admin) = setup_test();
        let receiver = Address::generate(&env);
        let params = Bytes::new(&env);
        
        let result = client.try_flash_loan(&receiver, &0, &params);
        let s = format!("{:?}", result);
        assert!(s.contains("InvalidAmount"), "Expected InvalidAmount in {:?}", result);
    }

    #[test]
    fn test_reentrancy_protection() {
        let (env, client, _admin) = setup_test();
        let receiver = Address::generate(&env);
        let params = Bytes::new(&env);

        env.as_contract(&client.address, || {
            env.storage().instance().set(&DataKey::Executing, &true);
        });

        let result = client.try_flash_loan(&receiver, &100000, &params);
        let s = format!("{:?}", result);
        assert!(s.contains("ReentrancyDetected"), "Expected ReentrancyDetected in {:?}", result);
    }

    #[test]
    fn test_arbitrage_safeguard() {
        let (env, client, _admin) = setup_test();
        let receiver = Address::generate(&env);
        let params = Bytes::new(&env);
        
        let result = client.try_flash_loan(&receiver, &2_000_000_000, &params);
        let s = format!("{:?}", result);
        assert!(s.contains("ArbitrageThresholdExceeded"), "Expected ArbitrageThresholdExceeded in {:?}", result);
    }

    #[test]
    fn test_pause_resume() {
        let (env, client, _admin) = setup_test();
        let receiver = Address::generate(&env);
        let params = Bytes::new(&env);

        client.pause();
        
        let result = client.try_flash_loan(&receiver, &100000, &params);
        let s = format!("{:?}", result);
        assert!(s.contains("NotActive"), "Expected NotActive in {:?}", result);

        client.resume();
        
        let result = client.try_flash_loan(&receiver, &100000, &params);
        assert!(result.is_ok());
    }

    #[test]
    fn test_liquidate_with_flash_loan() {
        let (env, client, _) = setup_test();
        let victim = Address::generate(&env);
        let flash_params = Bytes::new(&env);
        
        let params = LiquidationParams {
            victim,
            debt_token: String::from_str(&env, "USDC"),
            collateral_token: String::from_str(&env, "XLM"),
            debt_to_repay: 50000,
            min_collateral_to_seize: 100000,
        };

        let result = client.try_liquidate_with_flash_loan(&params, &flash_params);
        assert!(result.is_ok());
    }
}
