use super::menu::select;
use super::ui_language::tr;
use crate::brand::events::{dispatch, BrandEvent, SuccessKind};
use crate::brand::language::Language;
use crate::brand::render_context::RenderContext;
use crate::brand::roles::Roles;
use crate::config::{
    delete_restore_point, execute_prepared_restore, get_restore_point,
    inspect_restore_points_with_env, is_baseline_restore_point,
};
use crate::error::Result;

mod lifecycle;
mod listing;
use super::file_output as output;
mod preview;
mod receipt;
pub use listing::handle_list_with_options;

/// Handle `slate restore [ID] [--list] [--delete ID]` with structured clap arguments.
pub fn handle(restore_id: Option<&str>, list_mode: bool, delete_id: Option<&str>) -> Result<()> {
    handle_impl(restore_id, list_mode, delete_id, None)
}

pub(crate) fn handle_menu(auto: bool, quiet: bool) -> Result<()> {
    handle_impl(None, false, None, Some((auto, quiet)))
}

fn handle_impl(
    restore_id: Option<&str>,
    list_mode: bool,
    delete_id: Option<&str>,
    hub_sound: Option<(bool, bool)>,
) -> Result<()> {
    if list_mode {
        return handle_list_with_options(false, false);
    }
    let env = crate::env::SlateEnv::from_process()?;
    let _write_guard = if restore_id.is_some() || delete_id.is_some() {
        Some(crate::config::ConfigWriteGuard::acquire(&env)?)
    } else {
        None
    };
    // Build a RenderContext once at the top so every sub-handler shares
    // the same byte contract (daily chrome + sketch 003 tree shape +
    // D-01a severity). graceful degrade — plain text when the theme
    // registry cannot boot.
    let ctx = RenderContext::from_active_theme().ok();
    let roles = ctx.as_ref().map(Roles::new);

    if let Some(id) = delete_id {
        handle_delete(id, roles.as_ref())?;
        return Ok(());
    }

    if let Some(id) = restore_id {
        handle_restore_direct(id, roles.as_ref(), false, None)?;
    } else {
        handle_restore_picker(roles.as_ref(), hub_sound)?;
    }

    Ok(())
}

/// Navigation outcomes are separate from execution failures.
enum RestoreOutcome {
    Restored,
    Declined,
    Blocked,
}

