use super::*;
use crate::{
    adapter::ToolApplyStatus,
    cli::apply::ThemeApplyReport,
    config::{ConfigManager, ConfigWriteGuard},
    theme::ThemeRegistry,
};
use serde::Serialize;
use std::{collections::BTreeSet, path::PathBuf};

#[derive(Debug, PartialEq, Eq, Serialize)]
struct SyncPlan {
    schema_version: u8,
    theme: String,
    tools: Vec<String>,
    /// Potential writes, not a byte diff or a guarantee that every path changes.
    configuration_paths: Vec<PathBuf>,
    resolved_paths: Vec<PathBuf>,
    /// Private consent baseline: never emit file contents in JSON or Debug.
    #[serde(skip)]
    reviewed_files: ReviewedFiles,
    notes: Vec<&'static str>,
}

#[derive(PartialEq, Eq)]
struct ReviewedFiles(Vec<Option<crate::config::file_read::Source>>);

impl std::fmt::Debug for ReviewedFiles {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ReviewedFiles")
            .field("count", &self.0.len())
            .finish_non_exhaustive()
    }
}

fn capture_files(paths: &[PathBuf]) -> Result<ReviewedFiles> {
    use crate::config::file_read::{self, Links, MAX_TOOL_CONFIG_BYTES};
    // Same per-file and total byte budgets as operation checkpoints.
    let mut remaining = 64 * 1024 * 1024;
    let mut sources = Vec::with_capacity(paths.len());
    for path in paths {
        let source = file_read::read(path, MAX_TOOL_CONFIG_BYTES.min(remaining), Links::Reject)
            .map_err(|error| {
                let display = super::super::file_output::terminal_text(&path.to_string_lossy());
                let lossy = if path.to_str().is_none() {
                    " (lossy display; not an exact path)"
                } else {
                    ""
                };
                SlateError::InvalidConfig(format!(
                    "Cannot safely capture tool configuration for review:\n  {display}{lossy}\n  {error}\nReview this file's size, type and access before retrying (8 MiB per file, 64 MiB total). Nothing was changed; file contents omitted."
                ))
            })?;
        if let Some(source) = &source {
            remaining -= source.bytes.len() as u64;
        }
        sources.push(source);
    }
    Ok(ReviewedFiles(sources))
}

pub(super) fn validate_names(names: &[String]) -> Result<()> {
    let supported = supported_tools();
    if names.is_empty() || names.iter().any(|name| !supported.contains(&name.as_str())) {
        return Err(SlateError::InvalidConfig(
            "Choose one or more adapter IDs from `slate tools list`; no tools were changed.".into(),
        ));
    }
    Ok(())
}

