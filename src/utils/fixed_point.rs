use thiserror::Error;

pub const WAD: i128 = 1_000_000_000;
pub const BPS_DENOMINATOR: i128 = 10_000;
pub const YEAR_IN_SECONDS: i128 = 31_536_000;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MathError {
    #[error("arithmetic overflow")]
    Overflow,
    #[error("division by zero")]
    DivisionByZero,
}

pub fn mul_div(a: i128, b: i128, denominator: i128) -> Result<i128, MathError> {
    if denominator == 0 {
        return Err(MathError::DivisionByZero);
    }
    let numerator = a.checked_mul(b).ok_or(MathError::Overflow)?;
    numerator
        .checked_div(denominator)
        .ok_or(MathError::DivisionByZero)
}

pub fn wad_mul(a: i128, b: i128) -> Result<i128, MathError> {
    mul_div(a, b, WAD)
}

pub fn wad_div(a: i128, b: i128) -> Result<i128, MathError> {
    mul_div(a, WAD, b)
}

pub fn bps_mul(amount: i128, bps: u32) -> Result<i128, MathError> {
    mul_div(amount, i128::from(bps), BPS_DENOMINATOR)
}
