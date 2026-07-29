//! Token contract implementation for Stellar DeFi Toolkit
//!
//! Provides ERC-20-like token functionality on the Stellar blockchain
//! using Soroban smart contracts.
//!
//! ## Access Control
//! This is a plain in-memory Rust struct, not a deployed Soroban `#[contract]` — there
//! is no `Env`/`require_auth` capability in this file at all, so `mint`, `burn`,
//! `transfer`, `approve`, and `transfer_from` have **no access control whatsoever**;
//! any caller can act on behalf of any address. See `docs/ACCESS_CONTROL_MATRIX.md`
//! for the full breakdown and the deployable, correctly-authenticated alternative in
//! `soroban_token_contract.rs`.

use soroban_sdk::{contract, contractimpl, Address, Env, Symbol, Vec};
use crate::types::token::{TokenInfo, TokenMetadata, VestingSchedule};
use crate::utils::StellarClient;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

/// Token contract implementing standard token functionality
#[contract]
pub struct TokenContract {
    /// Token name
    name: String,
    /// Token symbol
    symbol: String,
    /// Total supply
    total_supply: u64,
    /// Token decimals
    decimals: u8,
    /// Contract address
    address: Option<Address>,
    /// Balances per address (address string -> balance)
    balances: HashMap<String, u64>,
    /// Allowances: owner -> spender -> amount
    allowances: HashMap<String, HashMap<String, u64>>,
    /// Admin address for admin-only operations
    admin: Option<String>,
    /// Recovery requests: (original_owner, tx_hash) -> (amount, request_time)
    recovery_requests: HashMap<String, (u64, u64)>,
    /// Recovery delay in seconds
    recovery_delay: u64,
    /// Whether the contract is paused (set after migration – issue #218).
    paused: bool,
    // ─── Issue #222: Token Permit (EIP-2612 style) ─────────────────────────
    /// Permit nonces per owner address (owner_string -> nonce)
    permit_nonces: HashMap<String, u64>,
    // ─── Issue #221: Transfer Hooks ─────────────────────────────────────────
    /// Registered before_transfer hooks: contract_address_string -> true
    before_transfer_hooks: HashMap<String, bool>,
    /// Registered after_transfer hooks: contract_address_string -> true
    after_transfer_hooks: HashMap<String, bool>,
    // ─── Issue #220: SEP-41 Metadata ────────────────────────────────────────
    /// Optional token URI for metadata (SEP-41 extension)
    token_uri: Option<String>,
}

impl TokenContract {
    /// Create a new token contract
    pub fn new(name: String, symbol: String, initial_supply: u64) -> Self {
        Self {
            name,
            symbol,
            total_supply: initial_supply,
            decimals: 7, // Stellar standard
            address: None,
            balances: HashMap::new(),
            allowances: HashMap::new(),
            admin: None,
            recovery_requests: HashMap::new(),
            recovery_delay: 86400,
            paused: false,
        }
    }

    // ─── Issue #218 – Migration Interface ─────────────────────────────────────

    /// Returns `true` if this contract has been paused (e.g. after migration).
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// Pause the contract.  Only the admin may call this.
    ///
    /// In a migration scenario the protocol calls this after all state has been
    /// transferred to the new contract.
    pub fn pause(&mut self, caller: &str) -> Result<(), String> {
        self.ensure_admin(caller)?;
        self.paused = true;
        println!("[Event] ContractPaused {{ contract: {:?} }}", self.address);
        Ok(())
    }

    /// Unpause the contract (admin only).
    pub fn unpause(&mut self, caller: &str) -> Result<(), String> {
        self.ensure_admin(caller)?;
        self.paused = false;
        println!("[Event] ContractUnpaused {{ contract: {:?} }}", self.address);
        Ok(())
    }

    /// Export a snapshot of all balances for migration verification.
    ///
    /// Returns a `HashMap<String, u64>` of address → balance together with the
    /// current total supply so the caller can verify sum(balances) == total_supply.
    pub fn export_balance_snapshot(&self) -> (HashMap<String, u64>, u64) {
        (self.balances.clone(), self.total_supply)
    }

    /// Export a snapshot of all allowances for migration.
    pub fn export_allowance_snapshot(&self) -> HashMap<String, HashMap<String, u64>> {
        self.allowances.clone()
    }

    /// Import balances from a migration snapshot into a *new* contract instance.
    ///
    /// This is called on the **new** contract to replay the old contract's state.
    /// Validates that the sum of imported balances equals `expected_total_supply`.
    pub fn import_migration_state(
        &mut self,
        balances: HashMap<String, u64>,
        allowances: HashMap<String, HashMap<String, u64>>,
        expected_total_supply: u64,
    ) -> Result<(), String> {
        if self.paused {
            return Err("contract is paused".to_string());
        }

        // Verify balance integrity.
        let sum: u64 = balances.values().sum();
        if sum != expected_total_supply {
            return Err(format!(
                "balance snapshot sum ({}) does not match expected total supply ({})",
                sum, expected_total_supply
            ));
        }

        self.balances = balances;
        self.allowances = allowances;
        self.total_supply = expected_total_supply;

        println!(
            "[Event] MigrationStateImported {{ total_supply: {}, accounts: {} }}",
            self.total_supply,
            self.balances.len()
        );

        Ok(())
    }

