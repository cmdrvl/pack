use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use serde_json::json;

use crate::refusal::{RefusalCode, RefusalEnvelope};
use crate::seal::manifest::Manifest;
use crate::staging::{
    create_staging_dir, ensure_output_available, ensure_parent_exists, output_parent,
    promote_staging,
};
use crate::verify::run_checks;

const TAR_BLOCK_SIZE: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveResult {
    pub pack_id: String,
    pub path: PathBuf,
}

pub fn execute_archive_export(
    pack_dir: &Path,
    out_file: &Path,
) -> Result<ArchiveResult, Box<RefusalEnvelope>> {
    if out_file.exists() {
        return Err(Box::new(RefusalEnvelope::new(
            RefusalCode::Io,
            Some(format!(
                "Archive output already exists: {}",
                out_file.display()
            )),
            Some(json!({
                "out": out_file.display().to_string(),
            })),
        )));
    }

    let manifest = load_manifest(pack_dir)?;
    validate_pack_for_archive(pack_dir, &manifest)?;

    let parent = output_parent(out_file);
    ensure_parent_exists(&parent)?;
    let mut temp = tempfile::NamedTempFile::new_in(&parent).map_err(|error| {
        Box::new(RefusalEnvelope::new(
            RefusalCode::Io,
            Some(format!("Cannot create archive staging file: {error}")),
            None,
        ))
    })?;

    write_archive(&mut temp, pack_dir, &manifest)?;
    temp.flush().map_err(|error| {
        Box::new(RefusalEnvelope::new(
            RefusalCode::Io,
            Some(format!("Cannot flush archive staging file: {error}")),
            None,
        ))
    })?;
    temp.persist(out_file).map_err(|error| {
        Box::new(RefusalEnvelope::new(
            RefusalCode::Io,
            Some(format!(
                "Cannot promote archive file to {}: {}",
                out_file.display(),
                error.error
            )),
            None,
        ))
    })?;

    Ok(ArchiveResult {
        pack_id: manifest.pack_id,
        path: out_file.to_path_buf(),
    })
}

pub fn execute_archive_import(
    archive_file: &Path,
    out_dir: &Path,
) -> Result<ArchiveResult, Box<RefusalEnvelope>> {
    ensure_output_available(out_dir)?;
    let parent = output_parent(out_dir);
    ensure_parent_exists(&parent)?;
    let staging_dir = create_staging_dir(&parent, ".pack-archive-import-")?;

    extract_archive(archive_file, staging_dir.path())?;
    let manifest = load_manifest(staging_dir.path())?;
    validate_pack_for_archive(staging_dir.path(), &manifest)?;
    promote_staging(staging_dir, out_dir)?;

    Ok(ArchiveResult {
        pack_id: manifest.pack_id,
        path: out_dir.to_path_buf(),
    })
}

fn load_manifest(pack_dir: &Path) -> Result<Manifest, Box<RefusalEnvelope>> {
    let manifest_path = pack_dir.join("manifest.json");
    let content = fs::read_to_string(&manifest_path).map_err(|error| {
        Box::new(RefusalEnvelope::new(
            RefusalCode::BadPack,
            Some(format!("Cannot read manifest.json: {error}")),
            Some(json!({
                "pack_dir": pack_dir.display().to_string(),
                "manifest_path": manifest_path.display().to_string(),
            })),
        ))
    })?;
    let manifest: Manifest = serde_json::from_str(&content).map_err(|error| {
        Box::new(RefusalEnvelope::new(
            RefusalCode::BadPack,
            Some(format!("Invalid manifest.json: {error}")),
            Some(json!({
                "pack_dir": pack_dir.display().to_string(),
                "manifest_path": manifest_path.display().to_string(),
            })),
        ))
    })?;
    if manifest.version != "pack.v0" {
        return Err(Box::new(RefusalEnvelope::new(
            RefusalCode::BadPack,
            Some(format!(
                "Unsupported manifest version: {}",
                manifest.version
            )),
            Some(json!({
                "pack_dir": pack_dir.display().to_string(),
                "manifest_path": manifest_path.display().to_string(),
                "version": manifest.version,
            })),
        )));
    }
    Ok(manifest)
}

fn validate_pack_for_archive(
    pack_dir: &Path,
    manifest: &Manifest,
) -> Result<(), Box<RefusalEnvelope>> {
    let (checks, findings) = run_checks(manifest, pack_dir);
    if !findings.is_empty() {
        return Err(Box::new(RefusalEnvelope::new(
            RefusalCode::BadPack,
            Some(format!(
                "Pack directory failed integrity checks for archive operation: {}",
                manifest.pack_id
            )),
            Some(json!({
                "pack_dir": pack_dir.display().to_string(),
                "pack_id": manifest.pack_id,
                "checks": checks,
                "invalid": findings,
            })),
        )));
    }
    Ok(())
}

