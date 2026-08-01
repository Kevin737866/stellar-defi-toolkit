//! Protocol-level Asset Registry (pure Rust, no Soroban SDK dependency)
//!
//! Implements a comprehensive asset registry used by all protocol contracts.
//! This module fulfils four GitHub issues:
//!
//! ## Issue #215 – Allowlist / Blocklist Management
//! - [`AssetRegistry::allowlist_asset`] / [`AssetRegistry::remove_from_allowlist`]
//! - [`AssetRegistry::blocklist_asset`] / [`AssetRegistry::remove_from_blocklist`]
//! - Blocklist always takes precedence over allowlist.
//! - Events emitted for every change via [`AssetRegistry::drain_events`].
//!
//! ## Issue #216 – Asset Metadata Registry with Token Standards
//! - [`AssetRegistry::register_asset`] stores [`ProtocolAssetMetadata`].
//! - [`AssetRegistry::update_asset_metadata`] lets admin update metadata.
//! - [`AssetRegistry::get_asset_metadata`] makes metadata queryable.
//! - Validates token standard on registration.
//!
//! ## Issue #217 – Asset Risk Parameters per Collateral Type
//! - [`AssetRegistry::set_risk_params`] stores [`AssetRiskParams`].
//! - [`AssetRegistry::get_risk_params`] makes params queryable.
//! - LTV ≤ liquidation threshold invariant enforced.
//!
//! ## Issue #218 – Asset Upgrade and Migration Path
//! - [`AssetRegistry::initiate_migration`] starts a migration.
//! - [`AssetRegistry::complete_migration`] verifies and finalises.
//! - [`AssetRegistry::cancel_migration`] rolls back before completion.
//! - Emits [`MigrationEvent`] entries for a full audit trail.

use std::collections::HashMap;

use crate::types::asset::{
    AssetRiskParams, ListChangeEvent, ListChangeReason, MigrationEvent, MigrationState,
    MigrationStatus, ProtocolAssetMetadata, RegistryListEntry, TokenStandard,
};

// ─── Registry-level errors ───────────────────────────────────────────────────

/// Errors returned by [`AssetRegistry`] operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    /// Caller is not an authorised admin.
    Unauthorized,
    /// The asset is already in the specified list.
    AlreadyInList,
    /// The asset is not in the specified list.
    NotInList,
    /// The asset metadata has already been registered.
    AlreadyRegistered,
    /// No metadata found for the given asset.
    NotRegistered,
    /// The supplied risk parameters fail the internal invariant check.
    InvalidRiskParams,
    /// A migration already exists for this asset and is not yet completed.
    MigrationAlreadyActive,
    /// No active migration found for this asset.
    NoActiveMigration,
    /// The migration has already been completed or cancelled.
    MigrationNotInProgress,
    /// Generic validation error with a message.
    ValidationError(String),
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegistryError::Unauthorized => write!(f, "caller is not an admin"),
            RegistryError::AlreadyInList => write!(f, "asset is already in this list"),
            RegistryError::NotInList => write!(f, "asset is not in this list"),
            RegistryError::AlreadyRegistered => write!(f, "asset metadata already registered"),
            RegistryError::NotRegistered => write!(f, "asset not registered"),
            RegistryError::InvalidRiskParams => write!(f, "invalid risk parameters"),
            RegistryError::MigrationAlreadyActive => {
                write!(f, "a migration is already active for this asset")
            }
            RegistryError::NoActiveMigration => {
                write!(f, "no active migration found for this asset")
            }
            RegistryError::MigrationNotInProgress => {
                write!(f, "migration is not in progress")
            }
            RegistryError::ValidationError(msg) => write!(f, "validation error: {}", msg),
        }
    }
}

// ─── Registry events (combined from all issues) ───────────────────────────────

/// All observable events emitted by the registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryEvent {
    /// An allowlist/blocklist change was made (issue #215).
    ListChange(ListChangeEvent),
    /// Asset metadata was registered or updated (issue #216).
    MetadataRegistered {
        asset_id: String,
    },
    /// Asset metadata was updated by an admin (issue #216).
    MetadataUpdated {
        asset_id: String,
        updated_by: String,
    },
    /// Risk parameters were set or updated (issue #217).
    RiskParamsSet {
        asset_id: String,
        updated_by: String,
    },
    /// A migration lifecycle event occurred (issue #218).
    Migration(MigrationEvent),
}