    // ─── Admin helper ─────────────────────────────────────────────────────────

    /// Set the admin address (can only be done once, or must be called before any admin ops).
    pub fn set_admin(&mut self, admin: String) {
        self.admin = Some(admin);
    }

    fn ensure_admin(&self, caller: &str) -> Result<(), String> {
        match &self.admin {
            Some(admin) if admin == caller => Ok(()),
            Some(_) => Err("caller is not the admin".to_string()),
            None => Err("no admin configured".to_string()),
        }
    }

    fn ensure_not_paused(&self) -> Result<(), String> {
        if self.paused {
            Err("contract is paused".to_string())
        } else {
            Ok(())
            // Issue #222
            permit_nonces: HashMap::new(),
            // Issue #221
            before_transfer_hooks: HashMap::new(),
            after_transfer_hooks: HashMap::new(),
            // Issue #220
            token_uri: None,
        }
    }

    /// Get token information
    pub fn get_info(&self) -> TokenInfo {
        TokenInfo {
            name: self.name.clone(),
            symbol: self.symbol.clone(),
            total_supply: self.total_supply,
            decimals: self.decimals,
        }
    }

    /// Deploy the token contract to Stellar
    pub async fn deploy(mut self, client: &StellarClient) -> anyhow::Result<String> {
        let contract_id = client.deploy_token_contract(&self).await?;
        self.address = Some(Address::from_contract_id(&contract_id));
        Ok(contract_id)
    }

    /// Mint new tokens
    pub fn mint(&mut self, to: Address, amount: u64) -> Result<(), String> {
        self.ensure_not_paused()?;
        if amount == 0 {
            return Err("Amount must be greater than 0".to_string());
        }

        self.total_supply = self.total_supply
            .checked_add(amount)
            .ok_or("Overflow: total supply exceeded u64::MAX")?;

        let key = format!("{:?}", to);
        let entry = self.balances.entry(key).or_insert(0);
        *entry = entry.checked_add(amount).ok_or("Overflow: balance exceeded u64::MAX")?;

        Ok(())
    }

    /// Burn tokens
    pub fn burn(&mut self, from: Address, amount: u64) -> Result<(), String> {
        if amount == 0 {
            return Err("Amount must be greater than 0".to_string());
        }

        if self.total_supply < amount {
            return Err("Insufficient supply to burn".to_string());
        }

        let key = format!("{:?}", from);
        let balance = self.balances.get(&key).copied().unwrap_or(0);
        if balance < amount {
            return Err("Insufficient balance to burn".to_string());
        }

        self.total_supply -= amount;
        *self.balances.entry(key).or_insert(0) -= amount;

        Ok(())
    }

    /// Transfer tokens between addresses
    ///
    /// Fixes issue #15: implements full transfer logic including balance check,
    /// deducting from sender, crediting receiver, and emitting a Transfer event.
    /// Also runs before_transfer and after_transfer hooks (issue #221).
    pub fn transfer(&mut self, from: Address, to: Address, amount: u64) -> Result<(), String> {
        self.ensure_not_paused()?;
        if amount == 0 {
            return Err("Amount must be greater than 0".to_string());
        }

        if from == to {
            return Err("Cannot transfer to the same address".to_string());
        }

        let from_key = format!("{:?}", from);
        let to_key = format!("{:?}", to);

        // 1. Run before_transfer hooks — any blocking hook reverts the transfer
        self.run_before_transfer_hooks(&from_key, &to_key, amount)?;

        // 2. Check sender's balance
        let sender_balance = self.balances.get(&from_key).copied().unwrap_or(0);
        if sender_balance < amount {
            return Err(format!(
                "Insufficient balance: sender has {}, tried to send {}",
                sender_balance, amount
            ));
        }

        // 3. Subtract amount from sender
        *self.balances.entry(from_key.clone()).or_insert(0) -= amount;

        // 4. Add amount to receiver
        let receiver_entry = self.balances.entry(to_key.clone()).or_insert(0);
        *receiver_entry = receiver_entry
            .checked_add(amount)
            .ok_or("Overflow: receiver balance exceeded u64::MAX")?;

        // 5. Emit Transfer event (logged as a structured record)
        self.emit_transfer_event(&from_key, &to_key, amount);

        // 6. Run after_transfer hooks (notification only)
        self.run_after_transfer_hooks(&from_key, &to_key, amount);

        Ok(())
    }

    /// Get balance of an address
    pub fn balance_of(&self, address: Address) -> u64 {
        self.balances.get(&format!("{:?}", address)).copied().unwrap_or(0)
    }

