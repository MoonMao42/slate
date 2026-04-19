use crate::brand::events::{dispatch, BrandEvent, SuccessKind};
use crate::brand::language::Language;
use crate::brand::render_context::RenderContext;
use crate::brand::roles::Roles;
use crate::config::{
    delete_restore_point, execute_restore, get_restore_point, is_baseline_restore_point,
    list_restore_points,
};
use crate::error::Result;
use cliclack::{confirm, select};

/// Handle `slate restore [ID] [--list] [--delete ID]` with structured clap arguments.
pub fn handle(restore_id: Option<&str>, list_mode: bool, delete_id: Option<&str>) -> Result<()> {
    // Build a RenderContext once at the top so every sub-handler shares
    // the same byte contract (D-01 daily chrome + sketch 003 tree shape +
    // D-01a severity). D-05 graceful degrade — plain text when the theme
    // registry cannot boot.
    let ctx = RenderContext::from_active_theme().ok();
    let roles = ctx.as_ref().map(Roles::new);

    if let Some(id) = delete_id {
        handle_delete(id, roles.as_ref())?;
        return Ok(());
    }

    if list_mode {
        handle_list()?;
        return Ok(());
    }

    if let Some(id) = restore_id {
        handle_restore_direct(id, roles.as_ref())?;
    } else {
        handle_restore_picker(roles.as_ref())?;
    }

    Ok(())
}

/// Handle list restore points
fn handle_list() -> Result<()> {
    let restore_points = list_restore_points()?;

    if restore_points.is_empty() {
        println!("{}", Language::RESTORE_NO_POINTS);
        return Ok(());
    }

    println!("{}", Language::RESTORE_LIST_HEADER);
    for point in restore_points {
        // Skip pre-restore snapshots in list
        if point.theme_name.starts_with("pre-restore-snapshot") {
            continue;
        }
        let summary =
            Language::restore_point_summary(&point.id, &point.theme_name, point.entries.len());
        println!("{}", summary);
    }

    Ok(())
}

/// Handle direct restore with explicit ID
fn handle_restore_direct(restore_id: &str, r: Option<&Roles<'_>>) -> Result<()> {
    // Validate the restore point exists
    let restore_point = get_restore_point(restore_id)?;

    // Baseline restore gets an extra warning
    let confirmed = if is_baseline_restore_point(&restore_point) {
        confirm("⚠ This will restore all config files to their state BEFORE slate was installed. Any manual changes you made since then will be lost. Continue?")
            .initial_value(false)
            .interact()?
    } else {
        confirm(format!(
            "Restore to {}? This will modify your configuration files.",
            restore_point.theme_name
        ))
        .interact()?
    };

    if !confirmed {
        println!("Restore cancelled.");
        return Ok(());
    }

    // Execute restore
    let receipt = execute_restore(restore_id)?;

    // D-10: completion receipt is a static tree-narrative anchor —
    // bypass cliclack and println! via Roles::heading / tree_branch /
    // tree_end. Sketch 003 canon: `◆ Restored … ┃ ├─ … └─ ★ Back on track`.
    let failures = receipt.failed_results();
    println!();
    println!(
        "{}",
        heading_text(r, &format!("Restored to {}", receipt.theme_name)),
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
            if let Some(error) = &result.error {
                println!(
                    "{}",
                    tree_branch_text(
                        r,
                        &status_error_line(r, &format!("{}: {}", result.display_tool, error)),
                    ),
                );
            }
        }
    }
    println!("{}", tree_end_text(r, "Back on track"));
    println!();

    // Re-apply the restored theme so managed files match the restored state
    let env = crate::env::SlateEnv::from_process()?;
    let config = crate::config::ConfigManager::with_env(&env)?;
    if let Ok(Some(theme_id)) = config.get_current_theme() {
        let registry = crate::theme::ThemeRegistry::new()?;
        if let Some(theme) = registry.get(&theme_id) {
            println!(
                "{}",
                status_success_line(r, &format!("Re-applying theme: {}", theme.name)),
            );
            // Apply without snapshotting again (we just restored)
            let report = crate::cli::theme_apply::ThemeApplyCoordinator::new(&env).apply(theme)?;
            crate::cli::theme_apply::log_apply_report(&report);
        }
    }

    // Sync watcher state: stop if auto-theme is now disabled, restart if enabled
    let _ = crate::platform::dark_mode_notify::stop();
    if config.is_auto_theme_enabled().unwrap_or(false) {
        let _ = crate::platform::dark_mode_notify::start(&config);
    }

    // D-17: restore success → `RestoreComplete`. Phase 20's SoundSink
    // maps this to the restore-complete SFX. Unlike `clean` we do NOT
    // pair an `ApplyComplete` here because the re-applied theme above
    // already dispatches its own ApplyComplete via ThemeApplyCoordinator
    // if the plan future-wires it — and double-firing would let the
    // sound layer play the completion cue twice.
    dispatch(BrandEvent::Success(SuccessKind::RestoreComplete));

    Ok(())
}

/// Handle interactive restore point picker
fn handle_restore_picker(r: Option<&Roles<'_>>) -> Result<()> {
    let restore_points = list_restore_points()?;

    if restore_points.is_empty() {
        println!("{}", Language::RESTORE_NO_POINTS);
        return Ok(());
    }

    // Filter out pre-restore snapshots from user-facing picker
    let user_visible: Vec<_> = restore_points
        .iter()
        .filter(|p| !p.theme_name.starts_with("pre-restore-snapshot"))
        .collect();

    if user_visible.is_empty() {
        println!("{}", Language::RESTORE_NO_POINTS);
        return Ok(());
    }

    println!("{}", Language::RESTORE_HEADER);
    println!();

    // Build selection options with formatted labels
    // We need to own the strings to keep them alive during the items() call
    let options: Vec<(String, String, String)> = user_visible
        .iter()
        .map(|p| {
            (
                p.id.clone(),
                p.theme_name.clone(),
                format!("{} files", p.entries.len()),
            )
        })
        .collect();

    // Convert to borrowed slices for cliclack
    // Note: select().interact() returns the FIRST element of the tuple, not the index
    let select_items: Vec<(&str, &str, &str)> = options
        .iter()
        .map(|(id, theme, count_label)| (id.as_str(), theme.as_str(), count_label.as_str()))
        .collect();

    let selected_id = select("Choose restore point:")
        .items(&select_items)
        .interact()?;

    handle_restore_direct(selected_id, r)?;

    Ok(())
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
    let confirmed = confirm(format!(
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
/// when Roles is unavailable (D-05 graceful degrade).
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

/// Render `✓ message` via `Roles::status_success` (theme.green —
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

    /// Wave 4 snapshot — byte-lock the `slate restore` completion tree
    /// in Basic mode (D-06 MockTheme stability).
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

    /// D-05 graceful degrade — without Roles the tree falls back to
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
