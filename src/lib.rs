use clap::{Args, Parser, Subcommand};
use anyhow::Result;

pub mod manifest;

#[derive(Parser)]
#[command(name = "pack")]
#[command(version = "0.1.0")]
#[command(about = "Seal lockfiles, reports, rules, and registry artifacts into immutable, self-verifiable evidence packs")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Print operator.json to stdout and exit 0
    #[arg(long, global = true)]
    pub describe: bool,

    /// Print JSON Schema for pack.v0 and exit 0
    #[arg(long, global = true)]
    pub schema: bool,

    /// Suppress witness ledger recording
    #[arg(long, global = true)]
    pub no_witness: bool,
}

#[derive(Subcommand)]
pub enum Command {
    /// Seal artifacts into an evidence pack directory
    Seal(SealArgs),
    /// Verify pack integrity (members + pack_id)
    Verify(VerifyArgs),
    /// Query witness ledger
    Witness(WitnessArgs),
    /// Deterministically diff two packs (deferred in v0.1)
    #[command(hide = true)]
    Diff(DiffArgs),
    /// Publish a pack to data-fabric (deferred in v0.1)
    #[command(hide = true)]
    Push(PushArgs),
    /// Fetch a pack by ID from data-fabric (deferred in v0.1)
    #[command(hide = true)]
    Pull(PullArgs),
}

#[derive(Args)]
pub struct SealArgs {
    /// Files/directories to include in the pack
    #[arg(required = true)]
    pub artifacts: Vec<String>,

    /// Output directory (default: pack/<pack_id>/)
    #[arg(long)]
    pub output: Option<String>,

    /// Optional annotation in manifest
    #[arg(long)]
    pub note: Option<String>,
}

#[derive(Args)]
pub struct VerifyArgs {
    /// Pack directory to verify
    pub pack_dir: String,

    /// Output verification result as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Args)]
pub struct WitnessArgs {
    #[command(subcommand)]
    pub command: WitnessCommand,
}

#[derive(Subcommand)]
pub enum WitnessCommand {
    /// Query witness ledger with filters
    Query {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show last witness entry
    Last {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Count witness entries with filters
    Count {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Args)]
pub struct DiffArgs {
    /// First pack to compare
    pub a: String,
    /// Second pack to compare
    pub b: String,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Args)]
pub struct PushArgs {
    /// Pack directory to publish
    pub pack_dir: String,
}

#[derive(Args)]
pub struct PullArgs {
    /// Pack ID to fetch
    pub pack_id: String,

    /// Output directory
    #[arg(long, required = true)]
    pub out: String,
}

/// Main entry point returning exit code
pub fn run() -> u8 {
    match run_inner() {
        Ok(exit_code) => exit_code,
        Err(e) => {
            eprintln!("Error: {}", e);
            2 // REFUSAL
        }
    }
}

fn run_inner() -> Result<u8> {
    let cli = Cli::parse();

    // Handle global flags that take precedence
    if cli.describe {
        return handle_describe();
    }

    if cli.schema {
        return handle_schema();
    }

    // Dispatch commands
    match cli.command {
        Some(Command::Seal(args)) => handle_seal(args, cli.no_witness),
        Some(Command::Verify(args)) => handle_verify(args, cli.no_witness),
        Some(Command::Witness(args)) => handle_witness(args),
        Some(Command::Diff(args)) => handle_diff(args, cli.no_witness),
        Some(Command::Push(args)) => handle_push(args, cli.no_witness),
        Some(Command::Pull(args)) => handle_pull(args, cli.no_witness),
        None => {
            eprintln!("Error: No command provided. Use --help for usage information.");
            Ok(2) // REFUSAL
        }
    }
}

fn handle_describe() -> Result<u8> {
    // TODO: Implement operator.json output
    println!("{{\"placeholder\": \"operator manifest not implemented yet\"}}");
    Ok(0)
}

fn handle_schema() -> Result<u8> {
    // TODO: Implement pack.v0 schema output
    println!("{{\"placeholder\": \"pack.v0 schema not implemented yet\"}}");
    Ok(0)
}

fn handle_seal(_args: SealArgs, _no_witness: bool) -> Result<u8> {
    // TODO: Implement seal command
    eprintln!("seal command not implemented yet");
    Ok(2) // REFUSAL
}

fn handle_verify(_args: VerifyArgs, _no_witness: bool) -> Result<u8> {
    // TODO: Implement verify command
    eprintln!("verify command not implemented yet");
    Ok(2) // REFUSAL
}

fn handle_witness(_args: WitnessArgs) -> Result<u8> {
    // TODO: Implement witness commands
    eprintln!("witness commands not implemented yet");
    Ok(2) // REFUSAL
}

fn handle_diff(_args: DiffArgs, _no_witness: bool) -> Result<u8> {
    eprintln!("diff command deferred in v0.1");
    Ok(2) // REFUSAL
}

fn handle_push(_args: PushArgs, _no_witness: bool) -> Result<u8> {
    eprintln!("push command deferred in v0.1");
    Ok(2) // REFUSAL
}

fn handle_pull(_args: PullArgs, _no_witness: bool) -> Result<u8> {
    eprintln!("pull command deferred in v0.1");
    Ok(2) // REFUSAL
}