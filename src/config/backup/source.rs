//! Snapshot budgets around the shared regular-file reader. Strict checkpoints
//! refuse final links; ordinary snapshots preserve linked-dotfile read support.
use crate::config::file_read::{self, Links, ReadError};
use crate::error::{Result, SlateError};
use std::path::Path;

pub(super) const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
pub(crate) const MAX_SNAPSHOT_BYTES: u64 = 64 * 1024 * 1024;

pub(super) use file_read::Source;

fn blocked(path: &Path, reason: &str) -> SlateError {
    SlateError::BackupFailed(format!("Cannot capture {}: {reason}", path.display()))
}

pub(crate) fn read(path: &Path, remaining: &mut u64) -> Result<Option<Source>> {
    read_with_links(path, remaining, Links::Reject)
}

pub(super) fn read_with_links(
    path: &Path,
    remaining: &mut u64,
    links: Links,
) -> Result<Option<Source>> {
    let source =
        file_read::read(path, MAX_FILE_BYTES.min(*remaining), links).map_err(|err| match err {
            ReadError::TooLarge(_) => blocked(
                path,
                "checkpoint limit exceeded (8 MiB per file, 64 MiB total)",
            ),
            ReadError::Unsafe(reason) => blocked(path, reason),
        })?;
    if let Some(source) = &source {
        *remaining -= source.bytes.len() as u64;
    }
    Ok(source)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn import_snapshot_source_retains_binary_bytes_and_enforces_remaining_budget() {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("config");
        let bytes = [0xff, 0, b'\n', b'x'];
        fs::write(&path, bytes).unwrap();
        let mut remaining = 6;
        let source = read(&path, &mut remaining).unwrap().unwrap();
        assert_eq!(source.bytes, bytes);
        assert_eq!(remaining, 2);
        assert!(
            matches!(read(&path, &mut remaining), Err(error) if error.to_string().contains("checkpoint limit"))
        );
        assert_eq!(
            remaining, 2,
            "failed reads must not consume the remaining budget"
        );
        assert!(read(&home.path().join("absent"), &mut remaining)
            .unwrap()
            .is_none());
        fs::write(&path, []).unwrap();
        remaining = 0;
        assert!(read(&path, &mut remaining)
            .unwrap()
            .unwrap()
            .bytes
            .is_empty());
    }
}
