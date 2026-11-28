//! Pure deterministic ustar encoding and decoding used by the in-memory APIs.
//!
//! The filesystem archive commands keep their existing staging behavior.  This module owns
//! only the byte-level format, so the same pack bytes can be produced and checked without a
//! path, clock, thread, or ambient witness.

use std::path::{Component, Path};

use serde_json::json;

use crate::refusal::{RefusalCode, RefusalEnvelope};

pub(crate) const TAR_BLOCK_SIZE: usize = 512;
const MAX_ARCHIVE_MEMBERS: usize = 10_000;
const MAX_EXTRACTED_BYTES: u64 = 1 << 30;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TarEntry {
    pub path: String,
    pub bytes: Vec<u8>,
}

pub(crate) fn encode(entries: &[(&str, &[u8])]) -> Result<Vec<u8>, Box<RefusalEnvelope>> {
    let mut output = Vec::new();
    for (path, bytes) in entries {
        write_tar_file(&mut output, path, bytes)?;
    }
    output.extend_from_slice(&[0; TAR_BLOCK_SIZE * 2]);
    Ok(output)
}

pub(crate) fn decode_entries(input: &[u8]) -> Result<Vec<TarEntry>, Box<RefusalEnvelope>> {
    let mut entries = Vec::new();
    let mut offset = 0usize;
    let mut seen = std::collections::HashSet::new();
    let mut total_bytes = 0u64;

    loop {
        if entries.len() >= MAX_ARCHIVE_MEMBERS {
            return Err(limit_refusal(
                "member_count",
                entries.len().saturating_add(1).to_string(),
                MAX_ARCHIVE_MEMBERS.to_string(),
            ));
        }

        let header = read_block(input, &mut offset, "archive header")?;
        if header.iter().all(|byte| *byte == 0) {
            let terminator = read_block(input, &mut offset, "archive terminator")?;
            if terminator.iter().any(|byte| *byte != 0) {
                return Err(bad_pack("Archive terminator is not two zero blocks", None));
            }
            break;
        }

        validate_header_checksum(header)?;
        let path = tar_path_from_header(header)?;
        if entries.is_empty() && path != "manifest.json" {
            return Err(bad_pack(
                "Archive first entry must be manifest.json",
                Some(json!({"first_entry": path})),
            ));
        }
        if !seen.insert(path.clone()) {
            return Err(bad_pack(
                &format!("Archive contains duplicate member path: {path}"),
                Some(json!({"path": path})),
            ));
        }

        let typeflag = header[156];
        if typeflag != 0 && typeflag != b'0' {
            return Err(bad_pack(
                &format!("Archive contains unsupported entry type for {path}"),
                Some(json!({"path": path, "typeflag": typeflag})),
            ));
        }

        let size = parse_octal(&header[124..136])?;
        let next_total = total_bytes.checked_add(size).ok_or_else(|| {
            limit_refusal(
                "total_extracted_bytes",
                "overflow".to_string(),
                MAX_EXTRACTED_BYTES.to_string(),
            )
        })?;
        if next_total > MAX_EXTRACTED_BYTES {
            return Err(limit_refusal(
                "total_extracted_bytes",
                next_total.to_string(),
                MAX_EXTRACTED_BYTES.to_string(),
            ));
        }
        total_bytes = next_total;

        let size_usize = usize::try_from(size)
            .map_err(|_| limit_refusal("member_size", size.to_string(), usize::MAX.to_string()))?;
        let end = offset.checked_add(size_usize).ok_or_else(|| {
            bad_pack(
                "Archive member size overflows the input",
                Some(json!({"path": path})),
            )
        })?;
        if end > input.len() {
            return Err(bad_pack(
                &format!("Archive member is truncated: {path}"),
                Some(json!({"path": path})),
            ));
        }
        let bytes = input[offset..end].to_vec();
        offset = end;

        let padding = padding_for(size);
        let padding_end = offset.checked_add(padding).ok_or_else(|| {
            bad_pack(
                "Archive padding overflows the input",
                Some(json!({"path": path})),
            )
        })?;
        if padding_end > input.len() {
            return Err(bad_pack(
                &format!("Archive member padding is truncated: {path}"),
                Some(json!({"path": path})),
            ));
        }
        offset = padding_end;

        entries.push(TarEntry { path, bytes });
    }

    if !seen.contains("manifest.json") {
        return Err(bad_pack("Archive does not contain manifest.json", None));
    }
    Ok(entries)
}

