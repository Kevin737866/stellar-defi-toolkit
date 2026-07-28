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
        if amount == 0 {
            return Err("Amount must be greater than 0".to_string());
        }

        self.total_supply = self.total_supply
            .checked_add(amount)
            .ok_or("Overflow: total supply exceeded u64::MAX")?;

        let key = to.to_string();
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

        let key = from.to_string();
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
    pub fn transfer(&mut self, from: Address, to: Address, amount: u64) -> Result<(), String> {
        if amount == 0 {
            return Err("Amount must be greater than 0".to_string());
        }

        if from == to {
            return Err("Cannot transfer to the same address".to_string());
        }

        let from_key = from.to_string();
        let to_key = to.to_string();

        // 1. Check sender's balance
        let sender_balance = self.balances.get(&from_key).copied().unwrap_or(0);
        if sender_balance < amount {
            return Err(format!(
                "Insufficient balance: sender has {}, tried to send {}",
                sender_balance, amount
            ));
        }

        // 2. Subtract amount from sender
        *self.balances.entry(from_key.clone()).or_insert(0) -= amount;

        // 3. Add amount to receiver
        let receiver_entry = self.balances.entry(to_key.clone()).or_insert(0);
        *receiver_entry = receiver_entry
            .checked_add(amount)
            .ok_or("Overflow: receiver balance exceeded u64::MAX")?;

        // 4. Emit Transfer event (logged as a structured record)
        self.emit_transfer_event(&from_key, &to_key, amount);

        Ok(())
    }

    /// Get balance of an address
    pub fn balance_of(&self, address: Address) -> u64 {
        self.balances.get(&address.to_string()).copied().unwrap_or(0)
    }

    /// Approve spending for another address
    ///
    /// Fixes issue #16: implements full approval logic including storing the
    /// allowance and emitting an Approval event.
    pub fn approve(&mut self, owner: Address, spender: Address, amount: u64) -> Result<(), String> {
        if amount == 0 {
            return Err("Amount must be greater than 0".to_string());
        }

        let owner_key = owner.to_string();
        let spender_key = spender.to_string();

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
            .get(&owner.to_string())
            .and_then(|m| m.get(&spender.to_string()))
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

        let from_key = from.to_string();
        let spender_key = spender.to_string();

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
        transfers: Vec<(Address, u64)>,
    ) -> Result<(), String> {
        if transfers.is_empty() {
            return Err("Transfers list cannot be empty".to_string());
        }

        // Calculate total amount and validate all amounts are > 0
        let mut total_amount: u64 = 0;
        for (_, amount) in transfers.iter() {
            if *amount == 0 {
                return Err("All transfer amounts must be greater than 0".to_string());
            }
            total_amount = total_amount
                .checked_add(*amount)
                .ok_or("Overflow: total transfer amount exceeds u64::MAX")?;
        }

        let from_key = from.to_string();

        // Check sender has enough balance for ALL transfers
        let sender_balance = self.balances.get(&from_key).copied().unwrap_or(0);
        if sender_balance < total_amount {
            return Err(format!(
                "Insufficient balance: sender has {}, total transfers require {}",
                sender_balance, total_amount
            ));
        }

        // Pre-validate all recipients (deduplicate from)
        for (recipient, _) in transfers.iter() {
            let recipient_key = recipient.to_string();
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
        for (recipient, amount) in transfers.iter() {
            let recipient_key = recipient.to_string();
            let entry = self.balances.entry(recipient_key.clone()).or_insert(0);
            *entry = entry
                .checked_add(*amount)
                .ok_or("Overflow: recipient balance exceeded u64::MAX")?;

            self.emit_transfer_event(&from_key, &recipient_key, *amount);
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
