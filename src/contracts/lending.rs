use std::cmp::min;
use std::collections::BTreeMap;

use crate::contracts::oracle::PriceOracle;
use crate::types::{
    AccountPosition, FlashLoanReceipt, InterestRateModel, LiquidationResult, PositionSnapshot,
    ProtocolError, ProtocolSnapshot, ReserveConfig, ReserveState,
};
use crate::utils::{bps_mul, mul_div, wad_div, WAD, YEAR_IN_SECONDS};

#[derive(Debug, Clone)]
pub struct LendingProtocol {
    admin: String,
    treasury: String,
    interest_rate_model: InterestRateModel,
    reserves: BTreeMap<String, ReserveState>,
    reserve_configs: BTreeMap<String, ReserveConfig>,
    accounts: BTreeMap<String, AccountPosition>,
    close_factor_bps: u32,
}

impl LendingProtocol {
    pub fn new(
        admin: impl Into<String>,
        treasury: impl Into<String>,
        interest_rate_model: InterestRateModel,
    ) -> Self {
        Self {
            admin: admin.into(),
            treasury: treasury.into(),
            interest_rate_model,
            reserves: BTreeMap::new(),
            reserve_configs: BTreeMap::new(),
            accounts: BTreeMap::new(),
            close_factor_bps: 5_000,
        }
    }

    pub fn admin(&self) -> &str {
        &self.admin
    }

    pub fn treasury(&self) -> &str {
        &self.treasury
    }

    pub fn set_close_factor(
        &mut self,
        caller: &str,
        close_factor_bps: u32,
    ) -> Result<(), ProtocolError> {
        self.ensure_admin(caller)?;
        self.close_factor_bps = close_factor_bps;
        Ok(())
    }

    pub fn register_asset(
        &mut self,
        caller: &str,
        config: ReserveConfig,
        now: u64,
    ) -> Result<(), ProtocolError> {
        self.ensure_admin(caller)?;
        if self.reserve_configs.contains_key(&config.asset) {
            return Err(ProtocolError::AssetAlreadyExists);
        }

        let asset = config.asset.clone();
        self.reserve_configs.insert(asset.clone(), config);
        self.reserves.insert(
            asset,
            ReserveState {
                last_accrual_ts: now,
                ..ReserveState::default()
            },
        );
        Ok(())
    }

    pub fn update_reserve_config(
        &mut self,
        caller: &str,
        config: ReserveConfig,
    ) -> Result<(), ProtocolError> {
        self.ensure_admin(caller)?;
        let asset = config.asset.clone();
        let stored = self
            .reserve_configs
            .get_mut(&asset)
            .ok_or(ProtocolError::UnknownAsset)?;
        *stored = config;
        Ok(())
    }

    pub fn accrue_interest(&mut self, asset: &str, now: u64) -> Result<i128, ProtocolError> {
        let state = self
            .reserves
            .get_mut(asset)
            .ok_or(ProtocolError::UnknownAsset)?;
        let config = self
            .reserve_configs
            .get(asset)
            .ok_or(ProtocolError::UnknownAsset)?;

        if now <= state.last_accrual_ts || state.total_debt == 0 {
            state.last_accrual_ts = now;
            return Ok(0);
        }

        let elapsed = i128::from(now - state.last_accrual_ts);
        let supplied = state.total_cash + state.total_debt - state.protocol_fees;
        let utilization = if supplied <= 0 {
            0
        } else {
            wad_div(state.total_debt, supplied).map_err(|_| ProtocolError::MathFailure)?
        };
        let borrow_rate = self.interest_rate_model.borrow_rate(utilization);
        let accrued = mul_div(
            state.total_debt,
            borrow_rate
                .checked_mul(elapsed)
                .ok_or(ProtocolError::MathFailure)?,
            YEAR_IN_SECONDS
                .checked_mul(WAD)
                .ok_or(ProtocolError::MathFailure)?,
        )
        .map_err(|_| ProtocolError::MathFailure)?;
        let reserve_cut =
            bps_mul(accrued, config.reserve_factor_bps).map_err(|_| ProtocolError::MathFailure)?;

        state.total_debt = state
            .total_debt
            .checked_add(accrued)
            .ok_or(ProtocolError::MathFailure)?;
        state.protocol_fees = state
            .protocol_fees
            .checked_add(reserve_cut)
            .ok_or(ProtocolError::MathFailure)?;
        state.last_accrual_ts = now;

        Ok(accrued)
    }