// ─── Main Registry struct ─────────────────────────────────────────────────────

/// Protocol-level asset registry.
///
/// Holds authorised admin addresses plus four independent data stores:
/// allowlist, blocklist, metadata, risk parameters, and migrations.
#[derive(Debug, Clone)]
pub struct AssetRegistry {
    /// Set of admin addresses that may mutate registry state.
    admins: Vec<String>,
    // ── Issue #215: allow / block lists ──────────────────────────────────────
    allowlist: HashMap<String, RegistryListEntry>,
    blocklist: HashMap<String, RegistryListEntry>,
    // ── Issue #216: metadata ─────────────────────────────────────────────────
    metadata: HashMap<String, ProtocolAssetMetadata>,
    // ── Issue #217: risk parameters ──────────────────────────────────────────
    risk_params: HashMap<String, AssetRiskParams>,
    // ── Issue #218: migrations ───────────────────────────────────────────────
    migrations: HashMap<String, MigrationState>,
    // ── Append-only event log ─────────────────────────────────────────────────
    events: Vec<RegistryEvent>,
}

impl AssetRegistry {
    // ─── Construction ─────────────────────────────────────────────────────────

    /// Create a new, empty registry with the given admin addresses.
    ///
    /// At least one admin must be provided.
    ///
    /// # Panics
    /// Panics if `admins` is empty.
    pub fn new(admins: Vec<String>) -> Self {
        assert!(!admins.is_empty(), "registry must have at least one admin");
        Self {
            admins,
            allowlist: HashMap::new(),
            blocklist: HashMap::new(),
            metadata: HashMap::new(),
            risk_params: HashMap::new(),
            migrations: HashMap::new(),
            events: Vec::new(),
        }
    }

    /// Return a reference to the accumulated event log.
    pub fn events(&self) -> &[RegistryEvent] {
        &self.events
    }

    /// Drain and return all accumulated events, clearing the internal log.
    pub fn drain_events(&mut self) -> Vec<RegistryEvent> {
        std::mem::take(&mut self.events)
    }

    // ─── Internal helpers ─────────────────────────────────────────────────────

    fn ensure_admin(&self, caller: &str) -> Result<(), RegistryError> {
        if self.admins.iter().any(|a| a == caller) {
            Ok(())
        } else {
            Err(RegistryError::Unauthorized)
        }
    }

