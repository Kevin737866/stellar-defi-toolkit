//! Token-related type definitions

// use serde::{Deserialize, Serialize};
use soroban_sdk::{Address, Env};

/// Token information structure
#[soroban_sdk::contracttype]
#[derive(Debug, Clone)]
pub struct TokenInfo {
    /// Token name
    pub name: soroban_sdk::String,
    /// Token symbol
    pub symbol: soroban_sdk::String,
    /// Total supply
    pub total_supply: u64,
    /// Number of decimal places
    pub decimals: u32,
}

/// Token metadata for contract deployment
#[soroban_sdk::contracttype]
#[derive(Debug, Clone)]
pub struct TokenMetadata {
    /// Token name
    pub name: soroban_sdk::String,
    /// Token symbol
    pub symbol: soroban_sdk::String,
    /// Token decimals
    pub decimals: u32,
    /// Initial supply
    pub initial_supply: u64,
    /// Token admin address
    pub admin: Option<Address>,
    /// Token description
    pub description: Option<soroban_sdk::String>,
    /// Token logo URL
    pub logo_url: Option<soroban_sdk::String>,
    /// Token website
    pub website: Option<soroban_sdk::String>,
}

/// Token balance information
#[derive(Debug, Clone)]
pub struct TokenBalance {
    /// Token contract address
    pub contract_id: String,
    /// Account address
    pub account: Address,
    /// Balance amount
    pub balance: u64,
    /// Last updated timestamp
    pub last_updated: u64,
}

/// Token transfer event
#[derive(Debug, Clone)]
pub struct TokenTransfer {
    /// From address
    pub from: Address,
    /// To address
    pub to: Address,
    /// Amount transferred
    pub amount: u64,
    /// Transaction hash
    pub tx_hash: String,
    /// Block number
    pub block_number: u64,
    /// Timestamp
    pub timestamp: u64,
}

/// Token approval event
#[derive(Debug, Clone)]
pub struct TokenApproval {
    /// Owner address
    pub owner: Address,
    /// Spender address
    pub spender: Address,
    /// Approved amount
    pub amount: u64,
    /// Transaction hash
    pub tx_hash: String,
    /// Block number
    pub block_number: u64,
    /// Timestamp
    pub timestamp: u64,
}

/// Token mint event
#[derive(Debug, Clone)]
pub struct TokenMint {
    /// Recipient address
    pub to: Address,
    /// Amount minted
    pub amount: u64,
    /// Transaction hash
    pub tx_hash: String,
    /// Block number
    pub block_number: u64,
    /// Timestamp
    pub timestamp: u64,
}

/// Token burn event
#[derive(Debug, Clone)]
pub struct TokenBurn {
    /// Burner address
    pub from: Address,
    /// Amount burned
    pub amount: u64,
    /// Transaction hash
    pub tx_hash: String,
    /// Block number
    pub block_number: u64,
    /// Timestamp
    pub timestamp: u64,
}

// Default removed as soroban_sdk::String requires Env

impl TokenMetadata {
    /// Create new token metadata
    pub fn new(env: &Env, name: String, symbol: String, initial_supply: u64) -> Self {
        Self {
            name: soroban_sdk::String::from_str(env, &name),
            symbol: soroban_sdk::String::from_str(env, &symbol),
            decimals: 7,
            initial_supply,
            admin: None,
            description: None,
            logo_url: None,
            website: None,
        }
    }

    /// Set token admin
    pub fn with_admin(mut self, admin: Address) -> Self {
        self.admin = Some(admin);
        self
    }

    /// Set token description
    pub fn with_description(mut self, env: &Env, description: String) -> Self {
        self.description = Some(soroban_sdk::String::from_str(env, &description));
        self
    }

    /// Set token logo URL
    pub fn with_logo_url(mut self, env: &Env, logo_url: String) -> Self {
        self.logo_url = Some(soroban_sdk::String::from_str(env, &logo_url));
        self
    }

    /// Set token website
    pub fn with_website(mut self, env: &Env, website: String) -> Self {
        self.website = Some(soroban_sdk::String::from_str(env, &website));
        self
    }

    /// Validate token metadata
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() || self.name.len() > 100 {
            return Err("Name must be 1-100 characters".to_string());
        }

        if self.symbol.is_empty() || self.symbol.len() > 10 {
            return Err("Symbol must be 1-10 characters".to_string());
        }

        if self.decimals > 18 {
            return Err("Decimals must be <= 18".to_string());
        }

        if self.initial_supply > u64::MAX / 10 {
            return Err("Initial supply too large".to_string());
        }

        Ok(())
    }
}
