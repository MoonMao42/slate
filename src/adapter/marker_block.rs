//! Marker block utilities for managed config modifications.
//! This module provides a reusable system for safely inserting and removing blocks
//! from configuration files. Marker blocks are delimited by START and END markers,
//! allowing slate to regenerate theme-specific configuration without destroying
//! user customizations outside the managed block.
//! Markers must occupy complete, matching Shell/TOML, Lua or Vim comment lines.
//! File edits reject ambiguous/reversed/incomplete blocks before writing. Legacy
//! String-returning helpers leave invalid input unchanged; validate first to report
//! the error instead of silently keeping the original.
//! ## Usage Pattern
//! ```ignore
//! // ALWAYS validate before modifying
//! validate_block_state(&content)?;
//! // Safe to strip or upsert
//! let cleaned = strip_managed_blocks(&content);
//! let updated = upsert_managed_block(&cleaned, &new_block);
//! ```

use crate::error::SlateError;
use std::fs;
use std::ops::Range;
use std::path::Path;

/// Start marker for managed blocks
pub const START: &str = "# slate:start — managed by slate, do not edit";

/// End marker for managed blocks
pub const END: &str = "# slate:end";

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
///
/// A pair must be on standalone lines with matching comment wrappers and START
/// before END. Rejects any other shape without including configuration contents.
pub fn validate_block_state(content: &str) -> Result<(), SlateError> {
    validate_block_state_bytes(content.as_bytes())
}

/// Preserve opaque bytes outside the marker lines when inspecting shell files.
pub(crate) fn validate_block_state_bytes(content: &[u8]) -> Result<(), SlateError> {
    validated_block_range(content).map(|_| ())
}

fn invalid_block(start_count: usize, end_count: usize, reason: &str) -> SlateError {
    SlateError::InvalidConfig(format!(
        "Marker block state corrupted: found {start_count} START markers and {end_count} END markers. \
{reason} Run: grep -n 'slate:' <config-file> to diagnose. \
Recovery: Inspect the marker lines manually or restore from a trusted backup."
    ))
}

#[derive(PartialEq, Eq)]
enum CommentStyle {
    Hash,
    Lua,
    Vim,
}

fn trim_horizontal(mut bytes: &[u8]) -> &[u8] {
    while matches!(bytes.first(), Some(b' ' | b'\t')) {
        bytes = &bytes[1..];
    }
    while matches!(bytes.last(), Some(b' ' | b'\t')) {
        bytes = &bytes[..bytes.len() - 1];
    }
    bytes
}

fn marker_line(
    content: &[u8],
    position: usize,
    length: usize,
) -> Option<(Range<usize>, CommentStyle)> {
    let start = content[..position]
        .iter()
        .rposition(|b| *b == b'\n')
        .map_or(0, |position| position + 1);
    let end = content[position..]
        .iter()
        .position(|b| *b == b'\n')
        .map_or(content.len(), |offset| position + offset + 1);
    let style = match trim_horizontal(&content[start..position]) {
        b"" => CommentStyle::Hash,
        b"--" => CommentStyle::Lua,
        b"\"" => CommentStyle::Vim,
        _ => return None,
    };
    let suffix = &content[position + length..end];
    let suffix = suffix.strip_suffix(b"\n").unwrap_or(suffix);
    let suffix = suffix.strip_suffix(b"\r").unwrap_or(suffix);
    trim_horizontal(suffix)
        .is_empty()
        .then_some((start..end, style))
}

fn validated_block_range(content: &[u8]) -> Result<Option<Range<usize>>, SlateError> {
    let start_count = count_marker_bytes(content, START.as_bytes());
    let end_count = count_marker_bytes(content, END.as_bytes());
    match (start_count, end_count) {
        (0, 0) => return Ok(None),
        (1, 1) => {}
        _ => {
            return Err(invalid_block(
                start_count,
                end_count,
                "Expected either (0, 0) or (1, 1).",
            ))
        }
    }
    let start = find_subslice(content, START.as_bytes()).expect("counted START");
    let end = find_subslice(content, END.as_bytes()).expect("counted END");
    if let (Some((start_line, start_style)), Some((end_line, end_style))) = (
        marker_line(content, start, START.len()),
        marker_line(content, end, END.len()),
    ) {
        if start_line.end <= end_line.start && start_style == end_style {
            return Ok(Some(start_line.start..end_line.end));
        }
    }
    Err(invalid_block(
        start_count,
        end_count,
        "Expected standalone, matching comment lines with START before END.",
    ))
}

