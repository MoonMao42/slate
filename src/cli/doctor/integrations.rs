use crate::env::SlateEnv;
use crate::error::Result;
use serde::Serialize;
#[cfg(test)]
use std::fs;
use std::path::Path;

mod btop;
mod eza;
mod fastfetch;
mod font;
mod lazygit;
mod menu_text;
mod nvim_version;
mod opacity;
mod opencode;
mod shell;
mod starship;
mod tool_files;
mod yazi;
mod zellij;

#[derive(Serialize)]
struct Report {
    schema_version: u8,
    target: String,
    scope: &'static str,
    checks: Vec<Check>,
    #[serde(skip_serializing_if = "Option::is_none")]
    version_probe: Option<nvim_version::VersionProbe>,
    #[serde(skip_serializing_if = "Option::is_none")]
    font_inventory: Option<font::Inventory>,
    #[serde(skip_serializing_if = "Option::is_none")]
    font_references: Option<Vec<font::ReferenceSummary>>,
}

#[derive(Serialize)]
struct Check {
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<&'static str>,
    status: &'static str,
    message: String,
    path: String,
    path_is_lossy: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    suggestion: Option<String>,
}

impl Report {
    fn add(
        &mut self,
        status: &'static str,
        message: impl Into<String>,
        path: &Path,
        suggestion: Option<String>,
    ) {
        self.checks.push(Check {
            code: None,
            status,
            message: message.into(),
            path: path.display().to_string(),
            path_is_lossy: path.to_str().is_none(),
            suggestion,
        });
    }

    fn add_code(
        &mut self,
        code: &'static str,
        status: &'static str,
        message: impl Into<String>,
        path: &Path,
        suggestion: Option<String>,
    ) {
        self.add(status, message, path, suggestion);
        self.checks.last_mut().expect("added check").code = Some(code);
    }

    fn read(&mut self, path: &Path, label: &str) -> Option<String> {
        self.read_with_requirement(path, label, true)
    }

    fn read_with_requirement(
        &mut self,
        path: &Path,
        label: &str,
        required: bool,
    ) -> Option<String> {
        use crate::config::file_read::{read_text, MAX_TOOL_CONFIG_BYTES};
        match read_text(path, MAX_TOOL_CONFIG_BYTES) {
            Ok(Some(content)) => {
                self.add("ok", format!("{label} is readable"), path, None);
                Some(content)
            }
            Ok(None) => {
                self.add(
                    if required { "warning" } else { "info" },
                    format!("{label} is missing"),
                    path,
                    required.then(|| "Run `slate setup` to connect this configuration.".into()),
                );
                None
            }
            Err(err) => {
                self.add(
                    "error",
                    format!("Cannot read {label}: {err}"),
                    path,
                    Some(
                        "Check permissions, UTF-8 encoding and the 8 MiB regular-file limit."
                            .into(),
                    ),
                );
                None
            }
        }
    }

    fn connection(&mut self, connected: bool, path: &Path) {
        if connected {
            self.add("ok", "Slate's loader reference is present", path, None);
        } else {
            self.add("warning", "No direct Slate loader reference found", path,
                Some("Run `slate setup`, or verify your manually configured includes. Indirect includes are not checked.".into()));
        }
    }
}

