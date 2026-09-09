use super::{manifest_path, BackupSession, OriginalFileState, RestoreEntry, RestorePoint};
use crate::config::file_read::{self, Links};
use crate::config::state_files::atomic_write_synced_mode;
use crate::error::{Result, SlateError};
use std::fs;
use std::path::Path;

const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
pub(crate) const MAX_ENTRIES: usize = 512;

fn invalid_record(reason: &str) -> SlateError {
    SlateError::BackupFailed(format!("Invalid restore record: {reason}"))
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct RestoreManifestMetadata {
    pub id: String,
    pub theme_name: String,
    pub created_at: String,
    #[serde(default)]
    pub is_baseline: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct RestoreManifest {
    pub metadata: RestoreManifestMetadata,
    pub entries: Vec<RestoreEntry>,
}

pub(super) fn write_manifest_raw(manifest_path: &Path, manifest: &RestoreManifest) -> Result<()> {
    let point = manifest_to_restore_point(manifest.clone())?;
    validate_restore_point_data(&point)?;
    validate_record_location(manifest_path, &point)?;
    let content = toml::to_string_pretty(manifest)
        .map_err(|_| invalid_record("cannot serialize manifest.toml"))?;
    if content.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(invalid_record("manifest.toml exceeds 1 MiB limit"));
    }

    atomic_write_synced_mode(manifest_path, content.as_bytes(), Some(0o600))
        .map_err(|e| SlateError::BackupFailed(format!("Failed to write manifest.toml: {}", e)))
}

pub(crate) fn read_manifest_raw(manifest_path: &Path) -> Result<RestoreManifest> {
    let failure = |reason: String| {
        SlateError::BackupFailed(format!("Cannot read {}: {reason}", manifest_path.display()))
    };
    let source = file_read::read(manifest_path, MAX_MANIFEST_BYTES, Links::Reject)
        .map_err(|err| failure(err.to_string()))?
        .ok_or_else(|| failure("restore point not found".into()))?;
    let content =
        String::from_utf8(source.bytes).map_err(|_| failure("expected UTF-8 text".into()))?;
    toml::from_str(&content)
        .map_err(|_| failure("invalid manifest TOML or schema; saved contents omitted".into()))
}

fn manifest_to_restore_point(manifest: RestoreManifest) -> Result<RestorePoint> {
    Ok(RestorePoint {
        id: manifest.metadata.id,
        theme_name: manifest.metadata.theme_name,
        created_at: super::time::timestamp_from_string(&manifest.metadata.created_at)?,
        entries: manifest.entries,
        is_baseline: manifest.metadata.is_baseline,
    })
}

pub(crate) fn append_manifest_entry(session: &BackupSession, entry: &RestoreEntry) -> Result<()> {
    let manifest_path = manifest_path(&session.restore_point_dir);
    let mut manifest = read_manifest_raw(&manifest_path)?;

    if let Some(idx) = manifest
        .entries
        .iter()
        .position(|existing| existing.tool_key == entry.tool_key)
    {
        manifest.entries[idx] = entry.clone();
    } else {
        manifest.entries.push(entry.clone());
    }

    write_manifest_raw(&manifest_path, &manifest)
}

fn display_tool_name(entry: &RestoreEntry) -> String {
    match entry.tool_key.as_str() {
        "delta" | "delta-gitconfig" => "Delta".to_string(),
        _ => entry.display_tool.clone(),
    }
}

pub fn display_tools(entries: &[RestoreEntry]) -> Vec<String> {
    let mut tools = Vec::new();
    for entry in entries {
        let tool = display_tool_name(entry);
        if !tools.iter().any(|existing| existing == &tool) {
            tools.push(tool);
        }
    }
    tools
}

pub(crate) fn validate_restore_point_data(restore_point: &RestorePoint) -> Result<()> {
    super::validate_restore_point_id(&restore_point.id)?;
    if !safe_label(&restore_point.theme_name) {
        return Err(invalid_record("invalid theme label"));
    }
    if restore_point.entries.len() > MAX_ENTRIES {
        return Err(invalid_record("too many entries (maximum 512)"));
    }
    if restore_point.entries.is_empty() {
        if restore_point.is_baseline {
            return Ok(());
        }
        return Err(SlateError::BackupFailed(format!(
            "No entries in restore point: {}",
            restore_point.id
        )));
    }

    let mut has_delta = false;
    let mut has_delta_gitconfig = false;
    let mut keys = std::collections::HashSet::new();
    let mut targets = std::collections::HashSet::new();
    let mut total_bytes = 0;

    for entry in &restore_point.entries {
        if entry.unix_mode.is_some_and(|mode| mode & !0o777 != 0) {
            return Err(invalid_record("invalid permission mode"));
        }
        if super::validate_restore_point_id(&entry.tool_key).is_err()
            || !keys.insert(&entry.tool_key)
        {
            return Err(invalid_record("invalid or duplicate entry key"));
        }
        if !safe_label(&entry.display_tool) {
            return Err(invalid_record("invalid tool label"));
        }
        if !safe_path(&entry.original_path) {
            return Err(invalid_record(
                "original_path must be an absolute file path without parent traversal or controls",
            ));
        }
        if !targets.insert(&entry.original_path) {
            return Err(SlateError::BackupFailed(format!(
                "Duplicate restore target: {}",
                entry.original_path.display()
            )));
        }
        if entry.original_state == OriginalFileState::Present {
            let Some(path) = entry.backup_path.as_ref() else {
                return Err(SlateError::BackupFailed(format!(
                    "Restore entry '{}' is missing backup_path metadata",
                    entry.tool_key
                )));
            };

            if !safe_path(path) {
                return Err(invalid_record("invalid backup_path"));
            }

            let metadata = fs::symlink_metadata(path).map_err(|e| {
                SlateError::BackupFailed(format!(
                    "Cannot read backup file {}: {}",
                    path.display(),
                    e
                ))
            })?;

            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err(SlateError::BackupFailed(format!(
                    "Backup path is not a file: {}",
                    path.display()
                )));
            }
            if metadata.len() > super::source::MAX_FILE_BYTES
                || metadata.len() > super::source::MAX_SNAPSHOT_BYTES - total_bytes
            {
                return Err(invalid_record(
                    "backup limit exceeded (8 MiB per file, 64 MiB total)",
                ));
            }
            total_bytes += metadata.len();
        } else if entry.backup_path.is_some() || entry.unix_mode.is_some() {
            return Err(invalid_record(
                "absent entry must not contain backup bytes or permission metadata",
            ));
        }

        if entry.tool_key == "delta" {
            has_delta = true;
        } else if entry.tool_key == "delta-gitconfig" {
            has_delta_gitconfig = true;
        }
    }

    if has_delta != has_delta_gitconfig {
        return Err(SlateError::BackupFailed(
            "Delta restore point is incomplete. Both delta and delta-gitconfig backups must exist together.".to_string(),
        ));
    }

    Ok(())
}

