//! Fuzz / property-based tests for core lending protocol functions.
//!
//! Uses `proptest` (a pure-Rust, stable-toolchain property testing framework)
//! rather than `cargo-fuzz`/libFuzzer, since the latter requires a nightly
//! toolchain and an external libFuzzer/clang setup that isn't portable across
//! contributor machines or CI. `proptest` generates hundreds of randomized
//! inputs per run — including boundary values like 0 and `i128::MAX` — and
//! shrinks any failing case to a minimal reproduction automatically.
//!
//! Coverage: deposit/withdraw/borrow/repay amounts spanning zero, negative,
//! ordinary, and extreme (`i128::MAX`-adjacent) values, asserting the
//! protocol never panics and — whenever an operation succeeds — the
//! solvency invariant still holds.

mod common;

use common::{assert_reserve_solvent, setup_protocol};
use proptest::prelude::*;
use std::panic::{catch_unwind, AssertUnwindSafe};
use stellar_defi_toolkit::ProtocolError;

/// Runs `f` and asserts it never unwinds (panics), regardless of what it
/// returns. This is the core "does the protocol crash on this input?" check.
fn assert_no_panic<F: FnOnce() -> R, R>(label: &str, f: F) -> R {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(result) => result,
        Err(_) => panic!("operation panicked instead of returning an error: {label}"),
    }
}

// ─── Deterministic edge cases ──────────────────────────────────────────────────

#[test]
fn deposit_rejects_zero_amount() {
    let (mut protocol, _oracle) = setup_protocol();
    let err = protocol.deposit("alice", "USDC", 0, 0).unwrap_err();
    assert_eq!(err, ProtocolError::InvalidAmount);
}

#[test]
fn deposit_rejects_negative_amount() {
    let (mut protocol, _oracle) = setup_protocol();
    let err = protocol.deposit("alice", "USDC", -1, 0).unwrap_err();
    assert_eq!(err, ProtocolError::InvalidAmount);
}

#[test]
fn withdraw_rejects_zero_and_negative_amounts() {
    let (mut protocol, oracle) = setup_protocol();
    protocol.deposit("alice", "USDC", 1_000, 0).unwrap();
    assert_eq!(
        protocol.withdraw("alice", "USDC", 0, &oracle, 0).unwrap_err(),
        ProtocolError::InvalidAmount
    );
    assert_eq!(
        protocol
            .withdraw("alice", "USDC", -500, &oracle, 0)
            .unwrap_err(),
        ProtocolError::InvalidAmount
    );
}

#[test]
fn borrow_rejects_zero_and_negative_amounts() {
    let (mut protocol, oracle) = setup_protocol();
    protocol.deposit("lp", "USDC", 1_000_000, 0).unwrap();
    protocol.deposit("alice", "XLM", 1_000_000, 0).unwrap();
    assert_eq!(
        protocol.borrow("alice", "USDC", 0, &oracle, 0).unwrap_err(),
        ProtocolError::InvalidAmount
    );
    assert_eq!(
        protocol
            .borrow("alice", "USDC", -1, &oracle, 0)
            .unwrap_err(),
        ProtocolError::InvalidAmount
    );
}

#[test]
fn repay_rejects_zero_and_negative_amounts() {
    let (mut protocol, oracle) = setup_protocol();
    protocol.deposit("lp", "USDC", 1_000_000, 0).unwrap();
    protocol.deposit("alice", "XLM", 1_000_000, 0).unwrap();
    protocol
        .borrow("alice", "USDC", 500_000, &oracle, 0)
        .unwrap();
    assert_eq!(
        protocol
            .repay("alice", "alice", "USDC", 0, 0)
            .unwrap_err(),
        ProtocolError::InvalidAmount
    );
    assert_eq!(
        protocol
            .repay("alice", "alice", "USDC", -1, 0)
            .unwrap_err(),
        ProtocolError::InvalidAmount
    );
}

/// Depositing `i128::MAX` into a fresh reserve must not panic. Whatever the
/// outcome (accepted or rejected), the reserve must remain solvent.
#[test]
fn deposit_of_i128_max_does_not_panic() {
    let (mut protocol, _oracle) = setup_protocol();
    let result = assert_no_panic("deposit i128::MAX", || {
        protocol.deposit("whale", "USDC", i128::MAX, 0)
    });
    if result.is_ok() {
        assert_reserve_solvent(&protocol, "USDC");
    }
}

/// Withdrawing far more than was ever deposited (including `i128::MAX`) must
/// be rejected gracefully (insufficient balance/liquidity), never panic.
#[test]
fn withdraw_of_i128_max_does_not_panic() {
    let (mut protocol, oracle) = setup_protocol();
    protocol.deposit("alice", "USDC", 1_000, 0).unwrap();
    let result = assert_no_panic("withdraw i128::MAX", || {
        protocol.withdraw("alice", "USDC", i128::MAX, &oracle, 0)
    });
    assert!(result.is_err(), "withdrawing far more than supplied must fail, not succeed");
    assert_reserve_solvent(&protocol, "USDC");
}

/// Borrowing `i128::MAX` against modest collateral must be rejected
/// gracefully (insufficient collateral/liquidity), never panic or overflow.
#[test]
fn borrow_of_i128_max_does_not_panic() {
    let (mut protocol, oracle) = setup_protocol();
    protocol.deposit("lp", "USDC", 1_000_000, 0).unwrap();
    protocol.deposit("alice", "XLM", 1_000_000, 0).unwrap();
    let result = assert_no_panic("borrow i128::MAX", || {
        protocol.borrow("alice", "USDC", i128::MAX, &oracle, 0)
    });
    assert!(result.is_err(), "borrowing i128::MAX against modest collateral must fail");
    assert_reserve_solvent(&protocol, "USDC");
    assert_reserve_solvent(&protocol, "XLM");
}

