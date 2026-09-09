//! Compact file-only menu view; native validation remains an explicit CLI action.
use super::{integrations::terminal_text, terminal_path, GhosttyDoctorReport};
use std::fmt::Write;

pub(super) fn render(report: &GhosttyDoctorReport) -> String {
    let mut out = String::from("◆ Ghostty · 文件检查\n");
    if let Some(entry) = report.entries.iter().find(|entry| entry.selected) {
        let _ = writeln!(
            out,
            "Slate 写入入口：{}{}",
            terminal_path(&entry.path),
            if entry.exists {
                ""
            } else {
                "（尚不存在）"
            }
        );
    } else {
        out.push_str("待确认：没有找到 Slate 写入入口。\n");
    }
    if !report.scan_issues.is_empty() {
        out.push_str("检查未完成：以下文件需要先处理，不能据此判断配置正常。\n");
        for issue in &report.scan_issues {
            let _ = writeln!(
                out,
                "  {}：{}\n    {}{}",
                terminal_text(issue.code),
                terminal_text(&issue.message),
                terminal_text(&issue.path),
                if issue.path_is_lossy {
                    " (lossy display; not an exact path)"
                } else {
                    ""
                }
            );
        }
    } else if report
        .entries
        .iter()
        .any(|entry| !entry.slate_refs.is_empty())
    {
        out.push_str("已发现 Slate 配置引用；不代表当前窗口已使用这些设置。\n");
    } else {
        out.push_str("尚未发现 Slate 配置引用；可先在工具页预览同步改动。\n");
    }
    for (reference, paths) in &report.duplicate_refs {
        let _ = writeln!(out, "重复引用：{}", terminal_text(reference));
        for path in paths {
            let _ = writeln!(out, "  {}", terminal_path(path));
        }
    }
    for cycle in &report.config_file_cycles {
        let _ = writeln!(
            out,
            "循环引用：{}",
            cycle
                .iter()
                .map(|p| terminal_path(p))
                .collect::<Vec<_>>()
                .join(" → ")
        );
    }
    if !report.duplicate_refs.is_empty() || !report.config_file_cycles.is_empty() {
        out.push_str("请核对上述 config-file 引用，保留需要的设置后再修正重复或循环关系。\n");
    }
    for assignment in &report.window_style_overrides {
        let _ = writeln!(
            out,
            "标题栏设置仍在生成文件内：{}{}:{}",
            terminal_text(&assignment.path),
            if assignment.path_is_lossy {
                " (lossy display; not an exact path)"
            } else {
                ""
            },
            assignment.first_assignment_line
        );
    }
    if !report.window_style_overrides.is_empty() {
        out.push_str("建议用当前 Slate 重新应用主题；个人 macos-titlebar-style 设置应放在自己的 Ghostty 配置中。\n");
    }
    out.push_str("未修改文件、启动校验或重载窗口；实际配色与标题栏外观未验证。\n");
    out.push_str("完整文件检查：slate doctor ghostty --files-only（不启动原生校验）\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cli::doctor::{ghostty_scan, ghostty_window_style, GhosttyConfigEntry, GhosttyValidation},
        env::SlateEnv,
    };
    use std::path::{Path, PathBuf};

    #[test]
    fn ghostty_menu_retains_problems_without_routine_diagnostics() {
        let temp = tempfile::tempdir().unwrap();
        let mut report = crate::cli::doctor::build_ghostty_report_with_validator(
            &SlateEnv::with_home(temp.path().into()),
            None,
        )
        .unwrap();
        let empty = render(&report);
        assert!(empty.contains("尚不存在"));
        assert!(empty.contains("尚未发现 Slate 配置引用"));
        assert!(!empty.contains("candidate entries:"));
        assert!(empty.contains("实际配色与标题栏外观未验证"));
        assert!(empty.contains("完整文件检查：slate doctor ghostty --files-only"));
        assert!(!empty.contains("会尝试运行"));
        assert!(matches!(report.validation, GhosttyValidation::Skipped(_)));
        report.entries = vec![GhosttyConfigEntry {
            label: "fixture",
            path: PathBuf::from("/entry"),
            exists: true,
            slate_refs: vec!["/theme".into()],
            selected: true,
            load_order_index: 0,
        }];
        assert!(render(&report).contains("已发现 Slate 配置引用"));
        report.duplicate_refs = vec![(
            "/theme".into(),
            vec![PathBuf::from("/a"), PathBuf::from("/b")],
        )];
        report.config_file_cycles = vec![vec![PathBuf::from("/a"), PathBuf::from("/a")]];
        report.scan_issues.push(ghostty_scan::Issue {
            code: "fixture",
            message: "Unreadable\u{1b}[2J".into(),
            path: "/bad".into(),
            path_is_lossy: true,
        });
        report
            .window_style_overrides
            .push(ghostty_window_style::Assignment::new(
                Path::new("/theme"),
                7,
            ));
        let problems = render(&report);
        for expected in [
            "检查未完成",
            "重复引用：/theme",
            "/b",
            "循环引用：/a → /a",
            "Unreadable\\u{1b}[2J",
            "lossy display",
            "/theme:7",
            "config-file",
            "macos-titlebar-style",
        ] {
            assert!(
                problems.contains(expected),
                "missing {expected}: {problems}"
            );
        }
        assert!(!problems.contains('\u{1b}'));
        assert!(!problems.contains("已发现 Slate 配置引用"));
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
    }
}