fn handle_restore_direct(
    restore_id: &str,
    r: Option<&Roles<'_>>,
    compact: bool,
    hub_sound: Option<(bool, bool)>,
) -> Result<RestoreOutcome> {
    let env = crate::env::SlateEnv::from_process()?;
    let prepared = crate::config::prepare_restore_with_env(&env, restore_id)?;
    let plan = prepared.plan();
    print_plan(plan, compact)?;
    if compact && plan.blocked_count() > 0 {
        return Ok(RestoreOutcome::Blocked);
    }
    ensure_plan_ready(plan)?;
    let may_reapply_theme = plan.may_regenerate_theme_files;

    // Baseline restore gets an extra warning
    let confirmed = if compact {
        super::menu::confirm_named(
            preview::menu_confirmation(plan),
            tr("取消", "Cancel"),
            tr("恢复", "Restore"),
        )
        .interact()?
    } else if plan.is_baseline {
        super::menu::confirm("⚠ This will restore all config files to their state BEFORE slate was installed. Any manual changes you made since then will be lost. Continue?")
            .interact()?
    } else {
        super::menu::confirm(format!(
            "Restore to {}? This will modify your configuration files.",
            output::terminal_text(&plan.theme_name)
        ))
        .interact()?
    };

    if !confirmed {
        println!(
            "{}",
            if compact {
                tr(
                    "未恢复，返回备份列表。",
                    "Not restored; returning to backups.",
                )
            } else {
                "Restore cancelled."
            }
        );
        return Ok(RestoreOutcome::Declined);
    }

    // Hub browsing/declines must not unpack audio files. Initialize only once
    // the user has authorized execution, preserving the hub's quiet/auto flags.
    if let Some((auto, quiet)) = hub_sound {
        crate::brand::SoundSink::install(&env, auto, quiet);
    }

    // Execute restore
    let receipt = execute_prepared_restore(prepared)?;

    // completion receipt is a static tree-narrative anchor
    // bypass cliclack and println! via Roles::heading / tree_branch /
    // tree_end. Sketch 003 canon: `◆ Restored … ┃ ├─ … └─ ★ Back on track`.
    let failures = receipt.failed_results();
    if compact && failures.is_empty() {
        println!(
            "\n{} {} {}",
            tr("已恢复", "Restored"),
            receipt.success_count(),
            tr("项文件记录。", "file records.")
        );
    } else {
        println!();
        println!(
            "{}",
            heading_text(
                r,
                &format!(
                    "Restore results for {}",
                    output::terminal_text(&receipt.theme_name)
                )
            ),
        );
        println!(
            "{}",
            tree_branch_text(
                r,
                &format!("{} file(s) restored successfully", receipt.success_count()),
            ),
        );
        if !failures.is_empty() {
            println!(
                "{}",
                tree_branch_text(r, &format!("{} file(s) failed", receipt.failure_count())),
            );
            for result in &failures {
                println!(
                    "{}",
                    tree_branch_text(r, &status_error_line(r, &receipt::failure_text(result)),),
                );
            }
        }
    }
    if compact {
        println!(
            "{}slate restore {} --dry-run",
            tr("撤销前先查看：", "Review before undoing: "),
            receipt.pre_restore_point_id
        );
    } else {
        println!(
            "Undo this restore: slate restore {}",
            receipt.pre_restore_point_id
        );
    }
    if !failures.is_empty() {
        return Err(crate::error::SlateError::BackupFailed(format!(
            "Partial restore: {} file(s) failed. Theme reapplication was skipped. Undo with `slate restore {}`.",
            receipt.failure_count(), receipt.pre_restore_point_id,
        )));
    }

    // Only legacy named-theme points regenerate managed files. Theme-operation,
    // baseline, clean, import and undo points retain restored bytes and absence.
    // Reading post-restore state must not create an otherwise absent config dir.
    let config = crate::config::ConfigManager::from_env_paths(&env);
    if may_reapply_theme {
        if let Ok(Some(theme_id)) = config.get_current_theme() {
            let registry = crate::theme::ThemeRegistry::new()?;
            if let Some(theme) = registry.get(&theme_id) {
                println!(
                    "{}",
                    status_success_line(r, &format!("Re-applying theme: {}", theme.name)),
                );
                // Apply without snapshotting again (we just restored)
                let report = crate::cli::theme_apply::ThemeApplyCoordinator::with_snapshot_policy(
                    &env,
                    crate::cli::theme_apply::SnapshotPolicy::Skip,
                )
                .preserving_auto_pair()
                .apply(theme)
                .map_err(|error| crate::error::SlateError::BackupFailed(format!(
                    "Snapshot files were restored, but theme reapplication failed: {error} Undo with `slate restore {}`.",
                    receipt.pre_restore_point_id,
                )))?;
                crate::cli::theme_apply::log_apply_report(&report);
                if let Err(error) = report.ensure_no_failures() {
                    return Err(crate::error::SlateError::BackupFailed(format!(
                        "Snapshot files were restored, but theme reapplication failed: {error} Undo with `slate restore {}`.",
                        receipt.pre_restore_point_id,
                    )));
                }
            }
        }
    }

    // Files are already restored. A lifecycle failure must remain visible,
    // without undoing files or treating an unreadable preference as disabled.
    lifecycle::sync(
        env.session().is_isolated(),
        || config.is_auto_theme_enabled(),
        || crate::platform::dark_mode_notify::stop_with_env(&env),
        || crate::platform::dark_mode_notify::start(&config),
    ).map_err(|error| crate::error::SlateError::BackupFailed(format!(
        "Files were restored, but auto-theme watcher synchronization did not finish: {error} No file rollback was attempted. Inspect `slate doctor auto-theme`. Review file undo with `slate restore {} --dry-run`; file recovery does not restore processes.",
        receipt.pre_restore_point_id,
    )))?;

    // restore success → `RestoreComplete`. SoundSink
    // maps this to the restore-complete SFX. Unlike `clean` we do NOT
    // pair an `ApplyComplete` here because the re-applied theme above
    // already dispatches its own ApplyComplete via ThemeApplyCoordinator
    // if the plan future-wires it — and double-firing would let the
    // sound layer play the completion cue twice.
    dispatch(BrandEvent::Success(SuccessKind::RestoreComplete));

    if compact {
        println!(
            "{}",
            tr(
                "文件恢复结束；正在运行工具的外观未验证。",
                "File restore finished; running tools' appearance was not verified."
            )
        );
    } else {
        println!("{}", tree_end_text(r, "Back on track"));
    }
    println!();

    Ok(RestoreOutcome::Restored)
}