fn read_block<'a>(
    input: &'a [u8],
    offset: &mut usize,
    label: &str,
) -> Result<&'a [u8; 512], Box<RefusalEnvelope>> {
    let end = offset
        .checked_add(TAR_BLOCK_SIZE)
        .ok_or_else(|| bad_pack("Archive offset overflows the input", None))?;
    if end > input.len() {
        return Err(bad_pack(&format!("{label} is truncated"), None));
    }
    let block = input[*offset..end]
        .try_into()
        .expect("checked 512-byte block");
    *offset = end;
    Ok(block)
}

fn write_tar_file(
    output: &mut Vec<u8>,
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
    header[148..156].fill(b' ');
    header[156] = b'0';
    write_bytes(&mut header[257..263], b"ustar\0");
    write_bytes(&mut header[263..265], b"00");
    write_bytes(&mut header[345..500], prefix.as_bytes());

    let checksum = header.iter().map(|byte| u64::from(*byte)).sum::<u64>();
    write_checksum(&mut header[148..156], checksum)?;
    output.extend_from_slice(&header);
    output.extend_from_slice(bytes);
    output.resize(output.len() + padding_for(bytes.len() as u64), 0);
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
    Err(bad_pack(
        &format!("Path is too long for deterministic ustar archive: {path}"),
        Some(json!({"path": path, "max_name_bytes": 100, "max_prefix_bytes": 155})),
    ))
}

fn tar_path_from_header(header: &[u8; 512]) -> Result<String, Box<RefusalEnvelope>> {
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
        .map(str::to_owned)
        .map_err(|error| bad_pack(&format!("Archive path is not valid UTF-8: {error}"), None))
}

fn ensure_safe_archive_path(path: &str) -> Result<(), Box<RefusalEnvelope>> {
    if path.is_empty()
        || path.ends_with('/')
        || path.ends_with('\\')
        || path.contains('\\')
        || path.split('/').any(|component| component == ".")
    {
        return Err(unsafe_path(path));
    }
    let path_obj = Path::new(path);
    if path_obj.is_absolute()
        || path_obj
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(unsafe_path(path));
    }
    Ok(())
}

fn parse_octal(field: &[u8]) -> Result<u64, Box<RefusalEnvelope>> {
    let text = field
        .iter()
        .take_while(|byte| **byte != 0 && **byte != b' ')
        .copied()
        .collect::<Vec<_>>();
    let text = std::str::from_utf8(&text)
        .map_err(|error| bad_pack(&format!("Archive octal field is not UTF-8: {error}"), None))?;
    u64::from_str_radix(text.trim(), 8)
        .map_err(|error| bad_pack(&format!("Archive octal field is invalid: {error}"), None))
}

fn validate_header_checksum(header: &[u8; 512]) -> Result<(), Box<RefusalEnvelope>> {
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
        return Err(bad_pack(
            "Archive header checksum mismatch",
            Some(json!({"expected": expected, "actual": actual})),
        ));
    }
    Ok(())
}

fn write_bytes(field: &mut [u8], bytes: &[u8]) {
    field[..bytes.len()].copy_from_slice(bytes);
}

fn write_octal(field: &mut [u8], value: u64, name: &str) -> Result<(), Box<RefusalEnvelope>> {
    let text = format!("{value:0width$o}\0", width = field.len() - 1);
    if text.len() != field.len() {
        return Err(bad_pack(
            &format!("Archive {name} value is too large for ustar header"),
            Some(json!({"field": name, "value": value, "field_bytes": field.len()})),
        ));
    }
    field.copy_from_slice(text.as_bytes());
    Ok(())
}

fn write_checksum(field: &mut [u8], value: u64) -> Result<(), Box<RefusalEnvelope>> {
    let text = format!("{value:06o}\0 ");
    if text.len() != field.len() {
        return Err(bad_pack(
            "Archive checksum value is too large for ustar header",
            None,
        ));
    }
    field.copy_from_slice(text.as_bytes());
    Ok(())
}

fn padding_for(size: u64) -> usize {
    let remainder = (size as usize) % TAR_BLOCK_SIZE;
    if remainder == 0 {
        0
    } else {
        TAR_BLOCK_SIZE - remainder
    }
}

fn bad_pack(message: &str, detail: Option<serde_json::Value>) -> Box<RefusalEnvelope> {
    Box::new(RefusalEnvelope::new(
        RefusalCode::BadPack,
        Some(message.to_string()),
        detail,
    ))
}

fn unsafe_path(path: &str) -> Box<RefusalEnvelope> {
    bad_pack(
        &format!("Archive contains unsafe member path: {path}"),
        Some(json!({"path": path})),
    )
}

fn limit_refusal(limit: &str, actual: String, maximum: String) -> Box<RefusalEnvelope> {
    bad_pack(
        &format!("Archive {limit} exceeds extraction limit"),
        Some(json!({"limit": limit, "actual": actual, "maximum": maximum})),
    )
}
