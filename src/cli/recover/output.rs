use super::{wording, RecoveryPlan};
use crate::{
    cli::file_output,
    error::{Result, SlateError},
};
use std::{fmt::Write, path::Path};

pub(super) fn path(path: &Path) -> String {
    let mut text = file_output::terminal_text(&path.to_string_lossy());
    if path.to_str().is_none() {
        text.push_str(wording(
            "（路径显示有损，并非精确路径）",
            " (lossy display; not an exact path)",
        ));
    }
    text
}

pub(super) fn plan(plan: &RecoveryPlan) -> String {
    let mut text = format!(
        "{}: {}\n",
        wording("预览恢复", "Preview recovery"),
        path(&plan.record_path)
    );
    if plan.active {
        text.push_str(wording(
            "预览或写入正在进行，暂时不能恢复。\n",
            "Recovery actions are blocked while a preview/write is active.\n",
        ));
    }
    if plan.interrupted_write {
        text.push_str(wording(
            "预览在写入时中断，请检查未记录的改动。\n",
            "The preview stopped while writing. Unrecorded changes require review.\n",
        ));
    }
    for change in &plan.changes {
        let _ = writeln!(
            text,
            "  {:9} {}",
            action_label(change.action),
            path(&change.path)
        );
        if let Some(reason) = &change.reason {
            let _ = writeln!(text, "            {}", file_output::terminal_text(reason));
        }
    }
    let _ = writeln!(
        text,
        "{} {}{} {}",
        plan.changes.len(),
        wording("个文件 · ", "file(s) listed; "),
        plan.blocked_count(),
        wording("处冲突", "conflict(s).")
    );
    text
}

fn action_label(action: crate::config::RestoreAction) -> &'static str {
    use crate::config::RestoreAction;
    match action {
        RestoreAction::Create => wording("新建", "create"),
        RestoreAction::Replace => wording("替换", "replace"),
        RestoreAction::Remove => wording("删除", "remove"),
        RestoreAction::Unchanged => wording("不变", "unchanged"),
        RestoreAction::Blocked => wording("受阻", "blocked"),
    }
}

pub(super) fn required(report: &str) -> Result<()> {
    file_output::write_required(report).map_err(|error| {
        std::io::Error::new(error.kind(), format!("Could not print the recovery plan; no recovery, export or discard was performed: {error}")).into()
    })
}

pub(super) fn completed(message: &str) -> Result<()> {
    completion_result(file_output::write_required(message), message)
}

fn completion_result(result: std::io::Result<()>, message: &str) -> Result<()> {
    result.map_err(|error| SlateError::IOError(std::io::Error::new(error.kind(), format!("{} Confirmation output could not be written: {error}. The completed action was not rolled back.", message.trim_end()))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recover_output_completion_errors_keep_the_completed_action_explicit() {
        for kind in [
            std::io::ErrorKind::BrokenPipe,
            std::io::ErrorKind::PermissionDenied,
        ] {
            for message in [
                "Preview files restored; recovery record cleared.\n",
                "Recovery record deleted; current config files were not changed.\n",
                "Original files exported. Recovery record retained.\n",
            ] {
                let error = completion_result(Err(std::io::Error::from(kind)), message)
                    .unwrap_err()
                    .to_string();
                assert!(error.contains(message.trim_end()));
                assert!(error.contains("completed action was not rolled back"));
                assert!(!error.contains("no recovery, export or discard was performed"));
            }
        }
        assert!(completion_result(Ok(()), "done").is_ok());
    }

    #[test]
    fn recover_output_invalid_public_options_fail_before_reading_or_creating_a_profile() {
        let td = tempfile::tempdir().unwrap();
        let home = td.path().join("absent");
        let env = crate::env::SlateEnv::with_home(home.clone());
        for (dry, json, yes, discard, export) in [
            (false, true, false, false, None),
            (true, false, true, false, None),
            (true, false, false, true, None),
            (true, false, false, false, Some(td.path())),
            (false, false, true, false, Some(td.path())),
            (false, false, false, true, Some(td.path())),
        ] {
            let error = crate::cli::recover::handle(&env, dry, json, yes, discard, export)
                .unwrap_err()
                .to_string();
            assert!(error.contains("options cannot be combined"));
            assert!(!home.exists());
        }
    }
}
