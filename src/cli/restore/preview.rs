use super::output::terminal_text;
use crate::cli::ui_language::tr;
use crate::config::{RestoreAction, RestorePlan};
use std::fmt::Write;

pub(super) fn menu_confirmation(plan: &RestorePlan) -> String {
    if plan.is_baseline {
        tr("恢复到安装 Slate 前的备份？上述文件后来的修改会被覆盖或移除。", "Restore the pre-Slate backup? Later changes to the listed files will be overwritten or removed.").into()
    } else {
        format!(
            "{}{}{}",
            tr("恢复到「", "Restore to “"),
            terminal_text(super::listing::menu_name(
                &plan.theme_name,
                plan.is_baseline
            )),
            tr(
                "」？将按上面的清单修改文件。",
                "”? Files will change as listed above."
            )
        )
    }
}

/// Menu review emphasizes actual changes; full CLI/JSON keeps every target.
pub(super) fn menu_text(plan: &RestorePlan) -> String {
    let mut output = format!(
        "\n{} · {}\n",
        tr("恢复预览", "Restore Preview"),
        terminal_text(super::listing::menu_name(
            &plan.theme_name,
            plan.is_baseline
        ))
    );
    let unchanged = plan
        .changes
        .iter()
        .filter(|c| c.action == RestoreAction::Unchanged)
        .count();
    let _ = writeln!(
        output,
        "{} {} {} · {} {} · {} {}\n",
        tr("将更改", "Will change"),
        plan.changed_count(),
        tr("个文件", "files"),
        unchanged,
        tr("个无需更改", "unchanged"),
        plan.blocked_count(),
        tr("个受阻", "blocked")
    );
    for change in &plan.changes {
        if change.action == RestoreAction::Unchanged && change.reason.is_none() {
            continue;
        }
        let action = match change.action {
            RestoreAction::Create => tr("新建", "Create"),
            RestoreAction::Replace => tr("覆盖", "Replace"),
            RestoreAction::Remove => tr("删除", "Remove"),
            RestoreAction::Unchanged => tr("保留", "Keep"),
            RestoreAction::Blocked => tr("受阻", "Blocked"),
        };
        let _ = writeln!(
            output,
            "  {}  {}{}",
            action,
            terminal_text(&change.original_path.to_string_lossy()),
            if change.original_path.to_str().is_none() {
                tr("（路径显示不完整）", " (lossy path display)")
            } else {
                ""
            }
        );
        if let Some(reason) = &change.reason {
            let _ = writeln!(output, "        {}", terminal_text(reason));
        }
    }
    output.push_str(if plan.may_regenerate_theme_files {
        tr("\n注意：恢复后还会重新生成主题配置，上面未列出这些额外更改。\n", "\nTheme configuration will also be regenerated; those additional changes are not listed above.\n")
    } else {
        tr("\n只恢复备份记录的文件内容、权限和原先不存在的状态，不重新生成主题。\n", "\nOnly recorded file contents, permissions and original absence are restored; the theme is not regenerated.\n")
    });
    output.push_str(tr("确认前不会恢复文件；执行前会创建撤销点。部分失败不会自动回滚。\n", "Nothing is restored before confirmation. An undo point precedes execution; partial failures are not rolled back automatically.\n"));
    let command = format!(
        "slate restore {} --dry-run",
        crate::detection::shell_quote(&plan.restore_point_id)
    );
    let _ = writeln!(
        output,
        "{}{}\n",
        tr("完整文件清单：", "Full file list: "),
        terminal_text(&command)
    );
    output
}

