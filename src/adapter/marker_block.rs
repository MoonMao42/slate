//! Marker block utilities for managed config modifications.
//! This module provides a reusable system for safely inserting and removing blocks
//! from configuration files. Marker blocks are delimited by START and END markers,
//! allowing slate to regenerate theme-specific configuration without destroying
//! user customizations outside the managed block.
//! ## Usage Pattern
//! Always call `validate_block_state()` BEFORE calling `strip_managed_blocks()` or
//! `upsert_managed_block()`. This ensures we catch malformed state early with guidance.
//! ```ignore
//! // ALWAYS validate before modifying
//! validate_block_state(&content)?;
//! // Safe to strip or upsert
//! let cleaned = strip_managed_blocks(&content);
//! let updated = upsert_managed_block(&cleaned, &new_block);
//! ```

use crate::error::SlateError;
use atomic_write_file::AtomicWriteFile;
use std::fs;
use std::io::Write;
use std::path::Path;

/// Start marker for managed blocks
pub const START: &str = "# slate:start — managed by slate, do not edit";

/// End marker for managed blocks
pub const END: &str = "# slate:end";

/// Count occurrences of a marker in content
fn count_marker(content: &str, marker: &str) -> usize {
    content.matches(marker).count()
}

fn count_marker_bytes(content: &[u8], marker: &[u8]) -> usize {
    if marker.is_empty() || content.len() < marker.len() {
        return 0;
    }
    content
        .windows(marker.len())
        .filter(|w| *w == marker)
        .count()
}

/// Validate marker block state before modification.
/// Accepts only valid states:
/// - (0 start + 0 end) — no existing managed block
/// - (1 start + 1 end) — exactly one managed block
/// Rejects any other combination with helpful error message including
/// marker counts and recovery instructions.
pub fn validate_block_state(content: &str) -> Result<(), SlateError> {
    let start_count = count_marker(content, START);
    let end_count = count_marker(content, END);

    match (start_count, end_count) {
        (0, 0) | (1, 1) => Ok(()),
        _ => Err(SlateError::InvalidConfig(format!(
            "Marker block state corrupted: found {} START markers and {} END markers. \
Expected either (0, 0) or (1, 1). \
Run: grep -n 'slate:' <config-file> to diagnose. \
Recovery: Remove markers manually or restore from backup.",
            start_count, end_count
        ))),
    }
}

fn validate_block_state_bytes(content: &[u8]) -> Result<(), SlateError> {
    let start_count = count_marker_bytes(content, START.as_bytes());
    let end_count = count_marker_bytes(content, END.as_bytes());

    match (start_count, end_count) {
        (0, 0) | (1, 1) => Ok(()),
        _ => Err(SlateError::InvalidConfig(format!(
            "Marker block state corrupted: found {} START markers and {} END markers. \
Expected either (0, 0) or (1, 1). \
Run: grep -n 'slate:' <config-file> to diagnose. \
Recovery: Remove markers manually or restore from backup.",
            start_count, end_count
        ))),
    }
}

