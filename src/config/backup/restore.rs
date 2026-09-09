use super::manifest::{read_manifest, validate_restore_point_data};
use super::{
    backup_directory, manifest_path, restore_point_directory, restore_point_directory_with_env,
    OriginalFileState, RestoreEntry, RestorePoint,
};
use crate::config::state_files::atomic_write_synced_mode;
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use std::fs;
use std::path::PathBuf;

mod inventory;
mod plan;
mod prepared;
pub use inventory::{
    inspect_restore_points_with_env, RestoreInventory, RestoreInventoryIssue,
    RestoreInventoryIssueKind,
};
pub use plan::{preview_restore_with_env, RestoreAction, RestoreChange, RestorePlan};
pub use prepared::{execute_prepared_restore, prepare_restore_with_env, PreparedRestore};

/// Result of a single file restoration attempt.
#[derive(Debug, Clone)]
pub struct RestoreFileResult {
    pub tool_key: String,
    pub display_tool: String,
    pub original_path: PathBuf,
    pub success: bool,
    pub error: Option<String>,
}

/// Aggregate receipt for a complete restore operation.
#[derive(Debug, Clone)]
pub struct RestoreReceipt {
    pub restore_point_id: String,
    pub pre_restore_point_id: String,
    pub theme_name: String,
    pub results: Vec<RestoreFileResult>,
}

impl RestoreReceipt {
    pub fn success_count(&self) -> usize {
        self.results.iter().filter(|r| r.success).count()
    }

    pub fn failure_count(&self) -> usize {
        self.results.iter().filter(|r| !r.success).count()
    }

    pub fn is_fully_successful(&self) -> bool {
        self.failure_count() == 0 && !self.results.is_empty()
    }

    pub fn failed_results(&self) -> Vec<&RestoreFileResult> {
        self.results.iter().filter(|r| !r.success).collect()
    }
}

pub fn list_restore_points() -> Result<Vec<RestorePoint>> {
    let env = SlateEnv::from_process().map_err(|_| {
        SlateError::Internal("Cannot initialize SlateEnv to list restore points".to_string())
    })?;
    list_restore_points_with_env(&env)
}

pub fn list_restore_points_with_env(env: &SlateEnv) -> Result<Vec<RestorePoint>> {
    Ok(inspect_restore_points_with_env(env)?.points)
}

pub fn is_baseline_restore_point(restore_point: &RestorePoint) -> bool {
    restore_point.is_baseline
}

pub fn get_restore_point(restore_point_id: &str) -> Result<RestorePoint> {
    let env = SlateEnv::from_process().map_err(|_| {
        SlateError::Internal("Cannot initialize SlateEnv to load restore point".to_string())
    })?;
    get_restore_point_with_env(&env, restore_point_id)
}

pub fn get_restore_point_with_env(env: &SlateEnv, restore_point_id: &str) -> Result<RestorePoint> {
    let restore_point_dir = restore_point_directory_with_env(env, restore_point_id)?;
    let manifest_path = manifest_path(&restore_point_dir);

    let restore_point = read_manifest(&manifest_path)?;
    if restore_point.id != restore_point_id {
        return Err(SlateError::BackupFailed(
            "Restore point metadata ID mismatch".into(),
        ));
    }

    Ok(restore_point)
}

fn restore_entry(entry: &RestoreEntry, content: Option<&[u8]>) -> Result<()> {
    if entry.original_state == OriginalFileState::Absent {
        match fs::remove_file(&entry.original_path) {
            Ok(()) => return Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(err) => {
                return Err(SlateError::BackupFailed(format!(
                    "Failed to remove restored file {}: {}",
                    entry.original_path.display(),
                    err
                )))
            }
        }
    }

    let original_path = &entry.original_path;
    let content = content.unwrap_or_default();

    if let Some(parent) = original_path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            SlateError::BackupFailed(format!(
                "Failed to create parent directory for {}: {}",
                original_path.display(),
                e
            ))
        })?;
    }

    atomic_write_synced_mode(original_path, content, entry.unix_mode).map_err(|e| {
        SlateError::BackupFailed(format!(
            "Failed to restore file {}: {}",
            original_path.display(),
            e
        ))
    })
}

pub fn delete_restore_point(restore_point_id: &str) -> Result<usize> {
    let restore_point_dir = restore_point_directory(restore_point_id)?;

    if !restore_point_dir.exists() {
        return Err(SlateError::BackupFailed(format!(
            "Restore point directory not found: {}",
            restore_point_id
        )));
    }

    let entries = fs::read_dir(&restore_point_dir).map_err(|e| {
        SlateError::BackupFailed(format!("Failed to read restore point directory: {}", e))
    })?;

    let file_count: usize = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .count();

    fs::remove_dir_all(&restore_point_dir).map_err(|e| {
        SlateError::BackupFailed(format!(
            "Failed to delete restore point {}: {}",
            restore_point_id, e
        ))
    })?;

    Ok(file_count)
}

pub fn clear_all_restore_points() -> Result<usize> {
    let backup_dir = backup_directory()?;
    if !backup_dir.exists() {
        return Ok(0);
    }

    let entries = fs::read_dir(&backup_dir)
        .map_err(|e| SlateError::BackupFailed(format!("Failed to read backup directory: {}", e)))?;

    let mut deleted_items = 0;
    for entry in entries {
        let entry = entry.map_err(|e| {
            SlateError::BackupFailed(format!("Failed to read backup directory entry: {}", e))
        })?;
        let path = entry.path();
        if path.is_dir() {
            fs::remove_dir_all(&path).map_err(|e| {
                SlateError::BackupFailed(format!(
                    "Failed to delete backup directory {}: {}",
                    path.display(),
                    e
                ))
            })?;
            deleted_items += 1;
        } else if path.is_file() {
            fs::remove_file(&path).map_err(|e| {
                SlateError::BackupFailed(format!(
                    "Failed to delete backup file {}: {}",
                    path.display(),
                    e
                ))
            })?;
            deleted_items += 1;
        }
    }

    Ok(deleted_items)
}

pub fn execute_restore(restore_point_id: &str) -> Result<RestoreReceipt> {
    let env = SlateEnv::from_process().map_err(|_| {
        SlateError::Internal("Cannot initialize SlateEnv to execute restore".to_string())
    })?;
    execute_restore_with_env(&env, restore_point_id)
}

pub fn execute_restore_with_env(env: &SlateEnv, restore_point_id: &str) -> Result<RestoreReceipt> {
    let _write_guard = crate::config::ConfigWriteGuard::acquire(env)?;
    execute_prepared_restore(prepare_restore_with_env(env, restore_point_id)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_list_restore_points_empty_directory() {
        let temp = TempDir::new().unwrap();
        let env = SlateEnv::with_home(temp.path().to_path_buf());
        let result = list_restore_points_with_env(&env);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }
}
