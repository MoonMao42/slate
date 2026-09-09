//! This binary's embedded capabilities, independent of user profiles and PATH.
use super::ui_language::{current, tr};
use crate::config::ui_language::UiLanguage;
use crate::{config::prompt::PromptStyle, error::Result, theme::ThemeRegistry};
use serde::Serialize;

pub const LONG_VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    "\nSource tag: ",
    env!("SLATE_SOURCE_TAG"),
    "\nTarget: ",
    env!("SLATE_BUILD_TARGET"),
    "\nCargo profile class: ",
    env!("SLATE_BUILD_PROFILE"),
    "\nCustom Cargo profile names are not recorded; compare executable path and source tag.",
    "\nRun `slate about` for this binary's path and built-in capabilities."
);

#[derive(Serialize)]
struct Build {
    version: &'static str,
    source_tag: Option<&'static str>,
    target: &'static str,
    cargo_profile: &'static str,
    cargo_profile_scope: &'static str,
    cargo_features: Vec<&'static str>,
}

#[derive(Serialize)]
struct Executable {
    path: String,
    path_is_lossy: bool,
}

impl Executable {
    fn quoted_command(&self) -> Option<String> {
        if self.path_is_lossy || self.path.is_empty() || self.path.chars().any(char::is_control) {
            return None;
        }
        Some(crate::detection::shell_quote(&self.path))
    }
}

#[derive(Serialize)]
struct Capabilities {
    theme_ids: Vec<String>,
    tool_adapters: Vec<&'static str>,
    prompt_styles: Vec<&'static str>,
    tool_theme_checks: Vec<&'static str>,
}

#[derive(Serialize)]
struct Report {
    schema_version: u8,
    scope: &'static str,
    build: Build,
    executable: Option<Executable>,
    capabilities: Capabilities,
}

impl Report {
    fn menu_details(&self) -> String {
        let path = self.executable.as_ref().map_or_else(
            || tr("无法取得程序路径", "Executable path unavailable").to_owned(),
            |exe| {
                let path = super::file_output::terminal_text(&exe.path);
                if exe.path_is_lossy {
                    format!(
                        "{path}{}",
                        tr(
                            "（路径显示有替换字符，勿直接复制使用）",
                            " (lossy path; do not copy as a command)"
                        )
                    )
                } else {
                    path
                }
            },
        );
        if current() == UiLanguage::English {
            return format!(
                "\n  This Slate build\n\n  Version     {}\n  Executable  {path}\n  Source tag  {}\n  Platform    {}\n\n  Source tags compare builds; they are not checksums or proof of remote sync.\n  Full report: slate about (use the same executable)\n\n",
                self.build.version,
                self.build.source_tag.unwrap_or("Unavailable"),
                self.build.target,
            );
        }
        format!(
            "\n  当前运行的 Slate\n\n  版本      {}\n  程序路径  {path}\n  源码标识  {}\n  平台      {}\n\n  路径用于确认正在运行哪份程序；源码标识用于比较构建。\n  源码标识不是程序校验码，也不代表已同步远端。\n  完整报告：slate about（请确认终端指向同一份程序）\n\n",
            self.build.version,
            self.build.source_tag.unwrap_or("无法取得"),
            self.build.target,
        )
    }

    fn overview(&self) -> String {
        if current() == UiLanguage::English {
            return format!(
                "  slate {}\n\n  Themes         {}\n  Tool adapters  {}\n  Prompt styles  {}\n\n  Built-in support, not installed or active tools\n",
                self.build.version,
                self.capabilities.theme_ids.len(),
                self.capabilities.tool_adapters.len(),
                self.capabilities.prompt_styles.len(),
            );
        }
        format!(
            "  slate {}\n\n  主题    {} 款\n  工具    {} 种适配\n  提示符  {} 种样式\n\n  内置支持数量，不代表工具已安装或正在生效\n",
            self.build.version,
            self.capabilities.theme_ids.len(),
            self.capabilities.tool_adapters.len(),
            self.capabilities.prompt_styles.len(),
        )
    }

