//! Helper functions and utilities for Stellar DeFi Toolkit

use soroban_sdk::{Address, Env};
use stellar_sdk::PublicKey;
use anyhow::Result;

/// Generate a new Stellar address
pub fn generate_address() -> Address {
    Address::generate(&Env::default())
}

/// Validate a Stellar public key
pub fn validate_public_key(public_key: &str) -> Result<bool> {
    // In a real implementation, this would validate the public key format
    // For now, just check if it's a reasonable length
    Ok(public_key.len() >= 56 && public_key.len() <= 64)
}

/// Convert address to string representation
pub fn address_to_string(address: &Address) -> String {
    // In a real implementation, this would convert the address to a string
    // For now, return a placeholder
    format!("ADDRESS_{}", uuid::Uuid::new_v4().to_string().replace("-", ""))
}

/// Parse string to address
pub fn string_to_address(address_str: &str) -> Result<Address> {
    // In a real implementation, this would parse the string to an address
    // For now, generate a mock address
    Ok(Address::generate(&Env::default()))
}

/// Calculate minimum liquidity amount
pub fn calculate_minimum_liquidity(amount_a: u64, amount_b: u64, slippage_tolerance_bps: u32) -> (u64, u64) {
    let slippage_factor = (10000 - slippage_tolerance_bps) as u64;
    let min_amount_a = amount_a.checked_mul(slippage_factor).unwrap() / 10000;
    let min_amount_b = amount_b.checked_mul(slippage_factor).unwrap() / 10000;
    (min_amount_a, min_amount_b)
}

/// Calculate minimum output amount for swaps
pub fn calculate_minimum_output(amount_out: u64, slippage_tolerance_bps: u32) -> u64 {
    let slippage_factor = (10000 - slippage_tolerance_bps) as u64;
    amount_out.checked_mul(slippage_factor).unwrap() / 10000
}

/// Format balance with proper decimal places
pub fn format_balance(balance: u64, decimals: u8) -> String {
    let divisor = 10_u64.pow(decimals as u32);
    let whole = balance / divisor;
    let fractional = balance % divisor;
    
    if fractional == 0 {
        format!("{}", whole)
    } else {
        format!("{}.{:0width$}", whole, fractional, width = decimals as usize)
    }
}

/// Parse balance from string with decimals
pub fn parse_balance(balance_str: &str, decimals: u8) -> Result<u64> {
    let parts: Vec<&str> = balance_str.split('.').collect();
    let whole = parts[0].parse::<u64>()?;
    
    let fractional = if parts.len() > 1 {
        let fractional_str = parts[1];
        let padded = format!("{:0<width$}", fractional_str, width = decimals as usize);
        padded[..decimals as usize].parse::<u64>()?
    } else {
        0
    };
    
    let divisor = 10_u64.pow(decimals as u32);
    Ok(whole.checked_mul(divisor).unwrap() + fractional)
}

/// Calculate APR from rewards
pub fn calculate_apr(rewards_per_year: u64, total_staked: u64) -> f64 {
    if total_staked == 0 {
        return 0.0;
    }
    (rewards_per_year as f64 / total_staked as f64) * 100.0
}

/// Calculate time lock duration in seconds
pub fn time_lock_duration(days: u32) -> u64 {
    (days as u64) * 24 * 60 * 60
}

/// Validate contract parameters
pub fn validate_contract_params(
    name: &str,
    symbol: &str,
    initial_supply: u64,
) -> Result<()> {
    if name.is_empty() || name.len() > 100 {
        return Err(anyhow::anyhow!("Name must be 1-100 characters"));
    }
    
    if symbol.is_empty() || symbol.len() > 10 {
        return Err(anyhow::anyhow!("Symbol must be 1-10 characters"));
    }
    
    if initial_supply > u64::MAX / 10 {
        return Err(anyhow::anyhow!("Initial supply too large"));
    }
    
    Ok(())
}

/// Generate unique contract ID
pub fn generate_contract_id() -> String {
    format!("CONTRACT_{}", uuid::Uuid::new_v4().to_string().replace("-", ""))
}

/// Check if two tokens are the same
pub fn is_same_token(token_a: &str, token_b: &str) -> bool {
    token_a.to_lowercase() == token_b.to_lowercase()
}

