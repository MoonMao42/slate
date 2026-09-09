use crate::cli::file_output;
use crate::cli::picker::{self, RecoveryPlan};
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use std::io::IsTerminal;
use std::path::Path;

mod output;
mod summary;
pub(crate) use summary::{PreviewRecoveryStatus, PreviewState};

fn wording(zh: &'static str, en: &'static str) -> &'static str {
    if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
        super::ui_language::tr(zh, en)
    } else {
        en
    }
}

/// Used before initializing sounds or opening the ordinary hub menu.
pub fn has_pending_preview(env: &SlateEnv) -> bool {
    PreviewRecoveryStatus::inspect(env).needs_attention()
}

/// Menu inspection can show conflicts and return to navigation. It never
/// authorizes restoration; CLI dry-run retains its nonzero conflict status.
pub(super) fn handle_menu_review(env: &SlateEnv) -> Result<()> {
    print_review(env, false, true).map(|_| ())
}

fn print_review(env: &SlateEnv, json: bool, required: bool) -> Result<RecoveryPlan> {
    let plan = picker::inspect_recovery(env)?;
    let report = if json {
        format!("{}\n", serde_json::to_string_pretty(&plan)?)
    } else if plan.available {
        output::plan(&plan)
    } else if plan.active {
        wording(
            "预览或写入正在进行，尚无完整的恢复记录。\n",
            "A preview/write is active; no completed recovery record is available yet.\n",
        )
        .into()
    } else {
        wording(
            "没有需要恢复的未完成预览。\n",
            "No unfinished preview to recover.\n",
        )
        .into()
    };
    if required {
        output::required(&report)?;
    } else {
        file_output::write_output(&report)?;
    }
    if plan.active {
        return Err(crate::config::write_guard::busy_error());
    }
    Ok(plan)
}

pub fn handle(
    env: &SlateEnv,
    dry_run: bool,
    json: bool,
    yes: bool,
    discard: bool,
    export: Option<&Path>,
) -> Result<()> {
    if (json && !dry_run)
        || (dry_run && (yes || discard || export.is_some()))
        || (export.is_some() && (yes || discard))
    {
        return Err(SlateError::InvalidConfig("Recovery inspection, confirmation, discard and export options cannot be combined this way; no recovery action was attempted.".into()));
    }
    if dry_run {
        let plan = print_review(env, json, false)?;
        return ensure_recoverable(&plan);
    }
    // This owner outlives plan output and the user's confirmation. Dropping it
    // (cancel, output failure, or an error) releases only the existing lock.
    let Some(mut prepared) = picker::prepare_recovery(env)? else {
        return file_output::write_output(if discard {
            wording(
                "没有需要放弃的未完成预览。\n",
                "No unfinished preview to discard.\n",
            )
        } else {
            wording(
                "没有需要恢复的未完成预览。\n",
                "No unfinished preview to recover.\n",
            )
        });
    };
    if discard {
        let report = match prepared.take_plan() {
            Ok(plan) => output::plan(&plan),
            Err(err) => {
                use std::io::Write;
                writeln!(
                    std::io::stderr().lock(),
                    "{}: {}",
                    wording("无法读取恢复记录", "Recovery record could not be read"),
                    file_output::terminal_text(&err.to_string())
                )?;
                wording("无法检查预览恢复记录。放弃恢复只删除这份记录，保留当前配置文件。\n", "The saved preview recovery record cannot be inspected. Discard removes only that record; current config files will be kept.\n").into()
            }
        };
        output::required(&report)?;
        if confirm_action(wording("放弃预览恢复？保留当前配置文件，删除这份恢复记录。", "Discard the saved preview recovery record? Current config files will be kept, but this recovery copy will be deleted."), wording("放弃恢复", "Discard"), yes)? {
            prepared.discard()?;
            output::completed(wording("已删除恢复记录，当前配置文件未改动。\n", "Recovery record deleted; current config files were not changed.\n"))?;
        }
        return Ok(());
    }
    let plan = prepared.take_plan()?;
    output::required(&output::plan(&plan))?;
    if let Some(directory) = export {
        prepared.export(directory)?;
        output::completed(&format!(
            "{}{}{}\n",
            wording("原始文件已导出至 ", "Original files exported to "),
            output::path(directory),
            wording("。恢复记录已保留。", ". Recovery record retained.")
        ))?;
        return Ok(());
    }
    ensure_recoverable(&plan)?;
    if confirm_action(
        wording(
            "将上列文件恢复为预览前保存的内容？",
            "Restore the preview files shown above to their saved contents?",
        ),
        wording("恢复", "Restore"),
        yes,
    )? {
        prepared.recover()?;
        output::completed(wording(
            "已恢复预览前的文件，并清除恢复记录。\n",
            "Preview files restored; recovery record cleared.\n",
        ))?;
    }
    Ok(())
}

fn ensure_recoverable(plan: &RecoveryPlan) -> Result<()> {
    if plan.blocked_count() > 0 {
        return Err(SlateError::InvalidConfig(wording("恢复存在冲突，未改动文件。可用 `slate recover --export <new-directory>` 导出原始文件供检查；或用 `--discard` 保留当前文件并放弃恢复。", "Recovery contains conflicts. No files were changed. Use `slate recover --export <new-directory>` to inspect originals, or `--discard` to keep the current files and abandon recovery.").into()));
    }
    Ok(())
}

fn confirm_action(prompt: &str, action: &str, yes: bool) -> Result<bool> {
    if !yes {
        if !std::io::stdin().is_terminal() {
            return Err(SlateError::InvalidConfig("Review `slate recover --dry-run`, then pass --yes to confirm in a non-interactive shell.".into()));
        }
        if !super::menu::confirm_named(prompt, wording("取消", "Cancel"), action).interact()? {
            return Ok(false);
        }
    }
    Ok(true)
}