    /// Approve spending for another address
    ///
    /// Fixes issue #16: implements full approval logic including storing the
    /// allowance and emitting an Approval event.
    pub fn approve(&mut self, owner: Address, spender: Address, amount: u64) -> Result<(), String> {
        if amount == 0 {
            return Err("Amount must be greater than 0".to_string());
        }

        let owner_key = format!("{:?}", owner);
        let spender_key = format!("{:?}", spender);

        // 1. Store the allowance: owner -> spender -> amount
        self.allowances
            .entry(owner_key.clone())
            .or_insert_with(HashMap::new)
            .insert(spender_key.clone(), amount);

        // 2. Emit Approval event
        self.emit_approval_event(&owner_key, &spender_key, amount);

        Ok(())
    }

    /// Get allowance for a spender
    pub fn allowance(&self, owner: Address, spender: Address) -> u64 {
        self.allowances
            .get(&format!("{:?}", owner))
            .and_then(|m| m.get(&format!("{:?}", spender)))
            .copied()
            .unwrap_or(0)
    }

    /// Transfer from approved address
    ///
    /// Fixes issue #17: implements full transfer_from logic including checking
    /// allowance, performing the transfer, updating the allowance, and emitting
    /// events.
    pub fn transfer_from(
        &mut self,
        spender: Address,
        from: Address,
        to: Address,
        amount: u64,
    ) -> Result<(), String> {
        if amount == 0 {
            return Err("Amount must be greater than 0".to_string());
        }

        let from_key = format!("{:?}", from);
        let spender_key = format!("{:?}", spender);

        // 1. Check allowance
        let current_allowance = self
            .allowances
            .get(&from_key)
            .and_then(|m| m.get(&spender_key))
            .copied()
            .unwrap_or(0);

        if current_allowance < amount {
            return Err(format!(
                "Insufficient allowance: spender has {}, tried to spend {}",
                current_allowance, amount
            ));
        }

        // 2. Perform transfer (reuses transfer logic which emits Transfer event)
        self.transfer(from.clone(), to, amount)?;

        // 3. Update allowance
        if let Some(owner_allowances) = self.allowances.get_mut(&from_key) {
            if let Some(allowance) = owner_allowances.get_mut(&spender_key) {
                *allowance -= amount;
            }
        }

        Ok(())
    }

    // ─── Issue #222: Token Permit (EIP-2612 style) ─────────────────────────

    /// Return the current permit nonce for `owner`.
    ///
    /// The nonce is included in the permit message and incremented on every
    /// successful `permit` call to prevent replay attacks.
    pub fn nonce_of(&self, owner: &Address) -> u64 {
        let key = format!("{:?}", owner);
        self.permit_nonces.get(&key).copied().unwrap_or(0)
    }

    /// Process an off-chain permit, granting `spender` an allowance of `value`
    /// tokens on behalf of `owner`.
    ///
    /// This implements an EIP-2612-style gasless approval for Stellar.  Because
    /// Soroban does not expose secp256k1 ecrecover directly in unit-test mode,
    /// we use HMAC-SHA256 as a deterministic stand-in for signature verification
    /// (suitable for the in-memory / testing context of this struct).
    ///
    /// # Arguments
    /// * `owner`    – Token holder granting the allowance.
    /// * `spender`  – Address being authorised to spend.
    /// * `value`    – Allowance amount.
    /// * `deadline` – UNIX timestamp after which the permit is invalid.
    /// * `now`      – Current UNIX timestamp (injected for testability).
    /// * `signature` – 32-byte permit hash produced by `compute_permit_hash`.
    pub fn permit(
        &mut self,
        owner: Address,
        spender: Address,
        value: u64,
        deadline: u64,
        now: u64,
        signature: [u8; 32],
    ) -> Result<(), String> {
        // 1. Enforce deadline
        if now > deadline {
            return Err("Permit: deadline expired".to_string());
        }

        let owner_key = format!("{:?}", owner);
        let spender_key = format!("{:?}", spender);
        let nonce = self.permit_nonces.get(&owner_key).copied().unwrap_or(0);

        // 2. Verify the signature
        let expected = crate::utils::helpers::compute_permit_hash(
            &owner_key,
            &spender_key,
            value,
            deadline,
            nonce,
        );
        if signature != expected {
            return Err("Permit: invalid signature".to_string());
        }

        // 3. Increment nonce (prevents replay)
        self.permit_nonces.insert(owner_key.clone(), nonce + 1);

        // 4. Set allowance
        self.allowances
            .entry(owner_key.clone())
            .or_insert_with(HashMap::new)
            .insert(spender_key.clone(), value);

        // 5. Emit Approval event
        self.emit_approval_event(&owner_key, &spender_key, value);

        Ok(())
    }

    // ─── Issue #221: Transfer Hooks ─────────────────────────────────────────

