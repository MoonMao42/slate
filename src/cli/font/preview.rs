//! Read-only projection of the same selection and prepared bytes as `font`.
//! A report is neither a writable-file reservation nor native activation proof.
use super::{
    choices::terminal_text,
    listing::{Issue, PathView},
    resolve_font_choice_with_scan, ResolvedFontChoice,
};
use crate::cli::ui_language::tr;
use crate::{
    adapter::font::{FontAdapter, FontFileAction, FontScanReport, PreparedFont},
    env::SlateEnv,
    error::Result,
    platform::fonts,
};
use serde::Serialize;
use std::io::Write;

const SCOPE: &str = "Read-only configuration projection from the current captured files and filename/header font discovery. No lock, recovery point, download, subprocess, cache refresh, write or terminal reload is performed. Candidate observation does not prove native font matching or glyph rendering. File actions are in publication order; unchanged/absent entries are checked but not written. Contents are omitted. Only the first observed blocker is reported; an incomplete file plan lists no actions. Write permissions, backup creation, active writers, pending recovery, network access and native activation are not tested. Actual application rechecks current state and can fail or leave earlier writes in place. File recovery excludes installed fonts, external caches, empty directories and live windows.";

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum Source {
    ObservedCandidate,
    Catalog,
}
#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum Installation {
    NotPlanned,
    NotRequested,
    WouldRequest,
}
#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum Checkpoint {
    NotPlanned,
    NotNeeded,
    WouldCreate,
}
#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum Reload {
    NotPlanned,
    SessionSuppressed,
    WouldRequestAfterCommit,
}
#[derive(Serialize)]
struct Blocker {
    stage: &'static str,
    reason: String,
}
#[derive(Serialize)]
struct FileAction {
    #[serde(flatten)]
    path: PathView,
    action: FontFileAction,
    before_bytes: Option<usize>,
    after_bytes: Option<usize>,
}
#[derive(Serialize)]
struct Preview {
    schema_version: u8,
    scope: &'static str,
    backend: &'static str,
    requested_family: String,
    resolved_family: Option<String>,
    selection_source: Option<Source>,
    scan_complete: bool,
    search_roots: Vec<PathView>,
    scan_issues: Vec<Issue>,
    omitted_issue_count: usize,
    file_plan_complete: bool,
    execution_readiness_checked: bool,
    files: Vec<FileAction>,
    installation: Installation,
    pre_font_checkpoint: Checkpoint,
    terminal_reload: Reload,
    blocker: Option<Blocker>,
}

fn report(env: &SlateEnv, request: &str, scan: &FontScanReport) -> Preview {
    let mut report = Preview {
        schema_version: 1,
        scope: SCOPE,
        backend: fonts::backend().label(),
        requested_family: request.into(),
        resolved_family: None,
        selection_source: None,
        scan_complete: scan.is_complete(),
        search_roots: fonts::font_search_paths(env)
            .iter()
            .map(|path| path.as_path().into())
            .collect(),
        scan_issues: scan.issues.iter().map(Issue::from).collect(),
        omitted_issue_count: scan.omitted_issues,
        file_plan_complete: false,
        execution_readiness_checked: false,
        files: Vec::new(),
        installation: Installation::NotPlanned,
        pre_font_checkpoint: Checkpoint::NotPlanned,
        terminal_reload: Reload::NotPlanned,
        blocker: None,
    };
    let selection = match resolve_font_choice_with_scan(request, scan) {
        Ok(selection) => selection,
        Err(error) => {
            report.blocker = Some(Blocker {
                stage: "selection",
                reason: error.to_string(),
            });
            return report;
        }
    };
    report.resolved_family = Some(selection.font_name().into());
    report.selection_source = Some(match selection {
        ResolvedFontChoice::Installed(_) => Source::ObservedCandidate,
        ResolvedFontChoice::Catalog(_) => Source::Catalog,
    });
    let prepared = match PreparedFont::capture(env, selection.font_name()) {
        Ok(prepared) => prepared,
        Err(error) => {
            report.blocker = Some(Blocker {
                stage: "configuration",
                reason: error.to_string(),
            });
            return report;
        }
    };
    report.installation = match selection {
        ResolvedFontChoice::Installed(_) => Installation::NotRequested,
        ResolvedFontChoice::Catalog(_) => Installation::WouldRequest,
    };
    report.pre_font_checkpoint = if prepared.changed() {
        Checkpoint::WouldCreate
    } else {
        Checkpoint::NotNeeded
    };
    report.terminal_reload = if env.session().can_reload_terminal() {
        Reload::WouldRequestAfterCommit
    } else {
        Reload::SessionSuppressed
    };
    report.files = prepared
        .file_plan()
        .map(|file| FileAction {
            path: file.path.as_path().into(),
            action: file.action,
            before_bytes: file.before_bytes,
            after_bytes: file.after_bytes,
        })
        .collect();
    report.file_plan_complete = true;
    report
}