    pub fn deposit(
        &mut self,
        user: &str,
        asset: &str,
        amount: i128,
        now: u64,
    ) -> Result<i128, ProtocolError> {
        self.ensure_positive(amount)?;
        self.accrue_interest(asset, now)?;
        let config = self
            .reserve_configs
            .get(asset)
            .ok_or(ProtocolError::UnknownAsset)?;
        if !config.deposit_enabled {
            return Err(ProtocolError::DepositsDisabled(asset.to_string()));
        }

        let reserve = self
            .reserves
            .get_mut(asset)
            .ok_or(ProtocolError::UnknownAsset)?;
        let net_assets = reserve.total_cash + reserve.total_debt - reserve.protocol_fees;
        let shares = if reserve.total_supply_shares == 0 || net_assets == 0 {
            amount
        } else {
            mul_div(amount, reserve.total_supply_shares, net_assets)
                .map_err(|_| ProtocolError::MathFailure)?
        };

        reserve.total_cash += amount;
        reserve.total_supply_shares += shares;

        let position = self.account_mut(user);
        *position
            .supplied_shares
            .entry(asset.to_string())
            .or_insert(0) += shares;
        position
            .collateral_enabled
            .entry(asset.to_string())
            .or_insert(true);

        Ok(shares)
    }

    pub fn withdraw(
        &mut self,
        user: &str,
        asset: &str,
        amount: i128,
        oracle: &PriceOracle,
        now: u64,
    ) -> Result<i128, ProtocolError> {
        self.ensure_positive(amount)?;
        self.accrue_interest(asset, now)?;

        let shares = {
            let reserve = self
                .reserves
                .get(asset)
                .ok_or(ProtocolError::UnknownAsset)?;
            self.amount_to_supply_shares_rounded_up(reserve, amount)?
        };

        {
            let position = self.account_mut(user);
            let supplied = position
                .supplied_shares
                .get_mut(asset)
                .ok_or(ProtocolError::InsufficientBalance)?;
            if *supplied < shares {
                return Err(ProtocolError::InsufficientBalance);
            }
            *supplied -= shares;
        }

        {
            let reserve = self
                .reserves
                .get_mut(asset)
                .ok_or(ProtocolError::UnknownAsset)?;
            if reserve.total_cash < amount {
                return Err(ProtocolError::InsufficientLiquidity);
            }
            reserve.total_cash -= amount;
            reserve.total_supply_shares -= shares;
        }

        let snapshot = self.position(user, oracle)?;
        if snapshot.debt_value > 0 && snapshot.health_factor < WAD {
            let position = self.account_mut(user);
            *position
                .supplied_shares
                .entry(asset.to_string())
                .or_insert(0) += shares;
            let reserve = self
                .reserves
                .get_mut(asset)
                .ok_or(ProtocolError::UnknownAsset)?;
            reserve.total_cash += amount;
            reserve.total_supply_shares += shares;
            return Err(ProtocolError::HealthFactorTooLow);
        }

        Ok(amount)
    }

