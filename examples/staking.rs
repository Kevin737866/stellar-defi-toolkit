use stellar_defi_toolkit::{InterestRateModel, LendingProtocol};

fn main() {
    let mut protocol = LendingProtocol::new(
        vec!["admin".to_string()],
        1,
        "treasury",
        InterestRateModel::default(),
    );
    println!(
        "protocol initialized with admins={:?} treasury={}",
        protocol.admins(),
        protocol.treasury()
    );
}