/// Read-only restore preview; avoid RenderContext and ConfigManager constructors.
pub fn handle_preview(id: &str, json: bool) -> Result<()> {
    let env = crate::env::SlateEnv::from_process()?;
    let plan = crate::config::preview_restore_with_env(&env, id)?;
    let output = if json {
        format!("{}\n", serde_json::to_string_pretty(&plan)?)
    } else {
        let mut text = preview::text(&plan);
        text.push_str("Preview only; no files or restore points were changed.\n");
        text
    };
    output::write_output(&output)?;
    ensure_plan_ready(&plan)
}

fn print_plan(plan: &crate::config::RestorePlan, compact: bool) -> Result<()> {
    // Unlike a read-only preview, an interactive restore must stop before
    // confirmation or file publication if its initial plan cannot be written.
    output::write_required(&if compact {
        preview::menu_text(plan)
    } else {
        preview::text(plan)
    })?;
    Ok(())
}

fn ensure_plan_ready(plan: &crate::config::RestorePlan) -> Result<()> {
    if plan.blocked_count() > 0 {
        return Err(crate::error::SlateError::BackupFailed(format!(
            "{} restore target(s) are blocked. No files were restored.",
            plan.blocked_count(),
        )));
    }
    Ok(())
}

/// Handle interactive restore point picker
fn handle_restore_picker(r: Option<&Roles<'_>>, hub_sound: Option<(bool, bool)>) -> Result<()> {
    let mut previous = String::new();
    while let Some(selected_id) = choose_restore_point(&previous, hub_sound.is_some())? {
        // Release the writer before navigation. Blocked plans cannot execute;
        // execution errors (including partial restores) still propagate.
        let outcome = {
            let env = crate::env::SlateEnv::from_process()?;
            let _write_guard = crate::config::ConfigWriteGuard::acquire(&env)?;
            handle_restore_direct(&selected_id, r, true, hub_sound)?
        };
        match outcome {
            RestoreOutcome::Restored => return Ok(()),
            RestoreOutcome::Declined => {}
            RestoreOutcome::Blocked => {
                select(tr(
                    "此恢复点当前无法恢复；没有恢复任何文件。",
                    "This restore point is blocked; no files were restored.",
                ))
                .item((), tr("返回恢复点列表", "Back to Restore Points"), "")
                .escape_value(())
                .interact()?;
            }
        }
        previous = selected_id;
    }
    Ok(())
}

fn empty_browser_return(from_hub: bool, message: &str) -> Result<Option<String>> {
    if from_hub {
        select(message)
            .item((), tr("返回主菜单", "Back"), "")
            .escape_value(())
            .interact()?;
    }
    Ok(None)
}

fn choose_restore_point(previous: &str, from_hub: bool) -> Result<Option<String>> {
    let inventory = inspect_restore_points_with_env(&crate::env::SlateEnv::from_process()?)?;
    listing::print_inventory_notes(&inventory)?;
    let restore_points = &inventory.points;

    if restore_points.is_empty() {
        if inventory.issues.is_empty() {
            if !from_hub {
                println!("{}", tr("还没有恢复点。Slate 更改配置前会保存备份。", "No restore points found. Slate saves backups before configuration changes."));
            }
        } else {
            println!(
                "{}",
                tr(
                    "没有可用恢复点，请检查上方列出的备份问题。",
                    "No usable restore points; inspect the backup entries listed above."
                )
            );
        }
        return empty_browser_return(
            from_hub,
            if inventory.issues.is_empty() {
                tr(
                    "还没有恢复点。Slate 更改配置前会保存备份。",
                    "No restore points yet. Slate saves backups before configuration changes.",
                )
            } else {
                tr(
                    "备份暂时无法读取，请检查上方列出的原因。",
                    "Backups could not be read; check the reasons above.",
                )
            },
        );
    }

    // Filter out pre-restore snapshots from user-facing picker
    let user_visible: Vec<_> = restore_points
        .iter()
        .filter(|p| !p.is_undo_checkpoint())
        .collect();

    if user_visible.is_empty() {
        println!("{}", tr("当前没有可选恢复点；用 --list --all 查看撤销点。", "No selectable restore points in this view. Undo checkpoints are available via --list --all."));
        return empty_browser_return(
            from_hub,
            tr(
                "这里只有撤销点；可用 slate restore --list --all 查看。",
                "Only undo points are available; see slate restore --list --all.",
            ),
        );
    }

    println!("{}", tr("从备份恢复配置", Language::RESTORE_HEADER));
    println!();

    // Build selection options with formatted labels
    // We need to own the strings to keep them alive during the items() call
    let options: Vec<(String, String, String)> = user_visible
        .iter()
        .enumerate()
        .map(|(index, p)| {
            (
                p.id.clone(),
                listing::menu_label(p, index),
                format!(
                    "{} {}",
                    p.entries.len(),
                    tr(
                        "个文件 · 选中后查看恢复范围，尚不执行",
                        "files · select to review, not restore"
                    )
                ),
            )
        })
        .collect();

    // Convert to borrowed slices for cliclack
    // Note: select().interact() returns the FIRST element of the tuple, not the index
    let select_items: Vec<(&str, &str, &str)> = options
        .iter()
        .map(|(id, theme, count_label)| (id.as_str(), theme.as_str(), count_label.as_str()))
        .collect();

    let initial = options
        .iter()
        .find(|(id, _, _)| id == previous)
        .unwrap_or(&options[0])
        .0
        .as_str();
    let selected_id = select(tr("选择恢复点：", "Choose restore point:"))
        .max_rows(8)
        .initial_value(initial)
        .items(&select_items)
        .item("", tr("返回", "Back"), "")
        .escape_value("")
        .interact()?;

    Ok((!selected_id.is_empty()).then(|| selected_id.to_owned()))
}