    pub fn set_collateral_enabled(
        &mut self,
        user: &str,
        asset: &str,
        enabled: bool,
        oracle: &PriceOracle,
    ) -> Result<(), ProtocolError> {
        let previous = {
            let position = self.account_mut(user);
            let entry = position
                .collateral_enabled
                .entry(asset.to_string())
                .or_insert(true);
            let old = *entry;
            *entry = enabled;
            old
        };

        if !enabled {
            let snapshot = self.position(user, oracle)?;
            if snapshot.debt_value > 0 && snapshot.health_factor < WAD {
                let position = self.account_mut(user);
                position
                    .collateral_enabled
                    .insert(asset.to_string(), previous);
                return Err(ProtocolError::HealthFactorTooLow);
            }
        }

        Ok(())
    }

    pub fn borrow(
        &mut self,
        user: &str,
        asset: &str,
        amount: i128,
        oracle: &PriceOracle,
        now: u64,
    ) -> Result<i128, ProtocolError> {
        self.ensure_positive(amount)?;
        self.accrue_interest(asset, now)?;
        let config = self
            .reserve_configs
            .get(asset)
            .ok_or(ProtocolError::UnknownAsset)?;
        if !config.borrow_enabled {
            return Err(ProtocolError::BorrowsDisabled(asset.to_string()));
        }

        {
            let reserve = self
                .reserves
                .get(asset)
                .ok_or(ProtocolError::UnknownAsset)?;
            if reserve.total_cash < amount {
                return Err(ProtocolError::InsufficientLiquidity);
            }
        }

        let shares = {
            let reserve = self
                .reserves
                .get(asset)
                .ok_or(ProtocolError::UnknownAsset)?;
            self.amount_to_debt_shares(reserve, amount)?
        };

        {
            let reserve = self
                .reserves
                .get_mut(asset)
                .ok_or(ProtocolError::UnknownAsset)?;
            reserve.total_cash -= amount;
            reserve.total_debt += amount;
            reserve.total_debt_shares += shares;
        }

        *self
            .account_mut(user)
            .debt_shares
            .entry(asset.to_string())
            .or_insert(0) += shares;

        let snapshot = self.position(user, oracle)?;
        if snapshot.debt_value > snapshot.collateral_value {
            let reserve = self
                .reserves
                .get_mut(asset)
                .ok_or(ProtocolError::UnknownAsset)?;
            reserve.total_cash += amount;
            reserve.total_debt -= amount;
            reserve.total_debt_shares -= shares;
            let debt = self
                .account_mut(user)
                .debt_shares
                .entry(asset.to_string())
                .or_insert(0);
            *debt -= shares;
            return Err(ProtocolError::InsufficientCollateral);
        }

        Ok(shares)
    }

    pub fn repay(
        &mut self,
        _payer: &str,
        borrower: &str,
        asset: &str,
        amount: i128,
        now: u64,
    ) -> Result<i128, ProtocolError> {
        self.ensure_positive(amount)?;
        self.accrue_interest(asset, now)?;

        let owed = self.user_debt_amount(borrower, asset)?;
        if owed == 0 {
            return Err(ProtocolError::NothingToRepay);
        }
        let actual = min(amount, owed);

        let shares = {
            let reserve = self
                .reserves
                .get(asset)
                .ok_or(ProtocolError::UnknownAsset)?;
            self.amount_to_debt_shares_rounded_up(reserve, actual)?
        };

        {
            let position = self.account_mut(borrower);
            let debt = position
                .debt_shares
                .get_mut(asset)
                .ok_or(ProtocolError::NothingToRepay)?;
            let burn = min(*debt, shares);
            *debt -= burn;
        }

        {
            let reserve = self
                .reserves
                .get_mut(asset)
                .ok_or(ProtocolError::UnknownAsset)?;
            reserve.total_cash += actual;
            reserve.total_debt -= actual;
            reserve.total_debt_shares -= min(reserve.total_debt_shares, shares);
        }

        Ok(actual)
    }

