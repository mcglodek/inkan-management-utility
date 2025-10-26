mod app;
mod ui;
mod screens;

mod abi;
mod commands;
mod process;
mod defaults;

// 👉 add these if you have these files (src/types.rs, src/util.rs, etc.)
mod types;
mod util;
mod signing;
mod key;
mod encoding;
mod decoder;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    app::run_menu().await
}