/// Handle deleting a restore point
fn handle_delete(restore_id: &str, r: Option<&Roles<'_>>) -> Result<()> {
    // Validate the restore point exists first
    let restore_point = get_restore_point(restore_id)?;

    // Prevent deleting baseline
    if is_baseline_restore_point(&restore_point) {
        println!(
            "{}",
            status_error_line(r, "Cannot delete baseline. This is a protected snapshot."),
        );
        return Ok(());
    }

    // Confirm deletion
    let confirmed = super::menu::confirm(format!(
        "Delete restore point {}? This cannot be undone.",
        restore_point.id
    ))
    .interact()?;

    if !confirmed {
        println!("Deletion cancelled.");
        return Ok(());
    }

    // Delete the restore point
    delete_restore_point(restore_id)?;
    println!("{}", status_success_line(r, "Restore point deleted"),);

    Ok(())
}

/// Render `◆ title` via `Roles::heading`, falling back to plain ◆ text
/// when Roles is unavailable (graceful degrade).
fn heading_text(r: Option<&Roles<'_>>, title: &str) -> String {
    match r {
        Some(r) => r.heading(title),
        None => format!("◆ {}", title),
    }
}

/// Render `┃ ├─ text` via `Roles::tree_branch`.
fn tree_branch_text(r: Option<&Roles<'_>>, text: &str) -> String {
    match r {
        Some(r) => r.tree_branch(text),
        None => format!("┃ ├─ {}", text),
    }
}

/// Render `└─ ★ text` via `Roles::tree_end`.
fn tree_end_text(r: Option<&Roles<'_>>, text: &str) -> String {
    match r {
        Some(r) => r.tree_end(text),
        None => format!("└─ ★ {}", text),
    }
}

/// Render `✓ message` via `Roles::status_success` (theme.green
/// NEVER lavender per D-01a), falling back to plain `✓ message`.
fn status_success_line(r: Option<&Roles<'_>>, message: &str) -> String {
    match r {
        Some(r) => r.status_success(message),
        None => format!("✓ {}", message),
    }
}