/// Remove all content between START and END markers.
/// Handles multiple blocks (all removed). Idempotent: running twice on
/// the same input produces the same output.
pub fn strip_managed_blocks(content: &str) -> String {
    let mut cleaned = String::with_capacity(content.len());
    let mut remaining = content;

    while let Some(start) = remaining.find(START) {
        cleaned.push_str(&remaining[..start]);

        let block_tail = &remaining[start..];
        let Some(end_rel) = block_tail.find(END) else {
            remaining = "";
            break;
        };

        let after_end = start + end_rel + END.len();
        remaining = &remaining[after_end..];

        // Skip trailing newline after end marker (handle CRLF and LF)
        if let Some(rest) = remaining.strip_prefix("\r\n") {
            remaining = rest;
        } else if let Some(rest) = remaining.strip_prefix('\n') {
            remaining = rest;
        }
    }

    cleaned.push_str(remaining);
    cleaned
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn strip_managed_blocks_bytes(content: &[u8]) -> Vec<u8> {
    let mut cleaned = Vec::with_capacity(content.len());
    let mut remaining = content;

    while let Some(start) = find_subslice(remaining, START.as_bytes()) {
        cleaned.extend_from_slice(&remaining[..start]);

        let block_tail = &remaining[start..];
        let Some(end_rel) = find_subslice(block_tail, END.as_bytes()) else {
            remaining = &[];
            break;
        };

        let after_end = start + end_rel + END.len();
        remaining = &remaining[after_end..];

        if let Some(rest) = remaining.strip_prefix(b"\r\n") {
            remaining = rest;
        } else if let Some(rest) = remaining.strip_prefix(b"\n") {
            remaining = rest;
        }
    }

    cleaned.extend_from_slice(remaining);
    cleaned
}

/// Strip old block and append new block at EOF with proper newline handling.
/// Idempotent: calling with the same block twice produces the same result.
pub fn upsert_managed_block(content: &str, block: &str) -> String {
    let mut cleaned = strip_managed_blocks(content);

    // Ensure single newline before block
    if !cleaned.is_empty() && !cleaned.ends_with('\n') {
        cleaned.push('\n');
    }

    cleaned.push_str(block);

    // Ensure single newline at EOF
    if !cleaned.ends_with('\n') {
        cleaned.push('\n');
    }

    cleaned
}

fn upsert_managed_block_bytes(content: &[u8], block: &[u8]) -> Vec<u8> {
    let mut cleaned = strip_managed_blocks_bytes(content);

    if !cleaned.is_empty() && !cleaned.ends_with(b"\n") {
        cleaned.push(b'\n');
    }

    cleaned.extend_from_slice(block);

    if !cleaned.ends_with(b"\n") {
        cleaned.push(b'\n');
    }

    cleaned
}

fn write_atomic(path: &Path, content: &[u8]) -> Result<(), SlateError> {
    let mut file = AtomicWriteFile::open(path)?;
    file.write_all(content)?;
    file.commit()?;
    Ok(())
}

/// Upsert a managed block in a file, creating the file if it does not exist.
pub fn upsert_managed_block_file(path: &Path, block: &str) -> Result<(), SlateError> {
    let content = if path.exists() {
        fs::read(path)?
    } else {
        Vec::new()
    };

    validate_block_state_bytes(&content)?;
    let updated = upsert_managed_block_bytes(&content, block.as_bytes());
    write_atomic(path, &updated)
}

/// Remove managed blocks from a file if it exists.
pub fn remove_managed_blocks_from_file(path: &Path) -> Result<(), SlateError> {
    if !path.exists() {
        return Ok(());
    }

    let content = fs::read(path)?;
    validate_block_state_bytes(&content)?;
    let cleaned = strip_managed_blocks_bytes(&content);
    write_atomic(path, &cleaned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_strip_managed_blocks_removes_exactly_one_block() {
        let content = "# Comment\n# slate:start — managed by slate, do not edit\ntheme = mocha\n# slate:end\n# User config\n";
        let result = strip_managed_blocks(content);
        assert_eq!(result, "# Comment\n# User config\n");
        assert!(!result.contains("theme = mocha"));
        assert!(!result.contains("slate:start"));
        assert!(!result.contains("slate:end"));
    }

    #[test]
    fn test_upsert_managed_block_adds_block_when_none_exists() {
        let content = "# User config\n";
        let block = "# slate:start — managed by slate, do not edit\ntheme = mocha\n# slate:end";
        let result = upsert_managed_block(content, block);

        assert!(result.contains("# User config"));
        assert!(result.contains("theme = mocha"));
        assert!(result.contains("slate:start"));
        assert!(result.contains("slate:end"));
    }

    #[test]
    fn test_upsert_managed_block_replaces_existing_block_idempotently() {
        let content = "# User config\n# slate:start — managed by slate, do not edit\nold = value\n# slate:end\n";
        let block = "# slate:start — managed by slate, do not edit\nnew = value\n# slate:end";

        let result1 = upsert_managed_block(content, block);
        let result2 = upsert_managed_block(&result1, block);

        assert_eq!(result1, result2);
        assert!(result1.contains("new = value"));
        assert!(!result1.contains("old = value"));
    }

    #[test]
    fn test_upsert_managed_block_handles_crlf_line_endings() {
        let content = "# User config\r\n# slate:start — managed by slate, do not edit\r\nold = value\r\n# slate:end\r\n";
        let block = "# slate:start — managed by slate, do not edit\r\nnew = value\r\n# slate:end";

        let result = upsert_managed_block(content, block);

        // Should strip CRLF and only have newline after marker
        assert!(result.contains("new = value"));
        assert!(!result.contains("old = value"));
    }

    #[test]
    fn test_validate_block_state_accepts_zero_zero() {
        let content = "# No markers here\n";
        assert!(validate_block_state(content).is_ok());
    }

    #[test]
    fn test_validate_block_state_accepts_one_one() {
        let content = "# slate:start — managed by slate, do not edit\ndata\n# slate:end\n";
        assert!(validate_block_state(content).is_ok());
    }

    #[test]
    fn test_validate_block_state_rejects_two_one() {
        let content = "# slate:start — managed by slate, do not edit\ndata\n# slate:start — managed by slate, do not edit\nmore\n# slate:end\n";
        let result = validate_block_state(content);

        assert!(result.is_err());
        if let Err(SlateError::InvalidConfig(msg)) = result {
            assert!(msg.contains("found 2 START markers"));
            assert!(msg.contains("1 END markers"));
            assert!(msg.contains("grep -n 'slate:'"));
        }
    }

    #[test]
    fn upsert_managed_block_file_handles_non_utf8_prefix_bytes() {
        let td = TempDir::new().unwrap();
        let path = td.path().join("init.lua");
        fs::write(&path, [0xff, 0xfe, b'\n']).unwrap();

        let block = "-- # slate:start — managed by slate, do not edit\npcall(require, 'slate')\n-- # slate:end";
        upsert_managed_block_file(&path, block).unwrap();

        let updated = fs::read(&path).unwrap();
        assert!(updated.starts_with(&[0xff, 0xfe, b'\n']));
        assert!(
            updated.windows(START.len()).any(|w| w == START.as_bytes()),
            "marker must be appended even when file is not UTF-8"
        );
    }

    #[test]
    fn remove_managed_blocks_from_file_handles_non_utf8_bytes() {
        let td = TempDir::new().unwrap();
        let path = td.path().join("init.lua");
        let mut seed = vec![0xff, b'\n'];
        seed.extend_from_slice(
            format!("-- {}\npcall(require, 'slate')\n-- {}\n", START, END).as_bytes(),
        );
        seed.extend_from_slice(b"user-tail\n");
        fs::write(&path, seed).unwrap();

        remove_managed_blocks_from_file(&path).unwrap();

        let cleaned = fs::read(&path).unwrap();
        assert_eq!(&cleaned[..2], &[0xff, b'\n']);
        assert!(
            !cleaned.windows(START.len()).any(|w| w == START.as_bytes()),
            "marker must be stripped even when file is not UTF-8"
        );
        assert!(cleaned.ends_with(b"user-tail\n"));
    }
}
