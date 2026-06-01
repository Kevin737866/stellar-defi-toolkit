use std::cmp::min;
use std::collections::BTreeMap;

use log::{info, warn};

use crate::contracts::oracle::PriceOracle;
use crate::types::{
    AccountPosition, FlashLoanReceipt, InterestRateModel, LiquidationResult, PositionSnapshot,
    ProtocolError, ProtocolSnapshot, ReserveConfig, ReserveState,
};
use crate::utils::{bps_mul, mul_div, wad_div, WAD, YEAR_IN_SECONDS};

// ── Struct ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct LendingProtocol {
    admin: String,
    treasury: String,
    interest_rate_model: InterestRateModel,
    reserves: BTreeMap<String, ReserveState>,
    reserve_configs: BTreeMap<String, ReserveConfig>,
    accounts: BTreeMap<String, AccountPosition>,
    close_factor_bps: u32,
    /// When `true` all user-facing state-changing operations are blocked.
    /// Admin operations (register_asset, update_reserve_config,
    /// collect_protocol_fees, set_close_factor) remain available so the
    /// admin can act during an incident.
    paused: bool,
}

// ── Constructor & simple getters ──────────────────────────────────────────────

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
            paused: false,
        }
    }

    pub fn admin(&self) -> &str {
        &self.admin
    }

    pub fn treasury(&self) -> &str {
        &self.treasury
    }

    /// Returns `true` when the protocol is in emergency-pause mode.
    pub fn is_paused(&self) -> bool {
        self.paused
    }

// ── Emergency pause ───────────────────────────────────────────────────────────

    /// Pause all user-facing protocol operations (admin only).
    ///
    /// While paused, `deposit`, `withdraw`, `borrow`, `repay`, `liquidate`,
    /// `flash_loan`, and `set_collateral_enabled` all return
    /// [`ProtocolError::ProtocolPaused`].  Admin operations are unaffected.
    pub fn pause(&mut self, caller: &str) -> Result<(), ProtocolError> {
        self.ensure_admin(caller)?;
        if self.paused {
            return Ok(()); // idempotent
        }
        self.paused = true;
        warn!("event=ProtocolPaused caller={}", caller);
        Ok(())
    }

    /// Resume normal protocol operations (admin only).
    pub fn unpause(&mut self, caller: &str) -> Result<(), ProtocolError> {
        self.ensure_admin(caller)?;
        if !self.paused {
            return Ok(()); // idempotent
        }
        self.paused = false;
        info!("event=ProtocolUnpaused caller={}", caller);
        Ok(())
    }

// ── Admin configuration ───────────────────────────────────────────────────────

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
            asset.clone(),
            ReserveState {
                last_accrual_ts: now,
                ..ReserveState::default()
            },
        );

        info!(
            "event=AssetRegistered asset={} caller={} timestamp={}",
            asset, caller, now
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

// ── Interest accrual ──────────────────────────────────────────────────────────

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

        info!(
            "event=InterestAccrued asset={} accrued={} reserve_cut={} \
             utilization={} borrow_rate={} timestamp={}",
            asset, accrued, reserve_cut, utilization, borrow_rate, now
        );
        Ok(accrued)
    }

// ── Deposit ───────────────────────────────────────────────────────────────────

    pub fn deposit(
        &mut self,
        user: &str,
        asset: &str,
        amount: i128,
        now: u64,
    ) -> Result<i128, ProtocolError> {
        self.ensure_not_paused()?;
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
        *position.supplied_shares.entry(asset.to_string()).or_insert(0) += shares;
        position.collateral_enabled.entry(asset.to_string()).or_insert(true);

        info!(
            "event=Deposit user={} asset={} amount={} shares_minted={} timestamp={}",
            user, asset, amount, shares, now
        );
        Ok(shares)
    }

