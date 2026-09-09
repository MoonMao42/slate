use super::{validate_restore_point_data, OriginalFileState, RestoreEntry, RestorePoint};
use crate::config::backup::source;
use crate::config::file_read::{self, Links};
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RestoreAction {
    Create,
    Replace,
    Remove,
    Unchanged,
    Blocked,
}

impl std::fmt::Display for RestoreAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Create => "create",
            Self::Replace => "replace",
            Self::Remove => "remove",
            Self::Unchanged => "unchanged",
            Self::Blocked => "blocked",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RestoreChange {
    pub tool_key: String,
    pub display_tool: String,
    pub original_path: PathBuf,
    pub action: RestoreAction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RestorePlan {
    pub restore_point_id: String,
    pub theme_name: String,
    pub is_baseline: bool,
    /// Legacy named-theme restores also regenerate files outside the snapshot.
    pub may_regenerate_theme_files: bool,
    pub changes: Vec<RestoreChange>,
}

impl RestorePlan {
    pub fn blocked_count(&self) -> usize {
        self.changes
            .iter()
            .filter(|c| c.action == RestoreAction::Blocked)
            .count()
    }

    pub fn changed_count(&self) -> usize {
        self.changes
            .iter()
            .filter(|c| {
                matches!(
                    c.action,
                    RestoreAction::Create | RestoreAction::Replace | RestoreAction::Remove
                )
            })
            .count()
    }
}

#[derive(PartialEq, Eq)]
pub(super) struct PreparedEntry {
    pub entry: RestoreEntry,
    pub content: Option<file_read::Source>,
    current: Option<CurrentFile>,
    pub change: RestoreChange,
}

/// Compare file bytes without creating caches, snapshots, or config directories.
/// The same preparation is used immediately before execution.
pub fn preview_restore_with_env(env: &SlateEnv, id: &str) -> Result<RestorePlan> {
    Ok(super::prepare_restore_with_env(env, id)?.into_plan())
}

pub(super) fn restore_plan(point: &RestorePoint, entries: &[PreparedEntry]) -> RestorePlan {
    RestorePlan {
        restore_point_id: point.id.clone(),
        theme_name: point.theme_name.clone(),
        is_baseline: point.is_baseline,
        may_regenerate_theme_files: point.reapplies_theme(),
        changes: entries.iter().map(|entry| entry.change.clone()).collect(),
    }
}

pub(super) fn prepare_restore(point: &RestorePoint) -> Result<Vec<PreparedEntry>> {
    validate_restore_point_data(point)?;
    let mut backup_budget = source::MAX_SNAPSHOT_BYTES;
    let mut target_budget = source::MAX_SNAPSHOT_BYTES;
    point
        .entries
        .iter()
        .map(|entry| prepare_entry(entry, &mut backup_budget, &mut target_budget))
        .collect()
}

// Revalidate one pair of files at a time, not another full 128 MiB plan.
pub(super) fn entries_still_match(
    point: &RestorePoint,
    expected: &[PreparedEntry],
) -> Result<bool> {
    if point.entries.len() != expected.len() {
        return Ok(false);
    }
    let mut backup_budget = source::MAX_SNAPSHOT_BYTES;
    let mut target_budget = source::MAX_SNAPSHOT_BYTES;
    for (entry, expected) in point.entries.iter().zip(expected) {
        if prepare_entry(entry, &mut backup_budget, &mut target_budget)? != *expected {
            return Ok(false);
        }
    }
    Ok(true)
}

fn prepare_entry(
    entry: &RestoreEntry,
    backup_budget: &mut u64,
    target_budget: &mut u64,
) -> Result<PreparedEntry> {
    let content = match entry.original_state {
        OriginalFileState::Absent => None,
        OriginalFileState::Present => {
            let path = entry.backup_path.as_ref().expect("validated backup path");
            Some(source::read(path, backup_budget)?.ok_or_else(|| {
                SlateError::BackupFailed("Backup file disappeared before restore".into())
            })?)
        }
    };
    let (current, action, reason) = match current_file(&entry.original_path, target_budget) {
        Ok(current) => {
            let action = match (&current.source, &content) {
                (None, None) => RestoreAction::Unchanged,
                (None, Some(_)) => RestoreAction::Create,
                (Some(_), None) => RestoreAction::Remove,
                (Some(before), Some(after))
                    if before.bytes == after.bytes
                        && entry.unix_mode.is_none_or(|mode| Some(mode) == before.mode) =>
                {
                    RestoreAction::Unchanged
                }
                _ => RestoreAction::Replace,
            };
            (Some(current), action, None)
        }
        Err(err) => (None, RestoreAction::Blocked, Some(err)),
    };
    Ok(PreparedEntry {
        entry: entry.clone(),
        content,
        current,
        change: RestoreChange {
            tool_key: entry.tool_key.clone(),
            display_tool: entry.display_tool.clone(),
            original_path: entry.original_path.clone(),
            action,
            reason,
        },
    })
}

#[derive(PartialEq, Eq)]
struct CurrentFile {
    location: PathBuf,
    source: Option<file_read::Source>,
}

fn current_file(path: &Path, remaining: &mut u64) -> std::result::Result<CurrentFile, String> {
    let source = file_read::read(path, source::MAX_FILE_BYTES.min(*remaining), Links::Reject)
        .map_err(|err| format!("Cannot safely compare target: {err}"))?;
    if let Some(source) = &source {
        *remaining -= source.bytes.len() as u64;
    }
    // Resolve existing directory aliases while retaining the absent path suffix.
    // The final component is separately guarded by the regular-file reader.
    for parent in path.parent().into_iter().flat_map(Path::ancestors) {
        if let Ok(parent_location) = fs::canonicalize(parent) {
            let tail = path
                .strip_prefix(parent)
                .map_err(|_| "Cannot resolve target location")?;
            return Ok(CurrentFile {
                location: parent_location.join(tail),
                source,
            });
        }
    }
    Err("Cannot resolve target location".into())
}
