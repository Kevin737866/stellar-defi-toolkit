use soroban_sdk::{contract, contractimpl, Address, Env, Symbol, symbol_short};

const ADMIN_KEY: Symbol = symbol_short!("ADMIN");

fn balance_key(env: &Env, owner: &Address) -> soroban_sdk::Bytes {
    let mut key = soroban_sdk::Bytes::new(env);
    key.append(&soroban_sdk::Bytes::from_slice(env, b"bal:"));
    key.append(&owner.clone().to_string().into_bytes(env));
    key
}

fn allowance_key(env: &Env, owner: &Address, spender: &Address) -> soroban_sdk::Bytes {
    let mut key = soroban_sdk::Bytes::new(env);
    key.append(&soroban_sdk::Bytes::from_slice(env, b"allow:"));
    key.append(&owner.clone().to_string().into_bytes(env));
    key.append(&soroban_sdk::Bytes::from_slice(env, b":"));
    key.append(&spender.clone().to_string().into_bytes(env));
    key
use soroban_sdk::{contract, contractimpl, Address, Env, Symbol, Vec};
use soroban_token_sdk::{Token, TokenInterface};
use crate::types::token::{TokenInfo, TokenMetadata};
use crate::utils::StellarClient;
use std::collections::HashMap;

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
}

#[contract]
pub struct TokenContract;

