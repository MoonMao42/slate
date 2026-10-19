use super::ui_language::tr;
use crate::brand::render_context::RenderContext;
use crate::brand::roles::Roles;
#[cfg(test)]
use crate::config::ConfigManager;
use crate::error::Result;
use std::io::IsTerminal;

mod auto_theme;
mod preferences;

use auto_theme::*;
use preferences::*;

/// Bare `slate`: recovery first, then an interactive hub or read-only status.
pub fn handle() -> Result<()> {
    handle_with_options(false, false)
}

pub fn handle_with_options(auto: bool, quiet: bool) -> Result<()> {
    let env = crate::env::SlateEnv::from_process()?;
    let recovery = super::recover::PreviewRecoveryStatus::inspect(&env);
    if recovery.needs_attention() {
        return show_recovery_hub(&env, &recovery);
    }
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        super::status::handle(false)?;
        println!("Open a terminal and run `slate` for the menu, `slate theme` to preview, or `slate setup` to connect tools.");
        return Ok(());
    }
    if !super::ui_language::initialize(&env)? {
        return Ok(());
    }
    let mut last_entry = "switch";
    loop {
        super::ui_language::refresh(&env)?;
        // A watcher or another invocation may begin a preview while a subpage
        // is open. Never return to ordinary actions over new recovery state.
        let recovery = super::recover::PreviewRecoveryStatus::inspect(&env);
        if recovery.status == super::recover::PreviewState::Busy {
            let refresh = super::menu::select(tr(
                "配置仍被占用 · 等另一项操作结束后刷新",
                "Configuration Busy · Refresh after the other operation finishes",
            ))
            .item(
                true,
                tr("刷新状态", "Refresh"),
                tr(
                    "只检查状态，不重试刚才的操作",
                    "Check status without retrying the operation",
                ),
            )
            .item(false, tr("退出", "Quit"), "")
            .escape_value(false)
            .interact()?;
            if !refresh {
                return Ok(());
            }
            continue;
        }
        if recovery.needs_attention() {
            return show_recovery_hub(&env, &recovery);
        }
        if !show_hub_once(&env, auto, quiet, &mut last_entry)? {
            return Ok(());
        }
    }
}

/// Resolve an interrupted preview before normal menu actions can mutate its
/// files or try to parse a config file that may have been interrupted mid-write.
fn show_recovery_hub(
    env: &crate::env::SlateEnv,
    recovery: &super::recover::PreviewRecoveryStatus,
) -> Result<()> {
    if !show_recovery_hub_once(env, recovery)? {
        return Ok(());
    }
    loop {
        let refreshed = super::recover::PreviewRecoveryStatus::inspect(env);
        if !refreshed.needs_attention() || !show_recovery_hub_once(env, &refreshed)? {
            return Ok(());
        }
    }
}