    fn inspect() -> Result<Self> {
        let tag = env!("SLATE_SOURCE_TAG");
        Ok(Self {
            schema_version: 1,
            scope: "compiled_capabilities_only",
            build: Build {
                version: env!("CARGO_PKG_VERSION"),
                source_tag: (tag != "unavailable").then_some(tag),
                target: env!("SLATE_BUILD_TARGET"),
                cargo_profile: env!("SLATE_BUILD_PROFILE"),
                cargo_profile_scope: "Cargo PROFILE classification; not the custom profile name or a complete optimization/debug-settings description",
                cargo_features: env!("SLATE_BUILD_FEATURES")
                    .split(',')
                    .filter(|feature| !feature.is_empty())
                    .collect(),
            },
            executable: std::env::current_exe().ok().map(|path| Executable {
                path_is_lossy: path.to_str().is_none(),
                path: path.to_string_lossy().into_owned(),
            }),
            capabilities: Capabilities {
                theme_ids: ThemeRegistry::new()?.list_ids(),
                tool_adapters: super::tools::supported_tools(),
                prompt_styles: PromptStyle::ALL.into_iter().map(PromptStyle::id).collect(),
                tool_theme_checks: super::doctor::TOOL_FILE_CHECKS
                    .iter()
                    .map(|(id, _, _)| *id)
                    .collect(),
            },
        })
    }

    fn text(&self) -> String {
        let executable = self.executable.as_ref().map_or_else(
            || "unavailable".to_owned(),
            |exe| {
                let path = super::file_output::terminal_text(&exe.path);
                if exe.path_is_lossy {
                    format!("{path} (non-UTF-8 path; display is lossy)")
                } else {
                    path
                }
            },
        );
        let features = if self.build.cargo_features.is_empty() {
            "none".to_owned()
        } else {
            self.build.cargo_features.join(", ")
        };
        let commands = self.executable.as_ref().and_then(Executable::quoted_command).map_or_else(
            || "Copyable commands omitted because the executable path is unavailable or cannot be displayed losslessly.\n".to_owned(),
            |exe| format!("Try this exact build (POSIX shell):\n  {exe} tools\n  {exe} prompt --list\nBare `slate` may resolve to a different binary or shell alias.\n"),
        );
        format!(
            "slate {} — About This Build\n\
             Executable: {executable}\n\
             Source tag: {}\n\
             Target: {} · Cargo profile class: {} · Cargo features: {features}\n\
             Custom Cargo profile names are not recorded; compare executable path and source tag.\n\n\
             Built in: {} themes · {} tool adapters · {} prompt styles\n\
             Prompt styles: {}\n\
             Tool theme checks: {}\n\n\
             This is built-in support, not installed tools or live theme status.\n\
             {commands}\
             Source tags label selected source inputs; not binary checksums or signatures.\n",
            self.build.version,
            self.build.source_tag.unwrap_or("unavailable"),
            self.build.target,
            self.build.cargo_profile,
            self.capabilities.theme_ids.len(),
            self.capabilities.tool_adapters.len(),
            self.capabilities.prompt_styles.len(),
            self.capabilities.prompt_styles.join(", "),
            self.capabilities.tool_theme_checks.join(", "),
        )
    }
}

pub fn handle(json: bool) -> Result<()> {
    let report = Report::inspect()?;
    let output = if json {
        format!("{}\n", serde_json::to_string_pretty(&report)?)
    } else {
        report.text()
    };
    super::file_output::write_output(&output)
}

pub(super) fn handle_menu() -> Result<()> {
    let report = Report::inspect()?;
    let mut page = super::menu::ReadOnlyPage::enter()?;
    let result = menu_pages(&report, &mut page);
    page.finish(result)
}

