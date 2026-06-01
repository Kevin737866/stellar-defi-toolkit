use clap::Args;

#[derive(Args, Debug)]
pub struct BorrowCommand {
    #[arg(long)]
    pub asset: String,

    #[arg(long)]
    pub amount: i128,

    #[arg(long)]
    pub account: String,
}