    /// Register a before_transfer hook for `hook_contract`.
    ///
    /// The registered contract address acts as a logical hook identifier.
    /// When a transfer is attempted, every registered before_transfer hook is
    /// consulted; the hook can signal rejection (simulated by
    /// `set_before_hook_blocks`). Gas is bounded because the hook set is limited
    /// to `MAX_HOOKS`.
    pub fn register_before_transfer_hook(&mut self, hook_contract: Address) -> Result<(), String> {
        const MAX_HOOKS: usize = 10;
        if self.before_transfer_hooks.len() >= MAX_HOOKS {
            return Err("Hook limit reached".to_string());
        }
        let key = format!("{:?}", hook_contract);
        println!("[Event] HookRegistered {{ type: before_transfer, contract: {:?} }}", hook_contract);
        self.before_transfer_hooks.insert(key, true);
        Ok(())
    }

    /// Register an after_transfer hook for `hook_contract`.
    ///
    /// After-transfer hooks are notification-only and cannot revert the transfer.
    pub fn register_after_transfer_hook(&mut self, hook_contract: Address) -> Result<(), String> {
        const MAX_HOOKS: usize = 10;
        if self.after_transfer_hooks.len() >= MAX_HOOKS {
            return Err("Hook limit reached".to_string());
        }
        let key = format!("{:?}", hook_contract);
        println!("[Event] HookRegistered {{ type: after_transfer, contract: {:?} }}", hook_contract);
        self.after_transfer_hooks.insert(key, true);
        Ok(())
    }

    /// Unregister a before_transfer hook.
    pub fn unregister_before_transfer_hook(&mut self, hook_contract: &Address) -> Result<(), String> {
        let key = format!("{:?}", hook_contract);
        if self.before_transfer_hooks.remove(&key).is_none() {
            return Err("Hook not registered".to_string());
        }
        Ok(())
    }

    /// Unregister an after_transfer hook.
    pub fn unregister_after_transfer_hook(&mut self, hook_contract: &Address) -> Result<(), String> {
        let key = format!("{:?}", hook_contract);
        if self.after_transfer_hooks.remove(&key).is_none() {
            return Err("Hook not registered".to_string());
        }
        Ok(())
    }

    /// Control whether a before_transfer hook blocks transfers.
    ///
    /// `blocks = true`  → the hook rejects the transfer (reverts).
    /// `blocks = false` → the hook allows the transfer.
    ///
    /// In a production Soroban contract this would be a cross-contract call.
    /// Here we simulate it as a flag so tests can exercise both paths.
    pub fn set_before_hook_blocks(&mut self, hook_contract: &Address, blocks: bool) {
        let key = format!("{:?}", hook_contract);
        if self.before_transfer_hooks.contains_key(&key) {
            // `true` stored = allowed; `false` stored = blocking
            self.before_transfer_hooks.insert(key, !blocks);
        }
    }

    /// Execute all registered before_transfer hooks.
    ///
    /// Returns `Err` if any hook is in blocking state.
    fn run_before_transfer_hooks(&self, from: &str, to: &str, amount: u64) -> Result<(), String> {
        for (hook, allowed) in &self.before_transfer_hooks {
            if !allowed {
                return Err(format!(
                    "Transfer rejected by before_transfer hook {}: from={} to={} amount={}",
                    hook, from, to, amount
                ));
            }
        }
        Ok(())
    }

    /// Execute all registered after_transfer hooks (notification only).
    fn run_after_transfer_hooks(&self, from: &str, to: &str, amount: u64) {
        for hook in self.after_transfer_hooks.keys() {
            println!(
                "[Hook] after_transfer {{ hook: {}, from: {}, to: {}, amount: {} }}",
                hook, from, to, amount
            );
        }
    }

    // ─── Issue #220: SEP-41 Token Metadata Standard ──────────────────────────

    /// Return the token name (SEP-41).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Return the token symbol (SEP-41).
    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    /// Return the number of decimals (SEP-41).
    pub fn decimals(&self) -> u8 {
        self.decimals
    }

    /// Return the metadata URI for this token (SEP-41 extension).
    ///
    /// Returns `None` if no URI has been set.
    pub fn token_uri(&self) -> Option<&str> {
        self.token_uri.as_deref()
    }

    /// Set the metadata URI.
    ///
    /// Emits a `MetadataUpdated` event. In a production Soroban contract this
    /// would be restricted to the admin via `require_auth`.
    pub fn set_token_uri(&mut self, uri: String) {
        println!("[Event] MetadataUpdated {{ token_uri: {} }}", uri);
        self.token_uri = Some(uri);
    }

    // ─── Batch Transfer (#227) ─────────────────────────────────────────────

