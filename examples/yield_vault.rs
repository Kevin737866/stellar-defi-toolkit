use stellar_defi_toolkit::{InterestRateModel, LendingProtocol, PriceOracle, ReserveConfig, WAD};

fn main() {
    let mut protocol = LendingProtocol::new("admin", "treasury", InterestRateModel::default());
    protocol
        .register_asset(
            "admin",
            ReserveConfig {
                asset: "XLM".to_string(),
                decimals: 7,
                collateral_factor_bps: 8_000,
                liquidation_threshold_bps: 8_500,
                liquidation_bonus_bps: 1_000,
                reserve_factor_bps: 1_000,
                flash_loan_fee_bps: 9,
                borrow_enabled: true,
                deposit_enabled: true,
                flash_loan_enabled: true,
            },
            0,
        )
        .unwrap();
    protocol
        .register_asset(
            "admin",
            ReserveConfig {
                asset: "USDC".to_string(),
                decimals: 7,
                collateral_factor_bps: 9_000,
                liquidation_threshold_bps: 9_500,
                liquidation_bonus_bps: 1_000,
                reserve_factor_bps: 1_000,
                flash_loan_fee_bps: 9,
                borrow_enabled: true,
                deposit_enabled: true,
                flash_loan_enabled: true,
            },
            0,
        )
        .unwrap();

    let mut oracle = PriceOracle::new("oracle");
    oracle.set_price("oracle", "XLM", WAD).unwrap();
    oracle.set_price("oracle", "USDC", WAD).unwrap();

    protocol.deposit("lp", "USDC", 5_000_000, 0).unwrap();
    protocol.deposit("borrower", "XLM", 1_000_000, 0).unwrap();
    protocol
        .borrow("borrower", "USDC", 500_000, &oracle, 0)
        .unwrap();

    let position = protocol.position("borrower", &oracle).unwrap();
    println!(
        "borrower health factor={} debt_value={}",
        position.health_factor, position.debt_value
    );
}