/// Display an already captured plan without filesystem reads or terminal writes.
/// The JSON plan remains unchanged; escaping is only a human-display concern.
pub(super) fn text(plan: &RestorePlan) -> String {
    let mut output = format!(
        "◆ Restore preview: {} ({})\n",
        terminal_text(&plan.theme_name),
        terminal_text(&plan.restore_point_id),
    );
    for change in &plan.changes {
        let _ = writeln!(
            output,
            "  {:9} {}{}",
            change.action.to_string(),
            terminal_text(&change.original_path.to_string_lossy()),
            if change.original_path.to_str().is_none() {
                " (lossy display; not an exact path)"
            } else {
                ""
            },
        );
        if let Some(reason) = &change.reason {
            let _ = writeln!(output, "            {}", terminal_text(reason));
        }
    }
    let _ = writeln!(
        output,
        "{} file(s) would change; {} blocked.",
        plan.changed_count(),
        plan.blocked_count()
    );
    output.push_str(if plan.may_regenerate_theme_files {
        "After restoring these snapshot files, Slate also regenerates managed theme files. Those additional changes are not included in this file preview.\n"
    } else {
        "File-only restore: recorded bytes, permissions and prior absence; no theme regeneration.\n"
    });
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{RestoreAction, RestoreChange};
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};

    #[test]
    fn restore_menu_review_hides_only_unremarkable_unchanged_targets() {
        let mut plan = RestorePlan {
            restore_point_id: "point'quoted".into(),
            theme_name: "Nord\n\u{202e}".into(),
            is_baseline: false,
            may_regenerate_theme_files: false,
            changes: [
                RestoreAction::Create,
                RestoreAction::Replace,
                RestoreAction::Remove,
                RestoreAction::Blocked,
                RestoreAction::Unchanged,
                RestoreAction::Unchanged,
            ]
            .into_iter()
            .enumerate()
            .map(|(index, action)| RestoreChange {
                tool_key: "fixture".into(),
                display_tool: "fixture".into(),
                original_path: format!("/target-{index}").into(),
                action,
                reason: matches!(index, 3 | 5).then(|| "reason\n\u{1b}".into()),
            })
            .collect(),
        };
        assert!(menu_confirmation(&plan).contains("Nord\\n\\u{202e}"));
        plan.is_baseline = true;
        assert!(menu_confirmation(&plan).contains("覆盖或移除"));
        assert!(!menu_confirmation(&plan).contains("Nord"));
        plan.is_baseline = false;
        for regenerate in [false, true] {
            plan.may_regenerate_theme_files = regenerate;
            let output = menu_text(&plan);
            assert!(output.contains("Nord\\n\\u{202e}"));
            assert!(output.contains("将更改 3 个文件 · 2 个无需更改 · 1 个受阻"));
            for index in [0, 1, 2, 3, 5] {
                assert!(output.contains(&format!("/target-{index}")));
            }
            assert!(!output.contains("/target-4"));
            assert!(output.contains("reason\\n\\u{1b}"));
            assert!(output.contains("部分失败不会自动回滚"));
            assert_eq!(output.contains("额外更改"), regenerate);
            assert_eq!(output.contains("不重新生成主题"), !regenerate);
            assert!(output.contains(&crate::detection::shell_quote(&plan.restore_point_id)));
            assert!(text(&plan).contains("/target-4"));
        }
    }

    #[test]
    fn restore_preview_text_escapes_all_display_fields_and_keeps_plan_semantics() {
        // Synthetic display inputs do not bypass the manifest's stricter path
        // validation or claim that non-UTF-8 restore records can be persisted.
        let mut plan = RestorePlan {
            restore_point_id: "id\n".into(),
            theme_name: "中文\u{202e}name".into(),
            is_baseline: false,
            may_regenerate_theme_files: false,
            changes: vec![RestoreChange {
                tool_key: "fixture".into(),
                display_tool: "fixture".into(),
                original_path: OsString::from_vec(b"/path-\xff\x1b".to_vec()).into(),
                action: RestoreAction::Blocked,
                reason: Some("conflict\r\t\u{2066}".into()),
            }],
        };
        for regenerate in [false, true] {
            plan.may_regenerate_theme_files = regenerate;
            let output = text(&plan);
            assert!(output.contains("中文\\u{202e}name (id\\n)"));
            assert!(output.contains("/path-\u{fffd}\\u{1b} (lossy display; not an exact path)"));
            assert!(output.contains("conflict\\r\\t\\u{2066}"));
            assert!(output.contains("0 file(s) would change; 1 blocked."));
            assert_eq!(
                output.contains("also regenerates managed theme files"),
                regenerate
            );
            assert_eq!(output.contains("File-only restore:"), !regenerate);
            assert_eq!(plan.blocked_count(), 1);
            let compact = menu_text(&plan);
            assert!(compact.contains("/path-\u{fffd}\\u{1b}（路径显示不完整）"));
            assert!(compact.contains("conflict\\r\\t\\u{2066}"));
        }
    }
}