    /// Batch transfer tokens to multiple recipients
    ///
    /// Sends tokens to multiple recipients in a single transaction,
    /// reducing gas costs for payroll, airdrops, and distributions.
    /// The operation is atomic: all transfers succeed or none do.
    ///
    /// # Arguments
    /// * `from` - Sender address
    /// * `transfers` - Vector of (recipient, amount) pairs
    pub fn batch_transfer(
        &mut self,
        from: Address,
        transfers: std::vec::Vec<(Address, u64)>,
    ) -> Result<(), String> {
        if transfers.is_empty() {
            return Err("Transfers list cannot be empty".to_string());
        }

        // Calculate total amount and validate all amounts are > 0
        let mut total_amount: u64 = 0;
        for i in 0..transfers.len() {
            let amount = transfers[i].1;
            if amount == 0 {
                return Err("All transfer amounts must be greater than 0".to_string());
            }
            total_amount = total_amount
                .checked_add(amount)
                .ok_or("Overflow: total transfer amount exceeds u64::MAX")?;
        }

        let from_key = format!("{:?}", from);

        // Check sender has enough balance for ALL transfers
        let sender_balance = self.balances.get(&from_key).copied().unwrap_or(0);
        if sender_balance < total_amount {
            return Err(format!(
                "Insufficient balance: sender has {}, total transfers require {}",
                sender_balance, total_amount
            ));
        }

        // Pre-validate all recipients (deduplicate from)
        for i in 0..transfers.len() {
            let recipient_key = format!("{:?}", transfers[i].0);
            if from_key == recipient_key {
                return Err(format!(
                    "Cannot transfer to self: {}",
                    recipient_key
                ));
            }
        }

        // Deduct total from sender once
        *self.balances.entry(from_key.clone()).or_insert(0) -= total_amount;

        // Credit each recipient and emit events
        for i in 0..transfers.len() {
            let recipient_key = format!("{:?}", transfers[i].0);
            let amount = transfers[i].1;
            let entry = self.balances.entry(recipient_key.clone()).or_insert(0);
            *entry = entry
                .checked_add(amount)
                .ok_or("Overflow: recipient balance exceeded u64::MAX")?;

            self.emit_transfer_event(&from_key, &recipient_key, amount);
        }

        Ok(())
    }

    // -------------------------------------------------------------------------
    // Internal event helpers
    // -------------------------------------------------------------------------

    /// Emit a Transfer event (structured log; in Soroban this would call env.events().publish())
    fn emit_transfer_event(&self, from: &str, to: &str, amount: u64) {
        // In a live Soroban contract this becomes:
        //   env.events().publish((symbol_short!("transfer"), from, to), amount);
        // Here we use a structured log so the event is visible in test output.
        println!(
            "[Event] Transfer {{ from: {}, to: {}, amount: {} }}",
            from, to, amount
        );
    }

