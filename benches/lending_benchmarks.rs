//! Gas/compute-cost benchmarks for the core lending operations.
//!
//! `LendingProtocol` runs as a plain-Rust simulation (it has no `#[contract]`
//! Soroban entry points of its own — see `src/contracts/lending.rs`), so
//! there is no Soroban host budget to read a real ledger "gas" number from
//! here. This harness measures wall-clock time per call instead, as the
//! closest available proxy for computational/gas cost — more work per call
//! shows up as both a longer measured time here and a bigger resource-fee
//! bill once this logic runs behind a real `#[contract]` wrapper.
//!
//! This is a small hand-rolled harness (`std::time::Instant`, no external
//! benchmark crate) rather than `criterion`: this repository's dependency
//! graph is fragile enough (soroban-sdk's macro crates, an unpinned
//! `Cargo.lock`) that adding a benchmark framework's transitive dependencies
//! was observed to perturb version resolution elsewhere in the tree. A
//! dependency-free harness avoids that risk entirely.
//!
//! Run with `cargo bench --bench lending_benchmarks`. See
//! `docs/gas_benchmarks.md` for the baseline-comparison workflow and
//! optimization notes.

use std::time::{Duration, Instant};

use stellar_defi_toolkit::{InterestRateModel, LendingProtocol, MockOracle, ReserveConfig, WAD};

const ITERATIONS: u32 = 2_000;

fn reserve(asset: &str, collateral_factor_bps: u32) -> ReserveConfig {
    ReserveConfig {
        asset: asset.to_string(),
        decimals: 7,
        collateral_factor_bps,
        liquidation_threshold_bps: collateral_factor_bps + 500,
        liquidation_bonus_bps: 1_000,
        reserve_factor_bps: 1_000,
        flash_loan_fee_bps: 9,
        borrow_enabled: true,
        deposit_enabled: true,
        flash_loan_enabled: true,
        supply_cap: 0,
        borrow_cap: 0,
        interest_rate_model: None,
    }
}

/// A protocol with XLM/USDC registered and an oracle primed at $1.00 for
/// both — the common starting point for every benchmark below.
fn setup() -> (LendingProtocol, MockOracle) {
    let mut protocol = LendingProtocol::new(
        vec!["admin".to_string()],
        1,
        "treasury",
        InterestRateModel::default(),
    );
    protocol
        .register_asset("admin", reserve("XLM", 8_000), 0)
        .unwrap();
    protocol
        .register_asset("admin", reserve("USDC", 9_000), 0)
        .unwrap();

    let mut oracle = MockOracle::new("oracle");
    oracle.set_price("oracle", "XLM", WAD).unwrap();
    oracle.set_price("oracle", "USDC", WAD).unwrap();
    (protocol, oracle)
}

struct BenchResult {
    name: &'static str,
    total: Duration,
}

impl BenchResult {
    fn per_call_nanos(&self) -> u128 {
        self.total.as_nanos() / ITERATIONS as u128
    }
}

fn bench_deposit() -> BenchResult {
    let start = Instant::now();
    for _ in 0..ITERATIONS {
        let (mut protocol, _oracle) = setup();
        protocol.deposit("alice", "XLM", 1_000_000, 0).unwrap();
    }
    BenchResult {
        name: "deposit",
        total: start.elapsed(),
    }
}

fn bench_withdraw() -> BenchResult {
    let start = Instant::now();
    for _ in 0..ITERATIONS {
        let (mut protocol, oracle) = setup();
        protocol.deposit("alice", "XLM", 10_000_000, 0).unwrap();
        protocol
            .withdraw("alice", "XLM", 1_000_000, &oracle, 0)
            .unwrap();
    }
    BenchResult {
        name: "withdraw",
        total: start.elapsed(),
    }
}

fn bench_borrow() -> BenchResult {
    let start = Instant::now();
    for _ in 0..ITERATIONS {
        let (mut protocol, oracle) = setup();
        protocol.deposit("lp", "USDC", 100_000_000, 0).unwrap();
        protocol.deposit("alice", "XLM", 10_000_000, 0).unwrap();
        protocol
            .borrow("alice", "USDC", 1_000_000, &oracle, 0)
            .unwrap();
    }
    BenchResult {
        name: "borrow",
        total: start.elapsed(),
    }
}

fn bench_repay() -> BenchResult {
    let start = Instant::now();
    for _ in 0..ITERATIONS {
        let (mut protocol, oracle) = setup();
        protocol.deposit("lp", "USDC", 100_000_000, 0).unwrap();
        protocol.deposit("alice", "XLM", 10_000_000, 0).unwrap();
        protocol
            .borrow("alice", "USDC", 5_000_000, &oracle, 0)
            .unwrap();
        protocol
            .repay("alice", "alice", "USDC", 1_000_000, 1)
            .unwrap();
    }
    BenchResult {
        name: "repay",
        total: start.elapsed(),
    }
}

fn bench_liquidate() -> BenchResult {
    let start = Instant::now();
    for _ in 0..ITERATIONS {
        let (mut protocol, oracle) = setup();
        protocol.deposit("lp", "USDC", 100_000_000, 0).unwrap();
        protocol.deposit("alice", "XLM", 10_000_000, 0).unwrap();
        protocol
            .borrow("alice", "USDC", 7_900_000, &oracle, 0)
            .unwrap();

        // Crash XLM 50% so Alice becomes liquidatable.
        let mut oracle = oracle;
        oracle.set_price("oracle", "XLM", WAD / 2).unwrap();

        protocol
            .liquidate("liquidator", "alice", "USDC", "XLM", 1_000_000, &oracle, 1)
            .unwrap();
    }
    BenchResult {
        name: "liquidate",
        total: start.elapsed(),
    }
}

/// Deposit cost scaling as the number of already-registered reserves grows,
/// to spot O(n)-in-reserve-count regressions in `deposit`.
fn bench_deposit_scaling() {
    println!("\ndeposit cost scaling by registered-reserve count:");
    for reserve_count in [1u32, 5, 10, 20] {
        let start = Instant::now();
        let batches = ITERATIONS / 10;
        for _ in 0..batches {
            let mut protocol = LendingProtocol::new(
                vec!["admin".to_string()],
                1,
                "treasury",
                InterestRateModel::default(),
            );
            for i in 0..reserve_count {
                let asset = format!("ASSET{i}");
                protocol
                    .register_asset("admin", reserve(&asset, 8_000), 0)
                    .unwrap();
            }
            protocol.deposit("alice", "ASSET0", 1_000_000, 0).unwrap();
        }
        let elapsed = start.elapsed();
        println!(
            "  {reserve_count:>3} reserves: {:>8} ns/call",
            elapsed.as_nanos() / batches as u128
        );
    }
}

fn main() {
    println!("=== Lending Protocol Gas/Compute-Cost Benchmarks ===");
    println!("({ITERATIONS} iterations per operation; see docs/gas_benchmarks.md)\n");

    let results = [
        bench_deposit(),
        bench_withdraw(),
        bench_borrow(),
        bench_repay(),
        bench_liquidate(),
    ];

    println!("{:<12} {:>14} {:>14}", "operation", "total", "per call");
    println!("{}", "-".repeat(42));
    for r in &results {
        println!(
            "{:<12} {:>14?} {:>11} ns",
            r.name,
            r.total,
            r.per_call_nanos()
        );
    }

    bench_deposit_scaling();
}
