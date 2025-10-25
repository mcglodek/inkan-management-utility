use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Inkan offline utility: batch signer + key generator
#[derive(Parser, Debug)]
#[command(version, about="Inkan offline utility")]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Sign a batch JSON of contract calls (your existing behavior)
    Batch {
        /// Path to the batch input JSON (array of items)
        #[arg(long)]
        batch: PathBuf,

        /// Path to the combined output file (default: ./batch_output.json)
        #[arg(long, default_value = "batch_output.json")]
        out: PathBuf,

        /// Default gas limit
        #[arg(long, default_value = "30000000")]
        gas_limit: String,

        /// Default max fee per gas (wei)
        #[arg(long, default_value = "30000000000")]
        max_fee_per_gas: String,

        /// Default max priority fee per gas (wei)
        #[arg(long, default_value = "2000000000")]
        max_priority_fee_per_gas: String,
    },

    /// Generate Ethereum/Nostr keys (npub/nsec, hex forms, address)
    Keygen {
        /// Number of keypairs to generate
        #[arg(long, default_value = "1")]
        count: u32,

        /// Optional path to write JSON output (pretty-printed)
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