fn menu_pages(report: &Report, page: &mut super::menu::ReadOnlyPage) -> Result<()> {
    loop {
        page.clear()?;
        super::file_output::write_output(&format!("\n{}\n", report.overview()))?;
        let action = super::menu::select(tr("关于 Slate", "About Slate"))
            .escape_value("back")
            .initial_value("back")
            .item("details", tr("诊断详情", "Build Details"), "")
            .item("back", tr("返回首页", "Back"), "")
            .interact()
            .map_err(menu_error)?;
        if action == "back" {
            return Ok(());
        }
        page.view(
            &report.menu_details(),
            tr("诊断详情", "Build Details"),
            tr("返回关于", "Back to About"),
        )
        .map_err(menu_error)?;
    }
}

fn menu_error(error: std::io::Error) -> crate::error::SlateError {
    if error.kind() == std::io::ErrorKind::Interrupted {
        crate::error::SlateError::UserCancelled
    } else {
        error.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_details_explain_identity_without_dumping_full_report() {
        let mut report = Report::inspect().unwrap();
        report.executable = Some(Executable {
            // ANSI-FIXTURE: raw input for escaping or width checks.
            path: "/tmp/private\n\x1b[31mslate".into(),
            path_is_lossy: true,
        });
        let text = report.menu_details();
        assert!(text.contains("程序路径") && text.contains("源码标识"));
        assert!(text.contains("\\n\\u{1b}[31mslate"));
        assert!(!text.contains('\x1b'));
        assert!(text.contains("勿直接复制使用"));
        assert!(text.contains("不代表已同步远端"));
        assert!(!text.contains("Cargo profile") && !text.contains("Prompt styles:"));
        report.executable = None;
        assert!(report.menu_details().contains("无法取得程序路径"));
    }

    #[test]
    fn about_labels_cargo_profile_as_classification_without_guessing_custom_name() {
        let mut report = Report::inspect().unwrap();
        report.executable = Some(Executable {
            path: "/tmp/custom-profile/slate".into(),
            path_is_lossy: false,
        });
        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["build"]["cargo_profile"], env!("SLATE_BUILD_PROFILE"));
        assert!(json["build"]["cargo_profile_scope"]
            .as_str()
            .unwrap()
            .contains("not the custom profile name"));
        assert!(report.text().contains("Cargo profile class:"));
        assert!(report
            .text()
            .contains("Custom Cargo profile names are not recorded"));
        assert!(LONG_VERSION.contains("Cargo profile class:"));
        assert!(!report
            .text()
            .contains("Cargo profile class: custom-profile"));
    }

    #[test]
    fn about_trial_commands_target_exact_executable_without_shell_expansion() {
        let path = "/tmp/Slate 中文's build/$(false); slate";
        let exe = Executable {
            path: path.into(),
            path_is_lossy: false,
        };
        let quoted = exe.quoted_command().unwrap();
        // Parse arguments only: never execute the displayed path.
        let output = std::process::Command::new("/bin/sh")
            .args([
                "-c",
                &format!("set -- {quoted} prompt --list; printf '%s\\n' \"$@\""),
            ])
            .env_clear()
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            format!("{path}\nprompt\n--list\n")
        );
        let mut report = Report::inspect().unwrap();
        report.executable = Some(exe);
        assert!(report.text().contains(&format!("{quoted} tools")));
        assert!(!report.text().contains("Try: slate tools"));
        for path in ["", "/tmp/new\nline", "/tmp/escape\x1b"] {
            report.executable = Some(Executable {
                path: path.into(),
                path_is_lossy: false,
            });
            assert!(report.text().contains("Copyable commands omitted"));
        }
        report.executable = Some(Executable {
            path: "/tmp/lossy".into(),
            path_is_lossy: true,
        });
        assert!(report.text().contains("Copyable commands omitted"));
        report.executable = None;
        assert!(report.text().contains("Copyable commands omitted"));
    }
}
