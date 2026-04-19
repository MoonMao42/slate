use super::{manifest_path, BackupSession, OriginalFileState, RestoreEntry, RestorePoint};
use crate::config::atomic_write_synced;
use crate::error::{Result, SlateError};
use std::fs;
use std::path::Path;

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
    let content = toml::to_string_pretty(manifest).map_err(|e| {
        SlateError::BackupFailed(format!("Failed to serialize manifest.toml: {}", e))
    })?;

    atomic_write_synced(manifest_path, content.as_bytes())
        .map_err(|e| SlateError::BackupFailed(format!("Failed to write manifest.toml: {}", e)))
}

pub(crate) fn read_manifest_raw(manifest_path: &Path) -> Result<RestoreManifest> {
    let content = fs::read_to_string(manifest_path)
        .map_err(|e| SlateError::BackupFailed(format!("Failed to read manifest.toml: {}", e)))?;

    toml::from_str(&content)
        .map_err(|e| SlateError::BackupFailed(format!("Failed to parse manifest.toml: {}", e)))
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

pub(crate) fn record_absent_entry(
    session: &BackupSession,
    entry: RestoreEntry,
) -> Result<RestoreEntry> {
    append_manifest_entry(session, &entry)?;
    Ok(entry)
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

    for entry in &restore_point.entries {
        if entry.original_path.as_os_str().is_empty() {
            return Err(SlateError::BackupFailed(format!(
                "Restore entry '{}' is missing original_path metadata",
                entry.tool_key
            )));
        }

        if entry.original_state == OriginalFileState::Present {
            let Some(path) = entry.backup_path.as_ref() else {
                return Err(SlateError::BackupFailed(format!(
                    "Restore entry '{}' is missing backup_path metadata",
                    entry.tool_key
                )));
            };

            if !path.exists() {
                return Err(SlateError::BackupFailed(format!(
                    "Backup file not found: {}",
                    path.display()
                )));
            }

            let metadata = fs::metadata(path).map_err(|e| {
                SlateError::BackupFailed(format!(
                    "Cannot read backup file {}: {}",
                    path.display(),
                    e
                ))
            })?;

            if metadata.len() == 0 {
                return Err(SlateError::BackupFailed(format!(
                    "Backup file is empty: {}",
                    path.display()
                )));
            }
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
    if restore_point.entries.is_empty() && !restore_point.is_baseline {
        return Err(SlateError::BackupFailed(
            "manifest.toml has empty entries array".to_string(),
        ));
    }
    Ok(restore_point)
}
