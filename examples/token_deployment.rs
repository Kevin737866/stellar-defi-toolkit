use stellar_defi_toolkit::{InterestRateModel, LendingProtocol, ReserveConfig};

fn main() {
    let mut protocol = LendingProtocol::new("admin", "treasury", InterestRateModel::default());
    protocol
        .register_asset(
            "admin",
            ReserveConfig {
                asset: "TOKEN".to_string(),
                decimals: 7,
                collateral_factor_bps: 7_500,
                liquidation_threshold_bps: 8_000,
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

    println!("registered asset TOKEN for the lending market");
}
