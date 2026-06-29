pub async fn borrow(
    &self,
    asset: String,
    amount: i128,
) -> Result<String>

Commands::Borrow(cmd) => {
    let tx_hash = client
        .borrow(
            cmd.asset,
            cmd.amount,
        )
        .await?;

    println!(
        "Borrow successful: {}",
        tx_hash
    );
}