    pub fn liquidate(
        &mut self,
        liquidator: &str,
        borrower: &str,
        debt_asset: &str,
        collateral_asset: &str,
        requested_repay_amount: i128,
        oracle: &PriceOracle,
        now: u64,
    ) -> Result<LiquidationResult, ProtocolError> {
        self.ensure_positive(requested_repay_amount)?;
        self.accrue_interest(debt_asset, now)?;
        self.accrue_interest(collateral_asset, now)?;

        let snapshot = self.position(borrower, oracle)?;
        if snapshot.debt_value == 0 || snapshot.health_factor >= WAD {
            return Err(ProtocolError::PositionNotLiquidatable);
        }

        let borrower_debt = self.user_debt_amount(borrower, debt_asset)?;
        let max_repay = bps_mul(borrower_debt, self.close_factor_bps)
            .map_err(|_| ProtocolError::MathFailure)?;
        let repay_amount = min(requested_repay_amount, max_repay);

        let debt_price = oracle.get_price(debt_asset)?;
        let collateral_price = oracle.get_price(collateral_asset)?;
        let collateral_cfg = self
            .reserve_configs
            .get(collateral_asset)
            .ok_or(ProtocolError::UnknownAsset)?;

        let repay_value =
            mul_div(repay_amount, debt_price, WAD).map_err(|_| ProtocolError::MathFailure)?;
        let discounted_value = repay_value
            .checked_mul(i128::from(10_000 + collateral_cfg.liquidation_bonus_bps))
            .ok_or(ProtocolError::MathFailure)?
            / 10_000;
        let seize_amount = mul_div(discounted_value, WAD, collateral_price)
            .map_err(|_| ProtocolError::MathFailure)?;

        let borrower_collateral = self.user_supply_amount(borrower, collateral_asset)?;
        if borrower_collateral < seize_amount {
            return Err(ProtocolError::InsufficientBalance);
        }

        self.repay(liquidator, borrower, debt_asset, repay_amount, now)?;
        self.force_withdraw_collateral(borrower, collateral_asset, seize_amount)?;

        {
            let reserve = self
                .reserves
                .get_mut(collateral_asset)
                .ok_or(ProtocolError::UnknownAsset)?;
            if reserve.total_cash < seize_amount {
                return Err(ProtocolError::InsufficientLiquidity);
            }
            reserve.total_cash -= seize_amount;
        }

        let _ = liquidator;

        Ok(LiquidationResult {
            repaid_amount: repay_amount,
            seized_collateral: seize_amount,
            liquidator_discount_value: discounted_value - repay_value,
        })
    }

    pub fn flash_loan(
        &mut self,
        _receiver: &str,
        asset: &str,
        amount: i128,
        returned_amount: i128,
        now: u64,
    ) -> Result<FlashLoanReceipt, ProtocolError> {
        self.ensure_positive(amount)?;
        self.accrue_interest(asset, now)?;

        let config = self
            .reserve_configs
            .get(asset)
            .ok_or(ProtocolError::UnknownAsset)?;
        if !config.flash_loan_enabled {
            return Err(ProtocolError::FlashLoansDisabled(asset.to_string()));
        }

        let reserve = self
            .reserves
            .get_mut(asset)
            .ok_or(ProtocolError::UnknownAsset)?;
        if reserve.total_cash < amount {
            return Err(ProtocolError::InsufficientLiquidity);
        }

        let fee =
            bps_mul(amount, config.flash_loan_fee_bps).map_err(|_| ProtocolError::MathFailure)?;
        let required_return = amount + fee;
        if returned_amount < required_return {
            return Err(ProtocolError::InvalidFlashLoanRepayment);
        }

        let extra = returned_amount - amount;
        let protocol_fee =
            bps_mul(extra, config.reserve_factor_bps).map_err(|_| ProtocolError::MathFailure)?;
        let supplier_fee = extra - protocol_fee;

        reserve.total_cash += supplier_fee + protocol_fee;
        reserve.protocol_fees += protocol_fee;

        Ok(FlashLoanReceipt {
            asset: asset.to_string(),
            amount,
            fee_paid: extra,
            protocol_fee,
            supplier_fee,
        })
    }

