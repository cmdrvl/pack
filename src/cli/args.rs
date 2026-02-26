use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "pack",
    about = "Seal lockfiles, reports, rules, and registry artifacts into one immutable, self-verifiable evidence pack.",
    version
)]
pub struct Cli {
    /// Print compiled operator.json and exit.
    #[arg(long, global = true)]
    pub describe: bool,

    /// Print pack.v0 JSON Schema and exit.
    #[arg(long, global = true)]
    pub schema: bool,

    /// Suppress witness ledger recording.
    #[arg(long, global = true)]
    pub no_witness: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Seal artifacts into an evidence pack directory.
    Seal {
        /// Files or directories to include.
        #[arg(required = true)]
        artifacts: Vec<PathBuf>,

        /// Output directory (default: pack/<pack_id>/).
        #[arg(long)]
        output: Option<PathBuf>,

        /// Optional annotation in manifest.
        #[arg(long)]
        note: Option<String>,
    },

    /// Verify pack integrity (members + pack_id).
    Verify {
        /// Path to the pack directory.
        pack_dir: PathBuf,

        /// Output as JSON.
        #[arg(long)]
        json: bool,
    },

    /// Deterministically diff two packs.
    Diff {
        /// First pack directory.
        a: PathBuf,

        /// Second pack directory.
        b: PathBuf,

        /// Output as JSON.
        #[arg(long)]
        json: bool,
    },

    /// Publish a pack to data-fabric (deferred in v0.1).
    Push {
        /// Pack directory to publish.
        pack_dir: PathBuf,
    },

    /// Fetch a pack by ID from data-fabric (deferred in v0.1).
    Pull {
        /// Pack ID to fetch.
        pack_id: String,

        /// Output directory.
        #[arg(long = "out")]
        out_dir: PathBuf,
    },

    /// Query witness ledger.
    Witness {
        #[command(subcommand)]
        command: WitnessCommand,
    },
}

#[derive(Subcommand, Debug)]
pub enum WitnessCommand {
    /// Query witness records with optional filters.
    Query {
        /// Output as JSON.
        #[arg(long)]
        json: bool,
    },

    /// Show the last witness record.
    Last {
        /// Output as JSON.
        #[arg(long)]
        json: bool,
    },

    /// Count witness records.
    Count {
        /// Output as JSON.
        #[arg(long)]
        json: bool,
    },
}