/// Only a completed read-only review loops; mutation and error paths exit.
fn show_recovery_hub_once(
    env: &crate::env::SlateEnv,
    recovery: &super::recover::PreviewRecoveryStatus,
) -> Result<bool> {
    use super::recover::PreviewState;
    cliclack::intro(tr("slate · 预览恢复", "slate · Preview Recovery"))?;
    for line in recovery.lines() {
        cliclack::log::warning(line)?;
    }
    if matches!(recovery.status, PreviewState::Active | PreviewState::Busy) {
        return Ok(false);
    }
    if !std::io::stdin().is_terminal() {
        if recovery.status != PreviewState::Unreadable {
            println!("Use `slate recover --export <new-directory>` to inspect saved originals.");
        } else {
            println!("Original-file export requires readable, valid recovery metadata; check access and record integrity first.");
        }
        println!("To keep current files, `slate recover --discard` asks before deleting only the recovery copy.");
        return Ok(false);
    }
    let mut menu = super::menu::select(tr(
        "上次预览尚未结束，请先检查恢复方案",
        "An unfinished preview needs review",
    ));
    if matches!(
        recovery.status,
        PreviewState::Pending | PreviewState::Conflicted
    ) {
        menu = menu.item(
            "review",
            tr("查看文件差异", "Review File Changes"),
            tr(
                "只读比较当前文件和预览前的副本，不修改文件",
                "Compare current files with saved originals; no writes",
            ),
        );
    } else {
        // An unreadable record cannot yield a diff. Never make deleting the
        // only recovery copy the default just because review is unavailable.
        menu = menu.initial_value("quit");
    }
    if recovery.status == PreviewState::Pending {
        menu = menu.item(
            "recover",
            tr("恢复预览前的文件", "Restore Pre-Preview Files"),
            tr(
                "下一步确认后恢复；可能覆盖当前文件",
                "Confirm next; may overwrite current files",
            ),
        );
    }
    let action = menu
        .item(
            "discard",
            tr("保留当前文件", "Keep Current Files"),
            tr(
                "下一步确认后仅删除预览恢复副本，不恢复旧文件",
                "Confirm next to delete only the recovery copy; no file restoration",
            ),
        )
        .item(
            "quit",
            tr("退出", "Quit"),
            tr(
                "保留当前文件与恢复副本",
                "Keep current files and the recovery copy",
            ),
        )
        .escape_value("quit")
        .interact()
        .map_err(|err| {
            if err.kind() == std::io::ErrorKind::Interrupted {
                crate::error::SlateError::UserCancelled
            } else {
                crate::error::SlateError::IOError(err)
            }
        })?;
    match action {
        "review" => {
            super::recover::handle_menu_review(env)?;
            super::menu::select(tr(
                "文件比较完成，尚未执行恢复。",
                "File review complete; nothing restored",
            ))
            .item((), tr("返回恢复菜单", "Back to Recovery"), "")
            .escape_value(())
            .interact()?;
            Ok(true)
        }
        "recover" => super::recover::handle(env, false, false, false, false, None).map(|()| false),
        "discard" => super::recover::handle(env, false, false, false, true, None).map(|()| false),
        _ => Ok(false),
    }
}

/// Compact, aligned status rows. Only the theme value carries an accent;
/// repeated bullets, pills and italic font names distract from the menu.
const HUB_STATUS_LABEL_WIDTH: usize = 8;

fn format_hub_status_row(label: &str, value: impl std::fmt::Display) -> String {
    let width = console::measure_text_width(label);
    let spaces = HUB_STATUS_LABEL_WIDTH.saturating_sub(width).max(1);
    format!("{label}{:spaces$}{value}", "")
}

fn format_hub_status_panel(
    r: &Roles<'_>,
    theme_name: &str,
    opacity: &str,
    font: Option<&str>,
) -> Vec<String> {
    let opacity = match opacity {
        "Solid" => tr("不透明", "Solid"),
        "Frosted" => tr("磨砂", "Frosted"),
        "Clear" => tr("透明", "Clear"),
        "Not saved" => tr("未设置", "Not set"),
        other => other,
    };
    vec![
        format_hub_status_row(tr("主题", "Theme"), r.theme_name(theme_name)),
        format_hub_status_row(tr("透明度", "Opacity"), opacity),
        format_hub_status_row(tr("字体", "Font"), font.unwrap_or(tr("未设置", "Not set"))),
    ]
}