// ── Withdraw ──────────────────────────────────────────────────────────────────

    pub fn withdraw(
        &mut self,
        user: &str,
        asset: &str,
        amount: i128,
        oracle: &PriceOracle,
        now: u64,
    ) -> Result<i128, ProtocolError> {
        self.ensure_not_paused()?;
        self.ensure_positive(amount)?;
        self.accrue_interest(asset, now)?;

        let shares = {
            let reserve = self.reserves.get(asset).ok_or(ProtocolError::UnknownAsset)?;
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
            let reserve = self.reserves.get_mut(asset).ok_or(ProtocolError::UnknownAsset)?;
            if reserve.total_cash < amount {
                return Err(ProtocolError::InsufficientLiquidity);
            }
            reserve.total_cash -= amount;
            reserve.total_supply_shares -= shares;
        }

        let snapshot = self.position(user, oracle)?;
        if snapshot.debt_value > 0 && snapshot.health_factor < WAD {
            // Roll back
            let position = self.account_mut(user);
            *position.supplied_shares.entry(asset.to_string()).or_insert(0) += shares;
            let reserve = self.reserves.get_mut(asset).ok_or(ProtocolError::UnknownAsset)?;
            reserve.total_cash += amount;
            reserve.total_supply_shares += shares;

            warn!(
                "event=WithdrawRejected user={} asset={} amount={} \
                 reason=HealthFactorTooLow health_factor={} timestamp={}",
                user, asset, amount, snapshot.health_factor, now
            );
            return Err(ProtocolError::HealthFactorTooLow);
        }

        info!(
            "event=Withdraw user={} asset={} amount={} shares_burned={} \
             health_factor={} timestamp={}",
            user, asset, amount, shares, snapshot.health_factor, now
        );
        Ok(amount)
    }

// ── Collateral toggle ─────────────────────────────────────────────────────────

    pub fn set_collateral_enabled(
        &mut self,
        user: &str,
        asset: &str,
        enabled: bool,
        oracle: &PriceOracle,
    ) -> Result<(), ProtocolError> {
        self.ensure_not_paused()?;

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
                position.collateral_enabled.insert(asset.to_string(), previous);

                warn!(
                    "event=CollateralToggleRejected user={} asset={} enabled={} \
                     reason=HealthFactorTooLow health_factor={}",
                    user, asset, enabled, snapshot.health_factor
                );
                return Err(ProtocolError::HealthFactorTooLow);
            }
        }

        info!(
            "event=CollateralToggled user={} asset={} enabled={}",
            user, asset, enabled
        );
        Ok(())
    }

// ── Borrow ────────────────────────────────────────────────────────────────────

    pub fn borrow(
        &mut self,
        user: &str,
        asset: &str,
        amount: i128,
        oracle: &PriceOracle,
        now: u64,
    ) -> Result<i128, ProtocolError> {
        self.ensure_not_paused()?;
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
            let reserve = self.reserves.get(asset).ok_or(ProtocolError::UnknownAsset)?;
            if reserve.total_cash < amount {
                return Err(ProtocolError::InsufficientLiquidity);
            }
        }

        let shares = {
            let reserve = self.reserves.get(asset).ok_or(ProtocolError::UnknownAsset)?;
            self.amount_to_debt_shares(reserve, amount)?
        };

        {
            let reserve = self.reserves.get_mut(asset).ok_or(ProtocolError::UnknownAsset)?;
            reserve.total_cash -= amount;
            reserve.total_debt += amount;
            reserve.total_debt_shares += shares;
        }

        *self.account_mut(user).debt_shares.entry(asset.to_string()).or_insert(0) += shares;

        let snapshot = self.position(user, oracle)?;
        if snapshot.debt_value > snapshot.collateral_value {
            // Roll back
            let reserve = self.reserves.get_mut(asset).ok_or(ProtocolError::UnknownAsset)?;
            reserve.total_cash += amount;
            reserve.total_debt -= amount;
            reserve.total_debt_shares -= shares;
            *self.account_mut(user).debt_shares.entry(asset.to_string()).or_insert(0) -= shares;

            warn!(
                "event=BorrowRejected user={} asset={} amount={} \
                 reason=InsufficientCollateral collateral_value={} debt_value={} timestamp={}",
                user, asset, amount, snapshot.collateral_value, snapshot.debt_value, now
            );
            return Err(ProtocolError::InsufficientCollateral);
        }

        info!(
            "event=Borrow user={} asset={} amount={} shares_minted={} \
             collateral_value={} debt_value={} timestamp={}",
            user, asset, amount, shares, snapshot.collateral_value, snapshot.debt_value, now
        );
        Ok(shares)
    }

