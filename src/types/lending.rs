use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::utils::WAD;

// ─── Oracle Sanity Configuration ─────────────────────────────────────────────

/// Configuration for oracle price sanity checks.
///
/// These guards protect the protocol against stale, zero, or wildly-deviated
/// prices that could be used to manipulate liquidations or collateral values.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OracleSanityConfig {
    /// Maximum age of a price in seconds before it is considered stale.
    /// A value of `0` disables the staleness check.
    pub max_price_age_secs: u64,
    /// Maximum allowed price deviation from the last accepted price, in basis
    /// points (e.g. `2000` = 20 %).  A value of `0` disables the circuit-breaker.
    pub max_price_deviation_bps: u32,
    /// Minimum acceptable price (inclusive).  Must be > 0.
    pub min_price: i128,
    /// Maximum acceptable price (inclusive).  `0` means no upper bound.
    pub max_price: i128,
}

impl Default for OracleSanityConfig {
    fn default() -> Self {
        Self {
            // Prices older than 1 hour are considered stale.
            max_price_age_secs: 3_600,
            // Reject price updates that deviate more than 20 % from the last
            // accepted price (circuit-breaker).
            max_price_deviation_bps: 2_000,
            // Prices must be strictly positive.
            min_price: 1,
            // No upper bound by default.
            max_price: 0,
        }
    }
}

// ─── Protocol Events ──────────────────────────────────────────────────────────

/// All observable state-changing actions emitted by the lending protocol.
///
/// Consumers (indexers, front-ends, tests) can subscribe to these events to
/// reconstruct protocol state or trigger off-chain workflows.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProtocolEvent {
    /// A new asset reserve was registered.
    AssetRegistered {
        asset: String,
    },
    /// A user deposited funds into a reserve.
    Deposit {
        user: String,
        asset: String,
        amount: i128,
        shares_minted: i128,
    },
    /// A user withdrew funds from a reserve.
    Withdraw {
        user: String,
        asset: String,
        amount: i128,
        shares_burned: i128,
    },
    /// A user borrowed funds from a reserve.
    Borrow {
        user: String,
        asset: String,
        amount: i128,
        shares_minted: i128,
    },
    /// A payer repaid debt on behalf of a borrower.
    Repay {
        payer: String,
        borrower: String,
        asset: String,
        amount: i128,
        shares_burned: i128,
    },
    /// A liquidator partially or fully liquidated an undercollateralised position.
    Liquidate {
        liquidator: String,
        borrower: String,
        debt_asset: String,
        collateral_asset: String,
        repaid_amount: i128,
        seized_collateral: i128,
        liquidator_discount_value: i128,
    },
    /// A flash loan was executed.
    FlashLoan {
        receiver: String,
        asset: String,
        amount: i128,
        fee_paid: i128,
        protocol_fee: i128,
        supplier_fee: i128,
    },
    /// Protocol fees were collected to the treasury.
    FeesCollected {
        asset: String,
        amount: i128,
        treasury: String,
    },
    /// Interest was accrued on a reserve.
    InterestAccrued {
        asset: String,
        accrued: i128,
        timestamp: u64,
    },
    /// Collateral was enabled or disabled for a user.
    CollateralToggled {
        user: String,
        asset: String,
        enabled: bool,
    },
    /// The protocol was paused by an admin.
    Paused {
        admin: String,
    },
    /// The protocol was unpaused by an admin.
    Unpaused {
        admin: String,
    },
    /// An oracle price was rejected by a sanity check.
    OraclePriceRejected {
        asset: String,
        price: i128,
        reason: String,
    },
}

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
    /// Maximum total amount that can be supplied for this asset (0 = no cap).
    pub supply_cap: i128,
    /// Maximum total amount that can be borrowed for this asset (0 = no cap).
    pub borrow_cap: i128,
    /// Per-asset interest rate model. When `None` the protocol-level default is used.
    pub interest_rate_model: Option<InterestRateModel>,
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
    #[error("supply cap exceeded for asset {0}")]
    SupplyCapExceeded(String),
    #[error("borrow cap exceeded for asset {0}")]
    BorrowCapExceeded(String),
    #[error("reserve factor must be <= 10000 bps")]
    InvalidReserveFactor,
    /// Emitted when the protocol is paused and a user-facing operation is attempted.
    #[error("protocol is paused")]
    ProtocolPaused,
    /// Emitted when an oracle price fails a sanity check.
    #[error("oracle price for {0} failed sanity check: {1}")]
    OracleSanityCheckFailed(String, String),
    /// Emitted when an oracle price is stale.
    #[error("oracle price for {0} is stale")]
    OraclePriceStale(String),
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