/// Inspect files only: do not initialize ConfigManager, start tools, or edit configs.
fn inspect(target: &str, env: &SlateEnv) -> Report {
    let mut report = Report {
        schema_version: 1,
        target: target.into(),
        scope: "Checks describe files on disk; they do not verify a running application.",
        checks: Vec::new(),
        version_probe: None,
        font_inventory: None,
        font_references: None,
    };
    match target {
        "btop" => btop::inspect(&mut report, env),
        "starship" => starship::inspect(&mut report, env),
        "yazi" => yazi::inspect(&mut report, env),
        "zellij" => zellij::inspect(&mut report, env),
        "lazygit" => lazygit::inspect(&mut report, env),
        "eza" => eza::inspect(&mut report, env),
        "fastfetch" => fastfetch::inspect(&mut report, env),
        "opencode" => opencode::inspect(&mut report, env),
        "opacity" => opacity::inspect(&mut report, env),
        "font" => font::inspect(&mut report, env),
        "nvim" => {
            report.scope = "File-only Neovim checks; no editor is launched or configuration changed. To explicitly check the executable/version too, run `slate doctor nvim --check-version`. Files do not prove a running editor's behavior.";
            let automatic = match crate::config::ConfigManager::from_env_paths(env)
                .is_editor_auto_activation_enabled()
            {
                Ok(enabled) => {
                    report.add_code("auto_activation", if enabled { "ok" } else { "info" },
                        if enabled { "Automatic setup activation is allowed; this does not prove a hook is installed" }
                        else { "Automatic setup activation is disabled for this profile; manual use remains available" },
                        &env.nvim_auto_activation_path(),
                        (!enabled).then(|| "To allow activation again: `slate config set editor enable`, then `slate setup`.".into()));
                    enabled
                }
                Err(error) => {
                    report.add_code("auto_activation", "error", format!("Cannot read activation preference: {error}"),
                        &env.nvim_auto_activation_path(), Some("Inspect the preference file; setup will not treat an unreadable record as consent.".into()));
                    false
                }
            };
            let init = env.nvim_init_path();
            if let Some(content) =
                report.read_with_requirement(&init, "Neovim init file", automatic)
            {
                let connected = content.lines().any(|line| {
                    let line = line.trim();
                    !line.starts_with("--")
                        && !line.starts_with('"')
                        && (line.contains("pcall(require, 'slate')")
                            || line.contains("require('slate')")
                            || line.contains("require(\"slate\")"))
                });
                if automatic {
                    report.connection(connected, &init);
                } else {
                    report.add("info", if connected {
                        "A direct loader reference remains; the preference only controls future automatic setup"
                    } else { "No direct Slate loader reference found; automatic activation is not requested" }, &init, None);
                }
            }
            if env.nvim_config_dir().join("init.lua").exists()
                && env.nvim_config_dir().join("init.vim").exists()
            {
                report.add("warning", "Both init.lua and init.vim exist", env.nvim_config_dir(),
                    Some("Keep one Neovim entry point; merge any settings you need before removing the other.".into()));
            }
            let loader = env.nvim_config_dir().join("lua/slate/init.lua");
            if let Some(content) =
                report.read_with_requirement(&loader, "Slate Neovim loader", automatic)
            {
                if content.contains(
                    "local STATE_PATH = vim.fn.expand('~/.cache/slate/current_theme.lua')",
                ) {
                    report.add("warning", "Loader uses the old fixed cache path", &loader,
                        Some(if automatic { "Run `slate setup` to regenerate the loader with the active cache path." }
                        else { "To regenerate, run `slate config set editor enable`, then `slate setup` and choose manual activation." }.into()));
                }
            }
            report.read_with_requirement(
                &env.slate_cache_dir().join("current_theme.lua"),
                "Current Neovim theme state",
                automatic,
            );
        }
        "zsh" | "bash" | "fish" => shell::inspect(&mut report, env, target),
        "kitty" => {
            let config = env.xdg_config_home().join("kitty/kitty.conf");
            let managed = env.config_dir().join("managed/kitty/theme.conf");
            if let Some(content) = report.read(&config, "Kitty configuration") {
                let lines =
                    crate::adapter::kitty_config::lines(content.as_bytes()).collect::<Vec<_>>();
                let connected = lines
                    .iter()
                    .filter_map(|line| line.value(b"include"))
                    .filter_map(|value| std::str::from_utf8(value).ok())
                    .any(|value| {
                        super::normalize_config_path(&resolve_path(value, &config, env))
                            == super::normalize_config_path(&managed)
                    });
                report.connection(connected, &config);
                let remote = lines
                    .iter()
                    .filter_map(|line| line.value(b"allow_remote_control"))
                    .filter_map(|value| std::str::from_utf8(value).ok())
                    .next_back();
                let listener = lines.iter().any(|line| line.value(b"listen_on").is_some());
                if !matches!(remote, Some("yes" | "socket" | "socket-only")) || !listener {
                    report.add("warning", "Live reload is not confirmed by this file", &config,
                        Some("Check allow_remote_control and listen_on, then restart Kitty after enabling them.".into()));
                }
            }
            report.read(&managed, "Slate Kitty colors");
        }
        "alacritty" => {
            let config = crate::adapter::AlacrittyAdapter::integration_config_path_with_env(env);
            let managed = env.config_dir().join("managed/alacritty/colors.toml");
            if let Some(content) = report.read(&config, "Alacritty configuration") {
                match content.parse::<toml_edit::DocumentMut>() {
                    Ok(doc) => {
                        let legacy = doc.get("import");
                        let imports =
                            crate::adapter::alacritty::integration::effective_import(&doc);
                        let valid_imports = imports.is_none_or(|item| {
                            item.as_array().is_some_and(|values| {
                                values.iter().all(|value| value.as_str().is_some())
                            })
                        });
                        if !valid_imports {
                            report.add(
                                "error",
                                "Effective Alacritty import must be an array of path strings",
                                &config,
                                Some(
                                    "Repair the effective import list before applying a theme."
                                        .into(),
                                ),
                            );
                        }
                        let connected = valid_imports
                            && imports.and_then(|v| v.as_array()).is_some_and(|paths| {
                                paths.iter().filter_map(|v| v.as_str()).any(|value| {
                                    super::normalize_config_path(&resolve_path(value, &config, env))
                                        == super::normalize_config_path(&managed)
                                })
                            });
                        report.connection(connected, &config);
                        if legacy.is_some() {
                            report.add(
                                "warning",
                                "Legacy top-level import is present",
                                &config,
                                Some(
                                    "Top-level import takes precedence. Reconcile both lists before migrating to [general]; theme application leaves their locations intact."
                                        .into(),
                                ),
                            );
                        }
                    }
                    Err(_) => report.add(
                        "error",
                        "Invalid Alacritty TOML; file contents omitted",
                        &config,
                        Some("Fix the TOML syntax before applying a theme.".into()),
                    ),
                }
            }
            if let Some(content) = report.read(&managed, "Slate Alacritty colors") {
                if content.parse::<toml::Value>().is_err() {
                    report.add(
                        "error",
                        "Invalid generated TOML; file contents omitted",
                        &managed,
                        Some("Reapply a theme to regenerate this file.".into()),
                    );
                }
            }
        }
        _ => unreachable!("doctor target is validated by the caller"),
    }
    report
}

