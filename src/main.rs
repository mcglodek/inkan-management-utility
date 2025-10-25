use anyhow::{Context, Result};
use clap::Parser;
use std::fs;

mod abi;
mod cli;
mod decoder;
mod encoding;
mod key;
mod process;
mod signing;
mod types;
mod util;
mod commands;

use crate::abi::load_abi;
use crate::cli::{Cli, Command};
use crate::process::{process_item, BatchOpts};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.cmd {
        Command::Batch { batch, out, gas_limit, max_fee_per_gas, max_priority_fee_per_gas } => {
            let abi = load_abi()?;
            let text = fs::read_to_string(&batch).context("reading batch JSON")?;
            let items: Vec<types::Item> =
                serde_json::from_str(&text).context("parsing batch JSON (array)")?;

            let opts = BatchOpts {
                gas_limit,
                max_fee_per_gas,
                max_priority_fee_per_gas,
            };

            let mut out_vec: Vec<types::BatchEntryOut> = Vec::with_capacity(items.len());
            for (i, it) in items.iter().enumerate() {
                let res = process_item(&abi, &opts, it)
                    .await
                    .with_context(|| format!("processing item #{} ({})", i, it.function_to_call));
                match res {
                    Ok(entry) => out_vec.push(entry),
                    Err(e) => return Err(e),
                }
            }

            fs::write(&out, serde_json::to_string_pretty(&out_vec)?)
                .with_context(|| format!("writing {}", out.display()))?;
            println!("✓ Wrote {}", out.display());
            Ok(())
        }

        Command::Keygen { count, out } => {
            let records = commands::keygen::generate(count)?;
            commands::keygen::emit(records, out)?;
            Ok(())
        }
    }
}

