//! Read-only history inventory. Individual bad entries do not hide healthy
//! records; a failed directory scan is an error, never a partial success report.
use super::{get_restore_point_with_env, RestorePoint};
use crate::config::backup::{time, validate_restore_point_id};
use crate::config::file_read;
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use serde::Serialize;
use std::fs;
use std::path::Path;

#[derive(Debug)]
pub struct RestoreInventory {
    pub backup_directory: std::path::PathBuf,
    pub points: Vec<RestorePoint>,
    pub issues: Vec<RestoreInventoryIssue>,
    /// Legacy per-tool backups and entries that do not identify a restore point.
    pub ignored_entries: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RestoreInventoryIssueKind {
    LinkedEntry,
    MissingManifest,
    InvalidId,
    InvalidRecord,
    UnreadableEntry,
}

#[derive(Debug, Serialize)]
pub struct RestoreInventoryIssue {
    /// Only populated when this name can safely be passed as an exact CLI ID.
    pub id: Option<String>,
    pub entry_name: String,
    /// Display-only; non-UTF-8 filenames are explicitly marked as lossy.
    pub path: String,
    pub path_is_lossy: bool,
    pub kind: RestoreInventoryIssueKind,
    pub message: String,
    pub next_step: String,
}

fn issue(path: &Path, kind: RestoreInventoryIssueKind, message: String) -> RestoreInventoryIssue {
    let name = path.file_name().unwrap_or_default();
    let id = name
        .to_str()
        .filter(|id| validate_restore_point_id(id).is_ok())
        .map(str::to_owned);
    let next_step = id.as_ref().map_or_else(
        || "Inspect this backup entry manually; its name is not a valid restore ID.".into(),
        |id| format!("Inspect with: slate restore '{id}' --dry-run"),
    );
    RestoreInventoryIssue {
        id,
        entry_name: name.to_string_lossy().into_owned(),
        path: path.display().to_string(),
        path_is_lossy: path.to_str().is_none(),
        kind,
        message,
        next_step,
    }
}

fn snapshot_shaped_name(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.get(..20)
                .is_some_and(|prefix| time::timestamp_from_string(prefix).is_ok())
                && (name.len() == 20 || name.as_bytes().get(20) == Some(&b'-'))
        })
}

pub fn inspect_restore_points_with_env(env: &SlateEnv) -> Result<RestoreInventory> {
    let backup_directory = env.slate_cache_dir().join("backups");
    let mut inventory = RestoreInventory {
        backup_directory,
        points: Vec::new(),
        issues: Vec::new(),
        ignored_entries: 0,
    };
    let scan_error = |reason: String| {
        SlateError::BackupFailed(format!(
            "Cannot inspect recovery directory {}: {reason}",
            inventory.backup_directory.display()
        ))
    };
    match fs::metadata(&inventory.backup_directory) {
        Ok(meta) if meta.is_dir() => {}
        Ok(_) => return Err(scan_error("expected a directory".into())),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            file_read::confirm_missing(&inventory.backup_directory)
                .map_err(|err| scan_error(err.to_string()))?;
            return Ok(inventory);
        }
        Err(err) => return Err(scan_error(err.to_string())),
    }
    let entries =
        fs::read_dir(&inventory.backup_directory).map_err(|err| scan_error(err.to_string()))?;
    for entry in entries {
        let entry = entry.map_err(|err| scan_error(err.to_string()))?;
        let path = entry.path();
        let kind = match entry.file_type() {
            Ok(kind) => kind,
            Err(_) => {
                inventory.issues.push(issue(
                    &path,
                    RestoreInventoryIssueKind::UnreadableEntry,
                    "Cannot inspect backup entry type; retry the listing.".into(),
                ));
                continue;
            }
        };
        if kind.is_symlink() {
            inventory.issues.push(issue(
                &path,
                RestoreInventoryIssueKind::LinkedEntry,
                "Linked backup entries are not opened as restore points.".into(),
            ));
            continue;
        }
        if !kind.is_dir() {
            inventory.ignored_entries += 1;
            continue;
        }
        // Missing manifests in legacy per-tool backup directories are normal.
        // Timestamp-shaped directories may be interrupted captures or damaged
        // records; neither becomes a usable point until its manifest exists.
        match fs::symlink_metadata(path.join("manifest.toml")) {
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                if snapshot_shaped_name(&path) {
                    inventory.issues.push(issue(&path, RestoreInventoryIssueKind::MissingManifest, "No manifest: this may be an incomplete capture or a damaged record. It is not a usable restore point.".into()));
                } else {
                    inventory.ignored_entries += 1;
                }
                continue;
            }
            Err(_) => {
                inventory.issues.push(issue(
                    &path,
                    RestoreInventoryIssueKind::UnreadableEntry,
                    "Cannot inspect this entry's manifest.".into(),
                ));
                continue;
            }
            Ok(_) => {}
        }
        let name = entry.file_name();
        let Some(id) = name
            .to_str()
            .filter(|id| validate_restore_point_id(id).is_ok())
        else {
            inventory.issues.push(issue(
                &path,
                RestoreInventoryIssueKind::InvalidId,
                "Directory name is not a valid restore ID.".into(),
            ));
            continue;
        };
        match get_restore_point_with_env(env, id) {
            Ok(point) => inventory.points.push(point),
            Err(err) => inventory.issues.push(issue(
                &path,
                RestoreInventoryIssueKind::InvalidRecord,
                err.to_string(),
            )),
        }
    }
    inventory.points.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| a.id.cmp(&b.id))
    });
    inventory.issues.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(inventory)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStringExt;

    #[test]
    fn inventory_non_utf8_names_are_display_only_never_lossy_restore_ids() {
        let path = std::path::PathBuf::from("/fixture")
            .join(std::ffi::OsString::from_vec(b"invalid-\xff".to_vec()));
        let issue = issue(
            &path,
            RestoreInventoryIssueKind::InvalidId,
            "invalid name".into(),
        );
        assert!(issue.id.is_none());
        assert!(issue.path_is_lossy);
        assert!(!issue.next_step.contains("slate restore"));
        let json = serde_json::to_value(issue).unwrap();
        assert!(json["path"].as_str().unwrap().contains('\u{fffd}'));
    }
}
