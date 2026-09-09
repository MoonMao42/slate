use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
use std::time::{SystemTime, UNIX_EPOCH};

mod manifest;
mod restore;
mod snapshot;
mod source;
mod time;

pub(crate) use manifest::safe_path as restore_path_is_safe;
pub(crate) use manifest::MAX_ENTRIES as MAX_RESTORE_ENTRIES;
pub(crate) use source::{read as read_snapshot_source, MAX_SNAPSHOT_BYTES};

pub(crate) use snapshot::{
    snapshot_clean_targets_with_env, snapshot_config_targets_with_env,
    snapshot_font_targets_with_env, snapshot_import_targets_with_env,
    snapshot_opacity_targets_with_env, snapshot_theme_targets_with_env,
};

pub use manifest::display_tools;
pub use restore::{
    clear_all_restore_points, delete_restore_point, execute_prepared_restore, execute_restore,
    execute_restore_with_env, get_restore_point, get_restore_point_with_env,
    inspect_restore_points_with_env, is_baseline_restore_point, list_restore_points,
    list_restore_points_with_env, prepare_restore_with_env, preview_restore_with_env,
    PreparedRestore, RestoreAction, RestoreChange, RestoreFileResult, RestoreInventory,
    RestoreInventoryIssue, RestoreInventoryIssueKind, RestorePlan, RestoreReceipt,
};
pub use snapshot::{
    begin_restore_point_baseline, begin_restore_point_baseline_with_env,
    create_backup_with_session, create_pre_restore_snapshot, create_pre_restore_snapshot_with_env,
    snapshot_current_state, snapshot_current_state_with_env,
};

pub(crate) use time::{format_iso8601_timestamp, generate_restore_point_id};

/// Represents a single backup file with persisted metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OriginalFileState {
    #[default]
    Present,
    Absent,
}

/// Represents a single backup file with persisted metadata.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RestoreEntry {
    pub tool_key: String,
    pub display_tool: String,
    pub original_path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup_path: Option<PathBuf>,
    #[serde(default)]
    pub original_state: OriginalFileState,
    /// New snapshots retain ordinary Unix access permissions, never special bits.
    /// Missing on older manifests: preserve the legacy restore behavior.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unix_mode: Option<u32>,
}

/// Represents a manifest-backed restore point with explicit directory structure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestorePoint {
    pub id: String,
    pub theme_name: String,
    pub created_at: std::time::SystemTime,
    pub entries: Vec<RestoreEntry>,
    pub is_baseline: bool,
}

impl RestorePoint {
    pub fn is_undo_checkpoint(&self) -> bool {
        self.theme_name.starts_with("pre-restore-snapshot")
    }

    /// Operation checkpoints restore captured files, without regenerating a
    /// theme over those exact bytes. Older clean snapshots retain this behavior.
    pub fn reapplies_theme(&self) -> bool {
        !self.is_baseline
            && !self.is_undo_checkpoint()
            && self.theme_name != "pre-clean"
            && !self.theme_name.starts_with("pre-clean-")
            && self.theme_name != "pre-import"
            && self.theme_name != "pre-opacity"
            && self.theme_name != "pre-theme"
            && self.theme_name != "pre-font"
            && self.theme_name != "pre-config"
    }
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
    restore_point_directory_with_env(&SlateEnv::from_process()?, restore_point_id)
}

pub(crate) fn restore_point_directory_with_env(
    env: &SlateEnv,
    restore_point_id: &str,
) -> Result<PathBuf> {
    let backup_dir = env.slate_cache_dir().join("backups");
    if backup_dir.exists() {
        resolve_restore_point_directory(&backup_dir, restore_point_id)
    } else {
        validate_restore_point_id(restore_point_id)?;
        Ok(backup_dir.join(restore_point_id))
    }
}

pub(crate) fn validate_restore_point_id(restore_point_id: &str) -> Result<()> {
    if restore_point_id.is_empty()
        || restore_point_id.len() > 128
        || !restore_point_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        || restore_point_id == "."
        || restore_point_id.contains("..")
        || restore_point_id.contains('/')
        || restore_point_id.contains('\\')
    {
        return Err(SlateError::BackupFailed(
            "Invalid restore point id; use the exact ID from slate restore --list".into(),
        ));
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
    let metadata = match fs::symlink_metadata(&restore_point_dir) {
        Ok(metadata) => Some(metadata),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => {
            return Err(SlateError::BackupFailed(
                "Cannot inspect restore point directory".into(),
            ))
        }
    };
    if let Some(metadata) = metadata {
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(SlateError::BackupFailed(
                "Restore point must be a real directory, not a symlink or special file".into(),
            ));
        }
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
    let mut remaining = source::MAX_SNAPSHOT_BYTES;
    let source =
        source::read_with_links(config_path, &mut remaining, super::file_read::Links::Follow)?
            .ok_or_else(|| SlateError::ConfigNotFound(config_path.display().to_string()))?;
    backup_captured_file(backup_root, config_path, &source.bytes)
}

pub(crate) fn backup_captured_file(
    backup_root: &Path,
    config_path: &Path,
    bytes: &[u8],
) -> Result<PathBuf> {
    if bytes.len() as u64 > source::MAX_SNAPSHOT_BYTES {
        return Err(SlateError::BackupFailed(
            "Captured configuration exceeds the checkpoint limit".into(),
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

    super::state_files::atomic_write_synced_mode(&backup_path, bytes, Some(0o600))?;

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
    fn restore_behavior_keeps_operation_checkpoints_file_only() {
        for (name, regenerates, undo) in [
            ("nord", true, false),
            ("Nord", true, false),
            ("Old custom theme", true, false),
            ("pre-clean", false, false),
            ("pre-clean-legacy", false, false),
            ("pre-import", false, false),
            ("pre-opacity", false, false),
            ("pre-theme", false, false),
            ("pre-font", false, false),
            ("pre-config", false, false),
            ("pre-restore-snapshot", false, true),
            ("pre-restore-snapshot-for-2026-09-05T00-00-00Z", false, true),
        ] {
            let mut point = RestorePoint {
                id: "fixture".into(),
                theme_name: name.into(),
                created_at: UNIX_EPOCH,
                entries: Vec::new(),
                is_baseline: false,
            };
            assert_eq!(point.reapplies_theme(), regenerates, "{name}");
            assert_eq!(point.is_undo_checkpoint(), undo, "{name}");
            point.is_baseline = true;
            assert!(!point.reapplies_theme(), "baseline {name}");
        }
    }

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
            original_path: PathBuf::from("~/.config/ghostty/config.ghostty"),
            backup_path: Some(PathBuf::from(
                "~/.cache/slate/backups/2026-04-09T10-00-00Z/ghostty.backup",
            )),
            original_state: OriginalFileState::Present,
            unix_mode: None,
        };

        let json = serde_json::to_string(&entry);
        assert!(json.is_ok());

        let parsed: RestoreEntry = serde_json::from_str(&json.unwrap()).unwrap();
        assert_eq!(parsed.tool_key, "ghostty");
        assert_eq!(parsed.original_state, OriginalFileState::Present);
    }
}