    /// Emit an Approval event (structured log; in Soroban this would call env.events().publish())
    fn emit_approval_event(&self, owner: &str, spender: &str, amount: u64) {
        // In a live Soroban contract this becomes:
        //   env.events().publish((symbol_short!("approval"), owner, spender), amount);
        println!(
            "[Event] Approval {{ owner: {}, spender: {}, amount: {} }}",
            owner, spender, amount
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_creation() {
        let token = TokenContract::new("Test Token".to_string(), "TEST".to_string(), 1000000);

        assert_eq!(token.name, "Test Token");
        assert_eq!(token.symbol, "TEST");
        assert_eq!(token.total_supply, 1000000);
        assert_eq!(token.decimals, 7);
    }

    #[test]
    fn test_mint() {
        let mut token = TokenContract::new("Test Token".to_string(), "TEST".to_string(), 1000000);
        let address = Address::generate(&Env::default());

        let initial_supply = token.total_supply;
        token.mint(address.clone(), 500000).unwrap();

        assert_eq!(token.total_supply, initial_supply + 500000);
        assert_eq!(token.balance_of(address), 500000);
    }

    #[test]
    fn test_burn() {
        let mut token = TokenContract::new("Test Token".to_string(), "TEST".to_string(), 1000000);
        let address = Address::generate(&Env::default());

        token.mint(address.clone(), 200000).unwrap();
        let supply_after_mint = token.total_supply;

        token.burn(address.clone(), 100000).unwrap();

        assert_eq!(token.total_supply, supply_after_mint - 100000);
        assert_eq!(token.balance_of(address), 100000);
    }

    #[test]
    fn test_invalid_mint_amount() {
        let mut token = TokenContract::new("Test Token".to_string(), "TEST".to_string(), 1000000);
        let address = Address::generate(&Env::default());

        let result = token.mint(address, 0);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Amount must be greater than 0");
    }

    #[test]
    fn test_invalid_burn_amount() {
        let mut token = TokenContract::new("Test Token".to_string(), "TEST".to_string(), 1000000);
        let address = Address::generate(&Env::default());

        let result = token.burn(address, 2000000);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Insufficient supply to burn");
    }
}

#[cfg(test)]
mod soroban_tests {
    use super::*;

    #[test]
    fn test_transfer_basic() {
        let mut token = TokenContract::new("Test Token".to_string(), "TEST".to_string(), 1000000);
        let from = Address::generate(&Env::default());
        let to = Address::generate(&Env::default());

        token.mint(from.clone(), 1000).unwrap();
        token.transfer(from.clone(), to.clone(), 400).unwrap();

        assert_eq!(token.balance_of(from), 600);
        assert_eq!(token.balance_of(to), 400);
    }

    #[test]
    fn test_transfer_insufficient_balance() {
        let mut token = TokenContract::new("Test Token".to_string(), "TEST".to_string(), 1000000);
        let from = Address::generate(&Env::default());
        let to = Address::generate(&Env::default());

        token.mint(from.clone(), 100).unwrap();

        let result = token.transfer(from, to, 500);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Insufficient balance"));
    }

    #[test]
    fn test_approve_and_allowance() {
        let mut token = TokenContract::new("Test Token".to_string(), "TEST".to_string(), 1000000);
        let owner = Address::generate(&Env::default());
        let spender = Address::generate(&Env::default());

        token.approve(owner.clone(), spender.clone(), 300).unwrap();
        assert_eq!(token.allowance(owner, spender), 300);
    }

    #[test]
    fn test_transfer_from_success() {
        let mut token = TokenContract::new("Test Token".to_string(), "TEST".to_string(), 1000000);
        let owner = Address::generate(&Env::default());
        let spender = Address::generate(&Env::default());
        let receiver = Address::generate(&Env::default());

        token.mint(owner.clone(), 1000).unwrap();
        token.approve(owner.clone(), spender.clone(), 500).unwrap();

        token.transfer_from(spender.clone(), owner.clone(), receiver.clone(), 200).unwrap();

        assert_eq!(token.balance_of(owner.clone()), 800);
        assert_eq!(token.balance_of(receiver), 200);
        assert_eq!(token.allowance(owner, spender), 300);
    }

    #[test]
    fn test_transfer_from_insufficient_allowance() {
        let mut token = TokenContract::new("Test Token".to_string(), "TEST".to_string(), 1000000);
        let owner = Address::generate(&Env::default());
        let spender = Address::generate(&Env::default());
        let receiver = Address::generate(&Env::default());

        token.mint(owner.clone(), 1000).unwrap();
        token.approve(owner.clone(), spender.clone(), 50).unwrap();

        let result = token.transfer_from(spender, owner, receiver, 200);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Insufficient allowance"));
    }

    // ─── Batch Transfer Tests (#227) ──────────────────────────────────────

    #[test]
    fn test_batch_transfer_basic() {
        let mut token = TokenContract::new("Test Token".to_string(), "TEST".to_string(), 1000000);
        let sender = Address::generate(&Env::default());
        let r1 = Address::generate(&Env::default());
        let r2 = Address::generate(&Env::default());
        let r3 = Address::generate(&Env::default());

        token.mint(sender.clone(), 1000).unwrap();

        let transfers = vec![
            (r1.clone(), 200u64),
            (r2.clone(), 150u64),
            (r3.clone(), 100u64),
        ];

        token.batch_transfer(sender.clone(), transfers).unwrap();

        assert_eq!(token.balance_of(sender), 550);
        assert_eq!(token.balance_of(r1), 200);
        assert_eq!(token.balance_of(r2), 150);
        assert_eq!(token.balance_of(r3), 100);
    }

    #[test]
    fn test_batch_transfer_insufficient_balance() {
        let mut token = TokenContract::new("Test Token".to_string(), "TEST".to_string(), 1000000);
        let sender = Address::generate(&Env::default());
        let r1 = Address::generate(&Env::default());
        let r2 = Address::generate(&Env::default());

        token.mint(sender.clone(), 100).unwrap();

        let transfers = vec![
            (r1.clone(), 80u64),
            (r2.clone(), 50u64),
        ];

        let result = token.batch_transfer(sender.clone(), transfers);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Insufficient balance"));

        assert_eq!(token.balance_of(sender), 100);
        assert_eq!(token.balance_of(r1), 0);
        assert_eq!(token.balance_of(r2), 0);
    }

    #[test]
    fn test_batch_transfer_empty_list() {
        let mut token = TokenContract::new("Test Token".to_string(), "TEST".to_string(), 1000000);
        let sender = Address::generate(&Env::default());

        let transfers: Vec<(Address, u64)> = vec![];
        let result = token.batch_transfer(sender, transfers);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("cannot be empty"));
    }

    #[test]
    fn test_batch_transfer_self_transfer() {
        let mut token = TokenContract::new("Test Token".to_string(), "TEST".to_string(), 1000000);
        let sender = Address::generate(&Env::default());
        let r1 = Address::generate(&Env::default());

        token.mint(sender.clone(), 1000).unwrap();

        let transfers = vec![
            (r1.clone(), 100u64),
            (sender.clone(), 50u64),
        ];

        let result = token.batch_transfer(sender.clone(), transfers);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Cannot transfer to self"));
    }
}

// ─── Issue #222: Permit Tests ─────────────────────────────────────────────────
#[cfg(test)]
mod permit_tests {
    use super::*;
    use crate::utils::helpers::compute_permit_hash;