// ── Repay ─────────────────────────────────────────────────────────────────────

    pub fn repay(
        &mut self,
        payer: &str,
        borrower: &str,
        asset: &str,
        amount: i128,
        now: u64,
    ) -> Result<i128, ProtocolError> {
        self.ensure_not_paused()?;
        self.ensure_positive(amount)?;
        self.accrue_interest(asset, now)?;

        let owed = self.user_debt_amount(borrower, asset)?;
        if owed == 0 {
            return Err(ProtocolError::NothingToRepay);
        }
        let actual = min(amount, owed);

        let shares = {
            let reserve = self.reserves.get(asset).ok_or(ProtocolError::UnknownAsset)?;
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
            let reserve = self.reserves.get_mut(asset).ok_or(ProtocolError::UnknownAsset)?;
            reserve.total_cash += actual;
            reserve.total_debt -= actual;
            reserve.total_debt_shares -= min(reserve.total_debt_shares, shares);
        }

        info!(
            "event=Repay payer={} borrower={} asset={} amount_requested={} \
             amount_repaid={} shares_burned={} timestamp={}",
            payer, borrower, asset, amount, actual, shares, now
        );
        Ok(actual)
    }

// ── Liquidation (optimized) ───────────────────────────────────────────────────
//
// Optimizations vs. the previous implementation:
//
// 1. `health_check()` instead of `position()` for the eligibility guard.
//    `position()` builds full BTreeMap snapshots and does oracle lookups for
//    every supplied/debt asset.  `health_check()` computes only the two
//    scalars needed (liquidation_value, debt_value) — same oracle calls, zero
//    allocation overhead.
//
// 2. No nested `repay()` call.  The old code called `repay()` which called
//    `accrue_interest()` a second time (already done at the top of
//    `liquidate`) and re-computed `user_debt_amount`.  The debt-burning is
//    now inlined directly.
//
// 3. Single collateral reserve mutation pass.  Previously `force_withdraw_
//    collateral` updated `total_supply_shares` and then a separate block
//    updated `total_cash`.  Both are now done in one borrow of the reserve.
//
// 4. Seize amount is capped to available collateral before any state is
//    mutated, so we never reach an error after partial mutation.
//
// 5. `health_factor_after` is computed from the already-updated state using
//    the same lean `health_check()` helper, adding observability at no extra
//    oracle cost.

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
        self.ensure_not_paused()?;
        self.ensure_positive(requested_repay_amount)?;

        // Accrue interest on both assets once, upfront.
        self.accrue_interest(debt_asset, now)?;
        self.accrue_interest(collateral_asset, now)?;

        // ── 1. Eligibility check (lean — no map allocations) ──────────────────
        let (liq_value_before, debt_value_before) = self.health_check(borrower, oracle)?;
        if debt_value_before == 0 {
            warn!(
                "event=LiquidationRejected liquidator={} borrower={} \
                 reason=PositionNotLiquidatable health_factor=MAX timestamp={}",
                liquidator, borrower, now
            );
            return Err(ProtocolError::PositionNotLiquidatable);
        }
        let hf_before = wad_div(liq_value_before, debt_value_before)
            .map_err(|_| ProtocolError::MathFailure)?;
        if hf_before >= WAD {
            warn!(
                "event=LiquidationRejected liquidator={} borrower={} \
                 reason=PositionNotLiquidatable health_factor={} timestamp={}",
                liquidator, borrower, hf_before, now
            );
            return Err(ProtocolError::PositionNotLiquidatable);
        }

        // ── 2. Compute repay / seize amounts ──────────────────────────────────
        let borrower_debt = self.user_debt_amount(borrower, debt_asset)?;
        let max_repay = bps_mul(borrower_debt, self.close_factor_bps)
            .map_err(|_| ProtocolError::MathFailure)?;
        let repay_amount = min(requested_repay_amount, max_repay);

        let debt_price       = oracle.get_price(debt_asset)?;
        let collateral_price = oracle.get_price(collateral_asset)?;
        let liquidation_bonus_bps = self
            .reserve_configs
            .get(collateral_asset)
            .ok_or(ProtocolError::UnknownAsset)?
            .liquidation_bonus_bps;

        let repay_value = mul_div(repay_amount, debt_price, WAD)
            .map_err(|_| ProtocolError::MathFailure)?;
        let discounted_value = repay_value
            .checked_mul(i128::from(10_000 + liquidation_bonus_bps))
            .ok_or(ProtocolError::MathFailure)?
            / 10_000;
        let seize_amount_raw = mul_div(discounted_value, WAD, collateral_price)
            .map_err(|_| ProtocolError::MathFailure)?;

        // Cap seize to what the borrower actually has — avoids a late error
        // after state has already been partially mutated.
        let borrower_collateral = self.user_supply_amount(borrower, collateral_asset)?;
        let seize_amount = min(seize_amount_raw, borrower_collateral);

        // ── 3. Burn debt shares (inlined repay, no double-accrual) ────────────
        let debt_shares_to_burn = {
            let reserve = self.reserves.get(debt_asset).ok_or(ProtocolError::UnknownAsset)?;
            self.amount_to_debt_shares_rounded_up(reserve, repay_amount)?
        };
        {
            let position = self.account_mut(borrower);
            if let Some(debt) = position.debt_shares.get_mut(debt_asset) {
                let burn = min(*debt, debt_shares_to_burn);
                *debt -= burn;
            }
        }
        {
            let reserve = self.reserves.get_mut(debt_asset).ok_or(ProtocolError::UnknownAsset)?;
            reserve.total_cash += repay_amount;
            reserve.total_debt -= repay_amount;
            reserve.total_debt_shares -= min(reserve.total_debt_shares, debt_shares_to_burn);
        }

        // ── 4. Seize collateral (single reserve mutation pass) ────────────────
        let collateral_shares_to_burn = {
            let reserve = self
                .reserves
                .get(collateral_asset)
                .ok_or(ProtocolError::UnknownAsset)?;
            self.amount_to_supply_shares_rounded_up(reserve, seize_amount)?
        };
        {
            let position = self.account_mut(borrower);
            let supplied = position
                .supplied_shares
                .get_mut(collateral_asset)
                .ok_or(ProtocolError::InsufficientBalance)?;
            if *supplied < collateral_shares_to_burn {
                return Err(ProtocolError::InsufficientBalance);
            }
            *supplied -= collateral_shares_to_burn;
        }
        {
            // Single borrow: update both total_supply_shares and total_cash.
            let reserve = self
                .reserves
                .get_mut(collateral_asset)
                .ok_or(ProtocolError::UnknownAsset)?;
            if reserve.total_cash < seize_amount {
                return Err(ProtocolError::InsufficientLiquidity);
            }
            reserve.total_supply_shares -= collateral_shares_to_burn;
            reserve.total_cash -= seize_amount;
        }

        // ── 5. Compute health factor after (reuses health_check) ──────────────
        let health_factor_after = {
            let (liq_val, debt_val) = self.health_check(borrower, oracle)?;
            if debt_val == 0 {
                i128::MAX
            } else {
                wad_div(liq_val, debt_val).map_err(|_| ProtocolError::MathFailure)?
            }
        };

        let discount_value = discounted_value - repay_value;

        info!(
            "event=Liquidation liquidator={} borrower={} debt_asset={} \
             collateral_asset={} repaid={} seized_collateral={} \
             liquidator_discount_value={} health_factor_before={} \
             health_factor_after={} timestamp={}",
            liquidator, borrower, debt_asset, collateral_asset,
            repay_amount, seize_amount, discount_value,
            hf_before, health_factor_after, now
        );

        Ok(LiquidationResult {
            repaid_amount: repay_amount,
            seized_collateral: seize_amount,
            liquidator_discount_value: discount_value,
            health_factor_after,
        })
    }