    pub fn collect_protocol_fees(
        &mut self,
        caller: &str,
        asset: &str,
        amount: i128,
    ) -> Result<i128, ProtocolError> {
        self.ensure_admin(caller)?;
        self.ensure_positive(amount)?;
        let reserve = self
            .reserves
            .get_mut(asset)
            .ok_or(ProtocolError::UnknownAsset)?;
        let actual = min(amount, reserve.protocol_fees);
        if reserve.total_cash < actual {
            return Err(ProtocolError::InsufficientLiquidity);
        }
        reserve.protocol_fees -= actual;
        reserve.total_cash -= actual;
        Ok(actual)
    }

    pub fn reserve_state(&self, asset: &str) -> Result<&ReserveState, ProtocolError> {
        self.reserves.get(asset).ok_or(ProtocolError::UnknownAsset)
    }

    pub fn position(
        &self,
        user: &str,
        oracle: &PriceOracle,
    ) -> Result<PositionSnapshot, ProtocolError> {
        let mut supplied_amounts = BTreeMap::new();
        let mut debt_amounts = BTreeMap::new();
        let position = self.accounts.get(user).cloned().unwrap_or_default();

        let mut collateral_value = 0_i128;
        let mut liquidation_value = 0_i128;
        let mut debt_value = 0_i128;

        for (asset, shares) in &position.supplied_shares {
            let reserve = self
                .reserves
                .get(asset)
                .ok_or(ProtocolError::UnknownAsset)?;
            let amount = self.supply_shares_to_amount(reserve, *shares)?;
            supplied_amounts.insert(asset.clone(), amount);

            if *position.collateral_enabled.get(asset).unwrap_or(&true) {
                let price = oracle.get_price(asset)?;
                let config = self
                    .reserve_configs
                    .get(asset)
                    .ok_or(ProtocolError::UnknownAsset)?;
                let value = mul_div(amount, price, WAD).map_err(|_| ProtocolError::MathFailure)?;
                collateral_value += value * i128::from(config.collateral_factor_bps) / 10_000;
                liquidation_value += value * i128::from(config.liquidation_threshold_bps) / 10_000;
            }
        }

        for (asset, shares) in &position.debt_shares {
            let reserve = self
                .reserves
                .get(asset)
                .ok_or(ProtocolError::UnknownAsset)?;
            let amount = self.debt_shares_to_amount(reserve, *shares)?;
            debt_amounts.insert(asset.clone(), amount);

            let price = oracle.get_price(asset)?;
            debt_value += mul_div(amount, price, WAD).map_err(|_| ProtocolError::MathFailure)?;
        }

        let health_factor = if debt_value == 0 {
            i128::MAX
        } else {
            wad_div(liquidation_value, debt_value).map_err(|_| ProtocolError::MathFailure)?
        };

        Ok(PositionSnapshot {
            supplied_amounts,
            debt_amounts,
            collateral_value,
            liquidation_value,
            debt_value,
            health_factor,
        })
    }

    pub fn snapshot(&self) -> ProtocolSnapshot {
        ProtocolSnapshot {
            reserves: self.reserves.clone(),
            reserve_configs: self.reserve_configs.clone(),
            treasury: self.treasury.clone(),
        }
    }

    fn force_withdraw_collateral(
        &mut self,
        user: &str,
        asset: &str,
        amount: i128,
    ) -> Result<(), ProtocolError> {
        let shares = {
            let reserve = self
                .reserves
                .get(asset)
                .ok_or(ProtocolError::UnknownAsset)?;
            self.amount_to_supply_shares_rounded_up(reserve, amount)?
        };

        let supplied = self
            .account_mut(user)
            .supplied_shares
            .get_mut(asset)
            .ok_or(ProtocolError::InsufficientBalance)?;
        if *supplied < shares {
            return Err(ProtocolError::InsufficientBalance);
        }
        *supplied -= shares;

        let reserve = self
            .reserves
            .get_mut(asset)
            .ok_or(ProtocolError::UnknownAsset)?;
        reserve.total_supply_shares -= shares;
        Ok(())
    }

