use stellar_defi_toolkit::{InterestRateModel, LendingProtocol};

fn main() {
    let protocol = LendingProtocol::new("admin", "treasury", InterestRateModel::default());
    println!(
        "protocol initialized with admin={} treasury={}",
        protocol.admin(),
        protocol.treasury()
    );
}