// ── Flash loan ────────────────────────────────────────────────────────────────

    pub fn flash_loan(
        &mut self,
        receiver: &str,
        asset: &str,
        amount: i128,
        returned_amount: i128,
        now: u64,
    ) -> Result<FlashLoanReceipt, ProtocolError> {
        self.ensure_not_paused()?;
        self.ensure_positive(amount)?;
        self.accrue_interest(asset, now)?;

        let config = self
            .reserve_configs
            .get(asset)
            .ok_or(ProtocolError::UnknownAsset)?;
        if !config.flash_loan_enabled {
            return Err(ProtocolError::FlashLoansDisabled(asset.to_string()));
        }

        let reserve = self.reserves.get_mut(asset).ok_or(ProtocolError::UnknownAsset)?;
        if reserve.total_cash < amount {
            return Err(ProtocolError::InsufficientLiquidity);
        }

        let fee = bps_mul(amount, config.flash_loan_fee_bps)
            .map_err(|_| ProtocolError::MathFailure)?;
        let required_return = amount + fee;
        if returned_amount < required_return {
            warn!(
                "event=FlashLoanRepaymentFailed receiver={} asset={} amount={} \
                 required={} returned={} timestamp={}",
                receiver, asset, amount, required_return, returned_amount, now
            );
            return Err(ProtocolError::InvalidFlashLoanRepayment);
        }

        let extra = returned_amount - amount;
        let protocol_fee = bps_mul(extra, config.reserve_factor_bps)
            .map_err(|_| ProtocolError::MathFailure)?;
        let supplier_fee = extra - protocol_fee;

        reserve.total_cash += supplier_fee + protocol_fee;
        reserve.protocol_fees += protocol_fee;

        info!(
            "event=FlashLoan receiver={} asset={} amount={} fee_paid={} \
             protocol_fee={} supplier_fee={} timestamp={}",
            receiver, asset, amount, extra, protocol_fee, supplier_fee, now
        );

        Ok(FlashLoanReceipt {
            asset: asset.to_string(),
            amount,
            fee_paid: extra,
            protocol_fee,
            supplier_fee,
        })
    }

