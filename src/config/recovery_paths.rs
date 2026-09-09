//! Path contract for operation checkpoints that must be restorable file-for-file.
//! Metadata checks are not a lock against unrelated editors or directory changes.
use super::{OriginalFileState, RestoreEntry};
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

fn blocked(context: &str, path: &Path, reason: &str) -> SlateError {
    SlateError::InvalidConfig(format!("{context}: {}: {reason}", path.display()))
}

pub(crate) fn validate_storage_paths(env: &SlateEnv, operation: &str) -> Result<()> {
    let context = format!("{operation} cancelled before applying settings");
    for path in [
        env.config_dir().to_owned(),
        env.slate_cache_dir().to_owned(),
        env.slate_cache_dir().join("backups"),
    ] {
        validate_path(env, &path, true, &context)?;
    }
    Ok(())
}

fn resolved(path: &Path, context: &str) -> Result<PathBuf> {
    for ancestor in path.ancestors() {
        match fs::canonicalize(ancestor) {
            Ok(root) => return Ok(root.join(path.strip_prefix(ancestor).expect("ancestor"))),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err(blocked(context, path, "cannot resolve path")),
        }
    }
    Err(blocked(context, path, "cannot resolve path"))
}

fn validate_path(env: &SlateEnv, path: &Path, directory: bool, context: &str) -> Result<()> {
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(blocked(
            context,
            path,
            "use an absolute path without '..' for file recovery",
        ));
    }
    // ENOENT is not enough to distinguish a missing entry from a dangling
    // directory link. Check ancestors before permitting later create_dir_all.
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(meta) if ancestor == path && meta.file_type().is_symlink() => {
                return Err(blocked(context, path, "a final symlink cannot be restored safely; preserve or relocate the link first"));
            }
            Ok(_)
                if (ancestor != path || directory)
                    && !fs::metadata(ancestor).is_ok_and(|m| m.is_dir()) =>
            {
                return Err(blocked(
                    context,
                    path,
                    "parent/storage path is not a directory",
                ));
            }
            Ok(meta) if ancestor == path && !directory && !meta.is_file() => {
                return Err(blocked(context, path, "expected a regular file"));
            }
            Ok(_) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(blocked(context, path, "cannot inspect path")),
        }
    }
    if env.session().is_isolated()
        && !resolved(path, context)?.starts_with(resolved(env.home(), context)?)
    {
        return Err(blocked(
            context,
            path,
            "path escapes the isolated SLATE_HOME",
        ));
    }
    Ok(())
}

/// Metadata-only validation for a single inspected file. Unlike full write
/// preflight, a broken backup directory must not hide readable configuration.
pub(crate) fn validate_file_path(env: &SlateEnv, path: &Path, label: &str) -> Result<()> {
    validate_path(env, path, false, &format!("Cannot inspect {label} file"))
}

/// Retain the selected path spelling, deduplicate directory aliases, and reject
/// overlap with the backup store. The snapshot fills in bytes/modes/absence.
pub(crate) fn targets(
    env: &SlateEnv,
    files: impl IntoIterator<Item = PathBuf>,
    operation: &str,
) -> Result<Vec<RestoreEntry>> {
    validate_storage_paths(env, operation)?;
    let context = format!("{operation} cancelled before applying settings");
    let backups = resolved(&env.slate_cache_dir().join("backups"), &context)?;
    let mut unique = BTreeSet::new();
    let mut targets = Vec::new();
    let key = operation.to_ascii_lowercase();
    for path in files {
        validate_path(env, &path, false, &context)?;
        let canonical = resolved(&path, &context)?;
        if canonical.starts_with(&backups) || backups.starts_with(&canonical) {
            return Err(blocked(
                &context,
                &path,
                "configuration overlaps its recovery storage",
            ));
        }
        if unique.insert(canonical) {
            targets.push(RestoreEntry {
                tool_key: format!("{key}-{:04}", targets.len()),
                display_tool: format!("Pre-{key} files"),
                original_path: path,
                backup_path: None,
                original_state: OriginalFileState::Absent,
                unix_mode: None,
            });
        }
    }
    Ok(targets)
}
