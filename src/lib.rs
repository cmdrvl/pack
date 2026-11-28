#![forbid(unsafe_code)]

pub mod detect;
pub mod refusal;
pub mod render;
pub mod schema;
pub mod seal;
pub(crate) mod tar;
pub mod verify;

#[cfg(feature = "cli")]
pub mod archive;
#[cfg(feature = "cli")]
pub mod cli;
#[cfg(feature = "cli")]
pub mod diff;
#[cfg(feature = "cli")]
pub mod doctor;
#[cfg(feature = "cli")]
pub mod inspect;
#[cfg(feature = "cli")]
pub mod operator;
#[cfg(feature = "cli")]
mod parallel;
#[cfg(feature = "cli")]
mod paths;
#[cfg(feature = "cli")]
mod staging;
#[cfg(feature = "cli")]
pub mod witness;

#[cfg(feature = "cli")]
use clap::Parser;
#[cfg(feature = "cli")]
use cli::{ArchiveCommand, Cli, Command, ExitCode, WitnessCommand};
#[cfg(feature = "cli")]
use serde_json::{Map, Value};
#[cfg(feature = "cli")]
use std::path::Path;

/// Run the pack CLI and return an exit code.
#[cfg(feature = "cli")]
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

    if cli.robot_triage {
        return doctor::dispatch_robot_triage();
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
            created,
            outcome,
        } => match seal::command::execute_seal(
            &artifacts,
            output.as_deref(),
            note.clone(),
            created.as_deref(),
            outcome.clone(),
        ) {
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
                    if let Some(outcome) = &outcome {
                        params.insert("outcome".to_string(), Value::String(outcome.clone()));
                    }
                    params.insert("created".to_string(), Value::String(result.created.clone()));
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
                    if let Some(created) = &created {
                        params.insert("created".to_string(), Value::String(created.clone()));
                    }
                    if let Some(outcome) = &outcome {
                        params.insert("outcome".to_string(), Value::String(outcome.clone()));
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
        Command::Verify {
            pack_dir,
            expect_id,
            json,
        } => {
            let (output, exit_code) =
                verify::execute_verify_expecting(&pack_dir, expect_id.as_deref(), json);
            if !no_witness {
                let outcome = match exit_code {
                    0 => "OK",
                    1 => "INVALID",
                    _ => "REFUSAL",
                };
                let mut params = Map::new();
                params.insert("pack_dir".to_string(), path_value(&pack_dir));
                params.insert("json".to_string(), Value::Bool(json));
                if let Some(expect_id) = expect_id {
                    params.insert("expect_id".to_string(), Value::String(expect_id));
                }
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
        Command::Render {
            pack_dir,
            format,
            out,
        } => match render::execute_render(&pack_dir, format, &out) {
            Ok(result) => {
                println!("RENDERED {}", result.pack_id);
                ExitCode::Success.into()
            }
            Err(envelope) => {
                println!("{}", envelope.to_json());
                ExitCode::Refusal.into()
            }
        },
        // Inspect is intentionally read-only metadata and does not append witness records.
        Command::Inspect { pack_dir, json } => {
            let (output, exit_code) = inspect::execute_inspect(&pack_dir, json);
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
        // Archive commands are deterministic transport wrappers and do not append witness records.
        Command::Archive { command } => dispatch_archive(command),
        // Witness query subcommands do NOT record witness.
        Command::Witness { command } => dispatch_witness(command),
        // Top-level agent discovery surfaces are read-only and do NOT record witness.
        Command::Capabilities(args) => doctor::dispatch_capabilities(args.json),
        Command::RobotDocs { action } => doctor::dispatch_robot_docs(action.as_ref()),
        // Doctor commands are read-only diagnostics and do NOT record witness.
        Command::Doctor {
            robot_triage,
            fix,
            json,
            action,
        } => doctor::dispatch(robot_triage, fix, json, action.as_ref()),
    }
}

#[cfg(feature = "cli")]
fn dispatch_archive(command: ArchiveCommand) -> u8 {
    match command {
        ArchiveCommand::Export { pack_dir, out } => {
            match archive::execute_archive_export(&pack_dir, &out) {
                Ok(result) => {
                    println!(
                        "ARCHIVE_CREATED {}\n{}",
                        result.pack_id,
                        result.path.display()
                    );
                    ExitCode::Success.into()
                }
                Err(envelope) => {
                    println!("{}", envelope.to_json());
                    ExitCode::Refusal.into()
                }
            }
        }
        ArchiveCommand::Import { archive, out } => {
            match archive::execute_archive_import(&archive, &out) {
                Ok(result) => {
                    println!(
                        "ARCHIVE_IMPORTED {}\n{}",
                        result.pack_id,
                        result.path.display()
                    );
                    ExitCode::Success.into()
                }
                Err(envelope) => {
                    println!("{}", envelope.to_json());
                    ExitCode::Refusal.into()
                }
            }
        }
    }
}

#[cfg(feature = "cli")]
fn dispatch_witness(command: WitnessCommand) -> u8 {
    match command {
        WitnessCommand::Query { filters, json } => {
            dispatch_witness_read(witness::query::execute_query(&filters, json))
        }
        WitnessCommand::Last { json } => dispatch_witness_read(witness::query::execute_last(json)),
        WitnessCommand::Count { filters, json } => {
            dispatch_witness_read(witness::query::execute_count(&filters, json))
        }
    }
}

#[cfg(feature = "cli")]
fn dispatch_witness_read(result: Result<String, String>) -> u8 {
    match result {
        Ok(output) => {
            println!("{output}");
            ExitCode::Success.into()
        }
        Err(error) => {
            eprintln!("pack: witness: {error}");
            ExitCode::Refusal.into()
        }
    }
}

#[cfg(feature = "cli")]
fn append_witness_warning(record: &witness::WitnessRecord) {
    if let Err(e) = witness::append_witness(record) {
        eprintln!("pack: witness append warning: {e}");
    }
}

#[cfg(feature = "cli")]
fn input_from_path(path: &Path) -> witness::WitnessInput {
    witness::WitnessRecord::input(path.display().to_string(), None, None)
}

#[cfg(feature = "cli")]
fn path_value(path: &Path) -> Value {
    Value::String(path.display().to_string())
}

#[cfg(feature = "cli")]
fn stdout_bytes(output: &str) -> Vec<u8> {
    let mut bytes = output.as_bytes().to_vec();
    bytes.push(b'\n');
    bytes
}

#[cfg(feature = "cli")]
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