/// Remove one valid managed block, including its complete comment marker lines.
/// Invalid or multiple blocks leave the original unchanged. Call
/// `validate_block_state` to report the error. Valid removal is idempotent.
pub fn strip_managed_blocks(content: &str) -> String {
    if let Ok(Some(range)) = validated_block_range(content.as_bytes()) {
        format!("{}{}", &content[..range.start], &content[range.end..])
    } else {
        content.to_owned()
    }
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

pub(crate) fn strip_managed_blocks_bytes(content: &[u8]) -> Result<Vec<u8>, SlateError> {
    let Some(range) = validated_block_range(content)? else {
        return Ok(content.to_vec());
    };
    let mut cleaned = Vec::with_capacity(content.len() - range.len());
    cleaned.extend_from_slice(&content[..range.start]);
    cleaned.extend_from_slice(&content[range.end..]);
    Ok(cleaned)
}

fn validate_new_block(block: &[u8]) -> Result<Range<usize>, SlateError> {
    if let Some(range) = validated_block_range(block)? {
        if block[..range.start].iter().all(u8::is_ascii_whitespace)
            && block[range.end..].iter().all(u8::is_ascii_whitespace)
        {
            return Ok(range);
        }
    }
    Err(invalid_block(
        count_marker_bytes(block, START.as_bytes()),
        count_marker_bytes(block, END.as_bytes()),
        "Replacement must contain exactly one managed block and no code outside it.",
    ))
}

/// Strip old block and append new block at EOF with proper newline handling.
/// Idempotent: calling with the same valid block twice produces the same result.
/// Invalid original or replacement blocks leave the original unchanged.
/// Blank padding outside the replacement's marker lines is not appended.
pub fn upsert_managed_block(content: &str, block: &str) -> String {
    upsert_managed_block_bytes(content.as_bytes(), block.as_bytes())
        .map(|bytes| String::from_utf8(bytes).expect("complete UTF-8 lines and replacement"))
        .unwrap_or_else(|_| content.to_owned())
}

pub(crate) fn upsert_managed_block_bytes(
    content: &[u8],
    block: &[u8],
) -> Result<Vec<u8>, SlateError> {
    let block_range = validate_new_block(block)?;
    let block = &block[block_range];
    let mut cleaned = strip_managed_blocks_bytes(content)?;

    if !cleaned.is_empty() && !cleaned.ends_with(b"\n") {
        cleaned.push(b'\n');
    }

    cleaned.extend_from_slice(block);

    if !cleaned.ends_with(b"\n") {
        cleaned.push(b'\n');
    }

    Ok(cleaned)
}

fn write_atomic(path: &Path, content: &[u8]) -> Result<(), SlateError> {
    crate::config::atomic_write_synced(path, content)
}

/// Upsert a managed block in a file, creating the file if it does not exist.
pub fn upsert_managed_block_file(path: &Path, block: &str) -> Result<(), SlateError> {
    let content = if path.exists() {
        fs::read(path)?
    } else {
        Vec::new()
    };

    let updated = upsert_managed_block_bytes(&content, block.as_bytes())?;
    if updated != content {
        write_atomic(path, &updated)?;
    }
    Ok(())
}

/// Remove managed blocks from a file if it exists.
pub fn remove_managed_blocks_from_file(path: &Path) -> Result<(), SlateError> {
    if !path.exists() {
        return Ok(());
    }

    let content = fs::read(path)?;
    let cleaned = strip_managed_blocks_bytes(&content)?;
    if cleaned != content {
        write_atomic(path, &cleaned)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn marker_boundaries_reject_reversed_inline_and_mixed_comment_markers() {
        for content in [
            format!("{END}\nPRIVATE_BEFORE\n{START}\nPRIVATE_TAIL\n"),
            format!("printf '{START}'\nPRIVATE_BODY\nprintf '{END}'\n"),
            format!("{START}\nPRIVATE_BODY\n{END}less\nPRIVATE_TAIL\n"),
            format!("-- {START}\nPRIVATE_BODY\n\" {END}\nPRIVATE_TAIL\n"),
            format!("{START}\nPRIVATE_UNCLOSED_TAIL\n"),
            format!("{START}\n{END}\n{START}\n{END}\nPRIVATE_TAIL\n"),
        ] {
            let error = validate_block_state(&content).unwrap_err().to_string();
            assert!(!error.contains("PRIVATE_"));
            assert_eq!(strip_managed_blocks(&content), content);
            assert_eq!(
                upsert_managed_block(&content, &format!("{START}\nnew\n{END}")),
                content
            );
        }
    }

    #[test]
    fn marker_boundaries_remove_whole_comment_lines_and_upsert_idempotently() {
        for prefix in ["", "-- ", "\" "] {
            for newline in ["\n", "\r\n"] {
                let block = format!("  {prefix}{START}{newline}managed{newline}\t{prefix}{END}");
                let content = format!("user-before{newline}{block}{newline}user-after{newline}");
                validate_block_state(&content).unwrap();
                assert_eq!(
                    strip_managed_blocks(&content),
                    format!("user-before{newline}user-after{newline}")
                );
                let once = upsert_managed_block(&content, &block);
                assert_eq!(upsert_managed_block(&once, &block), once);
                let padded = format!("\n{block}\n \t\n");
                let once = upsert_managed_block(&content, &padded);
                assert_eq!(upsert_managed_block(&once, &padded), once);
            }
        }
    }

    #[test]
    fn marker_file_edits_reject_invalid_sources_and_replacements_without_writes() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let td = TempDir::new().unwrap();
        let path = td.path().join("init.lua");
        let valid = format!("-- {START}\nload_slate\n-- {END}");
        for malformed in [
            format!("{END}\n{START}\nPRIVATE_TAIL\n"),
            format!("{START}\nPRIVATE_UNCLOSED\n"),
            format!("print('{START}')\nprint('{END}')\n"),
        ] {
            let bytes = [b"\xff\n".as_slice(), malformed.as_bytes()].concat();
            fs::write(&path, &bytes).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
            let inode = fs::metadata(&path).unwrap().ino();
            assert!(remove_managed_blocks_from_file(&path).is_err());
            assert!(upsert_managed_block_file(&path, &valid).is_err());
            assert_eq!(fs::read(&path).unwrap(), bytes);
            assert_eq!(fs::metadata(&path).unwrap().ino(), inode);
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o640
            );
        }
        fs::write(&path, b"PRIVATE_USER_FILE\xff\n").unwrap();
        let inode = fs::metadata(&path).unwrap().ino();
        for replacement in [
            "missing markers".to_string(),
            format!("{END}\n{START}"),
            format!("unmanaged code\n{valid}"),
            format!("{valid}\nunmanaged code"),
        ] {
            assert!(upsert_managed_block_file(&path, &replacement).is_err());
            assert_eq!(fs::read(&path).unwrap(), b"PRIVATE_USER_FILE\xff\n");
            assert_eq!(fs::metadata(&path).unwrap().ino(), inode);
            let absent = td.path().join("absent");
            assert!(upsert_managed_block_file(&absent, &replacement).is_err());
            assert!(!absent.exists());
        }
    }

    #[test]
    fn marker_file_round_trips_preserve_binary_tails_modes_and_noop_identity() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let td = TempDir::new().unwrap();
        for prefix in ["", "-- ", "\" "] {
            let path = td.path().join("config");
            let before = b"PRIVATE_USER_CODE\xff\n\xfe\n";
            fs::write(&path, before).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
            let inode = fs::metadata(&path).unwrap().ino();
            remove_managed_blocks_from_file(&path).unwrap();
            assert_eq!(fs::metadata(&path).unwrap().ino(), inode);

            let block = format!("{prefix}{START}\r\nmanaged\r\n{prefix}{END}\r\n");
            upsert_managed_block_file(&path, &block).unwrap();
            let first = fs::read(&path).unwrap();
            let inode = fs::metadata(&path).unwrap().ino();
            upsert_managed_block_file(&path, &block).unwrap();
            assert_eq!(fs::read(&path).unwrap(), first);
            assert_eq!(fs::metadata(&path).unwrap().ino(), inode);
            remove_managed_blocks_from_file(&path).unwrap();
            assert_eq!(fs::read(&path).unwrap(), before);
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o640
            );
        }
    }

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