    fn make_token() -> TokenContract {
        TokenContract::new("Permit Token".to_string(), "PT".to_string(), 1_000_000)
    }

    #[test]
    fn test_permit_basic() {
        let mut token = make_token();
        let env = Env::default();
        let owner = Address::generate(&env);
        let spender = Address::generate(&env);

        let value: u64 = 500;
        let deadline: u64 = 9_999_999_999;
        let now: u64 = 1_000;
        let nonce = token.nonce_of(&owner); // 0

        let sig = compute_permit_hash(
            &format!("{:?}", owner),
            &format!("{:?}", spender),
            value,
            deadline,
            nonce,
        );

        token
            .permit(owner.clone(), spender.clone(), value, deadline, now, sig)
            .unwrap();

        // Allowance should be set
        assert_eq!(token.allowance(owner.clone(), spender.clone()), value);
        // Nonce should have incremented
        assert_eq!(token.nonce_of(&owner), 1);
    }

    #[test]
    fn test_permit_expired_deadline() {
        let mut token = make_token();
        let env = Env::default();
        let owner = Address::generate(&env);
        let spender = Address::generate(&env);

        let nonce = token.nonce_of(&owner);
        let sig = compute_permit_hash(
            &format!("{:?}", owner),
            &format!("{:?}", spender),
            100,
            500, // deadline in the past
            nonce,
        );

        let result = token.permit(owner, spender, 100, 500, 1_000, sig);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("deadline expired"));
    }

    #[test]
    fn test_permit_invalid_signature() {
        let mut token = make_token();
        let env = Env::default();
        let owner = Address::generate(&env);
        let spender = Address::generate(&env);

        let bad_sig = [0xde_u8; 32];
        let result = token.permit(owner, spender, 100, 9_999_999_999, 1_000, bad_sig);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid signature"));
    }

    #[test]
    fn test_permit_replay_prevention() {
        // Using the same signature a second time must fail because the nonce
        // has advanced after the first successful permit.
        let mut token = make_token();
        let env = Env::default();
        let owner = Address::generate(&env);
        let spender = Address::generate(&env);

        let value: u64 = 200;
        let deadline: u64 = 9_999_999_999;
        let now: u64 = 1_000;
        let nonce = token.nonce_of(&owner); // 0

        let sig = compute_permit_hash(
            &format!("{:?}", owner),
            &format!("{:?}", spender),
            value,
            deadline,
            nonce,
        );

        // First call succeeds
        token
            .permit(owner.clone(), spender.clone(), value, deadline, now, sig)
            .unwrap();

        // Second call with the same signature must fail (nonce is now 1)
        let result = token.permit(owner, spender, value, deadline, now, sig);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid signature"));
    }

    #[test]
    fn test_permit_then_transfer_from() {
        // Full gasless flow: permit → transfer_from
        let mut token = make_token();
        let env = Env::default();
        let owner = Address::generate(&env);
        let spender = Address::generate(&env);
        let receiver = Address::generate(&env);

        token.mint(owner.clone(), 1_000).unwrap();

        let value: u64 = 400;
        let deadline: u64 = 9_999_999_999;
        let now: u64 = 1_000;
        let nonce = token.nonce_of(&owner);

        let sig = compute_permit_hash(
            &format!("{:?}", owner),
            &format!("{:?}", spender),
            value,
            deadline,
            nonce,
        );
        token
            .permit(owner.clone(), spender.clone(), value, deadline, now, sig)
            .unwrap();

        // Relayer (spender) submits the transfer
        token
            .transfer_from(spender.clone(), owner.clone(), receiver.clone(), 400)
            .unwrap();

        assert_eq!(token.balance_of(owner.clone()), 600);
        assert_eq!(token.balance_of(receiver), 400);
        assert_eq!(token.allowance(owner, spender), 0);
    }
}

// ─── Issue #221: Transfer Hook Tests ─────────────────────────────────────────
#[cfg(test)]
mod hook_tests {
    use super::*;

    fn make_token() -> TokenContract {
        TokenContract::new("Hook Token".to_string(), "HT".to_string(), 1_000_000)
    }

    #[test]
    fn test_register_before_hook() {
        let mut token = make_token();
        let env = Env::default();
        let hook = Address::generate(&env);
        token.register_before_transfer_hook(hook.clone()).unwrap();
        // Hook is registered and defaults to allowing transfers
        assert!(token.before_transfer_hooks.contains_key(&format!("{:?}", hook)));
    }

    #[test]
    fn test_register_after_hook() {
        let mut token = make_token();
        let env = Env::default();
        let hook = Address::generate(&env);
        token.register_after_transfer_hook(hook.clone()).unwrap();
        assert!(token.after_transfer_hooks.contains_key(&format!("{:?}", hook)));
    }