fn resolve_path(value: &str, config: &Path, env: &SlateEnv) -> std::path::PathBuf {
    let value = value.trim_matches(['\'', '"']);
    if let Some(relative) = value.strip_prefix("~/") {
        return env.home().join(relative);
    }
    let path = std::path::PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        config.parent().unwrap_or(Path::new(".")).join(path)
    }
}

pub(super) fn handle(target: &str, env: &SlateEnv, json: bool, check_version: bool) -> Result<()> {
    let mut report = inspect(target, env);
    if check_version {
        nvim_version::inspect(&mut report, env);
    }
    let output = if json {
        format!("{}\n", serde_json::to_string_pretty(&report)?)
    } else {
        text_report(&report)
    };
    super::write_report(&output)
}

/// Menu-only presentation. Keep the same observations as the full CLI report,
/// without repeating successful file paths or routine activation caveats.
pub(super) fn menu_output(target: &str, env: &SlateEnv) -> String {
    menu_report(&inspect(target, env))
}

fn menu_report(report: &Report) -> String {
    menu_report_in(report, crate::cli::ui_language::current())
}

fn menu_report_in(report: &Report, language: crate::config::ui_language::UiLanguage) -> String {
    use std::fmt::Write;
    let tr = |zh, en| crate::config::ui_language::Text { zh, en }.get(language);
    let mut output = format!("◆ {} doctor\n", terminal_text(&report.target));
    let errors = report.checks.iter().filter(|c| c.status == "error").count();
    let warnings = report
        .checks
        .iter()
        .filter(|c| c.status == "warning")
        .count();
    let notes = report.checks.iter().filter(|c| c.status == "info").count();
    let counts = match language {
        crate::config::ui_language::UiLanguage::Chinese => {
            format!("文件检查：{errors} 项错误 · {warnings} 项待确认 · {notes} 项补充说明")
        }
        crate::config::ui_language::UiLanguage::English => {
            format!("File checks: {errors} errors · {warnings} to review · {notes} notes")
        }
    };
    let _ = writeln!(output, "{counts}");
    for check in &report.checks {
        // Unknown future statuses must remain visible, not silently disappear.
        if matches!(check.status, "ok" | "info") {
            continue;
        }
        let label = match check.status {
            "error" => tr("错误", "Error"),
            "warning" => tr("待确认", "Review"),
            _ => tr("检查", "Check"),
        };
        let message = if report.target == "kitty" {
            match check.message.as_str() {
                "No direct Slate loader reference found" => tr(
                    "未找到直接引用 Slate 配色的 include；间接引用未检查。",
                    "No direct Slate color include found; indirect references are not checked.",
                ),
                "Live reload is not confirmed by this file" => tr(
                    "此文件不足以确认实时重载设置。",
                    "This file does not confirm live-reload settings.",
                ),
                "Kitty configuration is missing" => {
                    tr("Kitty 配置文件不存在。", "Kitty configuration is missing.")
                }
                "Slate Kitty colors is missing" => tr(
                    "Slate 生成的 Kitty 配色文件不存在。",
                    "Slate's generated Kitty color file is missing.",
                ),
                other => other,
            }
        } else if report.target == "alacritty" {
            match check.message.as_str() {
                "No direct Slate loader reference found" => tr("有效导入列表中未找到 Slate 配色文件；间接引用未检查。", "No Slate colors in the effective import list; indirect references are not checked."),
                "Effective Alacritty import must be an array of path strings" => tr("有效 import 必须是路径字符串数组。", "The effective import must be an array of path strings."),
                "Legacy top-level import is present" => tr("存在旧式顶层 import，其优先级高于 general.import。", "Legacy top-level import takes precedence over general.import."),
                "Invalid Alacritty TOML; file contents omitted" => tr("Alacritty TOML 语法错误；未显示文件内容。", "Invalid Alacritty TOML; file contents omitted."),
                "Invalid generated TOML; file contents omitted" => tr("生成的配色 TOML 语法错误；未显示文件内容。", "Invalid generated color TOML; file contents omitted."),
                "Alacritty configuration is missing" => tr("Alacritty 配置文件不存在。", "Alacritty configuration is missing."),
                "Slate Alacritty colors is missing" => tr("Slate 生成的 Alacritty 配色文件不存在。", "Slate's generated Alacritty color file is missing."),
                other => other,
            }
        } else {
            &check.message
        };
        let _ = writeln!(
            output,
            "  {label}：{}",
            terminal_text(menu_text::localized(message, language))
        );
        if !check.path.is_empty() {
            let _ = writeln!(
                output,
                "    {}{}",
                terminal_text(&check.path),
                if check.path_is_lossy {
                    tr(
                        "（路径显示有损，并非精确路径）",
                        " (lossy display; not an exact path)",
                    )
                } else {
                    ""
                }
            );
        }
        if let Some(suggestion) = &check.suggestion {
            let suggestion = if report.target == "kitty"
                && matches!(
                    check.message.as_str(),
                    "No direct Slate loader reference found"
                        | "Live reload is not confirmed by this file"
                        | "Kitty configuration is missing"
                        | "Slate Kitty colors is missing"
                ) {
                tr("先检查 kitty.conf 的引用与远程控制设置；此处不会自动修复。", "Review kitty.conf references and remote-control settings; this check does not repair them.")
            } else {
                suggestion
            };
            let _ = writeln!(
                output,
                "    {}",
                terminal_text(menu_text::localized(suggestion, language))
            );
        }
    }
    let _ = writeln!(
        output,
        "{}", tr("仅检查文件与可用性；配置可能缺失，未验证工具启动或实际配色。未修改文件或启动工具。", "File and availability checks only; configurations may be missing. Startup and live colors are unverified. No files changed or tools launched.")
    );
    let _ = writeln!(
        output,
        "{}slate doctor {}",
        tr("完整结果与补充说明：", "Full report: "),
        terminal_text(&report.target)
    );
    output
}

