use serde_json::{json, Value};
use std::{
    env,
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

const TOOL: &str = "pack";
const WITNESS_ENV: &str = "EPISTEMIC_WITNESS";

pub(crate) fn witness_path() -> PathBuf {
    witness_path_from_env(|key| env::var_os(key))
}

pub(crate) fn witness_path_for_append() -> Result<PathBuf, String> {
    witness_path_for_append_from_env(|key| env::var_os(key))
}

pub(crate) fn witness_path_for_query() -> Result<PathBuf, String> {
    witness_path_for_query_from_env(|key| env::var_os(key))
}

pub(crate) fn pack_state_dir_for_write() -> Result<PathBuf, String> {
    let cwd = env::current_dir()
        .map_err(|error| format!("failed to determine current directory: {error}"))?;
    pack_state_dir_for_write_from_env_and_cwd(|key| env::var_os(key), &cwd)
}

fn witness_path_from_env<F>(get_env: F) -> PathBuf
where
    F: Fn(&str) -> Option<OsString> + Copy,
{
    if let Some(path) = non_empty_env(get_env, WITNESS_ENV) {
        return PathBuf::from(path);
    }

    cmdrvl_root_from_env(get_env)
        .join("state")
        .join("witness")
        .join("witness.jsonl")
}

fn witness_path_for_append_from_env<F>(get_env: F) -> Result<PathBuf, String>
where
    F: Fn(&str) -> Option<OsString> + Copy,
{
    ensure_witness_migrated_from_env(get_env)?;
    let path = witness_path_from_env(get_env);
    if non_empty_env(get_env, WITNESS_ENV).is_none() {
        prepare_parent_from_env(get_env, &path)?;
    }
    Ok(path)
}

fn witness_path_for_query_from_env<F>(get_env: F) -> Result<PathBuf, String>
where
    F: Fn(&str) -> Option<OsString> + Copy,
{
    ensure_witness_migrated_from_env(get_env)?;
    Ok(witness_path_from_env(get_env))
}

fn pack_state_dir_for_write_from_env_and_cwd<F>(get_env: F, cwd: &Path) -> Result<PathBuf, String>
where
    F: Fn(&str) -> Option<OsString> + Copy,
{
    let canonical = cmdrvl_root_from_env(get_env).join("state").join("pack");
    migrate_dir_from_env(
        get_env,
        "pack_default_output_dir",
        &canonical,
        vec![cwd.join("pack")],
    )?;
    prepare_dir_from_env(get_env, &canonical)?;
    Ok(canonical)
}

fn ensure_witness_migrated_from_env<F>(get_env: F) -> Result<(), String>
where
    F: Fn(&str) -> Option<OsString> + Copy,
{
    if non_empty_env(get_env, WITNESS_ENV).is_some() {
        return Ok(());
    }

    migrate_file_from_env(
        get_env,
        "witness_ledger",
        &witness_path_from_env(get_env),
        legacy_witness_paths_from_env(get_env),
    )
}

fn migrate_file_from_env<F>(
    get_env: F,
    path_class: &str,
    canonical: &Path,
    legacy_paths: Vec<PathBuf>,
) -> Result<(), String>
where
    F: Fn(&str) -> Option<OsString> + Copy,
{
    let Some(legacy) = legacy_paths
        .into_iter()
        .find(|path| !same_path(path, canonical) && path.is_file())
    else {
        return Ok(());
    };

    let root = cmdrvl_root_from_env(get_env);
    let notice_path = root.join("notices").join("deprecated-paths.jsonl");
    let migration_path = root.join("migrations").join("applied.jsonl");

    if canonical.exists() {
        append_record_once(
            &notice_path,
            deprecation_record(
                path_class,
                &legacy,
                canonical,
                "legacy_path_present",
                "canonical_preferred",
            ),
        )?;
        return Ok(());
    }

    prepare_parent_from_env(get_env, canonical)?;
    fs::copy(&legacy, canonical).map_err(|error| {
        format!(
            "failed to copy legacy {path_class} '{}' to '{}': {error}",
            legacy.display(),
            canonical.display()
        )
    })?;
    harden_file(canonical)?;

    append_record_once(
        &migration_path,
        migration_record(path_class, &legacy, canonical, "copied_legacy_to_canonical"),
    )?;
    append_record_once(
        &notice_path,
        deprecation_record(
            path_class,
            &legacy,
            canonical,
            "legacy_path_migrated",
            "canonical_created",
        ),
    )?;

    Ok(())
}

fn migrate_dir_from_env<F>(
    get_env: F,
    path_class: &str,
    canonical: &Path,
    legacy_paths: Vec<PathBuf>,
) -> Result<(), String>
where
    F: Fn(&str) -> Option<OsString> + Copy,
{
    let Some(legacy) = legacy_paths
        .into_iter()
        .find(|path| !same_path(path, canonical) && path.is_dir())
    else {
        return Ok(());
    };

    let root = cmdrvl_root_from_env(get_env);
    let notice_path = root.join("notices").join("deprecated-paths.jsonl");
    let migration_path = root.join("migrations").join("applied.jsonl");

    if canonical.exists() {
        append_record_once(
            &notice_path,
            deprecation_record(
                path_class,
                &legacy,
                canonical,
                "legacy_path_present",
                "canonical_preferred",
            ),
        )?;
        return Ok(());
    }

    if let Some(parent) = canonical.parent() {
        prepare_dir_from_env(get_env, parent)?;
    }
    copy_dir_recursive(&legacy, canonical).map_err(|error| {
        format!(
            "failed to copy legacy {path_class} '{}' to '{}': {error}",
            legacy.display(),
            canonical.display()
        )
    })?;

    append_record_once(
        &migration_path,
        migration_record(path_class, &legacy, canonical, "copied_legacy_to_canonical"),
    )?;
    append_record_once(
        &notice_path,
        deprecation_record(
            path_class,
            &legacy,
            canonical,
            "legacy_path_migrated",
            "canonical_created",
        ),
    )?;

    Ok(())
}

fn cmdrvl_root_from_env<F>(get_env: F) -> PathBuf
where
    F: Fn(&str) -> Option<OsString> + Copy,
{
    if let Some(home) =
        non_empty_env(get_env, "HOME").or_else(|| non_empty_env(get_env, "USERPROFILE"))
    {
        return PathBuf::from(home).join(".cmdrvl");
    }

    PathBuf::from(".cmdrvl")
}

fn non_empty_env<F>(get_env: F, key: &str) -> Option<OsString>
where
    F: Fn(&str) -> Option<OsString> + Copy,
{
    let value = get_env(key)?;
    if value.is_empty() {
        return None;
    }
    if value.to_str().is_some_and(|value| value.trim().is_empty()) {
        return None;
    }
    Some(value)
}

fn legacy_witness_paths_from_env<F>(get_env: F) -> Vec<PathBuf>
where
    F: Fn(&str) -> Option<OsString> + Copy,
{
    let mut paths = Vec::new();

    if let Some(home) =
        non_empty_env(get_env, "HOME").or_else(|| non_empty_env(get_env, "USERPROFILE"))
    {
        paths.push(PathBuf::from(home).join(".epistemic").join("witness.jsonl"));
    }

    paths.push(PathBuf::from(".epistemic").join("witness.jsonl"));
    paths
}

fn prepare_parent_from_env<F>(get_env: F, path: &Path) -> Result<(), String>
where
    F: Fn(&str) -> Option<OsString> + Copy,
{
    let Some(parent) = path.parent() else {
        return Ok(());
    };

    prepare_dir_from_env(get_env, parent)
}

fn prepare_dir_from_env<F>(get_env: F, dir: &Path) -> Result<(), String>
where
    F: Fn(&str) -> Option<OsString> + Copy,
{
    let root = cmdrvl_root_from_env(get_env);
    fs::create_dir_all(&root)
        .map_err(|error| format!("failed to create cmdrvl root '{}': {error}", root.display()))?;
    harden_directory(&root)?;

    if let Some(parent) = dir.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create directory parent '{}': {error}",
                parent.display()
            )
        })?;
        if parent.starts_with(&root) {
            harden_directory(parent)?;
        }
    }

    fs::create_dir_all(dir)
        .map_err(|error| format!("failed to create directory '{}': {error}", dir.display()))?;
    harden_directory(dir)
}