fn write_archive<W: Write>(
    writer: &mut W,
    pack_dir: &Path,
    manifest: &Manifest,
) -> Result<(), Box<RefusalEnvelope>> {
    write_tar_file(writer, "manifest.json", &manifest.to_canonical_bytes())?;
    for member in &manifest.members {
        let bytes = fs::read(pack_dir.join(&member.path)).map_err(|error| {
            Box::new(RefusalEnvelope::new(
                RefusalCode::Io,
                Some(format!(
                    "Cannot read member for archive {}: {error}",
                    member.path
                )),
                Some(json!({
                    "pack_dir": pack_dir.display().to_string(),
                    "path": member.path,
                })),
            ))
        })?;
        write_tar_file(writer, &member.path, &bytes)?;
    }
    writer
        .write_all(&[0u8; TAR_BLOCK_SIZE * 2])
        .map_err(|error| {
            Box::new(RefusalEnvelope::new(
                RefusalCode::Io,
                Some(format!("Cannot write archive terminator: {error}")),
                None,
            ))
        })?;
    Ok(())
}

fn write_tar_file<W: Write>(
    writer: &mut W,
    path: &str,
    bytes: &[u8],
) -> Result<(), Box<RefusalEnvelope>> {
    let (name, prefix) = split_tar_path(path)?;
    let mut header = [0u8; TAR_BLOCK_SIZE];
    write_bytes(&mut header[0..100], name.as_bytes());
    write_octal(&mut header[100..108], 0o644, "mode")?;
    write_octal(&mut header[108..116], 0, "uid")?;
    write_octal(&mut header[116..124], 0, "gid")?;
    write_octal(&mut header[124..136], bytes.len() as u64, "size")?;
    write_octal(&mut header[136..148], 0, "mtime")?;
    for byte in &mut header[148..156] {
        *byte = b' ';
    }
    header[156] = b'0';
    write_bytes(&mut header[257..263], b"ustar\0");
    write_bytes(&mut header[263..265], b"00");
    write_bytes(&mut header[345..500], prefix.as_bytes());

    let checksum: u64 = header.iter().map(|byte| u64::from(*byte)).sum();
    write_checksum(&mut header[148..156], checksum)?;

    writer.write_all(&header).map_err(|error| {
        Box::new(RefusalEnvelope::new(
            RefusalCode::Io,
            Some(format!("Cannot write archive header for {path}: {error}")),
            None,
        ))
    })?;
    writer.write_all(bytes).map_err(|error| {
        Box::new(RefusalEnvelope::new(
            RefusalCode::Io,
            Some(format!("Cannot write archive member {path}: {error}")),
            None,
        ))
    })?;
    let padding = padding_for(bytes.len() as u64);
    if padding > 0 {
        writer.write_all(&vec![0u8; padding]).map_err(|error| {
            Box::new(RefusalEnvelope::new(
                RefusalCode::Io,
                Some(format!("Cannot write archive padding for {path}: {error}")),
                None,
            ))
        })?;
    }
    Ok(())
}

