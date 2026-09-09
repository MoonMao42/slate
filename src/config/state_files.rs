use super::file_read::{read_text, MAX_STATE_BYTES};
use crate::error::{Result, SlateError};
use atomic_write_file::AtomicWriteFile;
use std::fs;
use std::io::Write;
use std::path::Path;

pub(super) fn read_optional_state_file(path: &Path) -> Result<Option<String>> {
    let Some(content) = read_text(path, MAX_STATE_BYTES)? else {
        return Ok(None);
    };
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
    atomic_write_synced_mode(path, contents, None)
}

/// Apply saved permissions to the temporary file before publication, not after
/// rename. Explicit-mode writes start private even when replacing a public file.
pub(crate) fn atomic_write_synced_mode(
    path: &Path,
    contents: &[u8],
    mode: Option<u32>,
) -> Result<()> {
    let ticket = super::preview_write::prepare(path, contents.len())?;
    let is_link = match fs::symlink_metadata(path) {
        Ok(meta) => meta.file_type().is_symlink(),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => false,
        Err(err) => return Err(err.into()),
    };
    if is_link {
        return Err(SlateError::InvalidConfig(format!(
            "Refusing to write through symlink: {}",
            path.display()
        )));
    }

    let mut options = AtomicWriteFile::options();
    #[cfg(unix)]
    if mode.is_some() {
        use atomic_write_file::unix::OpenOptionsExt as _;
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600).preserve_mode(false);
    }
    let mut file = options.open(path)?;
    file.write_all(contents)?;
    #[cfg(unix)]
    if let Some(mode) = mode {
        use std::os::unix::fs::PermissionsExt;
        file.as_file()
            .set_permissions(fs::Permissions::from_mode(mode))?;
    }
    let written_mode = if let Some(ticket) = &ticket {
        use std::os::unix::fs::PermissionsExt;
        ticket.verify()?;
        Some(file.as_file().metadata()?.permissions().mode())
    } else {
        None
    };
    file.commit()?;
    if let Some(ticket) = ticket {
        ticket.committed(
            contents,
            written_mode.expect("tracked publication captured its mode"),
        )?;
    }

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
    if content.len() as u64 > MAX_STATE_BYTES {
        return Err(SlateError::ConfigWriteError(
            path.display().to_string(),
            "state exceeds 4 KiB limit".into(),
        ));
    }
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

        fs::remove_file(&real_target).unwrap();
        assert!(atomic_write_synced(&symlink_path, b"must not create target").is_err());
        assert!(!real_target.exists());
    }

    #[test]
    fn atomic_write_synced_mode_publishes_explicit_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let td = TempDir::new().unwrap();
        let path = td.path().join("private-config");
        for mode in [0o600, 0o755, 0o000, 0o640] {
            atomic_write_synced_mode(&path, b"fixture", Some(mode)).unwrap();
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                mode
            );
        }
        assert_eq!(fs::read(path).unwrap(), b"fixture");
    }
}