    #[test]
    fn test_before_hook_allows_transfer() {
        let mut token = make_token();
        let env = Env::default();
        let hook = Address::generate(&env);
        let from = Address::generate(&env);
        let to = Address::generate(&env);

        token.mint(from.clone(), 1_000).unwrap();
        token.register_before_transfer_hook(hook).unwrap();

        // Hook defaults to allowing; transfer should succeed
        token.transfer(from.clone(), to.clone(), 300).unwrap();
        assert_eq!(token.balance_of(from), 700);
        assert_eq!(token.balance_of(to), 300);
    }

    #[test]
    fn test_before_hook_blocks_transfer() {
        let mut token = make_token();
        let env = Env::default();
        let hook = Address::generate(&env);
        let from = Address::generate(&env);
        let to = Address::generate(&env);

        token.mint(from.clone(), 1_000).unwrap();
        token.register_before_transfer_hook(hook.clone()).unwrap();
        // Set the hook to blocking mode
        token.set_before_hook_blocks(&hook, true);

        let result = token.transfer(from.clone(), to.clone(), 300);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("rejected by before_transfer hook"));

        // Balances must be unchanged (atomic revert)
        assert_eq!(token.balance_of(from), 1_000);
        assert_eq!(token.balance_of(to), 0);
    }

    #[test]
    fn test_after_hook_does_not_revert_on_notification() {
        // After hook is always notification-only and never reverts.
        let mut token = make_token();
        let env = Env::default();
        let hook = Address::generate(&env);
        let from = Address::generate(&env);
        let to = Address::generate(&env);

        token.mint(from.clone(), 1_000).unwrap();
        token.register_after_transfer_hook(hook).unwrap();

        // Transfer must succeed even with an after hook registered.
        token.transfer(from.clone(), to.clone(), 200).unwrap();
        assert_eq!(token.balance_of(from), 800);
        assert_eq!(token.balance_of(to), 200);
    }

    #[test]
    fn test_unregister_before_hook() {
        let mut token = make_token();
        let env = Env::default();
        let hook = Address::generate(&env);
        let from = Address::generate(&env);
        let to = Address::generate(&env);

        token.mint(from.clone(), 1_000).unwrap();
        token.register_before_transfer_hook(hook.clone()).unwrap();
        token.set_before_hook_blocks(&hook, true);

        // Hook is blocking — transfer fails
        assert!(token.transfer(from.clone(), to.clone(), 100).is_err());

        // Unregister the hook
        token.unregister_before_transfer_hook(&hook).unwrap();

        // Transfer now succeeds
        token.transfer(from.clone(), to.clone(), 100).unwrap();
        assert_eq!(token.balance_of(to), 100);
    }

    #[test]
    fn test_hook_limit() {
        let mut token = make_token();
        let env = Env::default();

        for _ in 0..10 {
            let hook = Address::generate(&env);
            token.register_before_transfer_hook(hook).unwrap();
        }

        let extra_hook = Address::generate(&env);
        let result = token.register_before_transfer_hook(extra_hook);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Hook limit reached"));
    }
}

// ─── Issue #220: SEP-41 Metadata Tests ───────────────────────────────────────
#[cfg(test)]
mod sep41_metadata_tests {
    use super::*;

    #[test]
    fn test_name_returns_token_name() {
        let token = TokenContract::new("Stellar Token".to_string(), "XLM".to_string(), 0);
        assert_eq!(token.name(), "Stellar Token");
    }

    #[test]
    fn test_symbol_returns_token_symbol() {
        let token = TokenContract::new("Stellar Token".to_string(), "XLM".to_string(), 0);
        assert_eq!(token.symbol(), "XLM");
    }

    #[test]
    fn test_decimals_returns_seven() {
        let token = TokenContract::new("T".to_string(), "T".to_string(), 0);
        assert_eq!(token.decimals(), 7);
    }

    #[test]
    fn test_token_uri_initially_none() {
        let token = TokenContract::new("T".to_string(), "T".to_string(), 0);
        assert_eq!(token.token_uri(), None);
    }

    #[test]
    fn test_set_and_get_token_uri() {
        let mut token = TokenContract::new("T".to_string(), "T".to_string(), 0);
        token.set_token_uri("https://example.com/token-metadata.json".to_string());
        assert_eq!(
            token.token_uri(),
            Some("https://example.com/token-metadata.json")
        );
    }

    #[test]
    fn test_metadata_event_on_initialization() {
        // Verify we can retrieve all SEP-41 fields after construction.
        let mut token =
            TokenContract::new("DeFi Token".to_string(), "DFT".to_string(), 21_000_000);
        token.set_token_uri("ipfs://Qm123/metadata.json".to_string());

        assert_eq!(token.name(), "DeFi Token");
        assert_eq!(token.symbol(), "DFT");
        assert_eq!(token.decimals(), 7);
        assert_eq!(token.token_uri(), Some("ipfs://Qm123/metadata.json"));
    }
}
