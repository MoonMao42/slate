use crate::error::{Result, SlateError};
use atomic_write_file::AtomicWriteFile;
use std::fs;
use std::io::Write;
use std::path::Path;

pub(super) fn read_optional_state_file(path: &Path) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(path)?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    Ok(Some(trimmed.to_string()))
}

/// Atomic write with parent-directory fsync.
/// Single source of truth for `AtomicWriteFile::open + write_all + commit`
/// across slate. ALL atomic-write call sites in `src/` route through this so
/// the parent-dir fsync invariant cannot drift.
/// Behaviour:
/// 1. Refuses to write through symlinks (defence against symlink attacks on
/// user-owned dirs; `~/.config/slate/managed/...` is the typical target).
/// 2. Atomically writes via `AtomicWriteFile` (temp file + fsync + rename).
/// 3. After `commit()`, opens the parent directory and calls `sync_all()` to
/// flush the macOS APFS dirent cache. Without this step, `commit()` returns
/// Ok but immediate readers can observe the previous file via the stale
/// dirent (DELTA-01 symptom).
/// The parent-dir fsync is best-effort: errors are logged via `eprintln!`
/// and swallowed.
// WHY swallow parent-dir fsync errors: on some platforms / mount points
// (e.g., Windows, read-only `/`), `sync_all()` on a directory handle may
// return EACCES or ERROR_ACCESS_DENIED. The data is already safely on
// disk by `commit()` time; the directory fsync only flushes the dirent
// cache for immediate readers on macOS APFS. Propagating the error would
// turn a portability nuisance into a fatal write failure.
pub(crate) fn atomic_write_synced(path: &Path, contents: &[u8]) -> Result<()> {
    if path.exists() && fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err(SlateError::InvalidConfig(format!(
            "Refusing to write through symlink: {}",
            path.display()
        )));
    }

    let mut file = AtomicWriteFile::open(path)?;
    file.write_all(contents)?;
    file.commit()?;

    if let Some(parent) = path.parent() {
        if let Err(err) = fs::File::open(parent).and_then(|f| f.sync_all()) {
            // Best-effort: data is already on disk after commit(). The
            // parent-dir fsync only flushes the APFS dirent cache so
            // immediate readers see the new file. See WHY note above.
            eprintln!(
                "warning: parent-dir fsync failed for {}: {}",
                parent.display(),
                err
            );
        }
    }

    Ok(())
}

pub(super) fn write_state_file(path: &Path, content: &str) -> Result<()> {
    atomic_write_synced(path, content.as_bytes())
}

pub(super) fn write_managed_file(dir: &Path, filename: &str, content: &str) -> Result<()> {
    fs::create_dir_all(dir)?;

    let canonical_dir = fs::canonicalize(dir)?;
    let path = canonical_dir.join(filename);

    atomic_write_synced(&path, content.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use tempfile::TempDir;

    #[test]
    fn write_managed_file_round_trip_byte_equal() {
        let td = TempDir::new().unwrap();
        let dir = td.path().join("managed/delta");
        let content = "[delta]\nsyntax-theme = test\n";

        write_managed_file(&dir, "colors", content).unwrap();

        let path = fs::canonicalize(&dir).unwrap().join("colors");
        let read_back = fs::read_to_string(&path).unwrap();
        assert_eq!(read_back, content);
    }

    #[test]
    fn atomic_write_synced_refuses_symlink_targets() {
        let td = TempDir::new().unwrap();
        let real_target = td.path().join("real.txt");
        fs::write(&real_target, "original").unwrap();

        let symlink_path = td.path().join("link.txt");
        symlink(&real_target, &symlink_path).unwrap();

        let result = atomic_write_synced(&symlink_path, b"new content");
        assert!(matches!(result, Err(SlateError::InvalidConfig(_))));

        // Ensure the symlink target was NOT modified.
        let unchanged = fs::read_to_string(&real_target).unwrap();
        assert_eq!(unchanged, "original");
    }
}