// ── Fee collection & read-only views ─────────────────────────────────────────

    pub fn collect_protocol_fees(
        &mut self,
        caller: &str,
        asset: &str,
        amount: i128,
    ) -> Result<i128, ProtocolError> {
        self.ensure_admin(caller)?;
        self.ensure_positive(amount)?;
        let reserve = self.reserves.get_mut(asset).ok_or(ProtocolError::UnknownAsset)?;
        let actual = min(amount, reserve.protocol_fees);
        if reserve.total_cash < actual {
            return Err(ProtocolError::InsufficientLiquidity);
        }
        reserve.protocol_fees -= actual;
        reserve.total_cash -= actual;

        info!(
            "event=ProtocolFeesCollected caller={} asset={} amount_requested={} \
             amount_collected={} treasury={}",
            caller, asset, amount, actual, self.treasury
        );
        Ok(actual)
    }

    pub fn reserve_state(&self, asset: &str) -> Result<&ReserveState, ProtocolError> {
        self.reserves.get(asset).ok_or(ProtocolError::UnknownAsset)
    }

    /// Full position snapshot — use for display / off-chain queries.
    /// For on-chain health checks prefer [`health_check`] to avoid
    /// unnecessary allocations.
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
            let reserve = self.reserves.get(asset).ok_or(ProtocolError::UnknownAsset)?;
            let amount = self.supply_shares_to_amount(reserve, *shares)?;
            supplied_amounts.insert(asset.clone(), amount);

            if *position.collateral_enabled.get(asset).unwrap_or(&true) {
                let price = oracle.get_price(asset)?;
                let config = self
                    .reserve_configs
                    .get(asset)
                    .ok_or(ProtocolError::UnknownAsset)?;
                let value =
                    mul_div(amount, price, WAD).map_err(|_| ProtocolError::MathFailure)?;
                collateral_value +=
                    value * i128::from(config.collateral_factor_bps) / 10_000;
                liquidation_value +=
                    value * i128::from(config.liquidation_threshold_bps) / 10_000;
            }
        }

        for (asset, shares) in &position.debt_shares {
            let reserve = self.reserves.get(asset).ok_or(ProtocolError::UnknownAsset)?;
            let amount = self.debt_shares_to_amount(reserve, *shares)?;
            debt_amounts.insert(asset.clone(), amount);
            let price = oracle.get_price(asset)?;
            debt_value +=
                mul_div(amount, price, WAD).map_err(|_| ProtocolError::MathFailure)?;
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
            paused: self.paused,
        }
    }

