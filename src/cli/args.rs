use clap::{Args, Parser, Subcommand};
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

        /// Reproducible manifest creation timestamp (RFC3339, normalized to UTC).
        #[arg(long, value_name = "RFC3339")]
        created: Option<String>,

        /// Optional canonical outcome anchor (must start with cmdrvl://).
        #[arg(long, value_name = "TAG")]
        outcome: Option<String>,
    },

    /// Verify pack integrity (members + pack_id).
    Verify {
        /// Path to the pack directory.
        pack_dir: PathBuf,

        /// Output as JSON.
        #[arg(long)]
        json: bool,
    },

    /// Inspect pack metadata without verifying integrity.
    Inspect {
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

    /// Export or import deterministic archive wrappers.
    Archive {
        #[command(subcommand)]
        command: ArchiveCommand,
    },

    /// Query witness ledger.
    Witness {
        #[command(subcommand)]
        command: WitnessCommand,
    },

    /// Run read-only diagnostics for agents and operators.
    Doctor {
        /// Emit machine-readable triage JSON for agents.
        #[arg(long = "robot-triage")]
        robot_triage: bool,

        /// Output health as JSON when no doctor subcommand is provided.
        #[arg(long)]
        json: bool,

        #[command(subcommand)]
        action: Option<DoctorAction>,
    },
}

#[derive(Subcommand, Debug)]
pub enum DoctorAction {
    /// Run read-only health checks.
    Health {
        /// Output JSON.
        #[arg(long)]
        json: bool,
    },

    /// Describe supported doctor capabilities.
    Capabilities {
        /// Output JSON.
        #[arg(long)]
        json: bool,
    },

    /// Print agent-oriented doctor documentation.
    RobotDocs,
}

#[derive(Subcommand, Debug)]
pub enum ArchiveCommand {
    /// Export a pack directory to a deterministic tar archive.
    Export {
        /// Pack directory to archive.
        pack_dir: PathBuf,

        /// Output archive file.
        #[arg(long)]
        out: PathBuf,
    },

    /// Import a deterministic tar archive into a pack directory.
    Import {
        /// Archive file to import.
        archive: PathBuf,

        /// Output pack directory.
        #[arg(long)]
        out: PathBuf,
    },
}

#[derive(Subcommand, Debug)]
pub enum WitnessCommand {
    /// Query witness records with optional filters.
    Query {
        #[command(flatten)]
        filters: WitnessFilters,

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
        #[command(flatten)]
        filters: WitnessFilters,

        /// Output as JSON.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Args, Debug, Clone, Default)]
pub struct WitnessFilters {
    /// Restrict matches to a specific tool. Defaults to pack rows.
    #[arg(long)]
    pub tool: Option<String>,

    /// Only include records at or after this RFC3339 timestamp.
    #[arg(long)]
    pub since: Option<String>,

    /// Only include records at or before this RFC3339 timestamp.
    #[arg(long)]
    pub until: Option<String>,

    /// Only include records with this outcome.
    #[arg(long)]
    pub outcome: Option<String>,

    /// Only include records whose inputs include this hash.
    #[arg(long = "input-hash")]
    pub input_hash: Option<String>,
}
