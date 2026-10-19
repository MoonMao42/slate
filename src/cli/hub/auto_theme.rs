use crate::cli::config::{disable_auto_theme, enable_auto_theme};
use crate::cli::ui_language::tr;
use crate::config::ConfigManager;
use crate::error::Result;

pub(super) fn sync_auto_theme_toggle(config: &ConfigManager, enabled: bool) -> Result<()> {
    sync_auto_theme_toggle_with(config, enabled, enable_auto_theme, disable_auto_theme)
}

pub(super) fn sync_auto_theme_toggle_with<Enable, Disable>(
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

pub(super) fn auto_theme_failure_notice(result: Result<()>) -> Result<Option<String>> {
    use crate::error::SlateError;
    match result {
        Ok(()) => Ok(None),
        Err(error @ (SlateError::UserCancelled | SlateError::IOError(_))) => Err(error),
        Err(SlateError::ConfigurationBusy) => Ok(Some(
            tr(
                "配置正被另一个 Slate 操作占用，本次操作未写入设置，不自动重试。请等另一项操作结束后刷新状态。",
                "Another Slate operation holds the configuration. This operation wrote no settings and will not retry automatically. Refresh after the other operation finishes.",
            )
            .into(),
        )),
        Err(error) => Ok(Some(format!(
            "{}{}\n{}",
            tr("自动换色操作未完成：", "Auto-Theme operation incomplete: "),
            crate::cli::file_output::terminal_text(&error.to_string()),
            tr(
                "已返回首页并刷新状态，不自动重试。之前的文件或后台修改可能仍然保留。请先运行 slate doctor auto-theme，并查看上方恢复点；恢复文件不会恢复运行中的进程。",
                "Returned home with refreshed status; no automatic retry. Earlier file or service changes may remain. Run slate doctor auto-theme and inspect the recovery point above; file recovery does not restore running processes.",
            ),
        ))),
    }
}

pub(super) fn show_auto_theme_result(result: Result<()>) -> Result<()> {
    if let Some(message) = auto_theme_failure_notice(result)? {
        cliclack::log::warning(message)?;
    }
    Ok(())
}

pub(super) fn auto_theme_actions(
    enabled: Option<bool>,
) -> Vec<(&'static str, &'static str, &'static str)> {
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

pub(super) fn handle_auto_theme(env: &crate::env::SlateEnv, auto: bool, quiet: bool) -> Result<()> {
    let mut next_selection = None;
    loop {
        let currently_enabled = ConfigManager::from_env_paths(env)
            .is_auto_theme_enabled()
            .ok();
        if currently_enabled.is_none() {
            cliclack::log::warning(tr(
                "无法读取自动换色设置，请先检查配置；不会把未知状态当成已关闭。",
                "Auto-theme settings are unreadable; check configuration. Unknown does not mean off.",
            ))?;
        }
        crate::cli::file_output::write_output(&saved_pairing_summary(env))?;
        let mut menu = crate::cli::menu::select(tr("自动换色", "Auto-Theme")).escape_value("back");
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
                        crate::cli::doctor::handle_auto_theme_with_env(env, false)?;
                    } else {
                        crate::cli::doctor::handle_auto_theme_menu(env)?;
                    }
                    let back = crate::cli::menu::select(tr(
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