fn extract_archive(archive_file: &Path, dest_dir: &Path) -> Result<(), Box<RefusalEnvelope>> {
    let mut reader = File::open(archive_file).map_err(|error| {
        Box::new(RefusalEnvelope::new(
            RefusalCode::Io,
            Some(format!(
                "Cannot read archive file {}: {error}",
                archive_file.display()
            )),
            Some(json!({
                "archive": archive_file.display().to_string(),
            })),
        ))
    })?;
    let mut seen = HashSet::new();
    let mut entry_index = 0usize;

    loop {
        let mut header = [0u8; TAR_BLOCK_SIZE];
        reader.read_exact(&mut header).map_err(|error| {
            Box::new(RefusalEnvelope::new(
                RefusalCode::BadPack,
                Some(format!("Cannot read archive header: {error}")),
                Some(json!({
                    "archive": archive_file.display().to_string(),
                })),
            ))
        })?;

        if header.iter().all(|byte| *byte == 0) {
            break;
        }
        validate_header_checksum(&header)?;

        let path = tar_path_from_header(&header)?;
        if entry_index == 0 && path != "manifest.json" {
            return Err(Box::new(RefusalEnvelope::new(
                RefusalCode::BadPack,
                Some("Archive first entry must be manifest.json".to_string()),
                Some(json!({
                    "archive": archive_file.display().to_string(),
                    "first_entry": path,
                })),
            )));
        }
        if !seen.insert(path.clone()) {
            return Err(Box::new(RefusalEnvelope::new(
                RefusalCode::BadPack,
                Some(format!("Archive contains duplicate member path: {path}")),
                Some(json!({
                    "archive": archive_file.display().to_string(),
                    "path": path,
                })),
            )));
        }

        let typeflag = header[156];
        if typeflag != 0 && typeflag != b'0' {
            return Err(Box::new(RefusalEnvelope::new(
                RefusalCode::BadPack,
                Some(format!(
                    "Archive contains unsupported entry type for {path}"
                )),
                Some(json!({
                    "archive": archive_file.display().to_string(),
                    "path": path,
                    "typeflag": typeflag,
                })),
            )));
        }

        let size = parse_octal(&header[124..136])?;
        let output_path = safe_join(dest_dir, &path)?;
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                Box::new(RefusalEnvelope::new(
                    RefusalCode::Io,
                    Some(format!(
                        "Cannot create archive output parent {}: {error}",
                        parent.display()
                    )),
                    None,
                ))
            })?;
        }
        copy_archive_member(&mut reader, &output_path, size, &path)?;
        read_archive_padding(&mut reader, size, &path)?;

        entry_index += 1;
    }

    if !seen.contains("manifest.json") {
        return Err(Box::new(RefusalEnvelope::new(
            RefusalCode::BadPack,
            Some("Archive does not contain manifest.json".to_string()),
            Some(json!({
                "archive": archive_file.display().to_string(),
            })),
        )));
    }
    Ok(())
}

fn split_tar_path(path: &str) -> Result<(String, String), Box<RefusalEnvelope>> {
    let path_bytes = path.as_bytes();
    if path_bytes.len() <= 100 {
        return Ok((path.to_string(), String::new()));
    }

    for (index, byte) in path_bytes.iter().enumerate().rev() {
        if *byte != b'/' {
            continue;
        }
        let prefix = &path[..index];
        let name = &path[index + 1..];
        if !prefix.is_empty() && prefix.len() <= 155 && !name.is_empty() && name.len() <= 100 {
            return Ok((name.to_string(), prefix.to_string()));
        }
    }

    Err(Box::new(RefusalEnvelope::new(
        RefusalCode::BadPack,
        Some(format!(
            "Path is too long for deterministic ustar archive: {path}"
        )),
        Some(json!({
            "path": path,
            "max_name_bytes": 100,
            "max_prefix_bytes": 155,
        })),
    )))
}

fn tar_path_from_header(header: &[u8; TAR_BLOCK_SIZE]) -> Result<String, Box<RefusalEnvelope>> {
    let name = read_c_string(&header[0..100])?;
    let prefix = read_c_string(&header[345..500])?;
    let path = if prefix.is_empty() {
        name
    } else {
        format!("{prefix}/{name}")
    };
    ensure_safe_archive_path(&path)?;
    Ok(path)
}

fn read_c_string(bytes: &[u8]) -> Result<String, Box<RefusalEnvelope>> {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    std::str::from_utf8(&bytes[..end])
        .map(|value| value.to_string())
        .map_err(|error| {
            Box::new(RefusalEnvelope::new(
                RefusalCode::BadPack,
                Some(format!("Archive path is not valid UTF-8: {error}")),
                None,
            ))
        })
}

fn ensure_safe_archive_path(path: &str) -> Result<(), Box<RefusalEnvelope>> {
    if path.is_empty() {
        return Err(unsafe_archive_path(path));
    }

    let path = Path::new(path);
    if path.is_absolute() {
        return Err(unsafe_archive_path(&path.display().to_string()));
    }
    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(unsafe_archive_path(&path.display().to_string()));
    }
    Ok(())
}

fn unsafe_archive_path(path: &str) -> Box<RefusalEnvelope> {
    Box::new(RefusalEnvelope::new(
        RefusalCode::BadPack,
        Some(format!("Archive contains unsafe member path: {path}")),
        Some(json!({
            "path": path,
        })),
    ))
}

fn safe_join(base: &Path, relative: &str) -> Result<PathBuf, Box<RefusalEnvelope>> {
    ensure_safe_archive_path(relative)?;
    Ok(base.join(relative))
}

fn write_bytes(field: &mut [u8], bytes: &[u8]) {
    field[..bytes.len()].copy_from_slice(bytes);
}

