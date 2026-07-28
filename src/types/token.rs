//! Token-related type definitions

use serde::{Deserialize, Serialize};
use soroban_sdk::Address;

/// Token information structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenInfo {
    /// Token name
    pub name: String,
    /// Token symbol
    pub symbol: String,
    /// Total supply
    pub total_supply: u64,
    /// Number of decimal places
    pub decimals: u8,
}

/// Token metadata for contract deployment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenMetadata {
    /// Token name
    pub name: String,
    /// Token symbol
    pub symbol: String,
    /// Token decimals
    pub decimals: u8,
    /// Initial supply
    pub initial_supply: u64,
    /// Token admin address
    pub admin: Option<Address>,
    /// Token description
    pub description: Option<String>,
    /// Token logo URL
    pub logo_url: Option<String>,
    /// Token website
    pub website: Option<String>,
}

/// Token balance information
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// Token vesting schedule with cliff and linear release
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VestingSchedule {
    /// Unique schedule identifier
    pub id: u64,
    /// Beneficiary address who receives the tokens
    pub beneficiary: String,
    /// Total amount of tokens to be vested
    pub total_amount: u64,
    /// Amount already claimed
    pub claimed_amount: u64,
    /// Unix timestamp when vesting starts
    pub start_time: u64,
    /// Duration of the cliff period in seconds (no tokens released)
    pub cliff_duration: u64,
    /// Total vesting duration in seconds (after cliff, linear release)
    pub total_duration: u64,
    /// Whether the schedule is active
    pub active: bool,
    /// Whether the schedule has been revoked
    pub revoked: bool,
    /// Admin who created the schedule
    pub created_by: Option<String>,
}

impl VestingSchedule {
    /// Create a new vesting schedule
    pub fn new(
        id: u64,
        beneficiary: String,
        total_amount: u64,
        start_time: u64,
        cliff_duration: u64,
        total_duration: u64,
        created_by: Option<String>,
    ) -> Self {
        assert!(total_duration > 0, "Total duration must be greater than 0");
        assert!(cliff_duration <= total_duration, "Cliff duration cannot exceed total duration");
        
        Self {
            id,
            beneficiary,
            total_amount,
            claimed_amount: 0,
            start_time,
            cliff_duration,
            total_duration,
            active: true,
            revoked: false,
            created_by,
        }
    }

    /// Calculate the claimable amount at a given timestamp
    pub fn claimable_amount(&self, current_time: u64) -> u64 {
        if !self.active || self.revoked {
            return 0;
        }

        let elapsed = current_time.saturating_sub(self.start_time);

        // Cliff period: no tokens released
        if elapsed <= self.cliff_duration {
            return 0;
        }

        // After total duration: all tokens are vested
        if elapsed >= self.total_duration {
            return self.total_amount.saturating_sub(self.claimed_amount);
        }

        // Linear release after cliff
        let vesting_duration = self.total_duration - self.cliff_duration;
        let elapsed_after_cliff = elapsed - self.cliff_duration;
        
        let vested = (self.total_amount as u128 * elapsed_after_cliff as u128 / vesting_duration as u128) as u64;
        
        vested.saturating_sub(self.claimed_amount)
    }

    /// Check if the schedule is fully vested
    pub fn is_fully_vested(&self, current_time: u64) -> bool {
        current_time >= self.start_time + self.total_duration
    }

    /// Mark tokens as claimed
    pub fn claim(&mut self, current_time: u64) -> Result<u64, String> {
        let amount = self.claimable_amount(current_time);
        if amount == 0 {
            return Err("No tokens available to claim".to_string());
        }
        self.claimed_amount = self.claimed_amount.saturating_add(amount);
        Ok(amount)
    }

    /// Revoke the schedule (returns unvested tokens)
    pub fn revoke(&mut self, current_time: u64) -> u64 {
        self.revoked = true;
        self.active = false;
        let vested = self.total_amount.saturating_sub(self.claimed_amount);
        vested
    }
}

impl Default for TokenMetadata {
    fn default() -> Self {
        Self {
            name: String::new(),
            symbol: String::new(),
            decimals: 7, // Stellar standard
            initial_supply: 0,
            admin: None,
            description: None,
            logo_url: None,
            website: None,
        }
    }
}

impl TokenMetadata {
    /// Create new token metadata
    pub fn new(name: String, symbol: String, initial_supply: u64) -> Self {
        Self {
            name,
            symbol,
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
    pub fn with_description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }

    /// Set token logo URL
    pub fn with_logo_url(mut self, logo_url: String) -> Self {
        self.logo_url = Some(logo_url);
        self
    }

    /// Set token website
    pub fn with_website(mut self, website: String) -> Self {
        self.website = Some(website);
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
