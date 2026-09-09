//! Setup presentation/outcome tracking around the shared font installer.
use super::font_install::{self, chain};
use crate::cli::{file_output::terminal_text, wizard_support::wording as tr};
use crate::{
    cli::failure_handler::ExecutionSummary, env::SlateEnv, error::Result,
    platform::fonts::FontCacheRefresh,
};

pub(super) fn execute(
    selected: Option<&str>,
    env: &SlateEnv,
    summary: &mut ExecutionSummary,
) -> FontCacheRefresh {
    execute_with(
        selected,
        summary,
        |font| font_install::is_font_installed_with_env(env, font),
        |font, progress| chain::install_catalog(font, env, progress),
    )
}

fn execute_with(
    selected: Option<&str>,
    summary: &mut ExecutionSummary,
    check: impl FnOnce(&str) -> Result<bool>,
    install: impl FnOnce(&str, &mut dyn FnMut(chain::Stage)) -> Result<chain::Report>,
) -> FontCacheRefresh {
    let Some(font) = selected else {
        return FontCacheRefresh::NotRequested;
    };
    summary.font_requested = true;
    summary.font_available = false;
    let display = terminal_text(&font_install::font_display_name(font));
    let spinner = cliclack::spinner();
    spinner.start(format!(
        "{} {display}...",
        tr("正在检查字体", "Checking font")
    ));
    match check(font) {
        Ok(true) => {
            summary.font_available = true;
            spinner.stop(format!("✓ {display} {}", tr("已找到", "already installed")));
            return FontCacheRefresh::NotRequested;
        }
        Ok(false) => {}
        Err(error) => {
            spinner.error(format!(
                "{display}: {}",
                tr(
                    "无法确认是否已安装",
                    "Cannot determine whether it is installed"
                )
            ));
            summary.add_issue(format!(
                "{display}: {error}; {}",
                tr("未尝试下载字体", "no font download attempted")
            ));
            return FontCacheRefresh::NotRequested;
        }
    }
    let result = install(font, &mut |stage| {
        spinner.start(format!(
            "{} {display} ({})...",
            tr("正在安装", "Installing"),
            stage_label(stage)
        ));
    });
    match result {
        Ok(report) => {
            for notice in report.notices {
                summary.add_notice(format!("{display}: {notice}"));
            }
            summary.font_available = true;
            spinner.stop(format!(
                "✓ {display} {} ({})",
                tr("已安装", "installed"),
                stage_label(report.stage)
            ));
            report.cache
        }
        Err(error) => {
            let reason = if super::installation_fallback_allowed(&error) {
                let full = error.to_string();
                font_install::strip_error_prefix(&full).to_owned()
            } else {
                format!(
                    "{error}; {}",
                    tr(
                        "未继续尝试其他字体安装方式",
                        "no further font installation attempted"
                    )
                )
            };
            spinner.error(format!("✗ {display}: {}", terminal_text(&reason)));
            summary.add_issue(format!("{display}: {reason}"));
            FontCacheRefresh::NotRequested
        }
    }
}

fn stage_label(stage: chain::Stage) -> &'static str {
    match stage {
        chain::Stage::Homebrew => "Homebrew",
        chain::Stage::SharedCache => tr("共享字体缓存", "shared font cache"),
        chain::Stage::Download => tr("直接下载字体", "direct font download"),
    }
}

#[cfg(test)]
mod tests;
