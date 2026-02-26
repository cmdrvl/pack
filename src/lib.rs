pub mod cli;
pub mod detect;
pub mod diff;
pub mod operator;
pub mod refusal;
pub mod schema;
pub mod seal;
pub mod verify;
pub mod witness;

use clap::Parser;
use cli::{Cli, Command, ExitCode, WitnessCommand};

/// Run the pack CLI and return an exit code.
pub fn run() -> u8 {
    let cli = Cli::parse();

    // --describe short-circuits before input validation.
    if cli.describe {
        let op = operator::operator_json();
        println!(
            "{}",
            serde_json::to_string_pretty(&op).expect("operator json serialization cannot fail")
        );
        return ExitCode::Success.into();
    }

    // --schema short-circuits before input validation.
    if cli.schema {
        let s = schema::pack_schema();
        println!(
            "{}",
            serde_json::to_string_pretty(&s).expect("schema serialization cannot fail")
        );
        return ExitCode::Success.into();
    }

    let Some(command) = cli.command else {
        eprintln!("pack: no command provided. Try --help.");
        return ExitCode::Refusal.into();
    };

    let no_witness = cli.no_witness;

    match command {
        Command::Seal {
            artifacts,
            output,
            note,
        } => match seal::command::execute_seal(&artifacts, output.as_deref(), note) {
            Ok(result) => {
                if !no_witness {
                    let record = witness::WitnessRecord::new(
                        "seal",
                        "PACK_CREATED",
                        Some(result.pack_id.clone()),
                    );
                    if let Err(e) = witness::append_witness(&record) {
                        eprintln!("pack: witness append warning: {e}");
                    }
                }
                println!("PACK_CREATED {}", result.pack_id);
                println!("{}", result.output_dir.display());
                ExitCode::Success.into()
            }
            Err(envelope) => {
                if !no_witness {
                    let record = witness::WitnessRecord::new("seal", "REFUSAL", None);
                    if let Err(e) = witness::append_witness(&record) {
                        eprintln!("pack: witness append warning: {e}");
                    }
                }
                println!("{}", envelope.to_json());
                ExitCode::Refusal.into()
            }
        },
        Command::Verify { pack_dir, json } => {
            let (output, exit_code) = verify::execute_verify(&pack_dir, json);
            if !no_witness {
                let outcome = match exit_code {
                    0 => "OK",
                    1 => "INVALID",
                    _ => "REFUSAL",
                };
                let record = witness::WitnessRecord::new("verify", outcome, None);
                if let Err(e) = witness::append_witness(&record) {
                    eprintln!("pack: witness append warning: {e}");
                }
            }
            println!("{output}");
            exit_code
        }
        Command::Diff { a, b, json } => {
            let (output, exit_code) = diff::execute_diff(&a, &b, json);
            println!("{output}");
            exit_code
        }
        Command::Push { pack_dir: _ } => {
            eprintln!("pack push: deferred in v0.1");
            ExitCode::Refusal.into()
        }
        Command::Pull {
            pack_id: _,
            out_dir: _,
        } => {
            eprintln!("pack pull: deferred in v0.1");
            ExitCode::Refusal.into()
        }
        // Witness query subcommands do NOT record witness.
        Command::Witness { command } => dispatch_witness(command),
    }
}

fn dispatch_witness(command: WitnessCommand) -> u8 {
    match command {
        WitnessCommand::Query { json } => {
            println!("{}", witness::query::execute_query(json));
            ExitCode::Success.into()
        }
        WitnessCommand::Last { json } => {
            println!("{}", witness::query::execute_last(json));
            ExitCode::Success.into()
        }
        WitnessCommand::Count { json } => {
            println!("{}", witness::query::execute_count(json));
            ExitCode::Success.into()
        }
    }
}
