use super::manifest::{read_manifest, validate_restore_point_data};
use super::snapshot::create_pre_restore_snapshot_with_env;
use super::{
    backup_directory, backup_directory_with_env, manifest_path, restore_point_directory,
    restore_point_directory_with_env, OriginalFileState, RestoreEntry, RestorePoint,
};
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use atomic_write_file::AtomicWriteFile;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

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
    let backup_dir = backup_directory_with_env(env)?;

    if !backup_dir.exists() {
        return Ok(Vec::new());
    }

    let entries = fs::read_dir(&backup_dir)
        .map_err(|e| SlateError::BackupFailed(format!("Failed to read backup directory: {}", e)))?;

    let mut restore_points = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| {
            SlateError::BackupFailed(format!("Failed to read backup directory entry: {}", e))
        })?;

        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let manifest_path = path.join("manifest.toml");
        if !manifest_path.exists() {
            continue;
        }

        match read_manifest(&manifest_path) {
            Ok(restore_point) if validate_restore_point_data(&restore_point).is_ok() => {
                restore_points.push(restore_point);
            }
            _ => continue,
        }
    }

    restore_points.sort_by_key(|rp| std::cmp::Reverse(rp.created_at));
    Ok(restore_points)
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

    if !manifest_path.exists() {
        return Err(SlateError::BackupFailed(format!(
            "Restore point not found: {}",
            restore_point_id
        )));
    }

    let restore_point = read_manifest(&manifest_path)?;
    if restore_point.id != restore_point_id {
        return Err(SlateError::BackupFailed(format!(
            "Restore point metadata mismatch: expected {}, found {}",
            restore_point_id, restore_point.id
        )));
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

    let mut file = AtomicWriteFile::open(original_path).map_err(|e| {
        SlateError::BackupFailed(format!(
            "Failed to open config for writing {}: {}",
            original_path.display(),
            e
        ))
    })?;

    file.write_all(content).map_err(|e| {
        SlateError::BackupFailed(format!(
            "Failed to write restored content to {}: {}",
            original_path.display(),
            e
        ))
    })?;

    file.commit().map_err(|e| {
        SlateError::BackupFailed(format!(
            "Failed to commit restored file {}: {}",
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
    let restore_point = get_restore_point_with_env(env, restore_point_id)?;
    validate_restore_point_data(&restore_point)?;
    let _pre_restore = create_pre_restore_snapshot_with_env(env, restore_point_id)?;

    let mut results = Vec::new();
    for entry in &restore_point.entries {
        results.push(restore_single_entry(entry));
    }

    Ok(RestoreReceipt {
        restore_point_id: restore_point.id.clone(),
        theme_name: restore_point.theme_name.clone(),
        results,
    })
}

fn restore_single_entry(entry: &RestoreEntry) -> RestoreFileResult {
    let backup_content = if entry.original_state == OriginalFileState::Present {
        match entry.backup_path.as_ref() {
            Some(path) => match fs::read(path) {
                Ok(content) => Some(content),
                Err(e) => {
                    return RestoreFileResult {
                        tool_key: entry.tool_key.clone(),
                        display_tool: entry.display_tool.clone(),
                        original_path: entry.original_path.clone(),
                        success: false,
                        error: Some(format!("Failed to read backup file: {}", e)),
                    };
                }
            },
            None => {
                return RestoreFileResult {
                    tool_key: entry.tool_key.clone(),
                    display_tool: entry.display_tool.clone(),
                    original_path: entry.original_path.clone(),
                    success: false,
                    error: Some("Backup path not found in manifest".to_string()),
                };
            }
        }
    } else {
        None
    };

    match restore_entry(entry, backup_content.as_deref()) {
        Ok(()) => RestoreFileResult {
            tool_key: entry.tool_key.clone(),
            display_tool: entry.display_tool.clone(),
            original_path: entry.original_path.clone(),
            success: true,
            error: None,
        },
        Err(e) => RestoreFileResult {
            tool_key: entry.tool_key.clone(),
            display_tool: entry.display_tool.clone(),
            original_path: entry.original_path.clone(),
            success: false,
            error: Some(e.to_string()),
        },
    }
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
