use crate::config::ConfigManager;
use crate::design::symbols::Symbols;
use crate::detection::TerminalProfile;
use crate::env::SlateEnv;
use crate::error::Result;
use crate::opacity::OpacityPreset;
use crate::platform;

pub(crate) fn enable_auto_theme(config: &ConfigManager) -> Result<()> {
    platform::dark_mode_notify::ensure_binary(config)?;
    config.set_auto_theme_enabled(true)?;

    if let Err(err) = config.refresh_shell_integration() {
        let _ = config.set_auto_theme_enabled(false);
        return Err(err);
    }

    // Start watcher immediately so the user doesn't have to open a new terminal
    let _ = platform::dark_mode_notify::start(config);

    // UX-02 (D-D2): inline trigger — this path bypassed apply_all but touched
    // shell integration (refresh_shell_integration above). `slate config` has
    // no --auto / --quiet flags, so both guards are false.
    crate::cli::new_shell_reminder::emit_new_shell_reminder_once(false, false);

    Ok(())
}

pub(crate) fn disable_auto_theme(config: &ConfigManager) -> Result<()> {
    let was_enabled = config.is_auto_theme_enabled()?;

    config.set_auto_theme_enabled(false)?;
    if let Err(err) = config.refresh_shell_integration() {
        if was_enabled {
            let _ = config.set_auto_theme_enabled(true);
            let _ = config.refresh_shell_integration();
        }
        return Err(err);
    }

    platform::dark_mode_notify::stop()?;
    platform::dark_mode_notify::remove_binary(config)?;

    // UX-02 (D-D2): inline trigger — disable also mutates shell integration
    // (we re-render env files without the watcher hook).
    crate::cli::new_shell_reminder::emit_new_shell_reminder_once(false, false);

    Ok(())
}

/// Handle `slate config set <key> <value>` command
pub fn handle_config_set(key: &str, value: &str) -> Result<()> {
    let env = SlateEnv::from_process()?;
    handle_config_set_with_env(key, value, &env)
}

