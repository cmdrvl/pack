pub mod cli;
pub mod detect;
pub mod diff;
pub mod network;
pub mod operator;
pub mod refusal;
pub mod schema;
pub mod seal;
pub mod verify;
pub mod witness;

use clap::Parser;
use cli::{Cli, Command, ExitCode, WitnessCommand};
use serde_json::{Map, Value};
use std::path::Path;

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
        } => match seal::command::execute_seal(&artifacts, output.as_deref(), note.clone()) {
            Ok(result) => {
                let output_text = format!(
                    "PACK_CREATED {}\n{}",
                    result.pack_id,
                    result.output_dir.display()
                );
                if !no_witness {
                    let mut params = Map::new();
                    params.insert(
                        "artifacts".to_string(),
                        Value::Array(artifacts.iter().map(|path| path_value(path)).collect()),
                    );
                    if let Some(output_dir) = output.as_deref() {
                        params.insert("output".to_string(), path_value(output_dir));
                    }
                    if let Some(note) = &note {
                        params.insert("note".to_string(), Value::String(note.clone()));
                    }
                    params.insert(
                        "member_count".to_string(),
                        Value::from(result.member_count as u64),
                    );
                    params.insert("output_dir".to_string(), path_value(&result.output_dir));
                    let record = witness::WitnessRecord::new(
                        "seal",
                        result.witness_inputs.clone(),
                        "PACK_CREATED",
                        0,
                        params,
                        &stdout_bytes(&output_text),
                        Some(result.pack_id.clone()),
                    );
                    append_witness_warning(&record);
                }
                println!("{output_text}");
                ExitCode::Success.into()
            }
            Err(envelope) => {
                let output_text = envelope.to_json();
                if !no_witness {
                    let mut params = Map::new();
                    params.insert(
                        "artifacts".to_string(),
                        Value::Array(artifacts.iter().map(|path| path_value(path)).collect()),
                    );
                    if let Some(output_dir) = output.as_deref() {
                        params.insert("output".to_string(), path_value(output_dir));
                    }
                    if let Some(note) = &note {
                        params.insert("note".to_string(), Value::String(note.clone()));
                    }
                    let inputs = artifacts.iter().map(|path| input_from_path(path)).collect();
                    let record = witness::WitnessRecord::new(
                        "seal",
                        inputs,
                        "REFUSAL",
                        2,
                        params,
                        &stdout_bytes(&output_text),
                        None,
                    );
                    append_witness_warning(&record);
                }
                println!("{output_text}");
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
                let mut params = Map::new();
                params.insert("pack_dir".to_string(), path_value(&pack_dir));
                params.insert("json".to_string(), Value::Bool(json));
                let record = witness::WitnessRecord::new(
                    "verify",
                    vec![input_from_path(&pack_dir)],
                    outcome,
                    exit_code,
                    params,
                    &stdout_bytes(&output),
                    extract_pack_id(&output, json),
                );
                append_witness_warning(&record);
            }
            println!("{output}");
            exit_code
        }
        Command::Diff { a, b, json } => {
            let (output, exit_code) = diff::execute_diff(&a, &b, json);
            if !no_witness {
                let outcome = match exit_code {
                    0 => "NO_CHANGES",
                    1 => "CHANGES",
                    _ => "REFUSAL",
                };
                let mut params = Map::new();
                params.insert("a".to_string(), path_value(&a));
                params.insert("b".to_string(), path_value(&b));
                params.insert("json".to_string(), Value::Bool(json));
                let record = witness::WitnessRecord::new(
                    "diff",
                    vec![input_from_path(&a), input_from_path(&b)],
                    outcome,
                    exit_code,
                    params,
                    &stdout_bytes(&output),
                    None,
                );
                append_witness_warning(&record);
            }
            println!("{output}");
            exit_code
        }
        Command::Push { pack_dir: _ } => {
            println!(
                "{}",
                network::transport::deferred_network_refusal("push").to_json()
            );
            ExitCode::Refusal.into()
        }
        Command::Pull {
            pack_id: _,
            out_dir: _,
        } => {
            println!(
                "{}",
                network::transport::deferred_network_refusal("pull").to_json()
            );
            ExitCode::Refusal.into()
        }
        // Witness query subcommands do NOT record witness.
        Command::Witness { command } => dispatch_witness(command),
    }
}

fn dispatch_witness(command: WitnessCommand) -> u8 {
    match command {
        WitnessCommand::Query { filters, json } => {
            println!("{}", witness::query::execute_query(&filters, json));
            ExitCode::Success.into()
        }
        WitnessCommand::Last { json } => {
            println!("{}", witness::query::execute_last(json));
            ExitCode::Success.into()
        }
        WitnessCommand::Count { filters, json } => {
            println!("{}", witness::query::execute_count(&filters, json));
            ExitCode::Success.into()
        }
    }
}

fn append_witness_warning(record: &witness::WitnessRecord) {
    if let Err(e) = witness::append_witness(record) {
        eprintln!("pack: witness append warning: {e}");
    }
}

fn input_from_path(path: &Path) -> witness::WitnessInput {
    witness::WitnessRecord::input(path.display().to_string(), None, None)
}

fn path_value(path: &Path) -> Value {
    Value::String(path.display().to_string())
}

fn stdout_bytes(output: &str) -> Vec<u8> {
    let mut bytes = output.as_bytes().to_vec();
    bytes.push(b'\n');
    bytes
}

fn extract_pack_id(output: &str, json_output: bool) -> Option<String> {
    if json_output {
        let value: Value = serde_json::from_str(output).ok()?;
        return value
            .get("pack_id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
    }

    output
        .lines()
        .find_map(|line| line.trim().strip_prefix("pack_id: ").map(ToOwned::to_owned))
}