pub(crate) fn read_manifest(manifest_path: &Path) -> Result<RestorePoint> {
    let restore_point = manifest_to_restore_point(read_manifest_raw(manifest_path)?)?;
    validate_restore_point_data(&restore_point)?;
    validate_record_location(manifest_path, &restore_point)?;
    Ok(restore_point)
}

fn safe_label(value: &str) -> bool {
    value.len() <= 256
        && !value
            .chars()
            .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'))
}

pub(crate) fn safe_path(path: &Path) -> bool {
    path.is_absolute()
        && path.file_name().is_some()
        && path.as_os_str().len() <= 4096
        && !path
            .components()
            .any(|c| c == std::path::Component::ParentDir)
        && !path
            .to_string_lossy()
            .chars()
            .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'))
}

fn validate_record_location(manifest_path: &Path, point: &RestorePoint) -> Result<()> {
    let directory = manifest_path
        .parent()
        .ok_or_else(|| invalid_record("missing record directory"))?;
    if directory.file_name().and_then(|name| name.to_str()) != Some(&point.id) {
        return Err(invalid_record("metadata ID does not match its directory"));
    }
    let directory = fs::canonicalize(directory)
        .map_err(|_| invalid_record("cannot resolve record directory"))?;
    let recovery_root = directory
        .parent()
        .ok_or_else(|| invalid_record("missing recovery root"))?;
    let mut targets = std::collections::HashSet::new();
    for entry in &point.entries {
        if let Some(path) = &entry.backup_path {
            let parent = path
                .parent()
                .and_then(|parent| fs::canonicalize(parent).ok());
            if parent.as_deref() != Some(&directory) {
                return Err(invalid_record(
                    "backup source must be directly inside this restore point",
                ));
            }
        }
        // Compare directory identities, preserving the final component. Normal
        // dotfile symlinks remain a structured blocked target in preview.
        if let Some(target) = target_identity(&entry.original_path) {
            if target.starts_with(recovery_root) || recovery_root.starts_with(&target) {
                return Err(invalid_record("restore target overlaps recovery storage"));
            }
            if !targets.insert(target) {
                return Err(invalid_record("duplicate target through directory aliases"));
            }
        }
    }
    Ok(())
}

fn target_identity(path: &Path) -> Option<std::path::PathBuf> {
    for ancestor in path.parent()?.ancestors() {
        if let Ok(resolved) = fs::canonicalize(ancestor) {
            return Some(resolved.join(path.strip_prefix(ancestor).ok()?));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;
    use tempfile::TempDir;

    #[test]
    fn validate_restore_point_data_allows_empty_present_backup_file() {
        let td = TempDir::new().unwrap();
        let backup_path = td.path().join("ghostty-xdg-config-ghostty.backup");
        fs::write(&backup_path, []).unwrap();

        let restore_point = RestorePoint {
            id: "baseline-empty-ghostty".to_string(),
            theme_name: "baseline".to_string(),
            created_at: SystemTime::UNIX_EPOCH,
            entries: vec![RestoreEntry {
                tool_key: "ghostty-xdg-config-ghostty".to_string(),
                display_tool: "Ghostty".to_string(),
                original_path: td.path().join(".config/ghostty/config.ghostty"),
                backup_path: Some(backup_path),
                original_state: OriginalFileState::Present,
                unix_mode: None,
            }],
            is_baseline: true,
        };

        validate_restore_point_data(&restore_point).unwrap();
    }
}
