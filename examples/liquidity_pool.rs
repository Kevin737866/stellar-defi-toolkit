use stellar_defi_toolkit::{InterestRateModel, LendingProtocol, PriceOracle, ReserveConfig, WAD};

fn main() {
    let mut protocol = LendingProtocol::new("admin", "treasury", InterestRateModel::default());
    let reserve = ReserveConfig {
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
    };
    protocol.register_asset("admin", reserve, 0).unwrap();

    let mut oracle = PriceOracle::new("oracle");
    oracle.set_price("oracle", "USDC", WAD).unwrap();

    protocol
        .deposit("liquidity-provider", "USDC", 1_000_000, 0)
        .unwrap();
    let reserve = protocol.reserve_state("USDC").unwrap();

    println!(
        "USDC reserve ready: cash={}, supply_shares={}",
        reserve.total_cash, reserve.total_supply_shares
    );
}