fn copy_dir_recursive(source: &Path, destination: &Path) -> io::Result<()> {
    fs::create_dir_all(destination)?;
    harden_directory_io(destination)?;

    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());

        if file_type.is_dir() {
            copy_dir_recursive(&source_path, &destination_path)?;
        } else if file_type.is_file() {
            fs::copy(&source_path, &destination_path)?;
            harden_file_io(&destination_path)?;
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unsupported non-regular entry in legacy pack output",
            ));
        }
    }

    Ok(())
}

fn migration_record(path_class: &str, source: &Path, destination: &Path, action: &str) -> Value {
    json!({
        "version": "cmdrvl.migration.v1",
        "tool": TOOL,
        "path_class": path_class,
        "source_path": source.display().to_string(),
        "destination_path": destination.display().to_string(),
        "action": action,
        "outcome": "ok",
        "secret_values_recorded": false
    })
}

fn deprecation_record(
    path_class: &str,
    source: &Path,
    destination: &Path,
    action: &str,
    outcome: &str,
) -> Value {
    json!({
        "version": "cmdrvl.deprecated_path_notice.v1",
        "tool": TOOL,
        "path_class": path_class,
        "source_path": source.display().to_string(),
        "destination_path": destination.display().to_string(),
        "action": action,
        "outcome": outcome,
        "secret_values_recorded": false
    })
}

