use super::recover::PreviewRecoveryStatus;
use super::ui_language::tr;
use crate::config::ConfigManager;
use crate::env::SlateEnv;
use crate::error::Result;
use crate::opacity::OpacityPreset;
use crate::theme::ThemeRegistry;
use serde::Serialize;

#[derive(Serialize)]
pub(crate) struct SavedTheme {
    pub id: String,
    pub name: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct StatusWarning {
    pub field: &'static str,
    pub message: &'static str,
    #[serde(skip)]
    pub(super) menu_message: &'static str,
}

/// Saved configuration, not a claim that running tools have applied it.
#[derive(Serialize)]
pub(crate) struct StatusReport {
    schema_version: u8,
    // Display-only paths; filesystem operations use SlateEnv's original bytes.
    config_dir: String,
    config_dir_is_lossy: bool,
    cache_dir: String,
    cache_dir_is_lossy: bool,
    pub theme: Option<SavedTheme>,
    pub font: Option<String>,
    pub prompt_style: Option<crate::config::prompt::PromptStyle>,
    pub opacity: Option<String>,
    pub auto_theme_enabled: Option<bool>,
    pub recovery: PreviewRecoveryStatus,
    pub warnings: Vec<StatusWarning>,
}

impl StatusReport {
    pub(super) fn inspect(env: &SlateEnv) -> Result<Self> {
        let config = ConfigManager::from_env_paths(env);
        let registry = ThemeRegistry::new()?;
        let mut warnings = Vec::new();
        let theme = match config.get_current_theme() {
            Ok(id) => id.map(|id| {
                let name = registry.get(&id).map(|theme| theme.name.clone());
                if name.is_none() {
                    warnings.push(StatusWarning {
                        field: "theme",
                        menu_message: "已保存主题无法识别；可在主菜单选择可用主题，本次未替换。",
                        message: "Saved theme is unknown; no fallback theme has been applied.",
                    });
                }
                SavedTheme { id, name }
            }),
            Err(_) => {
                warnings.push(StatusWarning {
                    field: "theme",
                    menu_message:
                        "主题记录无法读取；请在详细诊断中确认配置目录，检查记录文件和权限。",
                    message:
                        "Cannot read the saved theme; check the tracking file and its permissions.",
                });
                None
            }
        };
        let font = match config.get_current_font() {
            Ok(font) => font,
            Err(_) => {
                warnings.push(StatusWarning {
                    field: "font",
                    menu_message:
                        "字体记录无法读取；请在详细诊断中确认配置目录，检查记录文件和权限。",
                    message:
                        "Cannot read the saved font; check the tracking file and its permissions.",
                });
                None
            }
        };
        let prompt_style = match config.get_prompt_style() {
            Ok(style) => style,
            Err(_) => {
                warnings.push(StatusWarning { field: "prompt_style", message: "Saved prompt style is unreadable or unknown; no default layout is assumed.", menu_message: "提示符样式无法读取或识别；请检查 config.toml 的格式、样式名称和权限，未使用默认样式。" });
                None
            }
        };
        let opacity = match config.get_current_opacity().and_then(|value| {
            value
                .map(|value| {
                    value
                        .parse::<OpacityPreset>()
                        .map(|preset| preset.to_string())
                })
                .transpose()
        }) {
            Ok(opacity) => opacity,
            Err(_) => {
                warnings.push(StatusWarning {
                    field: "opacity",
                    menu_message:
                        "透明度记录无法读取或取值无效；请检查记录文件和权限，未使用默认透明度。",
                    message: "Saved opacity is unreadable or invalid; no default is assumed.",
                });
                None
            }
        };
        let auto_theme_enabled = match config.is_auto_theme_enabled() {
            Ok(enabled) => Some(enabled),
            Err(_) => {
                warnings.push(StatusWarning { field: "auto_theme_enabled", message: "Auto-theme settings cannot be read; check config.toml syntax and permissions.", menu_message: "自动换色设置无法读取；请检查 config.toml 的格式和权限，本次未修改设置。" });
                None
            }
        };
        if crate::config::ui_language::read(env).is_err() {
            warnings.push(StatusWarning {
                field: "language",
                message: "Saved UI language is unreadable or invalid; check config.toml [preferences].language (zh-CN or en) and file permissions. The preference was not overwritten.",
                menu_message: "语言设置无法读取或识别；请检查 config.toml 的 [preferences].language（zh-CN 或 en）和文件权限，未覆盖原设置。",
            });
        }
        Ok(Self {
            schema_version: 1,
            config_dir: env.config_dir().to_string_lossy().into_owned(),
            config_dir_is_lossy: env.config_dir().to_str().is_none(),
            cache_dir: env.slate_cache_dir().to_string_lossy().into_owned(),
            cache_dir_is_lossy: env.slate_cache_dir().to_str().is_none(),
            theme,
            font,
            prompt_style,
            opacity,
            auto_theme_enabled,
            recovery: PreviewRecoveryStatus::inspect(env),
            warnings,
        })
    }
}

/// Inspect without initializing config, backups, or sound caches.
pub fn handle(json: bool) -> Result<()> {
    let env = SlateEnv::from_process()?;
    let report = StatusReport::inspect(&env)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
        Ok(())
    } else {
        super::status_panel::render_report(&env, &report)
    }
}