fn prepare(env: &SlateEnv, names: &[String]) -> Result<SyncPlan> {
    validate_names(names)?;
    let theme = ConfigManager::from_env_paths(env).get_current_theme()
        .map_err(|_| SlateError::InvalidConfig("Cannot read the saved theme; no tools were changed.".into()))?
        .ok_or_else(|| SlateError::InvalidConfig("Save a theme first with `slate theme`, or use `slate setup`. No tools were changed.".into()))?;
    if ThemeRegistry::new()?.get(&theme).is_none() {
        return Err(SlateError::InvalidConfig(
            "Saved theme is unknown; no fallback theme or tool changes were applied.".into(),
        ));
    }
    let tools: Vec<_> = names
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut paths = BTreeSet::new();
    let mut notes = vec![
        "Only the selected adapters run. The saved theme, auto-theme pairing, shell startup files, and other adapters are unchanged.",
        "This lists potential configuration writes, not a content diff. A file recovery point is required before applying; earlier writes are not automatically rolled back on partial failure.",
        "Recovery does not undo derived caches, reload running applications, or remove newly created empty directories. Adapter-local backup copies may also be created.",
        "Compatibility checks run only after confirmation. A listed executable or config does not prove a supported version or active integration.",
        "Configuration bytes, file identity and permissions are rechecked after confirmation; changes require a fresh review. This is not an atomic lock against external editors.",
    ];
    for tool in &tools {
        if !crate::detection::detect_tool_presence_with_env(tool, env).installed {
            return Err(missing_tool(tool));
        }
        crate::adapter::write_paths::add_theme_paths(env, tool, &mut paths)?;
        let hint = inventory::hint(tool);
        if !notes.contains(&hint) {
            notes.push(hint);
        }
    }
    let targets = crate::config::recovery_paths::targets(env, paths, "Tools")?;
    let resolved_paths = targets
        .iter()
        .map(|target| {
            crate::config::file_read::directory_alias_target(&target.original_path).ok_or_else(
                || {
                    SlateError::InvalidConfig(
                        "Cannot resolve a tool configuration destination; nothing was changed."
                            .into(),
                    )
                },
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let configuration_paths: Vec<_> = targets
        .into_iter()
        .map(|target| target.original_path)
        .collect();
    let reviewed_files = capture_files(&configuration_paths)?;
    Ok(SyncPlan {
        schema_version: 1,
        theme,
        tools,
        configuration_paths,
        resolved_paths,
        reviewed_files,
        notes,
    })
}

fn missing_tool(tool: &str) -> SlateError {
    // Every adapter has a read-only detail page, but not every adapter has a
    // guided installer. Do not send a selected-tool operation to full setup.
    SlateError::InvalidConfig(format!(
        "{tool} is not detected. Sync never installs software; use `slate tools info {tool}` to review detection and installation guidance for this tool. No tools were changed."
    ))
}

fn render(plan: &SyncPlan) -> String {
    use std::fmt::Write;
    let mut output = format!(
        "Sync {} to {}\nPotential configuration writes:\n",
        plan.theme,
        plan.tools.join(", ")
    );
    for path in &plan.configuration_paths {
        let _ = writeln!(
            output,
            "  {}",
            super::super::file_output::terminal_text(&path.to_string_lossy())
        );
    }
    for note in &plan.notes {
        let _ = writeln!(output, "• {note}");
    }
    output
}

fn render_menu(plan: &SyncPlan) -> String {
    use std::fmt::Write;
    let mut output = format!(
        "{} · {} → {}\n{} {} {}\n",
        tr("同步预览", "Sync Preview"),
        plan.theme,
        plan.tools.join(", "),
        tr("可能修改", "Potential writes:"),
        plan.configuration_paths.len(),
        tr(
            "个配置文件（不是逐行差异）：",
            "configuration files (not a line-by-line diff):"
        )
    );
    for path in &plan.configuration_paths {
        let _ = writeln!(
            output,
            "  {}{}",
            super::super::file_output::terminal_text(&path.to_string_lossy()),
            if path.to_str().is_none() {
                " (lossy display; not an exact path)"
            } else {
                ""
            }
        );
    }
    for tool in &plan.tools {
        let _ = writeln!(output, "  {tool}：{}", inventory::menu_sync_hint(tool));
    }
    output.push_str(tr("只同步上述工具；不安装软件，不改已保存主题、自动配对或 Shell 启动文件。\n", "Only these tools are synced; no installs or changes to the saved theme, pairing or shell startup files.\n"));
    output.push_str(
        tr("确认后才检查兼容性、创建恢复点并执行；可能生成缓存或重载应用，不保证实时配色。\n", "After confirmation: compatibility checks, a recovery point, then sync. Caches or application reloads may occur; live colors are not guaranteed.\n"),
    );
    output.push_str(tr("中途失败不会自动回滚；文件恢复不恢复运行状态或派生缓存，也不清理空目录。\n", "Partial failures are not rolled back automatically. File recovery does not restore running state or derived caches, or remove empty directories.\n"));
    output.push_str(tr(
        "确认前配置若有变化需重新审阅；不是阻止外部编辑的原子锁。\n",
        "Changes before confirmation require a new review; external editors are not locked out.\n",
    ));
    let _ = writeln!(
        output,
        "{}slate tools sync {} --dry-run",
        tr("完整说明：", "Full details: "),
        plan.tools.join(" ")
    );
    output
}

pub(super) fn handle_menu(env: &SlateEnv, tool: &str, dry_run: bool) -> Result<()> {
    handle_impl(env, &[tool.to_owned()], dry_run, false, false, true)
}

pub(super) fn handle(
    env: &SlateEnv,
    tools: &[String],
    dry_run: bool,
    yes: bool,
    json: bool,
) -> Result<()> {
    handle_impl(env, tools, dry_run, yes, json, false)
}

fn handle_impl(
    env: &SlateEnv,
    tools: &[String],
    dry_run: bool,
    yes: bool,
    json: bool,
    compact: bool,
) -> Result<()> {
    let plan = prepare(env, tools)?;
    if json {
        return super::super::file_output::write_output(&format!(
            "{}\n",
            serde_json::to_string_pretty(&plan)?
        ));
    }
    let review = if compact {
        render_menu(&plan)
    } else {
        render(&plan)
    };
    if dry_run {
        return super::super::file_output::write_output(&format!(
            "{}{}\n",
            review,
            if compact {
                tr("未修改文件或进程。", "No files or processes were changed.")
            } else {
                "No files or processes were changed."
            }
        ));
    }
    super::super::file_output::write_required(&review)?;
    if !yes {
        if !interactive() {
            return Err(SlateError::InvalidConfig("Non-interactive tool sync requires --yes. Review with --dry-run first; nothing was changed.".into()));
        }
        if !super::super::menu::select(tr("确认同步上述工具的配色？", "Sync these tools' colors?"))
            .escape_value(false)
            .initial_value(false)
            .item(false, tr("暂不同步", "Cancel"), "")
            .item(
                true,
                tr("确认同步", "Sync"),
                tr(
                    "写入上述配置 · 可能刷新正在运行的应用",
                    "Writes the listed files; may reload running apps",
                ),
            )
            .interact()
            .map_err(input_error)?
        {
            return Ok(());
        }
    }
    let report = execute(env, &plan)?;
    if !compact {
        crate::cli::apply::log_apply_report(&report);
    }
    // Preserve useful follow-up for successful members of a partial sync, but
    // never let an output error mask the actual adapter failure below.
    let follow_up_output = if compact {
        super::super::file_output::write_required(&menu_follow_up(&report)).map_err(|error| {
            SlateError::IOError(std::io::Error::new(error.kind(), format!(
                "Tool sync executed, but its result summary could not be written: {error}. Completed changes were not rolled back."
            )))
        })
    } else {
        super::super::file_output::write_output(&follow_up(&report))
    };
    if compact {
        // If stdout failed, retain the full stderr recovery/success receipt.
        crate::cli::apply::log_apply_report_with_summary(&report, follow_up_output.is_ok());
    }
    report.ensure_no_failures()?;
    if let Some(error) = skipped_sync_error(&report) {
        return Err(error);
    }
    follow_up_output?;
    if compact {
        return super::super::file_output::write_output(&format!(
            "{} {} {}\n",
            tr("已保存", "Saved colors for"),
            report.applied_count(),
            tr(
                "个工具的配色；已保存主题和其他工具未更改。",
                "tool(s); the saved theme and other tools were not changed."
            )
        ));
    }
    super::super::file_output::write_output(&format!(
        "Synced {} tool(s). The saved theme and other tools were not changed.\n",
        report.applied_count()
    ))
}

fn skipped_sync_error(report: &ThemeApplyReport) -> Option<SlateError> {
    use std::fmt::Write;
    let mut details = String::new();
    for result in &report.results {
        if let ToolApplyStatus::Skipped(reason) = &result.status {
            let id = super::super::file_output::terminal_text(&result.tool_name);
            let _ = writeln!(details, "  {id}: {reason}. Review: slate tools info {id}");
        }
    }
    if details.is_empty() {
        return None;
    }
    details.insert_str(0, "Selected-tool sync is incomplete:\n");
    details
        .push_str("Any successful syncs were kept; no automatic retry or rollback was performed.");
    if report.restore_point_id.is_some() {
        details.push_str(
            " Review this operation's recovery point with `slate restore --list` before retrying.",
        );
    }
    Some(SlateError::InvalidConfig(details))
}

fn menu_follow_up(report: &ThemeApplyReport) -> String {
    use std::fmt::Write;
    let mut output = String::new();
    for result in &report.results {
        if !matches!(result.status, ToolApplyStatus::Applied) {
            continue;
        }
        let id = super::super::file_output::terminal_text(&result.tool_name);
        let command = if super::super::doctor::has_tool_file_check(&result.tool_name) {
            "slate doctor"
        } else {
            "slate tools info"
        };
        let _ = writeln!(
            output,
            "  {id} · {} · {}{command} {id}",
            tr("配置已保存", "Configuration saved"),
            tr("检查：", "Check: ")
        );
        match result.tool_name.as_str() {
            "btop" => output.push_str(tr("  重新打开 btop；若退出时写回旧配色，请再同步一次。\n", "  Reopen btop; sync again if it restores old colors on exit.\n")),
            "opencode" => {
                output.push_str(tr("  重新打开 OpenCode，检查 system 主题；本次未重启会话。\n", "  Reopen OpenCode and check its system theme; no session was restarted.\n"))
            }
            "tmux" => {
                output.push_str(tr("  请检查目标 tmux 服务；其他服务不变，重载失败可能已部分生效。\n", "  Check the target tmux server; others are unchanged. A failed reload may have partially applied.\n"))
            }
            _ => {}
        }
    }
    if output.is_empty() {
        return output;
    }
    output.push_str(tr("实际生效取决于工具的启动方式和覆盖配置；未验证实时外观。\n", "Activation depends on tool startup and overriding configuration; live appearance was not verified.\n"));
    if crate::adapter::registry::requires_new_shell(&report.results) {
        output.push_str(tr("请在已加载 Slate 集成的新 Shell 中检查；本次没有修改 Shell 启动文件。\n", "Check in a new shell with Slate integration loaded; shell startup files were not changed.\n"));
    }
    if let Some(id) = &report.restore_point_id {
        let command = format!(
            "slate restore {} --dry-run",
            crate::detection::shell_quote(id)
        );
        let _ = writeln!(
            output,
            "{}{}{}",
            tr("恢复预览：", "Review recovery: "),
            super::super::file_output::terminal_text(&command),
            tr(
                "（只恢复文件，不恢复运行状态）",
                " (files only, not running state)"
            )
        );
    }
    output
}

fn follow_up(report: &ThemeApplyReport) -> String {
    use std::fmt::Write;
    let applied = report
        .results
        .iter()
        .filter(|result| matches!(result.status, ToolApplyStatus::Applied))
        .map(|result| result.tool_name.as_str())
        .collect::<BTreeSet<_>>();
    if applied.is_empty() {
        return String::new();
    }
    let mut output =
        String::from("Next steps for applied tools (live appearance is not verified):\n");
    for id in applied {
        let action = match id {
            "btop" => "Reopen btop to load the saved palette.",
            "opencode" => "Reopen OpenCode to check theme = system against your terminal's colors. Slate did not close or restart any running session; this configuration change does not require a new shell.",
            "eza" => "Run eza again from its configured shell; stale color exports may override the updated palette.",
            "lazygit" => "Reopen Lazygit from a fresh shell with updated Slate integration; custom config selection may override colors.",
            "yazi" => "Reopen Yazi; personal theme overrides may take precedence.",
            "zellij" => "Check in a new Zellij session; existing sessions, layouts or command-line choices may override colors.",
            "tmux" => "Inspect the target tmux server's status bar and pane colors. Reload targets the captured TMUX socket, or the default server outside tmux; other servers are unchanged. Sync never starts a server or replays your startup configuration. A reload warning can mean partial application; check before retrying. A new session on an existing server does not reread startup configuration. File recovery does not restore colors in a running server.",
            "starship" => "Draw a new prompt in a shell with Starship initialized; a generated config alone does not initialize the prompt.",
            "delta" => "Run a new diff through your existing Delta pager; sync preserved pager selection and did not activate Delta. Repository settings, environment variables or command-line options can override Git configuration. No shell restart is required for the saved colors.",
            "fastfetch" => "Run fastfetch manually in a new shell with the Slate wrapper loaded; startup autorun is not required or enabled by this sync. An explicit --config bypasses Slate's preset; personal layouts are not merged.",
            "zsh-syntax-highlighting" => "Open an interactive Zsh with the highlighting plugin loaded before Slate's color snippet, then type a command to check its colors. Sync writes colors only; it does not install or load the plugin. Later style assignments can override them.",
            _ => "Review this adapter's activation requirements before judging the result.",
        };
        let display = super::super::file_output::terminal_text(id);
        let _ = writeln!(output, "  {display}: {action}");
        if super::super::doctor::has_tool_file_check(id) {
            let _ = writeln!(output, "    Read-only check: slate doctor {display}");
        } else {
            let _ = writeln!(output, "    Adapter details: slate tools info {display}");
        }
    }
    if crate::adapter::registry::requires_new_shell(&report.results) {
        output.push_str("Open a new shell to load updated colors. Sync did not regenerate shell startup. If integration is missing or outdated, review `slate setup` first; it is a separate multi-tool workflow.\n");
    }
    output
}

fn execute(env: &SlateEnv, reviewed: &SyncPlan) -> Result<ThemeApplyReport> {
    // Preflight again before creating a lock, then compare under that lock.
    // A watcher/user changing the saved theme invalidates the reviewed request.
    if prepare(env, &reviewed.tools)? != *reviewed {
        return Err(changed_plan());
    }
    let _guard = ConfigWriteGuard::acquire(env)?;
    if prepare(env, &reviewed.tools)? != *reviewed {
        return Err(changed_plan());
    }
    let registry = ToolRegistry::default();
    let selected = reviewed.tools.iter().cloned().collect();
    let prepared = registry.prepare_theme_with_env(env, Some(&selected));
    prepared.ensure_all_ready()?;
    if prepared.ready_tools().count() != reviewed.tools.len() {
        return Err(changed_plan());
    }
    // Native compatibility probes may take time. Recheck before checkpointing
    // and applying; the cooperative lock cannot stop an unrelated editor.
    if prepare(env, &reviewed.tools)? != *reviewed {
        return Err(changed_plan());
    }
    let targets =
        crate::config::recovery_paths::targets(env, reviewed.configuration_paths.clone(), "Tools")?;
    let point = crate::config::snapshot_theme_targets_with_env(env, &targets)?;
    let themes = ThemeRegistry::new()?;
    let theme = themes
        .get(&reviewed.theme)
        .expect("validated built-in theme");
    // Deliberately do not call global theme apply: it writes shared shell files,
    // current theme, pairing and the editor notification even for a subset.
    let mut report = ThemeApplyReport {
        results: prepared.apply_with_env(theme, env),
        commit_failure: None,
        reload_warnings: Vec::new(),
        restore_point_id: Some(point.id),
    };
    super::super::apply::reload_theme_targets(env, &registry, &mut report);
    debug_assert!(report.results.iter().all(|result| {
        reviewed.tools.contains(&result.tool_name)
            && !matches!(
                result.status,
                ToolApplyStatus::Skipped(crate::adapter::SkipReason::ThemeNotCommitted)
            )
    }));
    Ok(report)
}

fn changed_plan() -> SlateError {
    SlateError::InvalidConfig("Tool availability, saved theme, target paths, or configuration files changed after review. Review again before syncing; no adapter applied changes.".into())
}

#[cfg(test)]
mod tests;