/// Repaying `i128::MAX` when only a small debt is outstanding must cap the
/// actual repayment at what's owed, never panic or overpay.
#[test]
fn repay_of_i128_max_caps_at_amount_owed() {
    let (mut protocol, oracle) = setup_protocol();
    protocol.deposit("lp", "USDC", 1_000_000, 0).unwrap();
    protocol.deposit("alice", "XLM", 1_000_000, 0).unwrap();
    protocol
        .borrow("alice", "USDC", 500_000, &oracle, 0)
        .unwrap();

    let repaid = assert_no_panic("repay i128::MAX", || {
        protocol.repay("alice", "alice", "USDC", i128::MAX, 0)
    })
    .unwrap();
    assert_eq!(repaid, 500_000, "repay must cap at the amount actually owed");
    assert_reserve_solvent(&protocol, "USDC");
}

/// A flash loan requesting `i128::MAX` against limited reserve liquidity
/// must be rejected gracefully, never panic.
#[test]
fn flash_loan_of_i128_max_does_not_panic() {
    let (mut protocol, _oracle) = setup_protocol();
    protocol.deposit("lp", "USDC", 1_000_000, 0).unwrap();
    let result = assert_no_panic("flash_loan i128::MAX", || {
        protocol.flash_loan("arb-bot", "USDC", i128::MAX, i128::MAX, 0)
    });
    assert!(result.is_err());
    assert_reserve_solvent(&protocol, "USDC");
}

// ─── Randomized property-based fuzzing ─────────────────────────────────────────

proptest! {
    // Deposit: any amount in a wide range (including 0, negative, and huge
    // values) must never panic; a successful deposit must keep the reserve
    // solvent.
    #[test]
    fn fuzz_deposit_amounts_never_panic(amount in any::<i128>()) {
        let (mut protocol, _oracle) = setup_protocol();
        let result = assert_no_panic("fuzz deposit", || {
            protocol.deposit("alice", "USDC", amount, 0)
        });
        if amount <= 0 {
            prop_assert_eq!(result.unwrap_err(), ProtocolError::InvalidAmount);
        } else if result.is_ok() {
            assert_reserve_solvent(&protocol, "USDC");
        }
    }

    // Withdraw: any amount against a fixed, modest deposit must never panic,
    // and can only succeed up to what was actually deposited.
    #[test]
    fn fuzz_withdraw_amounts_never_panic(amount in any::<i128>()) {
        let (mut protocol, oracle) = setup_protocol();
        protocol.deposit("alice", "USDC", 1_000_000, 0).unwrap();
        let result = assert_no_panic("fuzz withdraw", || {
            protocol.withdraw("alice", "USDC", amount, &oracle, 0)
        });
        if amount <= 0 {
            prop_assert_eq!(result.unwrap_err(), ProtocolError::InvalidAmount);
        } else if amount > 1_000_000 {
            prop_assert!(result.is_err(), "withdrawing more than supplied must fail");
        }
        assert_reserve_solvent(&protocol, "USDC");
    }

    // Borrow: any amount against a fixed collateral position must never
    // panic, regardless of magnitude.
    #[test]
    fn fuzz_borrow_amounts_never_panic(amount in any::<i128>()) {
        let (mut protocol, oracle) = setup_protocol();
        protocol.deposit("lp", "USDC", 1_000_000, 0).unwrap();
        protocol.deposit("alice", "XLM", 1_000_000, 0).unwrap();
        let result = assert_no_panic("fuzz borrow", || {
            protocol.borrow("alice", "USDC", amount, &oracle, 0)
        });
        if amount <= 0 {
            prop_assert_eq!(result.unwrap_err(), ProtocolError::InvalidAmount);
        } else if result.is_ok() {
            assert_reserve_solvent(&protocol, "USDC");
            assert_reserve_solvent(&protocol, "XLM");
        }
    }

    // Repay: any amount must never panic and must never repay more than the
    // amount actually owed.
    #[test]
    fn fuzz_repay_amounts_never_panic(amount in any::<i128>()) {
        let (mut protocol, oracle) = setup_protocol();
        protocol.deposit("lp", "USDC", 1_000_000, 0).unwrap();
        protocol.deposit("alice", "XLM", 1_000_000, 0).unwrap();
        protocol.borrow("alice", "USDC", 500_000, &oracle, 0).unwrap();

        let result = assert_no_panic("fuzz repay", || {
            protocol.repay("alice", "alice", "USDC", amount, 0)
        });
        if amount <= 0 {
            prop_assert_eq!(result.unwrap_err(), ProtocolError::InvalidAmount);
        } else if let Ok(repaid) = result {
            prop_assert!(repaid <= 500_000, "must never repay more than what's owed");
            assert_reserve_solvent(&protocol, "USDC");
        }
    }

    // Two consecutive deposits with independently fuzzed (potentially huge)
    // amounts must never panic even when they'd sum past reasonable bounds.
    #[test]
    fn fuzz_sequential_deposits_never_panic(
        first in 1i128..i128::MAX,
        second in 1i128..i128::MAX,
    ) {
        let (mut protocol, _oracle) = setup_protocol();
        let first_result = assert_no_panic("fuzz sequential deposit 1", || {
            protocol.deposit("whale", "USDC", first, 0)
        });
        let second_result = assert_no_panic("fuzz sequential deposit 2", || {
            protocol.deposit("whale", "USDC", second, 0)
        });
        if first_result.is_ok() && second_result.is_ok() {
            assert_reserve_solvent(&protocol, "USDC");
        }
    }
}
