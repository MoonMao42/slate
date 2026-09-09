//! Content-free recovery state shared by status and the recovery-first hub.

use crate::cli::picker;
use crate::config::RestoreAction;
use crate::env::SlateEnv;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PreviewState {
    Clear,
    Busy,
    Active,
    Pending,
    Conflicted,
    Unreadable,
}

#[derive(Serialize)]
pub(crate) struct PreviewRecoveryStatus {
    pub status: PreviewState,
    /// Display only, never used to locate a recovery record.
    pub record_path: String,
    pub record_path_is_lossy: bool,
    pub files_to_restore: Option<usize>,
    pub conflicts: Option<usize>,
    pub interrupted_write: Option<bool>,
    pub message: &'static str,
    pub next_step: Option<&'static str>,
}

impl PreviewRecoveryStatus {
    pub fn inspect(env: &SlateEnv) -> Self {
        let record_path = env.slate_cache_dir().join("preview-session.json");
        let mut summary = Self {
            status: PreviewState::Clear,
            record_path: record_path.to_string_lossy().into_owned(),
            record_path_is_lossy: record_path.to_str().is_none(),
            files_to_restore: Some(0),
            conflicts: Some(0),
            interrupted_write: Some(false),
            message: "No unfinished preview.",
            next_step: None,
        };
        match picker::inspect_recovery(env) {
            Ok(plan) if plan.active && !plan.available => {
                summary.status = PreviewState::Busy;
                summary.files_to_restore = None;
                summary.conflicts = None;
                summary.interrupted_write = None;
                summary.message =
                    "A Slate configuration operation is running; wait for it to finish.";
            }
            Ok(plan) if plan.active => {
                summary.status = PreviewState::Active;
                // A running preview is not an interrupted session. Its files
                // may be changing while inspected, so do not publish counts.
                summary.files_to_restore = None;
                summary.conflicts = None;
                summary.interrupted_write = None;
                summary.message = "A preview is still running; close its picker before recovering.";
            }
            Ok(plan) if plan.available => {
                let conflicts = plan.blocked_count();
                summary.status = if conflicts == 0 {
                    PreviewState::Pending
                } else {
                    PreviewState::Conflicted
                };
                summary.files_to_restore = Some(
                    plan.changes
                        .iter()
                        .filter(|change| {
                            matches!(
                                change.action,
                                RestoreAction::Create
                                    | RestoreAction::Replace
                                    | RestoreAction::Remove
                            )
                        })
                        .count(),
                );
                summary.conflicts = Some(conflicts);
                summary.interrupted_write = Some(plan.interrupted_write);
                summary.message = if conflicts == 0 {
                    "An unfinished preview remains. Review its saved files before making more changes."
                } else {
                    "Preview recovery needs review. Conflicting files and later edits will be preserved."
                };
                summary.next_step = Some("slate recover --dry-run");
            }
            Ok(_) => {}
            Err(_) => {
                summary.status = PreviewState::Unreadable;
                summary.files_to_restore = None;
                summary.conflicts = None;
                summary.interrupted_write = None;
                // Parser errors can contain fragments of private saved bytes.
                // Status must never echo recovery-record contents.
                summary.message = "Preview recovery metadata is unreadable or unsafe. Check its permissions and recovery options.";
                summary.next_step = Some("slate recover --dry-run");
            }
        }
        summary
    }

    pub fn needs_attention(&self) -> bool {
        self.status != PreviewState::Clear
    }

    pub fn lines(&self) -> Vec<String> {
        if !self.needs_attention() {
            return Vec::new();
        }
        let mut lines = vec![format!("Preview recovery: {}", self.message)];
        if let (Some(files), Some(conflicts)) = (self.files_to_restore, self.conflicts) {
            lines.push(format!(
                "{files} file(s) to restore; {conflicts} conflict(s)."
            ));
        }
        if self.interrupted_write == Some(true) {
            lines.push(
                "The preview stopped during a write; unrecorded changes require review.".into(),
            );
        }
        if let Some(command) = self.next_step {
            lines.push(format!("Next: {command}"));
        }
        lines
    }
}