/// Refresh saved state for each visit. `false` means an explicit Quit.
fn show_hub_once(
    env: &crate::env::SlateEnv,
    auto: bool,
    quiet: bool,
    last_entry: &mut &'static str,
) -> Result<bool> {
    cliclack::intro("slate")?;

    // One compact block, not one log entry and spacer per preference.
    let saved = super::status::StatusReport::inspect(env)?;
    let theme_name = saved.theme.as_ref().and_then(|theme| theme.name.as_deref());
    let has_theme = theme_name.is_some();
    for warning in &saved.warnings {
        cliclack::log::warning(tr(warning.menu_message, warning.message))?;
    }
    let needs_check = |field| saved.warnings.iter().any(|warning| warning.field == field);
    if !has_theme && !needs_check("theme") {
        cliclack::log::info(tr(
            "还没有可用的已保存主题。可以先预览配色，或连接常用工具。",
            "No saved theme. Preview a theme or connect your tools.",
        ))?;
    } else if let Some(theme_name) = theme_name {
        let ctx = RenderContext::from_active_theme()?;
        let r = Roles::new(&ctx);
        let font = saved
            .font
            .as_deref()
            .or_else(|| needs_check("font").then_some(tr("需检查", "Check settings")))
            .map(super::file_output::terminal_text);
        let mut lines = format_hub_status_panel(
            &r,
            theme_name,
            saved
                .opacity
                .as_deref()
                .unwrap_or(if needs_check("opacity") {
                    tr("需检查", "Check settings")
                } else {
                    "Not saved"
                }),
            font.as_deref(),
        );
        if let Some(style) = saved.prompt_style {
            let label = tr(super::prompt::menu_style_label(style), style.label());
            lines.push(format_hub_status_row(tr("提示符", "Prompt"), label));
        }
        let terminal = console::Term::stderr();
        for line in lines {
            terminal.write_line(&format!("│  {line}"))?;
        }
    }

    console::Term::stderr().write_line("│")?;

    let mut menu_builder =
        super::menu::select(tr("想调整什么？", "What would you like to change?"))
            .escape_value("quit")
            .initial_value(*last_entry);
    menu_builder = menu_builder.item(
        "switch",
        if has_theme {
            tr("切换主题", "Switch Theme")
        } else {
            tr("预览主题", "Preview Themes")
        },
        tr("先预览配色，再确认保存", "Preview before saving"),
    );
    menu_builder = menu_builder.item("setup", tr("工具配色", "Tool Themes"), "");
    menu_builder = menu_builder.item("font", tr("更换字体", "Change Font"), "");
    menu_builder = menu_builder.item("prompt", tr("命令提示符", "Prompt Style"), "");

    menu_builder = menu_builder.item(
        "auto-theme",
        match saved.auto_theme_enabled {
            Some(true) => tr("自动换色：开", "Auto-Theme: On"),
            Some(false) => tr("自动换色：关", "Auto-Theme: Off"),
            None => tr("自动换色：需检查", "Auto-Theme: Check Settings"),
        },
        tr("跟随系统深浅模式", "Follow system appearance"),
    );

    menu_builder = menu_builder.item("tools", tr("终端偏好", "Preferences"), "");

    menu_builder = menu_builder.item("status", tr("检查配置", "Check Configuration"), "");

    menu_builder = menu_builder.item(
        "restore",
        tr("恢复配置", "Restore Configuration"),
        tr("确认后再恢复", "Review before restoring"),
    );

    menu_builder = menu_builder.item("about", tr("关于 Slate", "About Slate"), "");

    menu_builder = menu_builder.item("quit", tr("退出", "Quit"), "");

    // Successful actions and explicit Back choices return here; cancellation
    // errors still propagate so Esc/Ctrl+C in a menu cannot trap the user.
    let selection = menu_builder.interact().map_err(|e| {
        if e.kind() == std::io::ErrorKind::Interrupted {
            crate::error::SlateError::UserCancelled
        } else {
            crate::error::SlateError::IOError(e)
        }
    })?;

    // Defer sound cache creation until a mutating action is actually selected.
    // Font and restore own their confirmation-time initialization.
    if selection == "switch" {
        crate::brand::SoundSink::install(env, auto, quiet);
    }

    let result = match selection {
        "switch" => crate::cli::picker::launch_picker(env),
        "status" => super::status::handle_menu(),
        "about" => super::about::handle_menu(),
        "font" => crate::cli::font::handle_font(None),
        "prompt" => super::prompt::handle_menu(env),
        "auto-theme" => show_auto_theme_result(handle_auto_theme(env, auto, quiet)),
        "tools" => show_shell_preferences_result(handle_tool_toggles(env)),
        "restore" => crate::cli::restore::handle_menu(auto, quiet),
        "setup" => super::tools::handle_hub_menu(env),
        "quit" => {
            cliclack::outro(tr("已退出", "Goodbye"))?;
            Ok(())
        }
        _ => {
            cliclack::outro(tr("已退出", "Goodbye"))?;
            Ok(())
        }
    };
    result?;
    // Session-only navigation state: returning does not reopen the subpage or
    // save another preference, and refreshed labels keep the same action ID.
    *last_entry = selection;
    Ok(selection != "quit")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::SlateEnv;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::TempDir;

    #[test]
    fn english_failure_notices_keep_partial_write_and_busy_boundaries() {
        std::thread::spawn(|| {
            use crate::{
                config::ui_language::{self, UiLanguage},
                error::SlateError,
            };
            let home = TempDir::new().unwrap();
            let env = SlateEnv::with_home(home.path().to_owned());
            ui_language::save(&env, UiLanguage::English).unwrap();
            super::super::ui_language::load_saved_ui_language(&env).unwrap();
            let message =
                auto_theme_failure_notice(Err(SlateError::InvalidConfig("denied\x1b[2J".into())))
                    .unwrap()
                    .unwrap();
            for text in [
                "Auto-Theme operation incomplete",
                "no automatic retry",
                "changes may remain",
                "file recovery does not restore running processes",
                "slate doctor auto-theme",
            ] {
                assert!(message.contains(text), "{message}");
            }
            assert!(!message.contains('\x1b'));
            let busy = auto_theme_failure_notice(Err(SlateError::ConfigurationBusy))
                .unwrap()
                .unwrap();
            assert!(busy.contains("wrote no settings") && !busy.contains("changes may remain"));
            let preferences = shell_preferences_failure_notice(Err(SlateError::InvalidConfig(
                "denied\x1b[2J".into(),
            )))
            .unwrap()
            .unwrap();
            assert!(preferences.starts_with("Preferences update incomplete:"));
            assert!(!preferences.contains('\x1b'));
        })
        .join()
        .unwrap();
    }

    #[test]
    fn shell_preference_feedback_preserves_errors_and_propagates_interrupts() {
        use crate::error::SlateError;
        assert!(shell_preferences_failure_notice(Ok(())).unwrap().is_none());
        let message = shell_preferences_failure_notice(Err(SlateError::ConfigWriteError(
            "file\n\x1b[2J".into(),
            "permission denied".into(),
        )))
        .unwrap()
        .unwrap();
        assert!(message.contains("permission denied") && message.contains("终端偏好未完成"));
        assert!(!message.chars().any(char::is_control));
        assert!(matches!(
            shell_preferences_failure_notice(Err(SlateError::UserCancelled)),
            Err(SlateError::UserCancelled)
        ));
        assert!(matches!(
            shell_preferences_failure_notice(Err(SlateError::IOError(std::io::Error::from(
                std::io::ErrorKind::BrokenPipe
            )))),
            Err(SlateError::IOError(_))
        ));
        assert!(matches!(
            shell_preferences_failure_notice(Err(SlateError::Internal("fixture".into()))),
            Err(SlateError::Internal(_))
        ));
    }

    #[test]
    fn auto_theme_menu_keeps_readonly_diagnostics_available_for_unreadable_preferences() {
        for enabled in [None, Some(false), Some(true)] {
            let actions = auto_theme_actions(enabled);
            assert!(actions.iter().any(|(id, _, _)| *id == "check"));
            assert!(actions.iter().any(|(id, _, _)| *id == "back"));
            assert_eq!(
                actions.iter().any(|(id, _, _)| *id == "toggle"),
                enabled.is_some()
            );
            assert_eq!(
                actions.iter().any(|(id, _, _)| *id == "configure"),
                enabled.is_some()
            );
        }
        let home = TempDir::new().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        fs::create_dir_all(env.config_dir()).unwrap();
        let path = env.managed_file("config.toml");
        fs::write(&path, "[broken").unwrap();
        super::super::doctor::handle_auto_theme_with_env(&env, true).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "[broken");
        assert_eq!(fs::read_dir(env.config_dir()).unwrap().count(), 1);
        assert!(!env.slate_cache_dir().exists());
    }

    #[test]
    fn auto_theme_failure_returns_notice_without_retry_and_preserves_exit_errors() {
        use crate::error::SlateError;
        let temp = TempDir::new().unwrap();
        let env = SlateEnv::with_home(temp.path().to_path_buf());
        let config = ConfigManager::with_env(&env).unwrap();
        config.set_auto_theme_enabled(false).unwrap();
        let attempts = AtomicUsize::new(0);
        let result = sync_auto_theme_toggle_with(
            &config,
            true,
            |_| {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err(SlateError::InvalidConfig(
                    "helper preparation failed: Permission denied\n\x1b".into(),
                ))
            },
            |_| panic!("must not disable as a fallback"),
        );
        let notice = auto_theme_failure_notice(result).unwrap().unwrap();
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
        assert!(!config.is_auto_theme_enabled().unwrap());
        assert!(notice.contains("Permission denied"));
        assert!(notice.contains("不自动重试"));
        assert!(notice.contains("修改可能仍然保留"));
        assert!(notice.contains("slate doctor auto-theme"));
        assert!(!notice.contains('\x1b'));
        let busy = auto_theme_failure_notice(Err(SlateError::ConfigurationBusy))
            .unwrap()
            .unwrap();
        assert!(busy.contains("本次操作未写入设置"));
        assert!(!busy.contains("修改可能仍然保留"));
        assert!(auto_theme_failure_notice(Ok(())).unwrap().is_none());
        assert!(matches!(
            auto_theme_failure_notice(Err(SlateError::UserCancelled)),
            Err(SlateError::UserCancelled)
        ));
        assert!(matches!(
            auto_theme_failure_notice(Err(
                std::io::Error::from(std::io::ErrorKind::BrokenPipe).into()
            )),
            Err(SlateError::IOError(_))
        ));
    }

    #[test]
    fn test_sync_auto_theme_toggle_enables_watcher_and_config() {
        let temp = TempDir::new().unwrap();
        let env = SlateEnv::with_home(temp.path().to_path_buf());
        let config = ConfigManager::with_env(&env).unwrap();
        let install_calls = AtomicUsize::new(0);
        let uninstall_calls = AtomicUsize::new(0);

        sync_auto_theme_toggle_with(
            &config,
            true,
            |config| {
                install_calls.fetch_add(1, Ordering::SeqCst);
                config.set_auto_theme_enabled(true)
            },
            |config| {
                uninstall_calls.fetch_add(1, Ordering::SeqCst);
                config.set_auto_theme_enabled(false)
            },
        )
        .unwrap();

        assert!(config.is_auto_theme_enabled().unwrap());
        assert_eq!(install_calls.load(Ordering::SeqCst), 1);
        assert_eq!(uninstall_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn test_sync_auto_theme_toggle_disables_watcher_and_config() {
        let temp = TempDir::new().unwrap();
        let env = SlateEnv::with_home(temp.path().to_path_buf());
        let config = ConfigManager::with_env(&env).unwrap();
        let install_calls = AtomicUsize::new(0);
        let uninstall_calls = AtomicUsize::new(0);

        config.set_auto_theme_enabled(true).unwrap();

        sync_auto_theme_toggle_with(
            &config,
            false,
            |config| {
                install_calls.fetch_add(1, Ordering::SeqCst);
                config.set_auto_theme_enabled(true)
            },
            |config| {
                uninstall_calls.fetch_add(1, Ordering::SeqCst);
                config.set_auto_theme_enabled(false)
            },
        )
        .unwrap();

        assert!(!config.is_auto_theme_enabled().unwrap());
        assert_eq!(install_calls.load(Ordering::SeqCst), 0);
        assert_eq!(uninstall_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_toggle_fastfetch_from_preferences_rewrites_shell_integration() {
        let temp = TempDir::new().unwrap();
        let env = SlateEnv::with_home(temp.path().to_path_buf());
        let config = ConfigManager::with_env(&env).unwrap();
        let shell_path = env.config_dir().join("managed/shell/env.zsh");

        config.set_current_theme("catppuccin-mocha").unwrap();

        toggle_fastfetch_from_preferences(&config).unwrap();

        let enabled_content = fs::read_to_string(&shell_path).unwrap();
        assert!(config.has_fastfetch_autorun().unwrap());
        assert!(enabled_content.contains("if command -v fastfetch >/dev/null 2>&1; then"));
        assert!(enabled_content.contains("  fastfetch\n"));

        toggle_fastfetch_from_preferences(&config).unwrap();

        let disabled_content = fs::read_to_string(&shell_path).unwrap();
        assert!(!config.has_fastfetch_autorun().unwrap());
        assert!(!disabled_content.contains("if command -v fastfetch >/dev/null 2>&1; then"));
    }

    #[test]
    fn test_toggle_starship_from_preferences_rewrites_shell_integration() {
        let temp = TempDir::new().unwrap();
        let env = SlateEnv::with_home(temp.path().to_path_buf());
        let config = ConfigManager::with_env(&env).unwrap();
        let shell_path = env.config_dir().join("managed/shell/env.zsh");

        config.set_current_theme("catppuccin-mocha").unwrap();

        toggle_starship_from_preferences(&config).unwrap();

        let disabled_content = fs::read_to_string(&shell_path).unwrap();
        assert!(!config.is_starship_enabled().unwrap());
        assert!(!disabled_content.contains("starship init zsh"));

        toggle_starship_from_preferences(&config).unwrap();

        let enabled_content = fs::read_to_string(&shell_path).unwrap();
        assert!(config.is_starship_enabled().unwrap());
        assert!(enabled_content.contains("starship init zsh"));
    }

    #[test]
    fn test_toggle_zsh_highlighting_from_preferences_rewrites_shell_integration() {
        let temp = TempDir::new().unwrap();
        let env = SlateEnv::with_home(temp.path().to_path_buf());
        let config = ConfigManager::with_env(&env).unwrap();
        let shell_path = env.config_dir().join("managed/shell/env.zsh");

        config.set_current_theme("catppuccin-mocha").unwrap();

        toggle_zsh_highlighting_from_preferences(&config).unwrap();

        let disabled_content = fs::read_to_string(&shell_path).unwrap();
        assert!(!config.is_zsh_highlighting_enabled().unwrap());
        assert!(!disabled_content.contains("highlight-styles.sh"));

        toggle_zsh_highlighting_from_preferences(&config).unwrap();

        let enabled_content = fs::read_to_string(&shell_path).unwrap();
        assert!(config.is_zsh_highlighting_enabled().unwrap());
        assert!(enabled_content.contains("highlight-styles.sh"));
    }

    /// Snapshot of the compact three-row panel in basic color mode.
    #[test]
    fn hub_status_panel_basic_mode_snapshot() {
        use crate::brand::render_context::{mock_context_with_mode, mock_theme, RenderMode};

        let theme = mock_theme();
        let ctx = mock_context_with_mode(&theme, RenderMode::Basic);
        let r = Roles::new(&ctx);
        let lines =
            format_hub_status_panel(&r, "Catppuccin Mocha", "Frosted", Some("JetBrains Mono"));
        insta::assert_snapshot!("hub_status_panel_basic", lines.join("\n"));
    }

    #[test]
    fn hub_status_panel_aligns_values_without_repeated_decorations() {
        use crate::brand::render_context::{mock_context_with_mode, mock_theme, RenderMode};

        let theme = mock_theme();
        for mode in [RenderMode::Truecolor, RenderMode::Basic, RenderMode::None] {
            let ctx = mock_context_with_mode(&theme, mode);
            let r = Roles::new(&ctx);
            let lines =
                format_hub_status_panel(&r, "Catppuccin Mocha", "Solid", Some("JetBrains Mono"));
            let joined = lines.join("\n");
            assert!(!joined.contains('◆'));
            assert!(!joined.contains('`'));
            let plain = console::strip_ansi_codes(&joined);
            assert_eq!(
                plain.as_ref(),
                "主题    Catppuccin Mocha\n透明度  不透明\n字体    JetBrains Mono"
            );
            for label in ["主题", "透明度", "字体"] {
                assert!(
                    joined.contains(label),
                    "missing label `{label}` in mode {mode:?}: {joined:?}"
                );
            }
        }

        std::thread::spawn(move || {
            use crate::config::ui_language::{self, UiLanguage};
            let home = TempDir::new().unwrap();
            let env = SlateEnv::with_home(home.path().to_owned());
            ui_language::save(&env, UiLanguage::English).unwrap();
            super::super::ui_language::load_saved_ui_language(&env).unwrap();
            let ctx = mock_context_with_mode(&theme, RenderMode::None);
            let r = Roles::new(&ctx);
            let mut lines =
                format_hub_status_panel(&r, "Catppuccin Mocha", "Solid", Some("JetBrains Mono"));
            lines.push(format_hub_status_row(tr("提示符", "Prompt"), "Minimal"));
            assert_eq!(
                lines.join("\n"),
                "Theme   Catppuccin Mocha\nOpacity Solid\nFont    JetBrains Mono\nPrompt  Minimal"
            );
            for (line, value) in
                lines
                    .iter()
                    .zip(["Catppuccin Mocha", "Solid", "JetBrains Mono", "Minimal"])
            {
                let prefix = line.strip_suffix(value).expect("row ends with value");
                assert_eq!(console::measure_text_width(prefix), HUB_STATUS_LABEL_WIDTH);
            }
        })
        .join()
        .unwrap();
    }

    #[test]
    fn hub_status_panel_handles_missing_font() {
        use crate::brand::render_context::{mock_context_with_mode, mock_theme, RenderMode};

        let theme = mock_theme();
        let ctx = mock_context_with_mode(&theme, RenderMode::None);
        let r = Roles::new(&ctx);
        let lines = format_hub_status_panel(&r, "Mock", "Solid", None);
        assert!(lines[2].contains("未设置"));
    }

    #[test]
    fn toggle_sound_from_preferences_creates_checkpoint_and_flips_state() {
        let _sink_guard = crate::brand::events::reset_sink_for_tests();
        let temp = TempDir::new().unwrap();
        let env = SlateEnv::with_home(temp.path().to_path_buf());
        let config = ConfigManager::from_env_paths(&env);

        assert!(config.is_sound_enabled().unwrap());
        toggle_sound_from_preferences(&config).unwrap();
        assert!(!config.is_sound_enabled().unwrap());
        assert_eq!(
            crate::config::list_restore_points_with_env(&env)
                .unwrap()
                .len(),
            1
        );

        toggle_sound_from_preferences(&config).unwrap();
        assert!(config.is_sound_enabled().unwrap());
    }

    #[test]
    fn apply_opacity_from_preferences_rejects_stale_expectation_and_persists_when_matching() {
        use crate::opacity::OpacityPreset;

        let temp = TempDir::new().unwrap();
        let env = SlateEnv::with_home(temp.path().to_path_buf());
        let config = ConfigManager::from_env_paths(&env);

        assert_eq!(
            config.get_current_opacity_preset().unwrap(),
            OpacityPreset::Solid
        );

        let err =
            apply_opacity_from_preferences(&env, OpacityPreset::Frosted, OpacityPreset::Clear)
                .unwrap_err()
                .to_string();
        assert!(err.contains("Opacity preference changed"), "{err}");
        assert_eq!(
            config.get_current_opacity_preset().unwrap(),
            OpacityPreset::Solid
        );

        apply_opacity_from_preferences(&env, OpacityPreset::Solid, OpacityPreset::Frosted).unwrap();
        assert_eq!(
            config.get_current_opacity_preset().unwrap(),
            OpacityPreset::Frosted
        );
    }
}