fn text_report(report: &Preview) -> String {
    use std::fmt::Write;
    let mut output = String::from("Font change preview — no changes made\n");
    let _ = writeln!(
        output,
        "Requested: {}",
        terminal_text(&report.requested_family)
    );
    if let Some(family) = &report.resolved_family {
        let _ = writeln!(output, "Resolved: {}", terminal_text(family));
    }
    if let Some(blocker) = &report.blocker {
        let _ = writeln!(
            output,
            "Blocked at {}: {}\nNo file actions planned.",
            blocker.stage,
            terminal_text(&blocker.reason)
        );
    }
    let install = match report.installation {
        Installation::NotPlanned => "not planned (resolve the blocker first)",
        Installation::NotRequested => "not requested (filename/header candidate observed)",
        Installation::WouldRequest => {
            "would request catalog installation before writing configuration"
        }
    };
    let checkpoint = match report.pre_font_checkpoint {
        Checkpoint::NotPlanned => "not planned",
        Checkpoint::NotNeeded => "not needed (no configuration changes)",
        Checkpoint::WouldCreate => "would create pre-font before any catalog installation",
    };
    let reload = match report.terminal_reload {
        Reload::NotPlanned => "not planned",
        Reload::SessionSuppressed => "suppressed by this session (remote or isolated)",
        Reload::WouldRequestAfterCommit => "would request Ghostty reload after successful commit",
    };
    let _ = writeln!(
        output,
        "Installation: {install}\nRecovery: {checkpoint}\nTerminal reload: {reload}"
    );
    for file in &report.files {
        let action = match file.action {
            FontFileAction::Create => "create",
            FontFileAction::Update => "update",
            FontFileAction::Unchanged => "unchanged",
            FontFileAction::PreserveAbsent => "keep absent",
        };
        let _ = writeln!(
            output,
            "  {action}: {}{}",
            terminal_text(&file.path.path),
            if file.path.path_is_lossy {
                " (lossy path display)"
            } else {
                ""
            }
        );
    }
    let _ = writeln!(
        output,
        "\nScan: {}; {} issue(s), {} more omitted.",
        if report.scan_complete {
            "complete within supported search"
        } else {
            "incomplete"
        },
        report.scan_issues.len(),
        report.omitted_issue_count
    );
    for issue in &report.scan_issues {
        let _ = writeln!(
            output,
            "  {}: {}{}",
            issue.reason,
            terminal_text(&issue.path.path),
            if issue.path.path_is_lossy {
                " (lossy path display)"
            } else {
                ""
            }
        );
    }
    let _ = writeln!(output, "\n{}", report.scope);
    output
}

/// A produced preview, including a blocker, exits successfully. Consumers must
/// inspect file_plan_complete and blocker; it is not an apply-readiness verdict.
pub fn handle_preview(env: &SlateEnv, name: &str, json: bool) -> Result<()> {
    crate::adapter::font_config::validate_family(name)?;
    let report = report(env, name, &FontAdapter::scan_fonts_with_env(env));
    let output = if json {
        format!("{}\n", serde_json::to_string_pretty(&report)?)
    } else {
        text_report(&report)
    };
    match std::io::stdout().lock().write_all(output.as_bytes()) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        Err(error) => Err(error.into()),
    }
}

/// Compact projection for the menu; never an authorization or reserved plan.
pub(super) fn handle_menu_preview(env: &SlateEnv, name: &str) -> Result<()> {
    use std::fmt::Write as _;
    let report = report(env, name, &FontAdapter::scan_fonts_with_env(env));
    let mut text = format!(
        "\n{} · {}\n",
        tr("字体配置预览", "Font Configuration Preview"),
        terminal_text(name)
    );
    if let Some(blocker) = &report.blocker {
        let _ = writeln!(
            text,
            "{}{}",
            tr("暂时无法生成改动计划：", "Could not prepare changes: "),
            terminal_text(&blocker.reason)
        );
    } else {
        let mut changed = 0;
        for file in &report.files {
            let action = match file.action {
                FontFileAction::Create => tr("新增", "Create"),
                FontFileAction::Update => tr("更新", "Update"),
                FontFileAction::Unchanged | FontFileAction::PreserveAbsent => continue,
            };
            changed += 1;
            let _ = writeln!(
                text,
                "  {action} {}{}",
                terminal_text(&file.path.path),
                if file.path.path_is_lossy {
                    tr("（路径显示不完整）", " (lossy path display)")
                } else {
                    ""
                }
            );
        }
        if changed == 0 {
            text.push_str(tr(
                "配置文件无需修改；这不代表无需安装字体。\n",
                "No configuration changes needed; font installation may still be required.\n",
            ));
        }
        if matches!(report.installation, Installation::WouldRequest) {
            text.push_str(tr(
                "确认使用时会请求下载字体；本次预览不会下载。\n",
                "Applying will request a font download; this preview does not download.\n",
            ));
        }
    }
    if !report.scan_complete {
        text.push_str(tr(
            "字体扫描不完整；完整明细见 slate font <字体名称> --dry-run。\n",
            "Font scan incomplete; see slate font <FONT_NAME> --dry-run for details.\n",
        ));
    }
    text.push_str(tr("仅预览：未安装、未写文件；实际应用会重新检查，可能失败。\n恢复点只覆盖配置，不撤销字体安装；实际显示未验证。\n\n", "Preview only: no installs or writes. Applying checks again and may fail.\nRecovery covers configuration, not font installation; actual display is unverified.\n\n"));
    crate::cli::file_output::write_required(&text)?;
    Ok(())
}

#[cfg(all(test, unix))]
#[path = "preview_tests.rs"]
mod tests;