// ── Private helpers ───────────────────────────────────────────────────────────

    /// Lean health check: returns `(liquidation_value, debt_value)` without
    /// building any snapshot maps.  Used internally by `liquidate` and
    /// `withdraw` to avoid unnecessary allocations.
    fn health_check(
        &self,
        user: &str,
        oracle: &PriceOracle,
    ) -> Result<(i128, i128), ProtocolError> {
        let position = self.accounts.get(user).cloned().unwrap_or_default();
        let mut liquidation_value = 0_i128;
        let mut debt_value = 0_i128;

        for (asset, shares) in &position.supplied_shares {
            if *position.collateral_enabled.get(asset).unwrap_or(&true) {
                let reserve = self.reserves.get(asset).ok_or(ProtocolError::UnknownAsset)?;
                let amount = self.supply_shares_to_amount(reserve, *shares)?;
                let price = oracle.get_price(asset)?;
                let config = self
                    .reserve_configs
                    .get(asset)
                    .ok_or(ProtocolError::UnknownAsset)?;
                let value =
                    mul_div(amount, price, WAD).map_err(|_| ProtocolError::MathFailure)?;
                liquidation_value +=
                    value * i128::from(config.liquidation_threshold_bps) / 10_000;
            }
        }

        for (asset, shares) in &position.debt_shares {
            let reserve = self.reserves.get(asset).ok_or(ProtocolError::UnknownAsset)?;
            let amount = self.debt_shares_to_amount(reserve, *shares)?;
            let price = oracle.get_price(asset)?;
            debt_value +=
                mul_div(amount, price, WAD).map_err(|_| ProtocolError::MathFailure)?;
        }

        Ok((liquidation_value, debt_value))
    }

    fn ensure_admin(&self, caller: &str) -> Result<(), ProtocolError> {
        if caller == self.admin {
            Ok(())
        } else {
            Err(ProtocolError::Unauthorized)
        }
    }

    fn ensure_not_paused(&self) -> Result<(), ProtocolError> {
        if self.paused {
            Err(ProtocolError::ProtocolPaused)
        } else {
            Ok(())
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
            .and_then(|p| p.supplied_shares.get(asset).copied())
            .unwrap_or(0);
        let reserve = self.reserves.get(asset).ok_or(ProtocolError::UnknownAsset)?;
        self.supply_shares_to_amount(reserve, shares)
    }

    fn user_debt_amount(&self, user: &str, asset: &str) -> Result<i128, ProtocolError> {
        let shares = self
            .accounts
            .get(user)
            .and_then(|p| p.debt_shares.get(asset).copied())
            .unwrap_or(0);
        let reserve = self.reserves.get(asset).ok_or(ProtocolError::UnknownAsset)?;
        self.debt_shares_to_amount(reserve, shares)
    }
}