fn append_record_once(path: &Path, record: Value) -> Result<(), String> {
    if record_already_exists(path, &record)? {
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create migration record directory '{}': {error}",
                parent.display()
            )
        })?;
        harden_directory(parent)?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| {
            format!(
                "failed to open migration record '{}': {error}",
                path.display()
            )
        })?;
    writeln!(file, "{record}").map_err(|error| {
        format!(
            "failed to append migration record '{}': {error}",
            path.display()
        )
    })?;
    harden_file(path)
}

fn record_already_exists(path: &Path, record: &Value) -> Result<bool, String> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(format!(
                "failed to read migration record '{}': {error}",
                path.display()
            ));
        }
    };

    Ok(contents.lines().any(|line| {
        let Ok(existing) = serde_json::from_str::<Value>(line) else {
            return false;
        };

        existing.get("tool") == record.get("tool")
            && existing.get("path_class") == record.get("path_class")
            && existing.get("source_path") == record.get("source_path")
            && existing.get("destination_path") == record.get("destination_path")
            && existing.get("action") == record.get("action")
    }))
}

fn same_path(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }

    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn harden_directory(path: &Path) -> Result<(), String> {
    harden_directory_io(path)
        .map_err(|error| format!("failed to harden directory '{}': {error}", path.display()))
}

fn harden_file(path: &Path) -> Result<(), String> {
    harden_file_io(path)
        .map_err(|error| format!("failed to harden file '{}': {error}", path.display()))
}

#[cfg(unix)]
fn harden_directory_io(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn harden_directory_io(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn harden_file_io(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn harden_file_io(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        pack_state_dir_for_write_from_env_and_cwd, witness_path_for_append_from_env,
        witness_path_from_env,
    };
    use std::{
        ffi::OsString,
        fs,
        path::{Path, PathBuf},
    };
    use tempfile::TempDir;

    fn env_for_home(home: &Path) -> impl Fn(&str) -> Option<OsString> + Copy + '_ {
        |key| match key {
            "HOME" => Some(home.as_os_str().to_owned()),
            "USERPROFILE" => None,
            "EPISTEMIC_WITNESS" => None,
            _ => None,
        }
    }

    #[test]
    fn witness_defaults_to_cmdrvl_state() {
        let path = witness_path_from_env(|key| match key {
            "HOME" => Some(OsString::from("/tmp/home")),
            _ => None,
        });

        assert_eq!(
            path,
            PathBuf::from("/tmp/home/.cmdrvl/state/witness/witness.jsonl")
        );
    }

    #[test]
    fn explicit_witness_override_wins() {
        let path = witness_path_from_env(|key| match key {
            "EPISTEMIC_WITNESS" => Some(OsString::from("/tmp/custom.jsonl")),
            "HOME" => Some(OsString::from("/tmp/home")),
            _ => None,
        });

        assert_eq!(path, PathBuf::from("/tmp/custom.jsonl"));
    }

    #[test]
    fn witness_append_migrates_legacy_ledger() {
        let tmp = TempDir::new().unwrap();
        let home = tmp.path().join("home");
        let legacy = home.join(".epistemic").join("witness.jsonl");
        fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        fs::write(
            &legacy,
            "{\"tool\":\"pack\",\"version\":\"0.0.0\",\"outcome\":\"OLD\"}\n",
        )
        .unwrap();

        let canonical = witness_path_for_append_from_env(env_for_home(&home)).unwrap();

        assert_eq!(canonical, home.join(".cmdrvl/state/witness/witness.jsonl"));
        assert!(fs::read_to_string(&canonical).unwrap().contains("\"OLD\""));
        assert!(
            fs::read_to_string(home.join(".cmdrvl/migrations/applied.jsonl"))
                .unwrap()
                .contains("\"path_class\":\"witness_ledger\"")
        );
    }

    #[test]
    fn pack_state_migrates_legacy_default_output_dir() {
        let tmp = TempDir::new().unwrap();
        let home = tmp.path().join("home");
        let cwd = tmp.path().join("work");
        let legacy_file = cwd.join("pack").join("old").join("manifest.json");
        fs::create_dir_all(legacy_file.parent().unwrap()).unwrap();
        fs::write(&legacy_file, "{}").unwrap();

        let state_dir =
            pack_state_dir_for_write_from_env_and_cwd(env_for_home(&home), &cwd).unwrap();

        assert_eq!(state_dir, home.join(".cmdrvl/state/pack"));
        assert!(state_dir.join("old").join("manifest.json").exists());
        assert!(
            fs::read_to_string(home.join(".cmdrvl/migrations/applied.jsonl"))
                .unwrap()
                .contains("\"path_class\":\"pack_default_output_dir\"")
        );
    }
}