fn write_octal(field: &mut [u8], value: u64, name: &str) -> Result<(), Box<RefusalEnvelope>> {
    let text = format!("{value:0width$o}\0", width = field.len() - 1);
    if text.len() != field.len() {
        return Err(Box::new(RefusalEnvelope::new(
            RefusalCode::BadPack,
            Some(format!(
                "Archive {name} value is too large for ustar header"
            )),
            Some(json!({
                "field": name,
                "value": value,
                "field_bytes": field.len(),
            })),
        )));
    }
    field.copy_from_slice(text.as_bytes());
    Ok(())
}

fn write_checksum(field: &mut [u8], value: u64) -> Result<(), Box<RefusalEnvelope>> {
    let text = format!("{value:06o}\0 ");
    if text.len() != field.len() {
        return Err(Box::new(RefusalEnvelope::new(
            RefusalCode::BadPack,
            Some("Archive checksum value is too large for ustar header".to_string()),
            Some(json!({
                "value": value,
                "field_bytes": field.len(),
            })),
        )));
    }
    field.copy_from_slice(text.as_bytes());
    Ok(())
}

fn parse_octal(field: &[u8]) -> Result<u64, Box<RefusalEnvelope>> {
    let text = field
        .iter()
        .take_while(|byte| **byte != 0 && **byte != b' ')
        .copied()
        .collect::<Vec<_>>();
    let text = std::str::from_utf8(&text).map_err(|error| {
        Box::new(RefusalEnvelope::new(
            RefusalCode::BadPack,
            Some(format!("Archive octal field is not UTF-8: {error}")),
            None,
        ))
    })?;
    u64::from_str_radix(text.trim(), 8).map_err(|error| {
        Box::new(RefusalEnvelope::new(
            RefusalCode::BadPack,
            Some(format!("Archive octal field is invalid: {error}")),
            None,
        ))
    })
}

fn validate_header_checksum(header: &[u8; TAR_BLOCK_SIZE]) -> Result<(), Box<RefusalEnvelope>> {
    let expected = parse_octal(&header[148..156])?;
    let actual = header
        .iter()
        .enumerate()
        .map(|(index, byte)| {
            if (148..156).contains(&index) {
                u64::from(b' ')
            } else {
                u64::from(*byte)
            }
        })
        .sum::<u64>();
    if expected != actual {
        return Err(Box::new(RefusalEnvelope::new(
            RefusalCode::BadPack,
            Some("Archive header checksum mismatch".to_string()),
            Some(json!({
                "expected": expected,
                "actual": actual,
            })),
        )));
    }
    Ok(())
}

fn copy_archive_member<R: Read>(
    reader: &mut R,
    output_path: &Path,
    size: u64,
    path: &str,
) -> Result<(), Box<RefusalEnvelope>> {
    let mut output = File::create(output_path).map_err(|error| {
        Box::new(RefusalEnvelope::new(
            RefusalCode::Io,
            Some(format!(
                "Cannot write archive member {}: {error}",
                output_path.display()
            )),
            None,
        ))
    })?;
    let copied = std::io::copy(&mut reader.by_ref().take(size), &mut output).map_err(|error| {
        Box::new(RefusalEnvelope::new(
            RefusalCode::BadPack,
            Some(format!("Cannot read archive member {path}: {error}")),
            None,
        ))
    })?;
    if copied != size {
        return Err(Box::new(RefusalEnvelope::new(
            RefusalCode::BadPack,
            Some(format!("Archive member {path} ended before declared size")),
            Some(json!({
                "path": path,
                "expected_bytes": size,
                "actual_bytes": copied,
            })),
        )));
    }
    Ok(())
}

fn read_archive_padding<R: Read>(
    reader: &mut R,
    size: u64,
    path: &str,
) -> Result<(), Box<RefusalEnvelope>> {
    let padding = padding_for(size);
    if padding == 0 {
        return Ok(());
    }
    let mut skip = [0u8; TAR_BLOCK_SIZE - 1];
    reader.read_exact(&mut skip[..padding]).map_err(|error| {
        Box::new(RefusalEnvelope::new(
            RefusalCode::BadPack,
            Some(format!("Cannot read archive padding for {path}: {error}")),
            None,
        ))
    })?;
    Ok(())
}

