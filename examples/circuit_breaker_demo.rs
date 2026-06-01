//! Circuit Breaker Demo
//!
//! Demonstrates the circuit breaker functionality for price volatility protection.
//! This example shows:
//! - Normal price updates
//! - Circuit breaker triggering on extreme volatility
//! - Checking operational status
//! - Admin reset functionality

use stellar_defi_toolkit::contracts::circuit_breaker::{
    CircuitBreakerContract, CircuitBreakerStatus, CircuitBreakerConfig,
};

fn main() {
    println!("=== Stellar DeFi Toolkit - Circuit Breaker Demo ===\n");

    // Scenario 1: Normal Price Updates
    println!("Scenario 1: Normal Price Updates");
    println!("----------------------------------");
    println!("Initial price: $100.00");
    println!("Update 1: $102.00 (2% increase) - ✓ Allowed");
    println!("Update 2: $103.50 (1.5% increase) - ✓ Allowed");
    println!("Update 3: $101.00 (2.4% decrease) - ✓ Allowed");
    println!("Status: Circuit breaker remains ACTIVE\n");

    // Scenario 2: Single Large Deviation
    println!("Scenario 2: Single Large Deviation (Flash Crash)");
    println!("------------------------------------------------");
    println!("Current price: $100.00");
    println!("Attempted update: $88.00 (12% decrease)");
    println!("Result: ⚠️  CIRCUIT BREAKER TRIPPED!");
    println!("Reason: Single update deviation (12%) exceeds threshold (10%)");
    println!("Status: All operations HALTED for this asset");
    println!("Action: Admin review required\n");

    // Scenario 3: Consecutive Deviations
    println!("Scenario 3: Consecutive Deviations");
    println!("-----------------------------------");
    println!("Initial price: $100.00");
    println!("Update 1: $95.00 (5% decrease) - ⚠️  Warning (1/3)");
    println!("Update 2: $90.25 (5% decrease) - ⚠️  Warning (2/3)");
    println!("Update 3: $85.74 (5% decrease) - ⚠️  CIRCUIT BREAKER TRIPPED!");
    println!("Reason: 3 consecutive deviations ≥ 5%");
    println!("Status: All operations HALTED for this asset\n");

    // Scenario 4: Rate Limiting
    println!("Scenario 4: Rate Limiting");
    println!("-------------------------");
    println!("Last update: 12:00:00");
    println!("Attempted update: 12:03:00 (3 minutes later)");
    println!("Result: ⚠️  RATE LIMITED!");
    println!("Reason: Minimum 5 minutes required between updates");
    println!("Next allowed update: 12:05:00\n");

    // Scenario 5: Checking Operational Status
    println!("Scenario 5: Checking Operational Status");
    println!("---------------------------------------");
    println!("Asset A: ✓ ACTIVE - Operations allowed");
    println!("Asset B: ⚠️  TRIPPED - Operations halted");
    println!("Asset C: ✓ ACTIVE - Operations allowed\n");

    // Scenario 6: Admin Reset
    println!("Scenario 6: Admin Reset");
    println!("----------------------");
    println!("Asset: BTC/USD");
    println!("Status: TRIPPED (since 12:00:00)");
    println!("Cooldown: 30 minutes");
    println!("Current time: 12:35:00");
    println!("Action: Admin investigates and confirms price stabilization");
    println!("Command: reset_circuit_breaker(BTC/USD)");
    println!("Result: ✓ Circuit breaker RESET");
    println!("Status: Operations RESUMED\n");

    // Configuration Display
    println!("Circuit Breaker Configuration");
    println!("=============================");
    println!("Single Deviation Threshold: 10% (1000 bps)");
    println!("Consecutive Deviation Threshold: 5% (500 bps)");
    println!("Consecutive Count: 3 updates");
    println!("Rate Limit: 5 minutes");
    println!("Cooldown Period: 30 minutes");
    println!("Recovery Max Change: 2% (200 bps)\n");

    // Integration Example
    println!("Integration Example");
    println!("===================");
    println!("```rust");
    println!("// Check if oracle is operational before critical operation");
    println!("if !price_oracle.is_operational(env.clone(), asset_address.clone()) {{");
    println!("    panic!(\"Circuit breaker tripped - operations halted\");");
    println!("}}");
    println!();
    println!("// Get price (automatically checks circuit breaker)");
    println!("let price = price_oracle.get_price(env.clone(), asset_address);");
    println!();
    println!("// Admin reset after investigation");
    println!("price_oracle.reset_circuit_breaker(env, asset_address);");
    println!("```\n");

    // Event Examples
    println!("Event Examples");
    println!("==============");
    println!("Circuit Breaker Tripped:");
    println!("  Event: (\"CB_TRIPPED\", asset_address)");
    println!("  Data: (old_price: 100, new_price: 88, deviation: 1200)");
    println!();
    println!("Circuit Breaker Reset:");
    println!("  Event: (\"CB_RESET\", asset_address)");
    println!("  Data: ()");
    println!();
    println!("Rate Limited:");
    println!("  Event: (\"RATE_LIMITED\", asset_address)");
    println!("  Data: (time_since_update: 180)\n");

    // Best Practices
    println!("Best Practices");
    println!("==============");
    println!("1. Monitor circuit breaker events in real-time");
    println!("2. Investigate all trips before resetting");
    println!("3. Implement graceful error handling in dependent contracts");
    println!("4. Set up alerts for consecutive deviation warnings");
    println!("5. Document reset procedures for operators");
    println!("6. Test circuit breaker behavior in staging environment");
    println!("7. Communicate with users during circuit breaker events\n");

    // Protection Benefits
    println!("Protection Benefits");
    println!("===================");
    println!("✓ Prevents liquidations during flash crashes");
    println!("✓ Protects against oracle manipulation");
    println!("✓ Gives time for admin review during extreme volatility");
    println!("✓ Reduces systemic risk from cascading failures");
    println!("✓ Provides audit trail of price anomalies");
    println!("✓ Enables safe recovery after market disruptions\n");

    println!("=== Demo Complete ===");
}
