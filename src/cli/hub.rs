use super::ui_language::tr;
use crate::brand::render_context::RenderContext;
use crate::brand::roles::Roles;
use crate::cli::config::{disable_auto_theme, enable_auto_theme};
use crate::config::ConfigManager;
use crate::error::Result;
use std::io::IsTerminal;

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
        format!("{}    {}", tr("主题", "Theme"), r.theme_name(theme_name)),
        format!("{}  {opacity}", tr("透明度", "Opacity")),
        format!(
            "{}    {}",
            tr("字体", "Font"),
            font.unwrap_or(tr("未设置", "Not set"))
        ),
    ]
}

fn sync_auto_theme_toggle(config: &ConfigManager, enabled: bool) -> Result<()> {
    sync_auto_theme_toggle_with(config, enabled, enable_auto_theme, disable_auto_theme)
}

fn sync_auto_theme_toggle_with<Enable, Disable>(
    config: &ConfigManager,
    enabled: bool,
    enable_auto_theme: Enable,
    disable_auto_theme: Disable,
) -> Result<()>
where
    Enable: Fn(&ConfigManager) -> Result<()>,
    Disable: Fn(&ConfigManager) -> Result<()>,
{
    if enabled {
        enable_auto_theme(config)?;
    } else {
        disable_auto_theme(config)?;
    }

    Ok(())
}

fn toggle_fastfetch_from_preferences(config: &ConfigManager) -> Result<()> {
    let was_enabled = config.has_fastfetch_autorun()?;

    super::config::set_shell_preference(
        config,
        crate::config::shell_change::ShellPreference::Fastfetch(!was_enabled),
    )?;
    if !was_enabled {
        // Warn when fastfetch isn't on the user's actual PATH — the shell wrapper runs
        // `command -v fastfetch`, which skips binaries sitting only in a homebrew fallback.
        // Checking is_tier1 mirrors what the autorun guard will resolve at shell startup.
        if !crate::detection::detect_tool_presence("fastfetch").is_tier1() {
            let _ = cliclack::log::warning(
                "fastfetch is not on PATH — run `brew install fastfetch` (or add it to PATH) to see the banner on shell startup.",
            );
        }
    }

    Ok(())
}

fn toggle_starship_from_preferences(config: &ConfigManager) -> Result<()> {
    let was_enabled = config.is_starship_enabled()?;
    super::config::set_shell_preference(
        config,
        crate::config::shell_change::ShellPreference::Starship(!was_enabled),
    )
}