    fn ensure_admin(&self, caller: &str) -> Result<(), ProtocolError> {
        if caller == self.admin {
            Ok(())
        } else {
            Err(ProtocolError::Unauthorized)
        }
    }

    fn ensure_positive(&self, amount: i128) -> Result<(), ProtocolError> {
        if amount > 0 {
            Ok(())
        } else {
            Err(ProtocolError::InvalidAmount)
        }
    }

    fn account_mut(&mut self, user: &str) -> &mut AccountPosition {
        self.accounts.entry(user.to_string()).or_default()
    }

    fn amount_to_supply_shares_rounded_up(
        &self,
        reserve: &ReserveState,
        amount: i128,
    ) -> Result<i128, ProtocolError> {
        let net_assets = reserve.total_cash + reserve.total_debt - reserve.protocol_fees;
        let shares = if reserve.total_supply_shares == 0 || net_assets == 0 {
            amount
        } else {
            mul_div(amount, reserve.total_supply_shares, net_assets)
                .map_err(|_| ProtocolError::MathFailure)?
        };
        if self.supply_shares_to_amount(reserve, shares)? < amount {
            Ok(shares + 1)
        } else {
            Ok(shares)
        }
    }

    fn supply_shares_to_amount(
        &self,
        reserve: &ReserveState,
        shares: i128,
    ) -> Result<i128, ProtocolError> {
        if reserve.total_supply_shares == 0 {
            Ok(0)
        } else {
            let net_assets = reserve.total_cash + reserve.total_debt - reserve.protocol_fees;
            mul_div(shares, net_assets, reserve.total_supply_shares)
                .map_err(|_| ProtocolError::MathFailure)
        }
    }

    fn amount_to_debt_shares(
        &self,
        reserve: &ReserveState,
        amount: i128,
    ) -> Result<i128, ProtocolError> {
        if reserve.total_debt_shares == 0 || reserve.total_debt == 0 {
            Ok(amount)
        } else {
            mul_div(amount, reserve.total_debt_shares, reserve.total_debt)
                .map_err(|_| ProtocolError::MathFailure)
        }
    }

    fn amount_to_debt_shares_rounded_up(
        &self,
        reserve: &ReserveState,
        amount: i128,
    ) -> Result<i128, ProtocolError> {
        let shares = self.amount_to_debt_shares(reserve, amount)?;
        if self.debt_shares_to_amount(reserve, shares)? < amount {
            Ok(shares + 1)
        } else {
            Ok(shares)
        }
    }

    fn debt_shares_to_amount(
        &self,
        reserve: &ReserveState,
        shares: i128,
    ) -> Result<i128, ProtocolError> {
        if reserve.total_debt_shares == 0 {
            Ok(0)
        } else {
            mul_div(shares, reserve.total_debt, reserve.total_debt_shares)
                .map_err(|_| ProtocolError::MathFailure)
        }
    }

    fn user_supply_amount(&self, user: &str, asset: &str) -> Result<i128, ProtocolError> {
        let shares = self
            .accounts
            .get(user)
            .and_then(|position| position.supplied_shares.get(asset).copied())
            .unwrap_or(0);
        let reserve = self
            .reserves
            .get(asset)
            .ok_or(ProtocolError::UnknownAsset)?;
        self.supply_shares_to_amount(reserve, shares)
    }

    fn user_debt_amount(&self, user: &str, asset: &str) -> Result<i128, ProtocolError> {
        let shares = self
            .accounts
            .get(user)
            .and_then(|position| position.debt_shares.get(asset).copied())
            .unwrap_or(0);
        let reserve = self
            .reserves
            .get(asset)
            .ok_or(ProtocolError::UnknownAsset)?;
        self.debt_shares_to_amount(reserve, shares)
    }
}
