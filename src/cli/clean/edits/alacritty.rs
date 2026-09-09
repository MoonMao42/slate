//! Remove only parser-identified import strings and their following commas.
//! Keeping the original byte slices preserves all comments, line endings, other
//! values, and empty arrays/tables without reserializing the user's document.
use super::{is_managed_path, Edit};
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use std::ops::Range;
use std::path::Path;

fn invalid() -> SlateError {
    SlateError::InvalidConfig("Invalid Alacritty TOML during clean; file contents omitted".into())
}

fn removals(
    array: &toml_edit::Array,
    bytes: &[u8],
    root: &Path,
    ranges: &mut Vec<Range<usize>>,
) -> Result<()> {
    let array_span = array.span().ok_or_else(invalid)?;
    let end = array_span.end.checked_sub(1).ok_or_else(invalid)?;
    let mut values = array.iter().peekable();
    while let Some(value) = values.next() {
        if !value
            .as_str()
            .is_some_and(|path| is_managed_path(path, root))
        {
            continue;
        }
        let span = value.span().ok_or_else(invalid)?;
        let next = match values.peek() {
            Some(value) => value.span().ok_or_else(invalid)?.start,
            None => end,
        };
        let gap = bytes.get(span.end..next).ok_or_else(invalid)?;
        // The parser has already validated this gap as whitespace/comments and
        // an optional comma. Commas inside comments are never delimiters.
        let mut in_comment = false;
        for (offset, byte) in gap.iter().enumerate() {
            match byte {
                b'\n' => in_comment = false,
                b'#' => in_comment = true,
                b',' if !in_comment => {
                    let comma = span.end + offset;
                    ranges.push(comma..comma + 1);
                    break;
                }
                _ => {}
            }
        }
        // If this was the final element without a following comma, leave its
        // predecessor's comma as a valid TOML trailing comma. No comment moves.
        ranges.push(span);
    }
    Ok(())
}

pub(super) fn clean(env: &SlateEnv, bytes: &[u8]) -> Result<Edit> {
    let Ok(content) = std::str::from_utf8(bytes) else {
        return Ok(Edit::Keep(Some(
            "Alacritty is not UTF-8; inspect its Slate imports manually",
        )));
    };
    let doc = toml_edit::ImDocument::parse(content).map_err(|_| invalid())?;
    let root = env.config_dir().join("managed/alacritty");
    let mut ranges = Vec::new();
    // Support the legacy root import and current general.import, including
    // dotted keys and inline tables, without migrating user configuration.
    for item in [
        doc.get("import"),
        doc.get("general").and_then(|v| v.get("import")),
    ]
    .into_iter()
    .flatten()
    {
        if let Some(array) = item.as_array() {
            removals(array, bytes, &root, &mut ranges)?;
        }
    }
    if ranges.is_empty() {
        return Ok(Edit::Keep(None));
    }
    drop(doc);
    ranges.sort_unstable_by_key(|span| span.start);
    let mut cleaned = Vec::with_capacity(bytes.len());
    let mut cursor = 0;
    for span in ranges {
        cleaned.extend_from_slice(bytes.get(cursor..span.start).ok_or_else(invalid)?);
        cursor = span.end;
    }
    cleaned.extend_from_slice(bytes.get(cursor..).ok_or_else(invalid)?);
    // Fail before mutation if a future parser/span change violates the editing
    // contract; errors never include the source document.
    let text = std::str::from_utf8(&cleaned).map_err(|_| invalid())?;
    toml_edit::ImDocument::parse(text).map_err(|_| invalid())?;
    Ok(Edit::replaced(bytes, cleaned))
}