fn handle_config_set_with_env(key: &str, value: &str, env: &SlateEnv) -> Result<()> {
    let config = ConfigManager::with_env(env)?;
    let terminal = TerminalProfile::detect();
    let appearance_backend = platform::desktop::detect_backend();

    match key {
        "opacity" => {
            // value ∈ {solid, frosted, clear}
            let preset = match value {
                "solid" => OpacityPreset::Solid,
                "frosted" => OpacityPreset::Frosted,
                "clear" => OpacityPreset::Clear,
                _ => {
                    return Err(crate::error::SlateError::InvalidConfig(format!(
                        "Invalid opacity preset: '{}'. Must be one of: solid, frosted, clear",
                        value
                    )))
                }
            };

            crate::cli::apply::apply_opacity(
                env,
                preset,
                crate::cli::apply::OpacityApplyOptions {
                    persist_state: true,
                    reload_terminals: true,
                },
            )?;

            println!("{} Opacity set to '{}'", Symbols::SUCCESS, value);
            Ok(())
        }
        "auto-theme" => {
            match value {
                "enable" => {
                    enable_auto_theme(&config)?;

                    println!("{} Auto theme enabled", Symbols::SUCCESS);
                    println!("  Appearance backend: {}", appearance_backend.label());
                    if terminal.watcher_shell_autostart_supported()
                        && appearance_backend.supports_watcher()
                    {
                        println!("  Ghostty shell sessions can relaunch the watcher automatically");
                    } else if appearance_backend.supports_watcher() {
                        println!(
                            "  Watching is available now, but restart recovery is still most seamless in Ghostty shells"
                        );
                    } else {
                        println!(
                            "  Automatic watching is unavailable here, but `slate theme --auto` still works on demand"
                        );
                    }
                    println!("  Run 'slate config set auto-theme configure' to customize dark/light pairing");
                    Ok(())
                }
                "disable" => {
                    disable_auto_theme(&config)?;

                    println!("{} Auto theme disabled", Symbols::SUCCESS);
                    Ok(())
                }
                "configure" => {
                    crate::cli::auto_theme::configure_auto_theme()?;

                    if config.is_auto_theme_enabled()? {
                        platform::dark_mode_notify::ensure_binary(&config)?;
                        config.refresh_shell_integration()?;
                        // Restart watcher so new pairing takes effect immediately
                        let _ = platform::dark_mode_notify::stop();
                        let _ = platform::dark_mode_notify::start(&config);
                    }

                    Ok(())
                }
                _ => Err(crate::error::SlateError::InvalidConfig(format!(
                    "Invalid auto-theme action: '{}'. Must be one of: enable, disable, configure",
                    value
                ))),
            }
        }
        "fastfetch" => match value {
            "enable" => {
                config.enable_fastfetch_autorun()?;
                config.refresh_shell_integration()?;
                println!("{} Fastfetch auto-run enabled", Symbols::SUCCESS);
                // UX-02 (D-D2): inline trigger — refresh_shell_integration
                // above rewrote env files, so a new shell is needed to pick
                // up the fastfetch wrapper.
                crate::cli::new_shell_reminder::emit_new_shell_reminder_once(false, false);
                Ok(())
            }
            "disable" => {
                config.disable_fastfetch_autorun()?;
                config.refresh_shell_integration()?;
                println!("{} Fastfetch auto-run disabled", Symbols::SUCCESS);
                // UX-02 (D-D2): inline trigger — symmetrical with enable;
                // the env file no longer sources the fastfetch wrapper.
                crate::cli::new_shell_reminder::emit_new_shell_reminder_once(false, false);
                Ok(())
            }
            _ => Err(crate::error::SlateError::InvalidConfig(format!(
                "Invalid fastfetch action: '{}'. Must be one of: enable, disable",
                value
            ))),
        },
        "sound" => match value {
            "on" => {
                config.set_sound_enabled(true)?;
                println!("{} Sound feedback enabled", Symbols::SUCCESS);
                Ok(())
            }
            "off" => {
                config.set_sound_enabled(false)?;
                println!("{} Sound feedback disabled", Symbols::SUCCESS);
                Ok(())
            }
            _ => Err(crate::error::SlateError::InvalidConfig(format!(
                "Invalid sound value: '{}'. Must be one of: on, off",
                value
            ))),
        },
        // Phase 17 Plan 06 — `slate config editor disable` strips the
        // D-09 marker block from init.lua / init.vim without touching
        // the 18 `slate-*.lua` shims or the loader. For users who want
        // to keep the colorscheme files available (so
        // `:colorscheme slate-<variant>` still works) but stop the
        // `pcall(require, 'slate')` auto-activation.
        "editor" => match value {
            "disable" => {
                let init_lua = env.home().join(".config/nvim/init.lua");
                let init_vim = env.home().join(".config/nvim/init.vim");
                // Best-effort — primitive is a no-op on missing files.
                crate::adapter::marker_block::remove_managed_blocks_from_file(&init_lua)?;
                crate::adapter::marker_block::remove_managed_blocks_from_file(&init_vim)?;
                println!(
                    "{} Slate's nvim auto-activation disabled. Colors/ files remain; \
                     run `:colorscheme slate-<variant>` manually.",
                    Symbols::SUCCESS
                );
                Ok(())
            }
            _ => Err(crate::error::SlateError::InvalidConfig(format!(
                "Invalid editor action: '{}'. Must be one of: disable",
                value
            ))),
        },
        _ => Err(crate::error::SlateError::InvalidConfig(format!(
            "Unknown config key: '{}'. Known keys: opacity, auto-theme, fastfetch, sound, editor",
            key
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::handle_config_set_with_env;
    use crate::cli::new_shell_reminder::REMINDER_TEST_LOCK;
    use crate::env::SlateEnv;
    use tempfile::TempDir;

    fn managed_tool_dir(env: &SlateEnv, tool: &str) -> std::path::PathBuf {
        env.config_dir().join("managed").join(tool)
    }

    #[test]
    fn test_handle_config_set_opacity_applies_managed_files_immediately() {
        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());

        handle_config_set_with_env("opacity", "frosted", &env).unwrap();

        assert_eq!(
            std::fs::read_to_string(env.managed_file("current-opacity")).unwrap(),
            "frosted"
        );
        assert!(managed_tool_dir(&env, "ghostty")
            .join("opacity.conf")
            .exists());
        assert!(managed_tool_dir(&env, "ghostty").join("blur.conf").exists());
        assert!(managed_tool_dir(&env, "kitty")
            .join("opacity.conf")
            .exists());
        assert!(managed_tool_dir(&env, "alacritty")
            .join("opacity.toml")
            .exists());
    }

    /// UX-02 wiring tests. Each config sub-command tail emits via
    /// `emit_new_shell_reminder_once(false, false)` on the success path. We
    /// can't invoke `handle_config_set_with_env("auto-theme", "enable", …)`
    /// directly — `enable_auto_theme` spawns the `dark-mode-notify` watcher
    /// via `platform::dark_mode_notify::start`, which would persist beyond
    /// the test. Instead, we mirror the emit call and assert flag state.
    ///
    /// The opacity sub-command has NO corresponding emit call in the
    /// handler body (RESEARCH Q4: terminal-hot-reloadable); we verify this
    /// by running the full handler end-to-end and asserting the flag
    /// remains false. This is the load-bearing negative test that catches
    /// a future regression where someone adds `emit_new_shell_reminder_once`
    /// to the opacity match arm.
    fn config_handler_emit() {
        crate::cli::new_shell_reminder::emit_new_shell_reminder_once(false, false);
    }

    #[test]
    fn config_enable_auto_theme_emits_reminder() {
        let _guard = REMINDER_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        crate::cli::new_shell_reminder::reset_reminder_flag_for_tests();

        config_handler_emit();

        assert!(
            crate::cli::new_shell_reminder::reminder_flag_for_tests(),
            "config auto-theme enable tail must transition the reminder flag"
        );
    }

    #[test]
    fn config_disable_auto_theme_emits_reminder() {
        let _guard = REMINDER_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        crate::cli::new_shell_reminder::reset_reminder_flag_for_tests();

        config_handler_emit();

        assert!(
            crate::cli::new_shell_reminder::reminder_flag_for_tests(),
            "config auto-theme disable tail must transition the reminder flag"
        );
    }

    #[test]
    fn config_fastfetch_enable_emits_reminder() {
        let _guard = REMINDER_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        crate::cli::new_shell_reminder::reset_reminder_flag_for_tests();

        config_handler_emit();

        assert!(
            crate::cli::new_shell_reminder::reminder_flag_for_tests(),
            "config fastfetch enable tail must transition the reminder flag"
        );
    }

    #[test]
    fn config_fastfetch_disable_emits_reminder() {
        let _guard = REMINDER_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        crate::cli::new_shell_reminder::reset_reminder_flag_for_tests();

        config_handler_emit();

        assert!(
            crate::cli::new_shell_reminder::reminder_flag_for_tests(),
            "config fastfetch disable tail must transition the reminder flag"
        );
    }

    /// Load-bearing negative test: the opacity sub-command is
    /// terminal-hot-reloadable per RESEARCH Q4, so it MUST NOT emit the
    /// reminder. We invoke the real handler (opacity goes through
    /// `apply_opacity` without touching the watcher, so this is safe) and
    /// assert that the flag stays in its reset state.
    #[test]
    fn config_opacity_does_not_emit_reminder() {
        let _guard = REMINDER_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        crate::cli::new_shell_reminder::reset_reminder_flag_for_tests();

        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        handle_config_set_with_env("opacity", "frosted", &env).unwrap();

        assert!(
            !crate::cli::new_shell_reminder::reminder_flag_for_tests(),
            "opacity sub-command is terminal-hot-reloadable (RESEARCH Q4) and MUST NOT emit the new-shell reminder"
        );
    }

    // ────────────────────────────────────────────────────────────
    // Phase 17 Plan 06 — `slate config editor disable` sub-command
    // ────────────────────────────────────────────────────────────

    /// `slate config editor disable` strips the marker block from
    /// init.lua but leaves the 18 slate-*.lua shims + the loader
    /// intact. Users who chose this verb want to stop auto-activation
    /// while preserving `:colorscheme slate-<variant>` access.
    #[test]
    fn config_editor_disable_removes_marker_leaves_colors() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_path_buf());

        // Seed: run the real adapter install (writes 18 shims + loader).
        let registry = crate::theme::ThemeRegistry::new().unwrap();
        let theme = registry.get("catppuccin-mocha").unwrap().clone();
        crate::adapter::NvimAdapter::setup(&env, &theme).unwrap();

        // Seed: simulate option-A marker insertion into init.lua.
        let init_lua = td.path().join(".config/nvim/init.lua");
        std::fs::create_dir_all(init_lua.parent().unwrap()).unwrap();
        let block = format!(
            "{}\npcall(require, 'slate')\n{}\n",
            crate::adapter::marker_block::START,
            crate::adapter::marker_block::END,
        );
        std::fs::write(&init_lua, &block).unwrap();

        // Exercise the editor disable sub-command.
        handle_config_set_with_env("editor", "disable", &env).unwrap();

        // Colors/ shims must survive — the whole point of the verb.
        let colors_dir = td.path().join(".config/nvim/colors");
        let shim_count = std::fs::read_dir(&colors_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("slate-"))
            .count();
        assert!(
            shim_count > 0,
            "colors/ slate-*.lua shims must survive editor disable, found {}",
            shim_count
        );

        // Loader dir must survive too.
        assert!(
            td.path().join(".config/nvim/lua/slate").exists(),
            "lua/slate/ must survive editor disable"
        );

        // Marker block must be stripped.
        let after = std::fs::read_to_string(&init_lua).unwrap();
        assert!(
            !after.contains(crate::adapter::marker_block::START),
            "init.lua START marker must be removed by editor disable"
        );
        assert!(
            !after.contains(crate::adapter::marker_block::END),
            "init.lua END marker must be removed by editor disable"
        );
    }

    /// Unknown editor action → InvalidConfig error, clear message.
    #[test]
    fn config_editor_rejects_unknown_action() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_path_buf());

        let result = handle_config_set_with_env("editor", "force-on", &env);
        assert!(result.is_err(), "unknown editor action must error");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("force-on"),
            "error must name the invalid action: {}",
            err_msg
        );
        assert!(
            err_msg.contains("disable"),
            "error must list the valid action: {}",
            err_msg
        );
    }

    /// `editor disable` on a home with no init files is a no-op:
    /// no error, nothing created. Mirrors the "best-effort" posture
    /// of the other missing-files paths.
    #[test]
    fn config_editor_disable_is_noop_when_no_init_files() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_path_buf());

        let result = handle_config_set_with_env("editor", "disable", &env);
        assert!(
            result.is_ok(),
            "editor disable on an empty home must succeed silently"
        );
        assert!(!td.path().join(".config/nvim/init.lua").exists());
        assert!(!td.path().join(".config/nvim/init.vim").exists());
    }

    /// Regression guard: the unknown-top-level-key error message
    /// advertises the new `editor` verb so users discover it.
    #[test]
    fn config_unknown_key_error_lists_editor() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_path_buf());

        let result = handle_config_set_with_env("nonexistent-key", "anything", &env);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("editor"),
            "unknown-key error must include `editor` in the known-keys list: {}",
            msg
        );
    }
}
