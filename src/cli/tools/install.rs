//! One explicitly reviewed installation, without setup's configuration phases.
use crate::{
    cli::{
        preflight, setup_executor,
        tool_selection::{InstallPlan, PlannedToolInstall, ToolCatalog},
    },
    config::ConfigWriteGuard,
    env::SlateEnv,
    error::{Result, SlateError},
    platform::packages::InstallContext,
};
use serde::Serialize;
use setup_executor::ToolInstallMethod;

fn wording(zh: &'static str, en: &'static str) -> &'static str {
    if super::interactive() {
        super::super::ui_language::tr(zh, en)
    } else {
        en
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Action {
    InstallMissing,
    AlreadyDetected,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct Review {
    schema_version: u8,
    tool: &'static str,
    label: &'static str,
    action: Action,
    route: Option<String>,
    fallback: Option<String>,
    notes: Vec<&'static str>,
}

struct PreparedInstall {
    review: Review,
    plan: Option<InstallPlan>,
}

pub(super) fn validate_name(id: &str) -> Result<()> {
    if super::installable_tools().contains(&id) {
        Ok(())
    } else {
        Err(SlateError::InvalidConfig(format!(
            "Choose one installable tool: {}. Other adapters require manual installation; nothing was changed.",
            super::installable_tools().join(", "),
        )))
    }
}

fn prepare(env: &SlateEnv, id: &str) -> Result<PreparedInstall> {
    validate_name(id)?;
    prepare_with(
        env,
        id,
        InstallContext::detect(),
        crate::detection::detect_tool_presence_with_env(id, env).installed,
    )
}

fn prepare_with(
    env: &SlateEnv,
    id: &str,
    context: InstallContext,
    detected: bool,
) -> Result<PreparedInstall> {
    validate_name(id)?;
    let tool = ToolCatalog::get_tool(id).expect("validated installable tool");
    let plan = if detected {
        None
    } else {
        Some(InstallPlan::capture(&[id.to_owned()], env, context)?)
    };
    let review = Review {
        schema_version: 1,
        tool: tool.id,
        label: tool.label,
        action: if detected {
            Action::AlreadyDetected
        } else {
            Action::InstallMissing
        },
        route: plan.as_ref().and_then(|plan| plan.description(id)),
        fallback: plan.as_ref().and_then(InstallPlan::fallback_description),
        notes: vec![
            "Only this tool is requested. Package managers may install or change its dependencies, caches and package records outside the Slate profile (including with SLATE_HOME).",
            "Slate does not apply themes, change fonts, activate shell/editor hooks or configure other adapters in this operation. Installation is not activation.",
            "This review does not run installers, network/permission probes or native version checks, and does not pin a package version or executable. The installation route and tool presence are rechecked after consent.",
            "Package installation has no Slate snapshot or automatic rollback. Failed or interrupted installers can leave partial changes; inspect the package manager before retrying. No automatic retry follows an uncertain result.",
        ],
    };
    Ok(PreparedInstall { review, plan })
}

fn render_prepared(prepared: &PreparedInstall) -> String {
    let mut display = prepared.review.clone();
    if super::interactive() {
        if let Some(plan) = &prepared.plan {
            let language = super::super::ui_language::current();
            display.route = plan.description_in(display.tool, language);
            display.fallback = plan.fallback_description_in(language);
        }
    }
    // Display-only translation must not alter the reviewed execution contract.
    render(&display)
}

fn render(review: &Review) -> String {
    use std::fmt::Write;
    let mut output = format!(
        "{} {} · {}\n",
        wording("安装", "Install"),
        review.label,
        review.tool
    );
    if review.action == Action::AlreadyDetected {
        output.push_str(wording("已检测到，不重复安装或升级；尚未验证配置与版本兼容性。\n", "Already detected. No installation or upgrade will be attempted; availability does not prove configuration or version compatibility.\n"));
    } else {
        let _ = writeln!(
            output,
            "{}: {}",
            wording("安装方式", "Route"),
            super::super::file_output::terminal_text(
                review.route.as_deref().expect("install has a route")
            )
        );
        if let Some(fallback) = &review.fallback {
            let _ = writeln!(
                output,
                "{}",
                super::super::file_output::terminal_text(fallback)
            );
        }
        let translations = [
            "只安装此工具；包管理器可能修改依赖、缓存及包记录，范围不受 Slate 配置目录或 SLATE_HOME 限制。",
            "不更改主题、字体或其他工具配置，也不启用 Shell/编辑器集成。安装不等于启用。",
            "预览不运行安装程序、联网/权限探测或版本检查，也不锁定包版本或可执行文件；确认后重新检查安装方式与工具是否已存在。",
            "包安装没有 Slate 快照或自动回滚。失败或中断可能留下部分改动，请先检查包管理器；结果不明确时不会自动重试。",
        ];
        for (index, note) in review.notes.iter().enumerate() {
            let note = wording(translations.get(index).copied().unwrap_or(note), note);
            let _ = writeln!(output, "• {note}");
        }
    }
    output
}

fn changed_review() -> SlateError {
    SlateError::InvalidConfig("Tool availability or installation route changed after review; review again. No installer was launched.".into())
}

fn preflight_install(id: &str) -> Result<()> {
    let report = preflight::run_checks_for_retry(id);
    if report.is_ready() {
        Ok(())
    } else {
        let reasons = report
            .checks
            .iter()
            .filter(|check| check.blocking && !check.passed)
            .map(|check| format!("{}: {}", check.name, check.description))
            .collect::<Vec<_>>()
            .join("; ");
        Err(SlateError::InvalidConfig(format!(
            "Installation checks failed: {reasons}. No installer was launched."
        )))
    }
}

fn execute_with(
    env: &SlateEnv,
    reviewed: &PreparedInstall,
    mut inspect: impl FnMut() -> Result<PreparedInstall>,
    preflight: impl FnOnce() -> Result<()>,
    install: impl FnOnce(&PlannedToolInstall, &SlateEnv) -> Result<ToolInstallMethod>,
) -> Result<Option<ToolInstallMethod>> {
    let Some(plan) = &reviewed.plan else {
        return Ok(None);
    };
    if inspect()?.review != reviewed.review {
        return Err(changed_review());
    }
    preflight()?;
    let _guard = ConfigWriteGuard::acquire(env)?;
    if inspect()?.review != reviewed.review {
        return Err(changed_review());
    }
    let selected = plan.tool(reviewed.review.tool)?;
    plan.verify_selection(&[selected.metadata], env)?;
    // The installer consumes the reviewed route, never the whole setup pipeline.
    install(selected, env).map(Some)
}

fn completion(
    id: &str,
    method: &ToolInstallMethod,
    presence: &crate::detection::ToolPresence,
) -> Result<String> {
    let language = if super::interactive() {
        super::super::ui_language::current()
    } else {
        crate::config::ui_language::UiLanguage::English
    };
    completion_in_language(id, method, presence, language)
}

fn completion_in_language(
    id: &str,
    method: &ToolInstallMethod,
    presence: &crate::detection::ToolPresence,
    language: crate::config::ui_language::UiLanguage,
) -> Result<String> {
    let chinese = language == crate::config::ui_language::UiLanguage::Chinese;
    let route = match method {
        ToolInstallMethod::Homebrew => "Homebrew",
        ToolInstallMethod::Apt => "apt",
        ToolInstallMethod::UserLocal(_) if chinese => "用户目录安装",
        ToolInstallMethod::UserLocal(_) => "user-local installation",
    };
    if chinese {
        if !presence.installed {
            return Err(SlateError::InvalidConfig(format!(
                "{route} 安装程序已结束，但仍未检测到 {id}。请检查安装结果和 PATH 后再重试；包变更未回滚，Slate 未配置主题、字体或 Shell 集成。"
            )));
        }
        let mut output = format!(
            "{id} 安装完成 · {route}\n已检测到工具，尚未配置主题、字体或启用 Shell/编辑器集成。\n\n查看工具：slate tools info {id}\n预览已保存主题的配色：slate tools sync {id} --dry-run\n"
        );
        match &presence.evidence {
            Some(crate::detection::ToolEvidence::Executable(path)) if !presence.in_path => {
                use std::fmt::Write;
                let _ = writeln!(output, "可执行文件不在当前 PATH 中：{}{}。直接输入命令可能仍不可用，请先检查 PATH，不要重复安装。Slate 未更改 PATH。",
                    super::super::file_output::terminal_text(&path.to_string_lossy()),
                    if path.to_str().is_none() { "（路径显示有损，并非精确路径）" } else { "" });
            }
            Some(crate::detection::ToolEvidence::Executable(_)) => {}
            _ => output.push_str("尚未确认 PATH 中存在可执行文件，请先查看工具详情。\n"),
        }
        return Ok(output);
    }
    if !presence.installed {
        return Err(SlateError::InvalidConfig(format!(
            "The {route} installer completed, but {id} is still not detected. Inspect installation and PATH before retrying; package changes were not rolled back. Slate did not configure themes, fonts or shell integration.",
        )));
    }
    let mut output = format!(
        "Installer completed for {id} via {route}; the tool is now detected.\nSlate did not configure themes, fonts or shell/editor hooks.\nNext: slate tools info {id}\nTo review colors for an existing saved theme: slate tools sync {id} --dry-run\n",
    );
    match &presence.evidence {
        Some(crate::detection::ToolEvidence::Executable(path)) if !presence.in_path => {
            use std::fmt::Write;
            let _ = writeln!(output, "Executable found outside the current PATH: {}{}. Its bare command may not work yet; review your shell's PATH before retrying installation. Slate did not change PATH.",
                super::super::file_output::terminal_text(&path.to_string_lossy()),
                if path.to_str().is_none() { " (lossy display; not an exact path)" } else { "" });
        }
        Some(crate::detection::ToolEvidence::Executable(_)) => {},
        _ => output.push_str("Detection did not establish an executable in PATH; inspect the tool details before assuming its shell command is available.\n"),
    }
    Ok(output)
}

pub(super) fn handle(env: &SlateEnv, id: &str, dry_run: bool, yes: bool, json: bool) -> Result<()> {
    let reviewed = prepare(env, id)?;
    if json {
        return super::super::file_output::write_output(&format!(
            "{}\n",
            serde_json::to_string_pretty(&reviewed.review)?
        ));
    }
    if dry_run || reviewed.review.action == Action::AlreadyDetected {
        return super::super::file_output::write_output(&format!(
            "{}{}",
            render_prepared(&reviewed),
            wording(
                "未运行安装程序，未改动文件。\n",
                "No installer was launched and no files were changed.\n"
            )
        ));
    }
    super::super::file_output::write_required(&render_prepared(&reviewed))?;
    if !yes {
        if !super::interactive() {
            return Err(SlateError::InvalidConfig("Non-interactive installation requires --yes. Review with --dry-run first; nothing was changed.".into()));
        }
        if !super::super::menu::select(wording(
            "安装此工具及所需依赖？",
            "Install only this tool (and its package dependencies)?",
        ))
        .escape_value(false)
        .initial_value(false)
        .item(false, wording("取消", "Cancel"), "")
        .item(true, wording("安装", "Install"), "")
        .interact()
        .map_err(super::input_error)?
        {
            return Ok(());
        }
    }
    let method = execute_with(
        env,
        &reviewed,
        || prepare(env, id),
        || preflight_install(id),
        |planned, env| {
            eprintln!(
                "Installing {} via the reviewed route...",
                planned.metadata.label
            );
            setup_executor::install_planned_tool(planned, env)
        },
    )?
    .expect("only an install action reaches execution");
    let presence = crate::detection::detect_tool_presence_with_env(id, env);
    super::super::file_output::write_output(&completion(id, &method, &presence)?)
}

#[cfg(test)]
mod tests;