#[contractimpl]
impl TokenContract {
    /// Initialize the contract and set the admin
    pub fn initialize(env: Env, admin: Address) {
        env.storage().instance().set(&ADMIN_KEY, &admin);
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
        }
    }

    /// Returns the balance of the given address using persistent storage
    pub fn balance_of(env: Env, owner: Address) -> i128 {
        let key = balance_key(&env, &owner);
        env.storage().persistent().get(&key).unwrap_or(0)
    }

    /// Returns the allowance that `spender` is allowed to spend on behalf of `owner`
    pub fn allowance(env: Env, owner: Address, spender: Address) -> i128 {
        let key = allowance_key(&env, &owner, &spender);
        env.storage().persistent().get(&key).unwrap_or(0)
    }

    /// Approve `spender` to spend up to `amount` tokens on behalf of the caller
    pub fn approve(env: Env, owner: Address, spender: Address, amount: i128) {
        owner.require_auth();
        let key = allowance_key(&env, &owner, &spender);
        env.storage().persistent().set(&key, &amount);
    }

    /// Transfer tokens from the caller to `to`
    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        let from_balance = Self::balance_of(env.clone(), from.clone());
        assert!(from_balance >= amount, "Insufficient balance");

        let from_key = balance_key(&env, &from);
        let to_key = balance_key(&env, &to);

        let to_balance = Self::balance_of(env.clone(), to.clone());
        env.storage().persistent().set(&from_key, &(from_balance - amount));
        env.storage().persistent().set(&to_key, &(to_balance + amount));
    }

    /// Transfer tokens on behalf of `from` using an allowance
    pub fn transfer_from(env: Env, spender: Address, from: Address, to: Address, amount: i128) {
        spender.require_auth();
        let current_allowance = Self::allowance(env.clone(), from.clone(), spender.clone());
        assert!(current_allowance >= amount, "Insufficient allowance");

        // Reduce allowance
        let allow_key = allowance_key(&env, &from, &spender);
        env.storage().persistent().set(&allow_key, &(current_allowance - amount));

        // Execute transfer
        let from_balance = Self::balance_of(env.clone(), from.clone());
        assert!(from_balance >= amount, "Insufficient balance");
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

        // 2. Perform transfer (reuses transfer logic)
        self.transfer(from.clone(), to, amount)?;

        // 3. Update allowance
        if let Some(owner_allowances) = self.allowances.get_mut(&from_key) {
            if let Some(allowance) = owner_allowances.get_mut(&spender_key) {
                *allowance -= amount;
            }
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

        let from_key = balance_key(&env, &from);
        let to_key = balance_key(&env, &to);
        let to_balance = Self::balance_of(env.clone(), to.clone());

        env.storage().persistent().set(&from_key, &(from_balance - amount));
        env.storage().persistent().set(&to_key, &(to_balance + amount));
    }

    /// Mint new tokens to `to` — admin only
    pub fn mint(env: Env, to: Address, amount: i128) {
        // Enforce admin-only access
        let admin: Address = env.storage().instance().get(&ADMIN_KEY)
            .expect("Contract not initialized");
        admin.require_auth();

        let key = balance_key(&env, &to);
        let current = Self::balance_of(env.clone(), to.clone());
        env.storage().persistent().set(&key, &(current + amount));
    #[test]
    fn test_mint() {
        let mut token = TokenContract::new("Test Token".to_string(), "TEST".to_string(), 1000000);
        let address = Address::generate(&Env::default());
        
        let initial_supply = token.total_supply;
        token.mint(address.clone(), 500000).unwrap();
        
        assert_eq!(token.total_supply, initial_supply + 500000);
        // Minted tokens should appear in the recipient's balance
        assert_eq!(token.balance_of(address), 500000);
    }

    #[test]
    fn test_burn() {
        let mut token = TokenContract::new("Test Token".to_string(), "TEST".to_string(), 1000000);
        let address = Address::generate(&Env::default());

        // Give the address some balance first
        token.mint(address.clone(), 200000).unwrap();
        let supply_after_mint = token.total_supply;

        token.burn(address.clone(), 100000).unwrap();
        
        assert_eq!(token.total_supply, supply_after_mint - 100000);
        assert_eq!(token.balance_of(address), 100000);
    }

    /// Burn tokens from `from` — caller must be the token holder or admin
    pub fn burn(env: Env, from: Address, amount: i128) {
        // Enforce that either the holder or the admin is authorizing the burn
        let admin: Address = env.storage().instance().get(&ADMIN_KEY)
            .expect("Contract not initialized");

        // Require auth from either the holder (self-burn) or the admin
        if from != admin {
            from.require_auth();
        } else {
            admin.require_auth();
        }

        let current = Self::balance_of(env.clone(), from.clone());
        assert!(current >= amount, "Insufficient balance to burn");

        let key = balance_key(&env, &from);
        env.storage().persistent().set(&key, &(current - amount));
    }

    /// Get the current admin address
    pub fn get_admin(env: Env) -> Address {
        env.storage().instance().get(&ADMIN_KEY)
            .expect("Contract not initialized")
    }

    // -------------------------------------------------------------------------
    // Issue #15 – Transfer tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_transfer_success() {
        let env = Env::default();
        let mut token = TokenContract::new("Test Token".to_string(), "TEST".to_string(), 0);
        let sender = Address::generate(&env);
        let receiver = Address::generate(&env);

        // Fund sender
        token.mint(sender.clone(), 1000).unwrap();
        assert_eq!(token.balance_of(sender.clone()), 1000);

        // Transfer 400 to receiver
        token.transfer(sender.clone(), receiver.clone(), 400).unwrap();

        assert_eq!(token.balance_of(sender), 600);
        assert_eq!(token.balance_of(receiver), 400);
    }

    #[test]
    fn test_transfer_insufficient_balance() {
        let env = Env::default();
        let mut token = TokenContract::new("Test Token".to_string(), "TEST".to_string(), 0);
        let sender = Address::generate(&env);
        let receiver = Address::generate(&env);

        token.mint(sender.clone(), 100).unwrap();

        let result = token.transfer(sender, receiver, 500);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Insufficient balance"));
    }

    #[test]
    fn test_transfer_zero_amount() {
        let env = Env::default();
        let mut token = TokenContract::new("Test Token".to_string(), "TEST".to_string(), 0);
        let sender = Address::generate(&env);
        let receiver = Address::generate(&env);

        let result = token.transfer(sender, receiver, 0);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Amount must be greater than 0");
    }

    #[test]
    fn test_transfer_to_same_address() {
        let env = Env::default();
        let mut token = TokenContract::new("Test Token".to_string(), "TEST".to_string(), 0);
        let addr = Address::generate(&env);

        token.mint(addr.clone(), 500).unwrap();

        let result = token.transfer(addr.clone(), addr, 100);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Cannot transfer to the same address");
    }

    // -------------------------------------------------------------------------
    // Issue #16 – Approve tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_approve_stores_allowance() {
        let env = Env::default();
        let mut token = TokenContract::new("Test Token".to_string(), "TEST".to_string(), 0);
        let owner = Address::generate(&env);
        let spender = Address::generate(&env);

        token.approve(owner.clone(), spender.clone(), 300).unwrap();

        assert_eq!(token.allowance(owner, spender), 300);
    }

    #[test]
    fn test_approve_zero_amount() {
        let env = Env::default();
        let mut token = TokenContract::new("Test Token".to_string(), "TEST".to_string(), 0);
        let owner = Address::generate(&env);
        let spender = Address::generate(&env);

        let result = token.approve(owner, spender, 0);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Amount must be greater than 0");
    }

    #[test]
    fn test_approve_overwrite_allowance() {
        let env = Env::default();
        let mut token = TokenContract::new("Test Token".to_string(), "TEST".to_string(), 0);
        let owner = Address::generate(&env);
        let spender = Address::generate(&env);

        token.approve(owner.clone(), spender.clone(), 300).unwrap();
        token.approve(owner.clone(), spender.clone(), 150).unwrap();

        // Second approval should overwrite the first
        assert_eq!(token.allowance(owner, spender), 150);
    }

    #[test]
    fn test_transfer_from_success() {
        let env = Env::default();
        let mut token = TokenContract::new("Test Token".to_string(), "TEST".to_string(), 0);
        let owner = Address::generate(&env);
        let spender = Address::generate(&env);
        let receiver = Address::generate(&env);

        token.mint(owner.clone(), 1000).unwrap();
        token.approve(owner.clone(), spender.clone(), 500).unwrap();

        token.transfer_from(spender.clone(), owner.clone(), receiver.clone(), 200).unwrap();

        assert_eq!(token.balance_of(owner.clone()), 800);
        assert_eq!(token.balance_of(receiver), 200);
        // Allowance should be reduced
        assert_eq!(token.allowance(owner, spender), 300);
    }

    #[test]
    fn test_transfer_from_insufficient_allowance() {
        let env = Env::default();
        let mut token = TokenContract::new("Test Token".to_string(), "TEST".to_string(), 0);
        let owner = Address::generate(&env);
        let spender = Address::generate(&env);
        let receiver = Address::generate(&env);

        token.mint(owner.clone(), 1000).unwrap();
        token.approve(owner.clone(), spender.clone(), 50).unwrap();

        let result = token.transfer_from(spender, owner, receiver, 200);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Insufficient allowance"));
    }
}