    fn emit(&mut self, event: RegistryEvent) {
        self.events.push(event);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Issue #215 – Allowlist / Blocklist Management
    // ─────────────────────────────────────────────────────────────────────────

    /// Add an asset to the **allowlist** (governance-controlled).
    ///
    /// Returns [`RegistryError::AlreadyInList`] if the asset is already allowed.
    pub fn allowlist_asset(
        &mut self,
        caller: &str,
        asset_id: &str,
        reason: ListChangeReason,
        now: u64,
    ) -> Result<(), RegistryError> {
        self.ensure_admin(caller)?;

        if self.allowlist.contains_key(asset_id) {
            return Err(RegistryError::AlreadyInList);
        }

        self.allowlist.insert(
            asset_id.to_string(),
            RegistryListEntry {
                asset_id: asset_id.to_string(),
                changed_by: caller.to_string(),
                reason: reason.clone(),
                recorded_at: now,
                active: true,
            },
        );

        self.emit(RegistryEvent::ListChange(ListChangeEvent::AllowlistAdded {
            asset_id: asset_id.to_string(),
            changed_by: caller.to_string(),
            reason,
        }));

        Ok(())
    }

    /// Remove an asset from the **allowlist**.
    ///
    /// Returns [`RegistryError::NotInList`] if the asset is not currently allowed.
    pub fn remove_from_allowlist(
        &mut self,
        caller: &str,
        asset_id: &str,
    ) -> Result<(), RegistryError> {
        self.ensure_admin(caller)?;

        if !self.allowlist.contains_key(asset_id) {
            return Err(RegistryError::NotInList);
        }

        self.allowlist.remove(asset_id);

        self.emit(RegistryEvent::ListChange(ListChangeEvent::AllowlistRemoved {
            asset_id: asset_id.to_string(),
            changed_by: caller.to_string(),
        }));

        Ok(())
    }

    /// Add an asset to the **blocklist** (governance-controlled).
    ///
    /// Returns [`RegistryError::AlreadyInList`] if the asset is already blocked.
    pub fn blocklist_asset(
        &mut self,
        caller: &str,
        asset_id: &str,
        reason: ListChangeReason,
        now: u64,
    ) -> Result<(), RegistryError> {
        self.ensure_admin(caller)?;

        if self.blocklist.contains_key(asset_id) {
            return Err(RegistryError::AlreadyInList);
        }

        self.blocklist.insert(
            asset_id.to_string(),
            RegistryListEntry {
                asset_id: asset_id.to_string(),
                changed_by: caller.to_string(),
                reason: reason.clone(),
                recorded_at: now,
                active: true,
            },
        );

        self.emit(RegistryEvent::ListChange(ListChangeEvent::BlocklistAdded {
            asset_id: asset_id.to_string(),
            changed_by: caller.to_string(),
            reason,
        }));

        Ok(())
    }

    /// Remove an asset from the **blocklist**.
    ///
    /// Returns [`RegistryError::NotInList`] if the asset is not currently blocked.
    pub fn remove_from_blocklist(
        &mut self,
        caller: &str,
        asset_id: &str,
    ) -> Result<(), RegistryError> {
        self.ensure_admin(caller)?;

        if !self.blocklist.contains_key(asset_id) {
            return Err(RegistryError::NotInList);
        }

        self.blocklist.remove(asset_id);

        self.emit(RegistryEvent::ListChange(ListChangeEvent::BlocklistRemoved {
            asset_id: asset_id.to_string(),
            changed_by: caller.to_string(),
        }));

        Ok(())
    }

    /// Returns `true` when the asset is allowed for protocol use.
    ///
    /// **Blocklist takes precedence**: an asset that is both allowlisted and
    /// blocklisted is treated as *not* allowed.
    pub fn is_allowed(&self, asset_id: &str) -> bool {
        if self.blocklist.contains_key(asset_id) {
            return false;
        }
        self.allowlist
            .get(asset_id)
            .map(|e| e.active)
            .unwrap_or(false)
    }

    /// Returns `true` when the asset is on the blocklist.
    pub fn is_blocked(&self, asset_id: &str) -> bool {
        self.blocklist.contains_key(asset_id)
    }

    /// Returns the full allowlist entries.
    pub fn allowlist(&self) -> Vec<&RegistryListEntry> {
        self.allowlist.values().collect()
    }

    /// Returns the full blocklist entries.
    pub fn blocklist(&self) -> Vec<&RegistryListEntry> {
        self.blocklist.values().collect()
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Issue #216 – Asset Metadata Registry with Token Standards
    // ─────────────────────────────────────────────────────────────────────────

    /// Register metadata for a new asset.
    ///
    /// Validates the token standard.  Returns [`RegistryError::AlreadyRegistered`]
    /// if metadata for this `asset_id` already exists.
    pub fn register_asset(
        &mut self,
        caller: &str,
        metadata: ProtocolAssetMetadata,
        now: u64,
    ) -> Result<(), RegistryError> {
        self.ensure_admin(caller)?;

        if self.metadata.contains_key(&metadata.asset_id) {
            return Err(RegistryError::AlreadyRegistered);
        }

        // Standard validation: native assets must not carry a contract address.
        if metadata.standard == TokenStandard::StellarNative
            && !metadata.contract_address.is_empty()
        {
            return Err(RegistryError::ValidationError(
                "StellarNative assets must not have a contract address".to_string(),
            ));
        }

        // SEP-41 and wrapped assets must carry a contract address.
        if (metadata.standard == TokenStandard::Sep41
            || metadata.standard == TokenStandard::Wrapped)
            && metadata.contract_address.is_empty()
        {
            return Err(RegistryError::ValidationError(format!(
                "{} assets must have a contract address",
                metadata.standard
            )));
        }

        let asset_id = metadata.asset_id.clone();
        let mut entry = metadata;
        entry.registered_at = now;
        entry.last_updated_at = now;
        entry.active = true;

        self.metadata.insert(asset_id.clone(), entry);

        self.emit(RegistryEvent::MetadataRegistered {
            asset_id,
        });

        Ok(())
    }

    /// Update metadata for an already-registered asset (admin only).
    ///
    /// The `asset_id` field in the new metadata must match the existing record.
    pub fn update_asset_metadata(
        &mut self,
        caller: &str,
        new_metadata: ProtocolAssetMetadata,
        now: u64,
    ) -> Result<(), RegistryError> {
        self.ensure_admin(caller)?;

        let entry = self
            .metadata
            .get_mut(&new_metadata.asset_id)
            .ok_or(RegistryError::NotRegistered)?;

        let mut updated = new_metadata;
        updated.registered_at = entry.registered_at; // preserve original timestamp
        updated.last_updated_at = now;

        let asset_id = updated.asset_id.clone();
        *entry = updated;

        self.emit(RegistryEvent::MetadataUpdated {
            asset_id,
            updated_by: caller.to_string(),
        });

        Ok(())
    }

    /// Retrieve metadata for an asset.
    pub fn get_asset_metadata(&self, asset_id: &str) -> Option<&ProtocolAssetMetadata> {
        self.metadata.get(asset_id)
    }

    /// Returns `true` if metadata exists for `asset_id`.
    pub fn is_registered(&self, asset_id: &str) -> bool {
        self.metadata.contains_key(asset_id)
    }

    /// Returns all registered asset metadata entries.
    pub fn all_assets(&self) -> Vec<&ProtocolAssetMetadata> {
        self.metadata.values().collect()
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Issue #217 – Asset Risk Parameters per Collateral Type
    // ─────────────────────────────────────────────────────────────────────────

    /// Set (or update) risk parameters for a collateral asset.
    ///
    /// Validates `ltv_bps ≤ liquidation_threshold_bps ≤ 10_000`.
    /// The asset must already be registered in the metadata store.
    pub fn set_risk_params(
        &mut self,
        caller: &str,
        params: AssetRiskParams,
        now: u64,
    ) -> Result<(), RegistryError> {
        self.ensure_admin(caller)?;

        if !self.metadata.contains_key(&params.asset_id) {
            return Err(RegistryError::NotRegistered);
        }

        if !params.is_valid() {
            return Err(RegistryError::InvalidRiskParams);
        }

        let asset_id = params.asset_id.clone();
        let mut p = params;
        p.last_updated_at = now;

        self.risk_params.insert(asset_id.clone(), p);

        self.emit(RegistryEvent::RiskParamsSet {
            asset_id,
            updated_by: caller.to_string(),
        });

        Ok(())
    }

    /// Retrieve risk parameters for a collateral asset.
    pub fn get_risk_params(&self, asset_id: &str) -> Option<&AssetRiskParams> {
        self.risk_params.get(asset_id)
    }

    /// Returns all risk parameter records.
    pub fn all_risk_params(&self) -> Vec<&AssetRiskParams> {
        self.risk_params.values().collect()
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Issue #218 – Asset Upgrade and Migration Path
    // ─────────────────────────────────────────────────────────────────────────

    /// Initiate a migration from `old_contract` to `new_contract`.
    ///
    /// Takes snapshots of `balances` and `allowances` for on-chain verification.
    /// The asset must be registered.  Only one active migration per asset is
    /// allowed at a time.
    ///
    /// # Arguments
    /// * `caller` – initiating admin.
    /// * `asset_id` – canonical identifier of the asset being migrated.
    /// * `old_contract` – address of the contract being replaced.
    /// * `new_contract` – address of the replacement contract.
    /// * `balances` – snapshot of all holder balances from the old contract.
    /// * `allowances` – snapshot of all approvals from the old contract.
    /// * `total_supply` – total supply on the old contract at snapshot time.
    /// * `now` – current ledger timestamp.
    #[allow(clippy::too_many_arguments)]
    pub fn initiate_migration(
        &mut self,
        caller: &str,
        asset_id: &str,
        old_contract: &str,
        new_contract: &str,
        balances: HashMap<String, u64>,
        allowances: HashMap<String, HashMap<String, u64>>,
        total_supply: u64,
        now: u64,
    ) -> Result<(), RegistryError> {
        self.ensure_admin(caller)?;

        if !self.metadata.contains_key(asset_id) {
            return Err(RegistryError::NotRegistered);
        }

        // Disallow a second concurrent migration.
        if let Some(existing) = self.migrations.get(asset_id) {
            if existing.status == MigrationStatus::Pending
                || existing.status == MigrationStatus::InProgress
            {
                return Err(RegistryError::MigrationAlreadyActive);
            }
        }

        // Basic validation: verify balance sum equals total_supply.
        let balance_sum: u64 = balances.values().sum();
        if balance_sum != total_supply {
            return Err(RegistryError::ValidationError(format!(
                "balance snapshot sum ({}) does not equal total_supply ({})",
                balance_sum, total_supply
            )));
        }

        self.migrations.insert(
            asset_id.to_string(),
            MigrationState {
                asset_id: asset_id.to_string(),
                old_contract: old_contract.to_string(),
                new_contract: new_contract.to_string(),
                balance_snapshot: balances,
                allowance_snapshot: allowances,
                total_supply_snapshot: total_supply,
                status: MigrationStatus::InProgress,
                initiated_by: caller.to_string(),
                initiated_at: now,
                completed_at: 0,
                old_contract_paused: false,
            },
        );

        self.emit(RegistryEvent::Migration(MigrationEvent::Initiated {
            asset_id: asset_id.to_string(),
            old_contract: old_contract.to_string(),
            new_contract: new_contract.to_string(),
            initiated_by: caller.to_string(),
        }));

        Ok(())
    }

    /// Complete a migration.
    ///
    /// Verifies that the on-chain balances on the new contract (`new_balances`)
    /// match the snapshot taken at initiation.  If verification passes:
    /// 1. Allowances are considered migrated.
    /// 2. Old contract is marked as paused.
    /// 3. Migration status is set to [`MigrationStatus::Completed`].
    ///
    /// # Arguments
    /// * `caller` – completing admin.
    /// * `asset_id` – asset under migration.
    /// * `new_balances` – on-chain balances read from the new contract.
    /// * `now` – current ledger timestamp.
    pub fn complete_migration(
        &mut self,
        caller: &str,
        asset_id: &str,
        new_balances: &HashMap<String, u64>,
        now: u64,
    ) -> Result<(), RegistryError> {
        self.ensure_admin(caller)?;

        let state = self
            .migrations
            .get_mut(asset_id)
            .ok_or(RegistryError::NoActiveMigration)?;

        if state.status != MigrationStatus::InProgress {
            return Err(RegistryError::MigrationNotInProgress);
        }

        // On-chain balance verification: every snapshotted holder must exist in
        // new_balances with the identical amount.
        for (address, expected_amount) in &state.balance_snapshot {
            let actual = new_balances.get(address).copied().unwrap_or(0);
            if actual != *expected_amount {
                return Err(RegistryError::ValidationError(format!(
                    "balance mismatch for {}: expected {}, got {}",
                    address, expected_amount, actual
                )));
            }
        }

        let accounts_migrated = state.balance_snapshot.len() as u64;
        let total_supply = state.total_supply_snapshot;
        let allowances_migrated: u64 = state
            .allowance_snapshot
            .values()
            .map(|m| m.len() as u64)
            .sum();
        let old_contract = state.old_contract.clone();
        let new_contract = state.new_contract.clone();

        // Mark old contract as paused.
        state.old_contract_paused = true;
        state.status = MigrationStatus::Completed;
        state.completed_at = now;

        self.emit(RegistryEvent::Migration(MigrationEvent::BalancesMigrated {
            asset_id: asset_id.to_string(),
            accounts_migrated,
            total_supply,
        }));

        self.emit(RegistryEvent::Migration(MigrationEvent::AllowancesMigrated {
            asset_id: asset_id.to_string(),
            allowances_migrated,
        }));

        self.emit(RegistryEvent::Migration(MigrationEvent::OldContractPaused {
            asset_id: asset_id.to_string(),
            old_contract: old_contract.clone(),
        }));

        self.emit(RegistryEvent::Migration(MigrationEvent::Completed {
            asset_id: asset_id.to_string(),
            old_contract,
            new_contract,
        }));

        Ok(())
    }

    /// Cancel an in-progress or pending migration.
    ///
    /// The old contract is left running; no state is transferred.
    pub fn cancel_migration(
        &mut self,
        caller: &str,
        asset_id: &str,
    ) -> Result<(), RegistryError> {
        self.ensure_admin(caller)?;

        let state = self
            .migrations
            .get_mut(asset_id)
            .ok_or(RegistryError::NoActiveMigration)?;

        if state.status == MigrationStatus::Completed {
            return Err(RegistryError::MigrationNotInProgress);
        }

        state.status = MigrationStatus::Cancelled;

        self.emit(RegistryEvent::Migration(MigrationEvent::Cancelled {
            asset_id: asset_id.to_string(),
            cancelled_by: caller.to_string(),
        }));

        Ok(())
    }

    /// Retrieve the current migration state for an asset.
    pub fn get_migration_state(&self, asset_id: &str) -> Option<&MigrationState> {
        self.migrations.get(asset_id)
    }
}