pub(crate) fn handle_menu() -> Result<()> {
    let env = SlateEnv::from_process()?;
    let mut selected = "back";
    loop {
        let report = StatusReport::inspect(&env)?;
        super::file_output::write_output(&menu_summary(&report))?;
        let action = super::menu::select(tr("检查配置", "Check Configuration"))
            .item("back", tr("返回主菜单", "Back"), "")
            .item("refresh", tr("刷新检查", "Refresh"), "")
            .item("details", tr("查看详细诊断", "Diagnostics"), "")
            .initial_value(selected)
            .escape_value("back")
            .interact()?;
        match action {
            "refresh" => selected = "refresh",
            "details" => {
                handle(false)?;
                super::menu::select(tr("详细诊断 · 只读", "Diagnostics · Read-only"))
                    .item((), tr("返回检查摘要", "Back to Summary"), "")
                    .escape_value(())
                    .interact()?;
                selected = "details";
            }
            _ => return Ok(()),
        }
    }
}

fn menu_summary(report: &StatusReport) -> String {
    use std::fmt::Write;
    let mut text = String::from(tr("\n已保存的配置\n", "\nSaved Configuration\n"));
    let theme = report
        .theme
        .as_ref()
        .map(|theme| theme.name.as_deref().unwrap_or(&theme.id));
    let style = report
        .prompt_style
        .map(|style| tr(super::prompt::menu_style_label(style), style.label()));
    for (field, label, value) in [
        ("theme", tr("主题", "Theme"), theme),
        ("font", tr("字体", "Font"), report.font.as_deref()),
        ("prompt_style", tr("提示符", "Prompt"), style),
        (
            "opacity",
            tr("透明度", "Opacity"),
            report.opacity.as_deref(),
        ),
    ] {
        let value = value.unwrap_or_else(|| {
            if report.warnings.iter().any(|warning| warning.field == field) {
                tr("需检查", "Check settings")
            } else {
                tr("未设置", "Not set")
            }
        });
        let value = match (field, value) {
            ("opacity", "Solid") => tr("不透明", "Solid"),
            ("opacity", "Frosted") => tr("磨砂", "Frosted"),
            ("opacity", "Clear") => tr("透明", "Clear"),
            _ => value,
        };
        let _ = writeln!(
            text,
            "  {label}  {}",
            super::file_output::terminal_text(value)
        );
    }
    let _ = writeln!(
        text,
        "  {}  {}",
        tr("自动换色", "Auto-Theme"),
        match report.auto_theme_enabled {
            Some(true) => tr("开", "On"),
            Some(false) => tr("关", "Off"),
            None => tr("需检查", "Check settings"),
        }
    );
    for warning in &report.warnings {
        let _ = writeln!(
            text,
            "  ⚠ {}",
            super::file_output::terminal_text(tr(warning.menu_message, warning.message))
        );
    }
    if report.recovery.needs_attention() {
        for line in report.recovery.lines() {
            let _ = writeln!(text, "  ⚠ {}", super::file_output::terminal_text(&line));
        }
    }
    text.push_str(tr(
        "\n这里只检查已保存设置；工具是否实际生效请查看详细诊断并在工具内确认。\n",
        "\nSaved settings only; confirm live appearance in each tool.\n",
    ));
    text
}

