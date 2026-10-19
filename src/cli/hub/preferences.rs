use crate::cli::ui_language::tr;
use crate::config::{recovery_paths, ConfigManager};
use crate::error::Result;
use crate::opacity::OpacityPreset;

pub(super) fn toggle_fastfetch_from_preferences(config: &ConfigManager) -> Result<()> {
    let was_enabled = config.has_fastfetch_autorun()?;

    crate::cli::config::set_shell_preference(
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

pub(super) fn toggle_starship_from_preferences(config: &ConfigManager) -> Result<()> {
    let was_enabled = config.is_starship_enabled()?;
    crate::cli::config::set_shell_preference(
        config,
        crate::config::shell_change::ShellPreference::Starship(!was_enabled),
    )
}

pub(super) fn toggle_zsh_highlighting_from_preferences(config: &ConfigManager) -> Result<()> {
    let was_enabled = config.is_zsh_highlighting_enabled()?;
    crate::cli::config::set_shell_preference(
        config,
        crate::config::shell_change::ShellPreference::Highlighting(!was_enabled),
    )
}

pub(super) fn toggle_sound_from_preferences(config: &ConfigManager) -> Result<()> {
    let env = config.environment();
    let _guard = crate::config::ConfigWriteGuard::acquire(env)?;
    let was_enabled = config.is_sound_enabled()?;
    let targets = recovery_paths::targets(env, [env.managed_file("config.toml")], "Configuration")?;
    let point = crate::config::snapshot_config_targets_with_env(env, &targets)?;
    eprintln!(
        "已创建恢复点 · 查看改动：slate restore {} --dry-run",
        point.id
    );
    std::fs::create_dir_all(config.base_path())?;
    config.set_sound_enabled(!was_enabled)
}

pub(super) fn apply_opacity_from_preferences(
    env: &crate::env::SlateEnv,
    expected_opacity: OpacityPreset,
    desired_opacity: OpacityPreset,
) -> Result<()> {
    let _write_guard = crate::config::ConfigWriteGuard::acquire(env)?;
    let current = ConfigManager::from_env_paths(env).get_current_opacity_preset()?;
    if current != expected_opacity {
        return Err(crate::error::SlateError::InvalidConfig(
            "Opacity preference changed while the menu was open; review again before saving. No change was applied.".into(),
        ));
    }
    crate::cli::apply::apply_opacity(
        env,
        desired_opacity,
        crate::cli::apply::OpacityApplyOptions {
            persist_state: true,
            reload_terminals: true,
            snapshot_policy: crate::cli::apply::SnapshotPolicy::Create,
        },
    )
}

fn choose_opacity_from_preferences(
    env: &crate::env::SlateEnv,
    current_opacity: OpacityPreset,
) -> Result<bool> {
    let selection = crate::cli::menu::select(tr("窗口透明度", "Window Opacity"))
        .escape_value(None)
        .initial_value(Some(current_opacity))
        .item(
            Some(OpacityPreset::Solid),
            tr("不透明", "Solid"),
            tr("完全不透明背景，文字对比度最高", "Fully opaque background"),
        )
        .item(
            Some(OpacityPreset::Frosted),
            tr("磨砂", "Frosted"),
            tr(
                "半透明磨砂背景（Ghostty 支持背景模糊）",
                "Translucent background with blur in Ghostty",
            ),
        )
        .item(
            Some(OpacityPreset::Clear),
            tr("透明", "Clear"),
            tr("高透背景，透出桌面壁纸", "High transparency background"),
        )
        .item(None, tr("返回终端偏好", "Back"), "")
        .interact()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::Interrupted {
                crate::error::SlateError::UserCancelled
            } else {
                crate::error::SlateError::IOError(e)
            }
        })?;
    let Some(preset) = selection else {
        return Ok(false);
    };
    apply_opacity_from_preferences(env, current_opacity, preset)?;
    Ok(true)
}

pub(super) fn shell_preferences_failure_notice(result: Result<()>) -> Result<Option<String>> {
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
            crate::cli::file_output::terminal_text(&error.to_string()),
        ))),
        Err(error) => Err(error),
    }
}

pub(super) fn show_shell_preferences_result(result: Result<()>) -> Result<()> {
    if let Some(message) = shell_preferences_failure_notice(result)? {
        cliclack::log::warning(message)?;
        crate::cli::file_output::write_output(tr(
            "返回首页，不自动重试。若已有部分文件写入，请先查看上方错误和恢复点；可从首页进入检查配置或恢复备份。\n",
            "Returning home without retrying. If files were partially written, review the error and recovery point; use Check Configuration or Restore Configuration from the main menu.\n",
        ))?;
    }
    Ok(())
}

pub(super) fn handle_tool_toggles(env: &crate::env::SlateEnv) -> Result<()> {
    let mut next_selection = "starship";
    loop {
        let config = ConfigManager::from_env_paths(env);
        let starship_enabled = config.is_starship_enabled()?;
        let zsh_highlighting_enabled = config.is_zsh_highlighting_enabled()?;
        let fastfetch_enabled = config.has_fastfetch_autorun()?;
        let current_opacity = config.get_current_opacity_preset()?;
        let sound_enabled = config.is_sound_enabled()?;

        let selection = crate::cli::menu::select(tr("终端偏好", "Preferences"))
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
            .item(
                "opacity",
                match current_opacity {
                    OpacityPreset::Solid => tr("窗口透明度：不透明", "Window Opacity: Solid"),
                    OpacityPreset::Frosted => tr("窗口透明度：磨砂", "Window Opacity: Frosted"),
                    OpacityPreset::Clear => tr("窗口透明度：透明", "Window Opacity: Clear"),
                },
                tr(
                    "切换终端背景为不透明、磨砂或透明",
                    "Choose Solid, Frosted, or Clear terminal background",
                ),
            )
            .item(
                "sound",
                if sound_enabled {
                    tr("操作音效：开", "Sound Feedback: On")
                } else {
                    tr("操作音效：关", "Sound Feedback: Off")
                },
                if sound_enabled {
                    tr("关闭菜单与切换操作提示音", "Mute interactive sound effects")
                } else {
                    tr("开启菜单与切换操作提示音", "Play interactive sound effects")
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
            crate::cli::ui_language::choose(env)?;
            return Ok(());
        }
        if selection == "opacity" {
            if choose_opacity_from_preferences(env, current_opacity)? {
                crate::cli::file_output::write_output(tr(
                    "设置已保存，新开终端标签页后生效。\n",
                    "Settings saved; open a new terminal tab to use them.\n",
                ))?;
                next_selection = "back";
            } else {
                next_selection = "opacity";
            }
            continue;
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
            "sound" => current.is_sound_enabled()? == sound_enabled,
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
            "sound" => {
                toggle_sound_from_preferences(&config)?;
            }
            "back" => return Ok(()),
            _ => return Ok(()),
        }
        crate::cli::file_output::write_output(tr(
            "设置已保存，新开终端标签页后生效。\n",
            "Settings saved; open a new terminal tab to use them.\n",
        ))?;
        next_selection = "back";
    }
}