/// Render `✗ message` via `Roles::status_error` (theme.red — NEVER
/// lavender per D-01a), falling back to plain `✗ message`.
fn status_error_line(r: Option<&Roles<'_>>, message: &str) -> String {
    match r {
        Some(r) => r.status_error(message),
        None => format!("✗ {}", message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brand::render_context::{mock_context_with_mode, mock_theme, RenderMode};

    /// Helper: render the restore-complete tree receipt without driving
    /// the full `handle_restore_direct` flow (which requires live
    /// `RestorePoint` data + filesystem side effects). Mirrors the
    /// `println!` block inside `handle_restore_direct` exactly.
    fn render_restore_receipt(
        r: Option<&Roles<'_>>,
        theme_name: &str,
        succeeded: usize,
        failed: usize,
        failure_lines: &[(&str, &str)],
    ) -> String {
        let mut out = String::new();
        out.push('\n');
        out.push_str(&heading_text(r, &format!("Restored to {}", theme_name)));
        out.push('\n');
        out.push_str(&tree_branch_text(
            r,
            &format!("{} file(s) restored successfully", succeeded),
        ));
        out.push('\n');
        if failed > 0 {
            out.push_str(&tree_branch_text(r, &format!("{} file(s) failed", failed)));
            out.push('\n');
            for (tool, err) in failure_lines {
                out.push_str(&tree_branch_text(
                    r,
                    &status_error_line(r, &format!("{}: {}", tool, err)),
                ));
                out.push('\n');
            }
        }
        out.push_str(&tree_end_text(r, "Back on track"));
        out.push('\n');
        out
    }

    /// snapshot — byte-lock the `slate restore` completion tree
    /// in Basic mode (MockTheme stability).
    #[test]
    fn restore_summary_basic_snapshot() {
        let theme = mock_theme();
        let ctx = mock_context_with_mode(&theme, RenderMode::Basic);
        let r = Roles::new(&ctx);
        let out = render_restore_receipt(Some(&r), "Catppuccin Mocha", 5, 0, &[]);
        insta::assert_snapshot!("restore_summary_basic", out);
    }

    /// Truecolor variant — anchors every tree glyph to the brand
    /// lavender byte triple (`38;2;114;135;253`) per Sketch 002.
    #[test]
    fn restore_summary_truecolor_snapshot() {
        let theme = mock_theme();
        let ctx = mock_context_with_mode(&theme, RenderMode::Truecolor);
        let r = Roles::new(&ctx);
        let out = render_restore_receipt(Some(&r), "Catppuccin Mocha", 5, 0, &[]);
        assert!(
            out.contains("38;2;114;135;253"),
            "tree chrome must carry brand-lavender bytes in truecolor, got: {out:?}"
        );
        insta::assert_snapshot!("restore_summary_truecolor", out);
    }

    /// Partial-failure variant — locks the `✗` severity line wrapped
    /// inside a tree branch so the failure rendering stays stable.
    #[test]
    fn restore_summary_with_failures_basic_snapshot() {
        let theme = mock_theme();
        let ctx = mock_context_with_mode(&theme, RenderMode::Basic);
        let r = Roles::new(&ctx);
        let out = render_restore_receipt(
            Some(&r),
            "Catppuccin Mocha",
            3,
            2,
            &[
                ("ghostty", "permission denied"),
                ("starship", "file not found"),
            ],
        );
        insta::assert_snapshot!("restore_summary_with_failures_basic", out);
    }

    /// graceful degrade — without Roles the tree falls back to
    /// plain glyphs, zero ANSI bytes.
    #[test]
    fn restore_summary_falls_back_to_plain_when_roles_absent() {
        let out = render_restore_receipt(None, "Catppuccin Mocha", 5, 0, &[]);
        assert!(
            !out.contains('\x1b'),
            "plain fallback must contain no ANSI bytes, got: {out:?}"
        );
        assert!(out.contains("◆ Restored to Catppuccin Mocha"));
        assert!(out.contains("┃ ├─ 5 file(s) restored successfully"));
        assert!(out.contains("└─ ★ Back on track"));
    }

    /// D-01a invariant — `status_error_line` uses theme.red, never
    /// brand lavender, across every RenderMode. Covers both the direct
    /// error line (e.g. the baseline-delete rejection) and the wrapped
    /// error lines inside the failure tree branches.
    #[test]
    fn status_error_line_never_emits_brand_lavender() {
        let theme = mock_theme();
        for mode in [RenderMode::Truecolor, RenderMode::Basic, RenderMode::None] {
            let ctx = mock_context_with_mode(&theme, mode);
            let r = Roles::new(&ctx);
            let out = status_error_line(Some(&r), "permission denied");
            assert!(
                !out.contains("38;2;114;135;253"),
                "D-01a violation in mode {mode:?}: {out:?}"
            );
        }
    }

    /// D-01a invariant — `status_success_line` uses theme.green, never
    /// brand lavender.
    #[test]
    fn status_success_line_never_emits_brand_lavender() {
        let theme = mock_theme();
        for mode in [RenderMode::Truecolor, RenderMode::Basic, RenderMode::None] {
            let ctx = mock_context_with_mode(&theme, mode);
            let r = Roles::new(&ctx);
            let out = status_success_line(Some(&r), "Restore point deleted");
            assert!(
                !out.contains("38;2;114;135;253"),
                "D-01a violation in mode {mode:?}: {out:?}"
            );
        }
    }
}