pub(super) fn terminal_text(text: &str) -> String {
    let mut output = String::new();
    for c in text.chars() {
        if c.is_control() || matches!(c, '\u{2028}'..='\u{202e}' | '\u{2066}'..='\u{2069}') {
            output.extend(c.escape_default());
        } else {
            output.push(c);
        }
    }
    output
}

fn text_report(report: &Report) -> String {
    use std::fmt::Write;
    let mut output = format!("◆ {} doctor\n", report.target);
    for check in &report.checks {
        let _ = writeln!(
            output,
            "{}: {}\n  {}{}",
            check.status,
            terminal_text(&check.message),
            terminal_text(&check.path),
            if check.path_is_lossy {
                " (lossy display; not an exact path)"
            } else {
                ""
            }
        );
        if let Some(suggestion) = &check.suggestion {
            let _ = writeln!(output, "  {}", terminal_text(suggestion));
        }
    }
    let _ = writeln!(output, "{}", report.scope);
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn menu_report_collapses_notes_but_preserves_actionable_checks() {
        let td = TempDir::new().unwrap();
        let mut report = inspect("fastfetch", &SlateEnv::with_home(td.path().into()));
        report.checks.clear();
        report.add("ok", "ROUTINE_SUCCESS", Path::new("/routine"), None);
        report.add("info", "ROUTINE_NOTE", Path::new("/note"), None);
        report.add(
            "warning",
            "Needs review",
            Path::new("/warning"),
            Some("Preview first".into()),
        );
        report.add(
            "error",
            "Unsafe\u{1b}[2J",
            Path::new("/error"),
            Some("Repair locally".into()),
        );
        report.checks.last_mut().unwrap().path_is_lossy = true;
        report.add(
            "future",
            "Unknown status remains visible",
            Path::new(""),
            None,
        );
        let compact = menu_report(&report);
        assert!(compact.contains("1 项错误 · 1 项待确认 · 1 项补充说明"));
        for expected in [
            "Needs review",
            "/warning",
            "Preview first",
            "Unsafe\\u{1b}[2J",
            "/error",
            "路径显示有损",
            "Repair locally",
            "Unknown status remains visible",
            "未验证工具启动或实际配色",
            "完整结果与补充说明：slate doctor fastfetch",
        ] {
            assert!(compact.contains(expected), "missing {expected}: {compact}");
        }
        assert!(!compact.contains('\u{1b}'));
        for hidden in [
            "ROUTINE_SUCCESS",
            "ROUTINE_NOTE",
            "/routine",
            "/note",
            report.scope,
        ] {
            assert!(!compact.contains(hidden));
            assert!(text_report(&report).contains(hidden));
        }
        report.checks.clear();
        let empty = menu_report(&report);
        assert!(empty.contains("配置可能缺失"));
        assert!(empty.contains("0 项错误 · 0 项待确认"));
    }

    #[test]
    fn editor_preference_doctor_distinguishes_manual_mode_and_bad_record() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().into());
        crate::config::ConfigManager::from_env_paths(&env)
            .set_editor_auto_activation_enabled(false)
            .unwrap();
        let report = inspect("nvim", &env);
        assert!(report
            .checks
            .iter()
            .all(|check| check.status != "warning" && check.status != "error"));
        assert!(report
            .checks
            .iter()
            .any(|check| check.code == Some("auto_activation") && check.status == "info"));
        assert!(!env.nvim_init_path().exists());
        fs::write(env.nvim_auto_activation_path(), "PRIVATE_BAD\u{1b}").unwrap();
        let report = inspect("nvim", &env);
        assert!(report
            .checks
            .iter()
            .any(|check| check.code == Some("auto_activation") && check.status == "error"));
        assert!(!serde_json::to_string(&report)
            .unwrap()
            .contains("PRIVATE_BAD"));
    }

    #[test]
    fn doctor_integrations_empty_home_is_read_only() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().into());
        for target in [
            "nvim",
            "zsh",
            "bash",
            "fish",
            "kitty",
            "alacritty",
            "opencode",
            "opacity",
            "yazi",
            "zellij",
        ] {
            let report = inspect(target, &env);
            assert!(report.checks.iter().any(|c| c.status == "warning"));
            assert!(serde_json::to_value(&report).unwrap()["checks"].is_array());
        }
        assert_eq!(fs::read_dir(td.path()).unwrap().count(), 0);
    }

    #[test]
    fn kitty_doctor_uses_logical_lines_for_loader_and_remote_settings() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().into());
        let config = env.xdg_config_home().join("kitty/kitty.conf");
        let managed = env.managed_file("managed/kitty/theme.conf");
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        fs::create_dir_all(managed.parent().unwrap()).unwrap();
        fs::write(&managed, "foreground #ffffff\n").unwrap();
        for (include, connected) in [
            (format!("include {}\n", managed.display()), true),
            (
                format!(
                    "include {}/\n\\theme.conf\n",
                    managed.parent().unwrap().display()
                ),
                true,
            ),
            (format!("include\r\n  \\ {}\r\n", managed.display()), true),
            (format!("include {}\n\\-other\n", managed.display()), false),
            (
                format!("# disabled\n\\include {}\n", managed.display()),
                false,
            ),
            (format!("include_other {}\n", managed.display()), false),
        ] {
            let content = format!("{include}allow_remote_control socket-\n\\only\nlisten_\n\\on unix:/fixture/kitty\n");
            fs::write(&config, &content).unwrap();
            let report = inspect("kitty", &env);
            assert_eq!(
                report
                    .checks
                    .iter()
                    .any(|c| c.message == "Slate's loader reference is present"),
                connected,
                "{content}"
            );
            assert!(
                !report
                    .checks
                    .iter()
                    .any(|c| c.message == "Live reload is not confirmed by this file"),
                "{content}"
            );
            assert_eq!(fs::read_to_string(&config).unwrap(), content);
            assert_eq!(fs::read(&managed).unwrap(), b"foreground #ffffff\n");
        }
        fs::write(&config, format!("include {}\nallow_remote_control socket-only\n\\-invalid\nlisten_on unix:/fixture/kitty\n", managed.display())).unwrap();
        assert!(inspect("kitty", &env)
            .checks
            .iter()
            .any(|c| c.message == "Live reload is not confirmed by this file"));
        assert!(!env.slate_cache_dir().exists());
    }

    #[test]
    // SWATCH-RENDERER: test-only path bytes verify escaping, not interface styling.
    fn doctor_integrations_display_non_utf8_paths_without_losing_the_json_report() {
        use std::os::unix::ffi::OsStringExt;
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().into());
        let mut report = inspect("opencode", &env);
        let path = std::path::PathBuf::from(std::ffi::OsString::from_vec(
            b"/tmp/config-\xff\x1b[31m\n".to_vec(),
        ));
        report.add(
            "error",
            "example\u{202e}",
            &path,
            Some("example\u{1b}".into()),
        );
        let json = serde_json::to_value(&report).unwrap();
        let last = json["checks"].as_array().unwrap().last().unwrap();
        assert_eq!(last["path_is_lossy"], true);
        assert!(last["path"].as_str().unwrap().contains('\u{fffd}'));
        let text = text_report(&report);
        assert!(!text.contains('\u{1b}') && !text.contains('\u{202e}'));
        assert!(text.contains("lossy display; not an exact path"));
        assert!(text.contains("\\u{1b}[31m\\n"));
    }

    #[test]
    fn doctor_integrations_distinguish_comments_broken_and_connected_configs() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().into());
        let config = env.xdg_config_home().join("alacritty/alacritty.toml");
        let managed = env.config_dir().join("managed/alacritty/colors.toml");
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        fs::create_dir_all(managed.parent().unwrap()).unwrap();
        fs::write(&managed, "[colors.primary]\nbackground = '#000000'\n").unwrap();
        for content in [
            format!("# import = [{:?}]\n", managed),
            "[general\n".into(),
            format!("[general]\nimport = [{:?}]\n", managed),
        ] {
            fs::write(&config, &content).unwrap();
            let report = inspect("alacritty", &env);
            let connected = report
                .checks
                .iter()
                .any(|c| c.message == "Slate's loader reference is present");
            assert_eq!(connected, content.starts_with("[general]"));
            assert_eq!(
                report.checks.iter().any(|c| c.status == "error"),
                content == "[general\n"
            );
            assert_eq!(fs::read_to_string(&config).unwrap(), content);
        }
        for content in ["include_other file", "# include file"] {
            assert!(crate::adapter::kitty_config::lines(content.as_bytes())
                .all(|line| line.value(b"include").is_none()));
        }
    }
}