fn toggle_zsh_highlighting_from_preferences(config: &ConfigManager) -> Result<()> {
    let was_enabled = config.is_zsh_highlighting_enabled()?;
    super::config::set_shell_preference(
        config,
        crate::config::shell_change::ShellPreference::Highlighting(!was_enabled),
    )
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
            lines.push(format!("{}  {label}", tr("提示符", "Prompt")));
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

fn shell_preferences_failure_notice(result: Result<()>) -> Result<Option<String>> {
    use crate::error::SlateError;
    match result {
        Ok(()) => Ok(None),
        Err(
            error @ (SlateError::InvalidConfig(_)
            | SlateError::ConfigReadError(_, _)
            | SlateError::ConfigWriteError(_, _)
            | SlateError::BackupFailed(_)
            | SlateError::ConfigurationBusy),
        ) => Ok(Some(format!(
            "{}{}",
            tr("终端偏好未完成：", "Preferences update incomplete: "),
            super::file_output::terminal_text(&error.to_string()),
        ))),
        Err(error) => Err(error),
    }
}

fn show_shell_preferences_result(result: Result<()>) -> Result<()> {
    if let Some(message) = shell_preferences_failure_notice(result)? {
        cliclack::log::warning(message)?;
        super::file_output::write_output(tr("返回首页，不自动重试。若已有部分文件写入，请先查看上方错误和恢复点；可从首页进入检查配置或恢复备份。\n", "Returning home without retrying. If files were partially written, review the error and recovery point; use Check Configuration or Restore Configuration from the main menu.\n"))?;
    }
    Ok(())
}

fn auto_theme_failure_notice(result: Result<()>) -> Result<Option<String>> {
    use crate::error::SlateError;
    match result {
        Ok(()) => Ok(None),
        Err(error @ (SlateError::UserCancelled | SlateError::IOError(_))) => Err(error),
        Err(SlateError::ConfigurationBusy) => Ok(Some(
            tr("配置正被另一个 Slate 操作占用，本次操作未写入设置，不自动重试。请等另一项操作结束后刷新状态。", "Another Slate operation holds the configuration. This operation wrote no settings and will not retry automatically. Refresh after the other operation finishes.").into(),
        )),
        Err(error) => Ok(Some(format!(
            "{}{}\n{}",
            tr("自动换色操作未完成：", "Auto-Theme operation incomplete: "),
            super::file_output::terminal_text(&error.to_string()),
            tr("已返回首页并刷新状态，不自动重试。之前的文件或后台修改可能仍然保留。请先运行 slate doctor auto-theme，并查看上方恢复点；恢复文件不会恢复运行中的进程。", "Returned home with refreshed status; no automatic retry. Earlier file or service changes may remain. Run slate doctor auto-theme and inspect the recovery point above; file recovery does not restore running processes."),
        ))),
    }
}

fn show_auto_theme_result(result: Result<()>) -> Result<()> {
    if let Some(message) = auto_theme_failure_notice(result)? {
        cliclack::log::warning(message)?;
    }
    Ok(())
}

fn auto_theme_actions(enabled: Option<bool>) -> Vec<(&'static str, &'static str, &'static str)> {
    let mut actions = Vec::new();
    if let Some(enabled) = enabled {
        actions.push((
            "toggle",
            if enabled {
                tr("关闭自动换色", "Turn Off Auto-Theme")
            } else {
                tr("开启自动换色", "Turn On Auto-Theme")
            },
            tr(
                "启用或停止自动主题后台",
                "Start or stop the auto-theme service",
            ),
        ));
        actions.push((
            "configure",
            tr("选择深浅主题", "Choose Dark and Light Themes"),
            tr(
                "保存深浅配对 · 不立即换色、不启停后台",
                "Save pairing without applying colors or changing the service",
            ),
        ));
    }
    actions.push((
        "check",
        tr("检查自动换色", "Check Auto-Theme"),
        tr(
            "只读检查权限和后台状态",
            "Inspect permissions and service status",
        ),
    ));
    actions.push(("back", tr("返回首页", "Back"), ""));
    actions
}

fn saved_pairing_summary(env: &crate::env::SlateEnv) -> String {
    use crate::theme::{ThemeAppearance, ThemeRegistry};
    let Ok(pair) = crate::config::pairing::inspect(env) else {
        return tr(
            "配对设置无法读取；请进入检查自动换色，文件未改动。\n",
            "Pairing is unreadable; check Auto-Theme. No files changed.\n",
        )
        .into();
    };
    let Ok(registry) = ThemeRegistry::new() else {
        return tr(
            "主题目录无法读取，暂时不能显示已保存配对。\n",
            "Theme catalog is unreadable; saved pairing cannot be displayed.\n",
        )
        .into();
    };
    let label = |id: Option<&str>, appearance| match id {
        None => tr("自动选择（未固定）", "Automatic (not pinned)"),
        Some(id) => match registry.get(id) {
            Some(theme) if theme.appearance == appearance => theme.name.as_str(),
            Some(_) => tr(
                "深浅类别不匹配，请重新选择",
                "Appearance mismatch; choose again",
            ),
            None => tr("主题未识别，请重新选择", "Unknown theme; choose again"),
        },
    };
    format!(
        "{}\n{}  {}\n{}  {}\n",
        tr(
            "已保存配对 · 不代表当前外观",
            "Saved Pairing · Not Live Appearance"
        ),
        tr("深色", "Dark"),
        label(pair.dark_theme.as_deref(), ThemeAppearance::Dark),
        tr("浅色", "Light"),
        label(pair.light_theme.as_deref(), ThemeAppearance::Light),
    )
}

fn handle_auto_theme(env: &crate::env::SlateEnv, auto: bool, quiet: bool) -> Result<()> {
    let mut next_selection = None;
    loop {
        let currently_enabled = ConfigManager::from_env_paths(env)
            .is_auto_theme_enabled()
            .ok();
        if currently_enabled.is_none() {
            cliclack::log::warning(
                tr("无法读取自动换色设置，请先检查配置；不会把未知状态当成已关闭。", "Auto-theme settings are unreadable; check configuration. Unknown does not mean off."),
            )?;
        }
        super::file_output::write_output(&saved_pairing_summary(env))?;
        let mut menu = super::menu::select(tr("自动换色", "Auto-Theme")).escape_value("back");
        if let Some(selection) = next_selection {
            menu = menu.initial_value(selection);
        }
        for (id, label, hint) in auto_theme_actions(currently_enabled) {
            menu = menu.item(id, label, hint);
        }
        let selection = menu.interact().map_err(|e| {
            if e.kind() == std::io::ErrorKind::Interrupted {
                crate::error::SlateError::UserCancelled
            } else {
                crate::error::SlateError::IOError(e)
            }
        })?;

        match selection {
            "toggle" => {
                let currently_enabled = currently_enabled.ok_or_else(|| {
                    crate::error::SlateError::InvalidConfig(
                        "Auto-theme state is unreadable; inspect it before toggling.".into(),
                    )
                })?;
                let _write_guard = crate::config::ConfigWriteGuard::acquire(env)?;
                let config = ConfigManager::from_env_paths(env);
                if config.is_auto_theme_enabled()? != currently_enabled {
                    return Err(crate::error::SlateError::InvalidConfig(
                    "Auto-theme preference changed while the menu was open; review again before toggling.".into(),
                ));
                }
                crate::brand::SoundSink::install(env, auto, quiet);
                let config = ConfigManager::with_env(env)?;
                let new_state = !currently_enabled;
                sync_auto_theme_toggle(&config, new_state)?;
            }
            "configure" => {
                crate::cli::auto_theme::configure_auto_theme()?;
            }
            "check" => {
                let mut detailed = false;
                loop {
                    if detailed {
                        super::doctor::handle_auto_theme_with_env(env, false)?;
                    } else {
                        super::doctor::handle_auto_theme_menu(env)?;
                    }
                    let back = super::menu::select(tr(
                        "自动换色检查 · 只读，未修改设置",
                        "Auto-Theme Check · Read-only",
                    ))
                    .item(true, tr("返回自动换色", "Back to Auto-Theme"), "")
                    .item(
                        false,
                        if detailed {
                            tr("查看摘要", "Summary")
                        } else {
                            tr("查看完整诊断", "Full Diagnostics")
                        },
                        tr(
                            "重新读取状态 · 不修改配置、不启停后台",
                            "Refresh without changing configuration or service",
                        ),
                    )
                    .initial_value(true)
                    .escape_value(true)
                    .interact()?;
                    if back {
                        break;
                    }
                    detailed = !detailed;
                }
            }
            _ => return Ok(()),
        }
        // Re-read state after each completed action. Never leave another toggle
        // selected immediately after enabling/disabling a background service.
        next_selection = Some(if selection == "toggle" {
            "back"
        } else {
            selection
        });
    }
}

fn handle_tool_toggles(env: &crate::env::SlateEnv) -> Result<()> {
    let mut next_selection = "starship";
    loop {
        let config = ConfigManager::from_env_paths(env);
        let starship_enabled = config.is_starship_enabled()?;
        let zsh_highlighting_enabled = config.is_zsh_highlighting_enabled()?;
        let fastfetch_enabled = config.has_fastfetch_autorun()?;

        let selection = super::menu::select(tr("终端偏好", "Preferences"))
            .escape_value("back")
            .initial_value(next_selection)
            .item(
                "starship",
                if starship_enabled {
                    tr("命令提示符：开", "Prompt: On")
                } else {
                    tr("命令提示符：关", "Prompt: Off")
                },
                if starship_enabled {
                    tr(
                        "关闭 Slate 的 Starship 自动加载，保留提示符配置",
                        "Disable Starship loading; keep its configuration",
                    )
                } else {
                    tr(
                        "开启 Starship 自动加载，显示路径和 Git 等信息",
                        "Load Starship in new shells",
                    )
                },
            )
            .item(
                "zsh-highlighting",
                if zsh_highlighting_enabled {
                    tr("语法高亮：开", "Syntax Highlighting: On")
                } else {
                    tr("语法高亮：关", "Syntax Highlighting: Off")
                },
                if zsh_highlighting_enabled {
                    tr(
                        "关闭 Slate 加载的命令着色，不卸载高亮插件",
                        "Disable loading; keep the highlighting plugin",
                    )
                } else {
                    tr(
                        "开启命令着色，需要已安装 Zsh 高亮插件",
                        "Requires the Zsh highlighting plugin",
                    )
                },
            )
            .item(
                "fastfetch",
                if fastfetch_enabled {
                    tr("启动信息：开", "Startup Summary: On")
                } else {
                    tr("启动信息：关", "Startup Summary: Off")
                },
                if fastfetch_enabled {
                    tr(
                        "关闭自动展示，不卸载 Fastfetch，仍可手动运行",
                        "Disable startup display; keep Fastfetch",
                    )
                } else {
                    tr(
                        "开启终端启动时的系统信息展示，需要已安装 Fastfetch",
                        "Show a startup summary; requires Fastfetch",
                    )
                },
            )
            .item("language", "语言 / Language", "")
            .item("back", tr("返回首页", "Back"), "")
            .interact()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::Interrupted {
                    crate::error::SlateError::UserCancelled
                } else {
                    crate::error::SlateError::IOError(e)
                }
            })?;

        if selection == "back" {
            return Ok(());
        }
        if selection == "language" {
            super::ui_language::choose(env)?;
            return Ok(());
        }
        // Merely entering/leaving preferences must not acquire a writer or
        // initialize directories/sound. Freeze the displayed intent under the
        // guard; do not invert a newer setting saved by another Slate process.
        let _write_guard = crate::config::ConfigWriteGuard::acquire(env)?;
        let current = ConfigManager::from_env_paths(env);
        let unchanged = match selection {
            "starship" => current.is_starship_enabled()? == starship_enabled,
            "zsh-highlighting" => {
                current.is_zsh_highlighting_enabled()? == zsh_highlighting_enabled
            }
            "fastfetch" => current.has_fastfetch_autorun()? == fastfetch_enabled,
            _ => return Ok(()),
        };
        if !unchanged {
            return Err(crate::error::SlateError::InvalidConfig(
                "Shell preference changed while the menu was open; review again before toggling. No toggle was applied.".into(),
            ));
        }
        // Prepared writers validate targets before creating any default files.
        let config = ConfigManager::from_env_paths(env);
        match selection {
            "starship" => {
                toggle_starship_from_preferences(&config)?;
            }
            "zsh-highlighting" => {
                toggle_zsh_highlighting_from_preferences(&config)?;
            }
            "fastfetch" => {
                toggle_fastfetch_from_preferences(&config)?;
            }
            "back" => return Ok(()),
            _ => return Ok(()),
        }
        super::file_output::write_output(tr(
            "设置已保存，新开终端标签页后生效。\n",
            "Settings saved; open a new terminal tab to use them.\n",
        ))?;
        next_selection = "back";
    }
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
}
