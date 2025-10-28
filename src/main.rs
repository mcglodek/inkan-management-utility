mod app;
mod ui;
mod screens;

mod abi;
mod commands;
mod process;
mod defaults;

mod types;
mod util;
mod signing;
mod key;
mod encoding;
mod decoder;


mod crypto;

mod write_signed_transactions_to_file;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    app::run_menu().await
}