fn padding_for(size: u64) -> usize {
    let remainder = (size % TAR_BLOCK_SIZE as u64) as usize;
    if remainder == 0 {
        0
    } else {
        TAR_BLOCK_SIZE - remainder
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seal::command::execute_seal;
    use crate::verify::execute_verify;

    fn create_pack() -> (tempfile::TempDir, PathBuf, String) {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a.lock.json");
        let b = tmp.path().join("nested").join("b.report.json");
        fs::create_dir_all(b.parent().unwrap()).unwrap();
        fs::write(&a, r#"{"version":"lock.v0","rows":5}"#).unwrap();
        fs::write(&b, r#"{"version":"rvl.v0","outcome":"NO_REAL_CHANGE"}"#).unwrap();
        let pack_dir = tmp.path().join("pack");
        let result = execute_seal(
            &[a, b],
            Some(&pack_dir),
            Some("archive me".to_string()),
            Some("2026-01-15T10:30:00Z"),
        )
        .unwrap();
        (tmp, pack_dir, result.pack_id)
    }

    #[test]
    fn archive_export_is_deterministic_for_same_pack() {
        let (_tmp, pack_dir, pack_id) = create_pack();
        let archive_a = pack_dir.with_extension("a.tar");
        let archive_b = pack_dir.with_extension("b.tar");

        let result_a = execute_archive_export(&pack_dir, &archive_a).unwrap();
        let result_b = execute_archive_export(&pack_dir, &archive_b).unwrap();

        assert_eq!(result_a.pack_id, pack_id);
        assert_eq!(result_b.pack_id, pack_id);
        assert_eq!(fs::read(&archive_a).unwrap(), fs::read(&archive_b).unwrap());
    }

    #[test]
    fn archive_import_round_trips_to_valid_pack() {
        let (tmp, pack_dir, pack_id) = create_pack();
        let archive = tmp.path().join("pack.tar");
        let imported = tmp.path().join("imported");
        execute_archive_export(&pack_dir, &archive).unwrap();

        let result = execute_archive_import(&archive, &imported).unwrap();

        assert_eq!(result.pack_id, pack_id);
        let (_output, code) = execute_verify(&imported, true);
        assert_eq!(code, 0);
        assert_eq!(
            fs::read(imported.join("b.report.json")).unwrap(),
            fs::read(pack_dir.join("b.report.json")).unwrap()
        );
    }

    #[test]
    fn archive_import_tamper_refuses_and_leaves_no_output_dir() {
        let (tmp, pack_dir, _pack_id) = create_pack();
        let archive = tmp.path().join("tampered.tar");
        let imported = tmp.path().join("imported");
        let manifest: Manifest =
            serde_json::from_str(&fs::read_to_string(pack_dir.join("manifest.json")).unwrap())
                .unwrap();
        let mut bytes = Vec::new();
        write_tar_file(&mut bytes, "manifest.json", &manifest.to_canonical_bytes()).unwrap();
        write_tar_file(
            &mut bytes,
            "a.lock.json",
            &fs::read(pack_dir.join("a.lock.json")).unwrap(),
        )
        .unwrap();
        write_tar_file(
            &mut bytes,
            "b.report.json",
            &fs::read(pack_dir.join("b.report.json")).unwrap(),
        )
        .unwrap();
        write_tar_file(&mut bytes, "extra.txt", b"not declared").unwrap();
        bytes.extend_from_slice(&[0u8; TAR_BLOCK_SIZE * 2]);
        fs::write(&archive, bytes).unwrap();

        let error = execute_archive_import(&archive, &imported).unwrap_err();

        assert_eq!(error.refusal.code, "E_BAD_PACK");
        assert!(!imported.exists());
    }

    #[test]
    fn archive_import_refuses_unsafe_member_path() {
        let tmp = tempfile::tempdir().unwrap();
        let archive = tmp.path().join("unsafe.tar");
        let imported = tmp.path().join("imported");
        let mut bytes = Vec::new();
        write_tar_file(&mut bytes, "../evil.txt", b"escape").unwrap();
        bytes.extend_from_slice(&[0u8; TAR_BLOCK_SIZE * 2]);
        fs::write(&archive, bytes).unwrap();

        let error = execute_archive_import(&archive, &imported).unwrap_err();

        assert_eq!(error.refusal.code, "E_BAD_PACK");
        assert!(!imported.exists());
    }

    #[test]
    fn archive_export_refuses_invalid_pack() {
        let (tmp, pack_dir, _pack_id) = create_pack();
        fs::write(pack_dir.join("a.lock.json"), "tampered").unwrap();

        let error = execute_archive_export(&pack_dir, &tmp.path().join("bad.tar")).unwrap_err();

        assert_eq!(error.refusal.code, "E_BAD_PACK");
    }
}
