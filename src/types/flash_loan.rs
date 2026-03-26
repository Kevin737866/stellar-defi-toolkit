//! Flash loan related type definitions

// use serde::{Deserialize, Serialize};
use soroban_sdk::{Address, Bytes, Env};

/// Flash loan information
#[soroban_sdk::contracttype]
#[derive(Debug, Clone)]
pub struct FlashLoanInfo {
    /// Token contract address
    pub token: soroban_sdk::String,
    /// Amount available for loan
    pub amount_available: u64,
    /// Fee percentage in basis points (e.g., 9 = 0.09%)
    pub fee_bps: u32,
    /// Whether the flash loan is active
    pub is_active: bool,
}

/// Flash loan parameters
#[soroban_sdk::contracttype]
#[derive(Debug, Clone)]
pub struct FlashLoanParams {
    /// Token contract address
    pub token: soroban_sdk::String,
    /// Amount to borrow
    pub amount: u64,
    /// Receiver address (must implement on_flash_loan)
    pub receiver: Address,
    /// Arbitrary data for the callback
    pub params: Bytes,
}

/// Flash loan result
#[soroban_sdk::contracttype]
#[derive(Debug, Clone)]
pub struct FlashLoanResult {
    /// Amount borrowed
    pub amount: u64,
    /// Fee paid
    pub fee_amount: u64,
    /// Transaction hash
    pub tx_hash: soroban_sdk::String,
}

/// Flash loan event types
#[soroban_sdk::contracttype]
#[derive(Debug, Clone)]
pub enum FlashLoanEvent {
    /// Flash loan taken event
    LoanTaken(LoanTakenEvent),
    /// Flash loan repaid event
    LoanRepaid(LoanRepaidEvent),
}

/// Flash loan taken event
#[soroban_sdk::contracttype]
#[derive(Debug, Clone)]
pub struct LoanTakenEvent {
    /// Receiver address
    pub receiver: Address,
    /// Token contract address
    pub token: soroban_sdk::String,
    /// Amount borrowed
    pub amount: u64,
    /// Fee to be paid
    pub fee: u64,
    /// Timestamp
    pub timestamp: u64,
}

/// Flash loan repaid event
#[soroban_sdk::contracttype]
#[derive(Debug, Clone)]
pub struct LoanRepaidEvent {
    /// Receiver address
    pub receiver: Address,
    /// Token contract address
    pub token: soroban_sdk::String,
    /// Amount repaid
    pub amount: u64,
    /// Fee paid
    pub fee: u64,
    /// Timestamp
    pub timestamp: u64,
}

/// Liquidation helper parameters
#[soroban_sdk::contracttype]
#[derive(Debug, Clone)]
pub struct LiquidationParams {
    /// Address of the user to liquidate
    pub victim: Address,
    /// Debt token to repay
    pub debt_token: soroban_sdk::String,
    /// Collateral token to seize
    pub collateral_token: soroban_sdk::String,
    /// Amount of debt to repay
    pub debt_to_repay: u64,
    /// Minimum collateral to seize
    pub min_collateral_to_seize: u64,
}

impl FlashLoanInfo {
    /// Create new flash loan info
    pub fn new(_env: &Env, token: soroban_sdk::String, amount_available: u64, fee_bps: u32) -> Self {
        Self {
            token,
            amount_available,
            fee_bps,
            is_active: true,
        }
    }

    /// Calculate the fee for a given amount
    pub fn calculate_fee(&self, amount: u64) -> u64 {
        amount.checked_mul(self.fee_bps as u64).unwrap_or(0) / 10000
    }

    /// Check if a loan for the given amount is possible
    pub fn can_borrow(&self, amount: u64) -> bool {
        self.is_active && amount <= self.amount_available
    }
}
