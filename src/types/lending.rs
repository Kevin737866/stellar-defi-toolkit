use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::utils::WAD;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InterestRateModel {
    pub base_rate: i128,
    pub slope_1: i128,
    pub slope_2: i128,
    pub optimal_utilization: i128,
}

impl Default for InterestRateModel {
    fn default() -> Self {
        Self {
            base_rate: 20_000_000,
            slope_1: 80_000_000,
            slope_2: 1_200_000_000,
            optimal_utilization: 800_000_000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReserveConfig {
    pub asset: String,
    pub decimals: u32,
    pub collateral_factor_bps: u32,
    pub liquidation_threshold_bps: u32,
    pub liquidation_bonus_bps: u32,
    pub reserve_factor_bps: u32,
    pub flash_loan_fee_bps: u32,
    pub borrow_enabled: bool,
    pub deposit_enabled: bool,
    pub flash_loan_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReserveState {
    pub total_cash: i128,
    pub total_debt: i128,
    pub total_supply_shares: i128,
    pub total_debt_shares: i128,
    pub protocol_fees: i128,
    pub last_accrual_ts: u64,
}

impl Default for ReserveState {
    fn default() -> Self {
        Self {
            total_cash: 0,
            total_debt: 0,
            total_supply_shares: 0,
            total_debt_shares: 0,
            protocol_fees: 0,
            last_accrual_ts: 0,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccountPosition {
    pub supplied_shares: std::collections::BTreeMap<String, i128>,
    pub debt_shares: std::collections::BTreeMap<String, i128>,
    pub collateral_enabled: std::collections::BTreeMap<String, bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PositionSnapshot {
    pub supplied_amounts: std::collections::BTreeMap<String, i128>,
    pub debt_amounts: std::collections::BTreeMap<String, i128>,
    pub collateral_value: i128,
    pub liquidation_value: i128,
    pub debt_value: i128,
    pub health_factor: i128,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FlashLoanReceipt {
    pub asset: String,
    pub amount: i128,
    pub fee_paid: i128,
    pub protocol_fee: i128,
    pub supplier_fee: i128,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LiquidationResult {
    pub repaid_amount: i128,
    pub seized_collateral: i128,
    pub liquidator_discount_value: i128,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProtocolSnapshot {
    pub reserves: std::collections::BTreeMap<String, ReserveState>,
    pub reserve_configs: std::collections::BTreeMap<String, ReserveConfig>,
    pub treasury: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProtocolError {
    #[error("asset already exists")]
    AssetAlreadyExists,
    #[error("unknown asset")]
    UnknownAsset,
    #[error("only admin can perform this action")]
    Unauthorized,
    #[error("deposits are disabled for asset {0}")]
    DepositsDisabled(String),
    #[error("borrows are disabled for asset {0}")]
    BorrowsDisabled(String),
    #[error("flash loans are disabled for asset {0}")]
    FlashLoansDisabled(String),
    #[error("amount must be positive")]
    InvalidAmount,
    #[error("insufficient liquidity")]
    InsufficientLiquidity,
    #[error("insufficient balance")]
    InsufficientBalance,
    #[error("insufficient collateral")]
    InsufficientCollateral,
    #[error("position remains undercollateralized")]
    HealthFactorTooLow,
    #[error("loan is healthy and cannot be liquidated")]
    PositionNotLiquidatable,
    #[error("nothing to repay")]
    NothingToRepay,
    #[error("invalid flash loan repayment")]
    InvalidFlashLoanRepayment,
    #[error("collateral already disabled")]
    CollateralAlreadyDisabled,
    #[error("math error")]
    MathFailure,
    #[error("price unavailable for asset {0}")]
    MissingPrice(String),
}

impl InterestRateModel {
    pub fn borrow_rate(&self, utilization: i128) -> i128 {
        if utilization <= self.optimal_utilization {
            self.base_rate + utilization * self.slope_1 / self.optimal_utilization
        } else if self.optimal_utilization >= WAD {
            self.base_rate + self.slope_1
        } else {
            let excess_utilization = utilization - self.optimal_utilization;
            let excess_capacity = WAD - self.optimal_utilization;
            self.base_rate + self.slope_1 + excess_utilization * self.slope_2 / excess_capacity
        }
    }
}
