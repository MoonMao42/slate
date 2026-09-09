//! A non-cloneable, in-memory confirmation plan. Only metadata is exposed;
//! execution consumes it and verifies all captured inputs before restoring.
use super::{
    get_restore_point_with_env, plan, RestoreAction, RestoreFileResult, RestorePlan, RestorePoint,
    RestoreReceipt,
};
use crate::config::backup::{
    restore_point_directory_with_env, snapshot::create_pre_restore_snapshot_for_point,
};
use crate::config::file_read::FileIdentity;
use crate::config::ConfigWriteGuard;
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use std::path::PathBuf;

pub struct PreparedRestore {
    env: SlateEnv,
    point: RestorePoint,
    record_location: (PathBuf, FileIdentity),
    entries: Vec<plan::PreparedEntry>,
    plan: RestorePlan,
}

// Never log the saved/current bytes through a convenient Debug derivation.
impl std::fmt::Debug for PreparedRestore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PreparedRestore")
            .field("plan", &self.plan)
            .finish_non_exhaustive()
    }
}

impl PreparedRestore {
    pub fn plan(&self) -> &RestorePlan {
        &self.plan
    }

    pub(super) fn into_plan(self) -> RestorePlan {
        self.plan
    }

    fn ensure_ready(&self) -> Result<()> {
        if let Some(blocked) = self
            .entries
            .iter()
            .find(|entry| entry.change.action == RestoreAction::Blocked)
        {
            return Err(SlateError::BackupFailed(format!(
                "Cannot restore {}: {}. No files were restored.",
                blocked.change.original_path.display(),
                blocked
                    .change
                    .reason
                    .as_deref()
                    .unwrap_or("target is blocked")
            )));
        }
        Ok(())
    }

    fn revalidate(&self) -> Result<()> {
        let matches = (|| -> Result<bool> {
            if record_location(&self.env, &self.point.id)? != self.record_location {
                return Ok(false);
            }
            let current_point = get_restore_point_with_env(&self.env, &self.point.id)?;
            if current_point != self.point {
                return Ok(false);
            }
            plan::entries_still_match(&self.point, &self.entries)
        })();
        match matches {
            Ok(true) => Ok(()),
            // Unreadable or now-invalid inputs are also stale. Do not expose
            // file contents or silently replace the user's confirmed plan.
            _ => Err(SlateError::BackupFailed(format!("Restore inputs changed since preview or can no longer be verified. No files were restored. Run `slate restore {} --dry-run` and confirm a new plan.", self.point.id))),
        }
    }
}

fn record_location(env: &SlateEnv, id: &str) -> Result<(PathBuf, FileIdentity)> {
    let location = restore_point_directory_with_env(env, id)?;
    let metadata = std::fs::metadata(&location)?;
    Ok((location, FileIdentity::from_metadata(&metadata)))
}

/// Read-only preparation. Retains bounded backup/current bytes, ordinary modes,
/// file identities and resolved target locations; does not acquire a writer lock.
pub fn prepare_restore_with_env(env: &SlateEnv, id: &str) -> Result<PreparedRestore> {
    let record_location = record_location(env, id)?;
    let point = get_restore_point_with_env(env, id)?;
    let entries = plan::prepare_restore(&point)?;
    let plan = plan::restore_plan(&point, &entries);
    Ok(PreparedRestore {
        env: env.clone(),
        point,
        record_location,
        entries,
        plan,
    })
}

/// Execute precisely this one-shot plan in its captured environment. A caller
/// seeking user confirmation must display `plan()` before passing ownership here.
pub fn execute_prepared_restore(prepared: PreparedRestore) -> Result<RestoreReceipt> {
    execute_with_checkpoint_observer(prepared, |_| {})
}

fn execute_with_checkpoint_observer(
    prepared: PreparedRestore,
    after_checkpoint: impl FnOnce(&RestorePoint),
) -> Result<RestoreReceipt> {
    let _guard = ConfigWriteGuard::acquire(&prepared.env)?;
    prepared.ensure_ready()?;
    prepared.revalidate()?;
    let checkpoint = create_pre_restore_snapshot_for_point(&prepared.env, &prepared.point)?;
    after_checkpoint(&checkpoint);
    if let Err(error) = prepared.revalidate() {
        return Err(SlateError::BackupFailed(format!(
            "{error} A pre-restore checkpoint was saved as {}; inspect it before use.",
            checkpoint.id
        )));
    }
    let mut results = Vec::new();
    for entry in prepared.entries {
        let result = if entry.change.action == RestoreAction::Unchanged {
            Ok(())
        } else {
            super::restore_entry(
                &entry.entry,
                entry.content.as_ref().map(|source| source.bytes.as_slice()),
            )
        };
        results.push(RestoreFileResult {
            tool_key: entry.entry.tool_key,
            display_tool: entry.entry.display_tool,
            original_path: entry.entry.original_path,
            success: result.is_ok(),
            error: result.err().map(|err| err.to_string()),
        });
    }
    Ok(RestoreReceipt {
        restore_point_id: prepared.point.id,
        pre_restore_point_id: checkpoint.id,
        theme_name: prepared.point.theme_name,
        results,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{begin_restore_point_baseline_with_env, list_restore_points_with_env};
    use std::fs;

    #[test]
    fn late_restore_change_keeps_checkpoint_but_never_starts_file_restoration() {
        let home = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        let bashrc = home.path().join(".bashrc");
        fs::write(env.zshrc_path(), "original zsh\n").unwrap();
        fs::write(&bashrc, "original bash\n").unwrap();
        let point = begin_restore_point_baseline_with_env(&env).unwrap();
        fs::write(env.zshrc_path(), "current zsh\n").unwrap();
        fs::write(&bashrc, "current bash\n").unwrap();
        let prepared = prepare_restore_with_env(&env, &point.id).unwrap();
        let mut checkpoint_id = String::new();

        // Deterministic inter-stage mutation, not a proof against all filesystem races.
        let error = execute_with_checkpoint_observer(prepared, |checkpoint| {
            checkpoint_id = checkpoint.id.clone();
            fs::write(&bashrc, "PRIVATE_LATE_EDIT\n").unwrap();
        })
        .unwrap_err()
        .to_string();
        assert!(!checkpoint_id.is_empty());
        assert!(error.contains("changed since preview"), "{error}");
        assert!(error.contains(&checkpoint_id), "{error}");
        assert!(!error.contains("PRIVATE_LATE_EDIT"));
        assert_eq!(fs::read(env.zshrc_path()).unwrap(), b"current zsh\n");
        assert_eq!(fs::read(&bashrc).unwrap(), b"PRIVATE_LATE_EDIT\n");
        let checkpoint = get_restore_point_with_env(&env, &checkpoint_id).unwrap();
        let saved_bash = checkpoint
            .entries
            .iter()
            .find(|e| e.original_path == bashrc)
            .unwrap();
        assert_eq!(
            fs::read(saved_bash.backup_path.as_ref().unwrap()).unwrap(),
            b"current bash\n"
        );
        assert_eq!(list_restore_points_with_env(&env).unwrap().len(), 2);
    }
}
