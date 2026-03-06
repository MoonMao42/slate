use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
use std::time::{SystemTime, UNIX_EPOCH};

mod manifest;
mod restore;
mod snapshot;
mod time;

pub use manifest::display_tools;
pub use restore::{
    clear_all_restore_points, delete_restore_point, execute_restore, execute_restore_with_env,
    get_restore_point, get_restore_point_with_env, is_baseline_restore_point, list_restore_points,
    list_restore_points_with_env, RestoreFileResult, RestoreReceipt,
};
pub use snapshot::{
    begin_restore_point_baseline, begin_restore_point_baseline_with_env,
    create_backup_with_session, create_pre_restore_snapshot, create_pre_restore_snapshot_with_env,
    snapshot_current_state, snapshot_current_state_with_env,
};

pub(crate) use time::{format_iso8601_timestamp, generate_restore_point_id};

/// Represents a single backup file with persisted metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OriginalFileState {
    Present,
    Absent,
}

impl Default for OriginalFileState {
    fn default() -> Self {
        Self::Present
    }
}

/// Represents a single backup file with persisted metadata.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RestoreEntry {
    pub tool_key: String,
    pub display_tool: String,
    pub original_path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup_path: Option<PathBuf>,
    #[serde(default)]
    pub original_state: OriginalFileState,
}

/// Represents a manifest-backed restore point with explicit directory structure.
#[derive(Debug, Clone)]
pub struct RestorePoint {
    pub id: String,
    pub theme_name: String,
    pub created_at: std::time::SystemTime,
    pub entries: Vec<RestoreEntry>,
    pub is_baseline: bool,
}

/// Explicit backup session created at the start of a set operation.
#[derive(Debug, Clone)]
pub struct BackupSession {
    pub restore_point_id: String,
    pub theme_name: String,
    pub restore_point_dir: PathBuf,
}

pub(crate) static RESTORE_POINT_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Get the backup directory path (~/.cache/slate/backups/)
pub fn backup_directory() -> Result<PathBuf> {
    let env = SlateEnv::from_process().map_err(|_| {
        SlateError::Internal("Cannot initialize SlateEnv to determine cache directory".to_string())
    })?;
    backup_directory_with_env(&env)
}

/// Get the backup directory path with injected SlateEnv (preferred for testing).
pub fn backup_directory_with_env(env: &SlateEnv) -> Result<PathBuf> {
    let backup_dir = env.slate_cache_dir().join("backups");
    fs::create_dir_all(&backup_dir).map_err(|e| {
        SlateError::BackupFailed(format!("Failed to create backup directory: {}", e))
    })?;
    Ok(backup_dir)
}

pub(crate) fn restore_point_directory(restore_point_id: &str) -> Result<PathBuf> {
    let backup_dir = backup_directory()?;
    resolve_restore_point_directory(&backup_dir, restore_point_id)
}

pub(crate) fn restore_point_directory_with_env(
    env: &SlateEnv,
    restore_point_id: &str,
) -> Result<PathBuf> {
    let backup_dir = backup_directory_with_env(env)?;
    resolve_restore_point_directory(&backup_dir, restore_point_id)
}

pub(crate) fn validate_restore_point_id(restore_point_id: &str) -> Result<()> {
    if restore_point_id.is_empty()
        || restore_point_id.contains("..")
        || restore_point_id.contains('/')
        || restore_point_id.contains('\\')
    {
        return Err(SlateError::BackupFailed(format!(
            "Invalid restore point id: {}",
            restore_point_id
        )));
    }

    Ok(())
}

pub(crate) fn resolve_restore_point_directory(
    backup_dir: &Path,
    restore_point_id: &str,
) -> Result<PathBuf> {
    validate_restore_point_id(restore_point_id)?;

    let canonical_backup_dir = fs::canonicalize(backup_dir).map_err(|e| {
        SlateError::BackupFailed(format!(
            "Failed to canonicalize backup directory {}: {}",
            backup_dir.display(),
            e
        ))
    })?;

    let restore_point_dir = backup_dir.join(restore_point_id);
    if restore_point_dir.exists() {
        let canonical_restore_point = fs::canonicalize(&restore_point_dir).map_err(|e| {
            SlateError::BackupFailed(format!(
                "Failed to canonicalize restore point {}: {}",
                restore_point_dir.display(),
                e
            ))
        })?;

        if !canonical_restore_point.starts_with(&canonical_backup_dir) {
            return Err(SlateError::BackupFailed(format!(
                "Restore point escapes backup directory: {}",
                restore_point_id
            )));
        }

        Ok(canonical_restore_point)
    } else {
        Ok(restore_point_dir)
    }
}

pub(crate) fn manifest_path(restore_point_dir: &Path) -> PathBuf {
    restore_point_dir.join("manifest.toml")
}

pub(crate) fn backup_file(backup_root: &Path, config_path: &Path) -> Result<PathBuf> {
    if !config_path.exists() {
        return Err(SlateError::ConfigNotFound(
            config_path.to_string_lossy().to_string(),
        ));
    }

    let tool = infer_tool_name(config_path);
    let backup_dir = backup_root.join(&tool);
    fs::create_dir_all(&backup_dir)?;

    let original_name = config_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("config");
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let backup_path = backup_dir.join(format!("{timestamp}-{original_name}.bak"));

    fs::copy(config_path, &backup_path)?;

    Ok(backup_path)
}

fn infer_tool_name(config_path: &Path) -> String {
    let file_name = config_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("config");

    if file_name == "config" {
        config_path
            .parent()
            .and_then(|parent| parent.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("config")
            .to_string()
    } else {
        Path::new(file_name)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or(file_name)
            .trim_start_matches('.')
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backup_directory_creation() {
        let result = backup_directory();
        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(path.exists());
        assert!(path.to_string_lossy().contains("slate"));
        assert!(path.to_string_lossy().contains("backups"));
    }

    #[test]
    fn test_restore_entry_serialization() {
        let entry = RestoreEntry {
            tool_key: "ghostty".to_string(),
            display_tool: "Ghostty".to_string(),
            original_path: PathBuf::from("~/.config/ghostty/config"),
            backup_path: Some(PathBuf::from(
                "~/.cache/slate/backups/2026-04-09T10-00-00Z/ghostty.backup",
            )),
            original_state: OriginalFileState::Present,
        };

        let json = serde_json::to_string(&entry);
        assert!(json.is_ok());

        let parsed: RestoreEntry = serde_json::from_str(&json.unwrap()).unwrap();
        assert_eq!(parsed.tool_key, "ghostty");
        assert_eq!(parsed.original_state, OriginalFileState::Present);
    }
}
