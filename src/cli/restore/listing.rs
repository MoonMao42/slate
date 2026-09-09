use super::output::{terminal_text, write_output};
use crate::brand::language::Language;
use crate::cli::ui_language::tr;
use crate::config::{
    inspect_restore_points_with_env, RestoreInventory, RestoreInventoryIssue, RestorePoint,
};
use crate::env::SlateEnv;
use crate::error::Result;
use serde::Serialize;
use std::fmt::Write as _;

/// Translate known operation categories for menus, retaining raw metadata.
pub(super) fn menu_name(name: &str, baseline: bool) -> &str {
    if baseline {
        return tr("初始备份", "Initial Backup");
    }
    match name {
        "pre-config" => tr("设置修改前", "Before Settings Change"),
        "pre-font" => tr("字体修改前", "Before Font Change"),
        "pre-theme" => tr("主题修改前", "Before Theme Change"),
        "pre-opacity" => tr("透明度修改前", "Before Opacity Change"),
        "pre-import" => tr("导入配置前", "Before Configuration Import"),
        _ => name,
    }
}

/// Use validated manifest time, not an assumed timestamp prefix in the ID.
pub(super) fn menu_label(point: &RestorePoint, index: usize) -> String {
    let stamp = crate::config::format_iso8601_timestamp(point.created_at);
    let (date, clock) = stamp.split_once('T').expect("generated ISO timestamp");
    format!(
        "{}. {} {} UTC · {}",
        index + 1,
        date,
        clock.trim_end_matches('Z').replace('-', ":"),
        menu_name(&point.theme_name, point.is_baseline)
    )
}

#[derive(Serialize)]
struct ListedPoint<'a> {
    id: &'a str,
    theme_name: &'a str,
    created_at_unix_seconds: u64,
    entry_count: usize,
    tools: Vec<String>,
    is_baseline: bool,
    is_undo: bool,
    may_regenerate_theme_files: bool,
}

impl<'a> From<&'a RestorePoint> for ListedPoint<'a> {
    fn from(point: &'a RestorePoint) -> Self {
        Self {
            id: &point.id,
            theme_name: &point.theme_name,
            created_at_unix_seconds: point
                .created_at
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            entry_count: point.entries.len(),
            tools: crate::config::display_tools(&point.entries),
            is_baseline: point.is_baseline,
            is_undo: point.is_undo_checkpoint(),
            may_regenerate_theme_files: point.reapplies_theme(),
        }
    }
}

#[derive(Serialize)]
struct ListReport<'a> {
    schema_version: u32,
    backup_directory: String,
    backup_directory_is_lossy: bool,
    /// Metadata inventory is not a byte comparison with current restore targets.
    target_contents_checked: bool,
    includes_undo: bool,
    valid_count: usize,
    hidden_undo_count: usize,
    ignored_entries: usize,
    points: Vec<ListedPoint<'a>>,
    issues: &'a [RestoreInventoryIssue],
}

fn report(inventory: &RestoreInventory, all: bool) -> ListReport<'_> {
    let points: Vec<_> = inventory
        .points
        .iter()
        .filter(|point| all || !point.is_undo_checkpoint())
        .map(ListedPoint::from)
        .collect();
    ListReport {
        schema_version: 1,
        backup_directory: inventory.backup_directory.display().to_string(),
        backup_directory_is_lossy: inventory.backup_directory.to_str().is_none(),
        target_contents_checked: false,
        includes_undo: all,
        valid_count: inventory.points.len(),
        hidden_undo_count: inventory.points.len() - points.len(),
        ignored_entries: inventory.ignored_entries,
        points,
        issues: &inventory.issues,
    }
}

fn text_report(report: &ListReport<'_>) -> String {
    let mut output = String::new();
    if report.points.is_empty() {
        if report.valid_count == 0 && report.issues.is_empty() {
            writeln!(output, "{}", Language::RESTORE_NO_POINTS).unwrap();
        } else {
            writeln!(output, "No usable restore points in this view.").unwrap();
        }
    } else {
        writeln!(output, "{}", Language::RESTORE_LIST_HEADER).unwrap();
        for point in &report.points {
            writeln!(
                output,
                "{}",
                terminal_text(&Language::restore_point_summary(
                    point.id,
                    point.theme_name,
                    point.entry_count
                ))
            )
            .unwrap();
        }
    }
    output.push_str(&text_notes(report));
    output
}

fn text_notes(report: &ListReport<'_>) -> String {
    let mut output = String::new();
    if report.hidden_undo_count > 0 {
        writeln!(
            output,
            "{} undo checkpoint(s) hidden; use: slate restore --list --all",
            report.hidden_undo_count
        )
        .unwrap();
    }
    if !report.issues.is_empty() {
        writeln!(
            output,
            "\n{} backup entry(s) need attention (not deleted):",
            report.issues.len()
        )
        .unwrap();
        for issue in report.issues {
            writeln!(
                output,
                "  {}\n    {}\n    {}",
                terminal_text(&issue.path),
                terminal_text(&issue.message),
                terminal_text(&issue.next_step)
            )
            .unwrap();
        }
    }
    output
}

pub(super) fn print_inventory_notes(inventory: &RestoreInventory) -> Result<()> {
    write_output(&text_notes(&report(inventory, false)))
}

pub fn handle_list_with_options(json: bool, all: bool) -> Result<()> {
    let env = SlateEnv::from_process()?;
    let inventory = inspect_restore_points_with_env(&env)?;
    let report = report(&inventory, all);
    let output = if json {
        format!("{}\n", serde_json::to_string_pretty(&report)?)
    } else {
        text_report(&report)
    };
    write_output(&output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_checkpoint_names_are_readable_without_changing_raw_metadata() {
        for (raw, label) in [
            ("pre-config", "设置修改前"),
            ("pre-font", "字体修改前"),
            ("pre-theme", "主题修改前"),
            ("pre-opacity", "透明度修改前"),
            ("pre-import", "导入配置前"),
            ("Nord", "Nord"),
            ("pre-future-operation", "pre-future-operation"),
        ] {
            let point = RestorePoint {
                id: "fixture".into(),
                theme_name: raw.into(),
                created_at: std::time::UNIX_EPOCH,
                entries: vec![],
                is_baseline: false,
            };
            assert!(menu_label(&point, 0).ends_with(label));
            assert_eq!(ListedPoint::from(&point).theme_name, raw);
        }
        assert_eq!(menu_name("pre-config", true), "初始备份");
    }

    #[test]
    fn inventory_text_preserves_readable_unicode_and_escapes_terminal_controls() {
        assert_eq!(
            terminal_text("中文 \u{1b}[2J\n\u{202e}name"),
            "中文 \\u{1b}[2J\\n\\u{202e}name"
        );
    }
}
