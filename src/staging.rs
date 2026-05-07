use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use crate::refusal::{RefusalCode, RefusalEnvelope};

pub(crate) fn output_parent(final_dir: &Path) -> PathBuf {
    final_dir
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

pub(crate) fn ensure_parent_exists(parent: &Path) -> Result<(), Box<RefusalEnvelope>> {
    if parent.exists() {
        return Ok(());
    }

    fs::create_dir_all(parent).map_err(|error| {
        Box::new(RefusalEnvelope::new(
            RefusalCode::Io,
            Some(format!(
                "Cannot create output parent directory {}: {error}",
                parent.display()
            )),
            None,
        ))
    })
}

pub(crate) fn ensure_output_available(final_dir: &Path) -> Result<(), Box<RefusalEnvelope>> {
    if !final_dir.exists() {
        return Ok(());
    }

    if !is_empty_directory(final_dir)? {
        return Err(Box::new(RefusalEnvelope::new(
            RefusalCode::Io,
            Some(format!(
                "Output directory already exists and is non-empty: {}",
                final_dir.display()
            )),
            None,
        )));
    }

    Ok(())
}

pub(crate) fn create_staging_dir(
    parent: &Path,
    prefix: &str,
) -> Result<TempDir, Box<RefusalEnvelope>> {
    tempfile::Builder::new()
        .prefix(prefix)
        .tempdir_in(parent)
        .map_err(|error| {
            Box::new(RefusalEnvelope::new(
                RefusalCode::Io,
                Some(format!("Cannot create staging directory: {error}")),
                None,
            ))
        })
}

pub(crate) fn promote_staging(
    staging_dir: TempDir,
    final_dir: &Path,
) -> Result<(), Box<RefusalEnvelope>> {
    ensure_output_available(final_dir)?;

    fs::rename(staging_dir.path(), final_dir).map_err(|error| {
        Box::new(RefusalEnvelope::new(
            RefusalCode::Io,
            Some(format!(
                "Cannot atomically promote staging directory {} to {}: {error}",
                staging_dir.path().display(),
                final_dir.display()
            )),
            None,
        ))
    })?;

    let _ = staging_dir.keep();
    Ok(())
}

fn is_empty_directory(path: &Path) -> Result<bool, Box<RefusalEnvelope>> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        Box::new(RefusalEnvelope::new(
            RefusalCode::Io,
            Some(format!(
                "Cannot inspect output path {}: {error}",
                path.display()
            )),
            None,
        ))
    })?;

    if !metadata.is_dir() {
        return Ok(false);
    }

    let mut entries = fs::read_dir(path).map_err(|error| {
        Box::new(RefusalEnvelope::new(
            RefusalCode::Io,
            Some(format!(
                "Cannot inspect output directory {}: {error}",
                path.display()
            )),
            None,
        ))
    })?;
    Ok(entries.next().is_none())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn promotes_staging_to_new_output_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let staging = create_staging_dir(tmp.path(), ".pack-test-").unwrap();
        fs::write(staging.path().join("manifest.json"), "{}").unwrap();
        let final_dir = tmp.path().join("pack-out");

        promote_staging(staging, &final_dir).unwrap();

        assert_eq!(
            fs::read_to_string(final_dir.join("manifest.json")).unwrap(),
            "{}"
        );
    }

    #[test]
    fn promotes_staging_to_existing_empty_output_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let staging = create_staging_dir(tmp.path(), ".pack-test-").unwrap();
        fs::write(staging.path().join("manifest.json"), "{}").unwrap();
        let final_dir = tmp.path().join("pack-out");
        fs::create_dir(&final_dir).unwrap();

        promote_staging(staging, &final_dir).unwrap();

        assert_eq!(
            fs::read_to_string(final_dir.join("manifest.json")).unwrap(),
            "{}"
        );
    }

    #[test]
    fn refuses_non_empty_output_without_mutating_it() {
        let tmp = tempfile::tempdir().unwrap();
        let staging = create_staging_dir(tmp.path(), ".pack-test-").unwrap();
        fs::write(staging.path().join("manifest.json"), "{}").unwrap();
        let final_dir = tmp.path().join("pack-out");
        fs::create_dir(&final_dir).unwrap();
        fs::write(final_dir.join("existing.txt"), "keep").unwrap();

        let error = promote_staging(staging, &final_dir).unwrap_err();

        assert_eq!(error.refusal.code, "E_IO");
        assert_eq!(
            fs::read_to_string(final_dir.join("existing.txt")).unwrap(),
            "keep"
        );
        assert!(!final_dir.join("manifest.json").exists());
    }
}
