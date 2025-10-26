use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(version, about="Inkan offline utility")]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Sign a batch JSON of contract calls
    Batch {
        #[arg(long)]
        batch: PathBuf,
        #[arg(long, default_value = "batch_output.json")]
        out: PathBuf,
        #[arg(long, default_value = "30000000")]
        gas_limit: String,
        #[arg(long, default_value = "30000000000")]
        max_fee_per_gas: String,
        #[arg(long, default_value = "2000000000")]
        max_priority_fee_per_gas: String,
    },

    /// Generate Ethereum/Nostr keys
    Keygen {
        #[arg(long, default_value = "1")]
        count: u32,
        #[arg(long)]
        out: Option<PathBuf>,
    },

    /// Launch an interactive terminal menu
    Menu,
}