#[cfg(test)]
mod menu_tests {
    use super::*;

    #[test]
    fn status_warning_menu_translation_does_not_change_json() {
        let home = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        std::fs::create_dir_all(env.config_dir()).unwrap();
        std::fs::write(env.managed_file("current"), "unknown-theme\n").unwrap();
        let report = StatusReport::inspect(&env).unwrap();
        assert!(menu_summary(&report).contains("已保存主题无法识别"));
        let warning = serde_json::to_value(&report.warnings[0]).unwrap();
        assert_eq!(warning.as_object().unwrap().len(), 2);
        assert_eq!(warning["field"], "theme");
        assert_eq!(
            warning["message"],
            "Saved theme is unknown; no fallback theme has been applied."
        );
    }

    #[test]
    fn status_summary_uses_the_same_style_names_as_the_hub() {
        let home = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        let mut report = StatusReport::inspect(&env).unwrap();
        for style in crate::config::prompt::PromptStyle::ALL {
            report.prompt_style = Some(style);
            assert!(menu_summary(&report).contains(&format!(
                "提示符  {}",
                super::super::prompt::menu_style_label(style)
            )));
        }
        assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 0);
    }

    #[test]
    fn status_summary_distinguishes_absence_from_errors_and_keeps_all_warnings() {
        let home = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        let mut report = StatusReport::inspect(&env).unwrap();
        report.prompt_style = None;
        let empty = menu_summary(&report);
        for label in ["主题", "字体", "提示符", "透明度"] {
            assert!(empty.contains(&format!("{label}  未设置")));
        }
        for (field, label) in [
            ("theme", "主题"),
            ("font", "字体"),
            ("prompt_style", "提示符"),
            ("opacity", "透明度"),
        ] {
            report.warnings.push(StatusWarning {
                field,
                message: "fixture unreadable",
                menu_message: "fixture unreadable",
            });
            assert!(menu_summary(&report).contains(&format!("{label}  需检查")));
        }
        report.auto_theme_enabled = None;
        let output = menu_summary(&report);
        assert!(output.contains("自动换色  需检查"));
        assert_eq!(output.matches("fixture unreadable").count(), 4);
        assert!(output.contains("工具内确认"));
        assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 0);
    }

    #[test]
    fn status_summary_escapes_display_values_and_translates_opacity_only() {
        let home = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        let mut report = StatusReport::inspect(&env).unwrap();
        report.theme = Some(SavedTheme {
            id: "unknown\n\u{202e}".into(),
            name: None,
        });
        report.font = Some("Solid".into());
        report.warnings.push(StatusWarning {
            field: "theme",
            message: "warning\u{1b}\r",
            menu_message: "warning\u{1b}\r",
        });
        for (opacity, label) in [("Solid", "不透明"), ("Frosted", "磨砂"), ("Clear", "透明")]
        {
            report.opacity = Some(opacity.into());
            let output = menu_summary(&report);
            assert!(output.contains("字体  Solid"));
            assert!(output.contains(&format!("透明度  {label}")));
            assert!(output.contains("unknown\\n\\u{202e}"));
            assert!(output.contains("warning\\u{1b}\\r"));
            assert!(!output.contains('\u{1b}'));
            assert!(!output.contains('\u{202e}'));
        }
        assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 0);
    }
}