/// Sort token pair for consistent ordering
pub fn sort_token_pair(token_a: &str, token_b: &str) -> (String, String) {
    if token_a.to_lowercase() <= token_b.to_lowercase() {
        (token_a.to_string(), token_b.to_string())
    } else {
        (token_b.to_string(), token_a.to_string())
    }
}

/// Calculate impermanent loss
pub fn calculate_impermanent_loss(
    initial_price_ratio: f64,
    current_price_ratio: f64,
) -> f64 {
    let sqrt_ratio = current_price_ratio.sqrt();
    let price_impact = (2.0 * sqrt_ratio) / (1.0 + current_price_ratio);
    1.0 - price_impact
}

/// Calculate liquidity provider fees earned
pub fn calculate_lp_fees(volume: u64, fee_percentage: u32) -> u64 {
    volume.checked_mul(fee_percentage as u64).unwrap() / 10000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_public_key() {
        assert!(validate_public_key("GABCDEFGHIJKLMNOPQRSTUVWXYZ123456789").unwrap());
        assert!(!validate_public_key("short").unwrap());
        assert!(!validate_public_key("").unwrap());
    }

    #[test]
    fn test_calculate_minimum_liquidity() {
        let (min_a, min_b) = calculate_minimum_liquidity(1000, 2000, 500); // 5% slippage
        assert_eq!(min_a, 950); // 1000 * 9500 / 10000
        assert_eq!(min_b, 1900); // 2000 * 9500 / 10000
    }

    #[test]
    fn test_format_balance() {
        assert_eq!(format_balance(1000000000, 7), "100.0000000");
        assert_eq!(format_balance(123456789, 7), "12.3456789");
        assert_eq!(format_balance(100000000, 7), "10.0000000");
    }

    #[test]
    fn test_parse_balance() {
        assert_eq!(parse_balance("100.0000000", 7).unwrap(), 1000000000);
        assert_eq!(parse_balance("12.3456789", 7).unwrap(), 123456789);
        assert_eq!(parse_balance("10", 7).unwrap(), 100000000);
    }

    #[test]
    fn test_calculate_apr() {
        let apr = calculate_apr(100000, 1000000); // 10% APR
        assert_eq!(apr, 10.0);
        
        let apr_zero = calculate_apr(0, 1000000);
        assert_eq!(apr_zero, 0.0);
        
        let apr_infinite = calculate_apr(100000, 0);
        assert_eq!(apr_infinite, 0.0);
    }

    #[test]
    fn test_time_lock_duration() {
        assert_eq!(time_lock_duration(1), 86400); // 1 day in seconds
        assert_eq!(time_lock_duration(7), 604800); // 7 days in seconds
        assert_eq!(time_lock_duration(30), 2592000); // 30 days in seconds
    }

    #[test]
    fn test_is_same_token() {
        assert!(is_same_token("TOKEN", "token"));
        assert!(is_same_token("Token", "TOKEN"));
        assert!(!is_same_token("TOKENA", "TOKENB"));
    }

    #[test]
    fn test_sort_token_pair() {
        let (a, b) = sort_token_pair("TOKENB", "TOKENA");
        assert_eq!(a, "TOKENA");
        assert_eq!(b, "TOKENB");
        
        let (a, b) = sort_token_pair("TOKENA", "TOKENB");
        assert_eq!(a, "TOKENA");
        assert_eq!(b, "TOKENB");
    }

    #[test]
    fn test_calculate_impermanent_loss() {
        // No price change = no impermanent loss
        let il = calculate_impermanent_loss(1.0, 1.0);
        assert!((il - 0.0).abs() < f64::EPSILON);
        
        // 2x price change
        let il = calculate_impermanent_loss(1.0, 2.0);
        assert!(il > 0.0 && il < 1.0);
    }

    #[test]
    fn test_validate_contract_params() {
        assert!(validate_contract_params("Valid Token", "VT", 1000000).is_ok());
        assert!(validate_contract_params("", "VT", 1000000).is_err());
        assert!(validate_contract_params("Valid Token", "", 1000000).is_err());
        assert!(validate_contract_params("a".repeat(101).as_str(), "VT", 1000000).is_err());
    }
}
