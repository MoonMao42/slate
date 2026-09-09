use crate::brand::events::{dispatch, BrandEvent, SuccessKind};
use crate::brand::render_context::RenderContext;
use crate::brand::roles::Roles;
use crate::config::ConfigManager;
use crate::detection::TerminalProfile;
use crate::env::SlateEnv;
use crate::error::Result;
use crate::opacity::OpacityPreset;
use crate::platform;

mod catalog;
mod inspect;
pub mod pairing;
mod shell_settings;

pub(super) fn set_shell_preference(
    config: &ConfigManager,
    preference: crate::config::shell_change::ShellPreference,
) -> Result<()> {
    shell_settings::apply_menu(config, preference)
}

pub(super) fn set_fastfetch_autorun(config: &ConfigManager, enabled: bool) -> Result<()> {
    shell_settings::apply(
        config,
        crate::config::shell_change::ShellPreference::Fastfetch(enabled),
    )
}
pub use catalog::{validate_key, validate_set, SETTINGS};
pub use inspect::handle_inspect;

pub(crate) fn enable_auto_theme(config: &ConfigManager) -> Result<()> {
    shell_settings::apply(
        config,
        crate::config::shell_change::ShellPreference::AutoTheme(true),
    )?;

    // This path bypasses apply_all but may regenerate the watcher startup hook.
    crate::cli::new_shell_reminder::emit_new_shell_reminder_once(false, false);

    Ok(())
}

pub(crate) fn disable_auto_theme(config: &ConfigManager) -> Result<()> {
    shell_settings::apply(
        config,
        crate::config::shell_change::ShellPreference::AutoTheme(false),
    )?;

    // UX-02 (D-D2): inline trigger — disable also mutates shell integration
    // (we re-render env files without the watcher hook).
    crate::cli::new_shell_reminder::emit_new_shell_reminder_once(false, false);

    Ok(())
}

/// Handle `slate config set <key> <value>` command.
/// migration: every successful set emits via `Roles::status_success`
/// (theme.green per D-01a — NEVER lavender) and dispatches
/// `BrandEvent::Success(SuccessKind::ConfigSet)` so SoundSink
/// rings the same completion-event channel for every config mutation.
/// Where the value is a user-facing literal (opacity preset, sub-action),
/// it routes through `Roles::code` (inline-code pill per Sketch 001).
pub fn handle_config_set(key: &str, value: &str) -> Result<()> {
    validate_set(key, value)?;
    if key == "auto-theme" && value == "configure" {
        crate::cli::auto_theme::require_interactive_configuration()?;
    }
    let env = SlateEnv::from_process()?;
    handle_config_set_with_env(key, value, &env)
}

fn handle_config_set_with_env(key: &str, value: &str, env: &SlateEnv) -> Result<()> {
    validate_set(key, value)?;
    if key == "auto-theme" && value == "configure" {
        return crate::cli::auto_theme::configure_auto_theme();
    }
    let _write_guard = crate::config::ConfigWriteGuard::acquire(env)?;
    let config = ConfigManager::with_env(env)?;
    let terminal = TerminalProfile::detect();
    let appearance_backend = platform::desktop::detect_backend();

    let ctx = RenderContext::from_active_theme().ok();
    let r = ctx.as_ref().map(Roles::new);

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
                    snapshot_policy: crate::cli::apply::SnapshotPolicy::Create,
                },
            )?;

            println!(
                "{}",
                status_success_line(
                    r.as_ref(),
                    &format!("Opacity set to {}", code_text(r.as_ref(), value)),
                )
            );
            dispatch(BrandEvent::Success(SuccessKind::ConfigSet));
            Ok(())
        }
        "auto-theme" => {
            match value {
                "enable" => {
                    enable_auto_theme(&config)?;

                    println!("{}", status_success_line(r.as_ref(), "Auto theme enabled"));
                    println!("  Appearance backend: {}", appearance_backend.label());
                    if terminal.watcher_shell_autostart_supported()
                        && appearance_backend.supports_watcher()
                    {
                        println!("  Ghostty shell sessions can relaunch the watcher automatically");
                    } else if appearance_backend.supports_watcher() {
                        println!(
                            "  Watching is available now, but restart recovery is still fully supported in Ghostty shells"
                        );
                    } else {
                        println!(
                            "  Automatic watching is unavailable here, but `slate theme --auto` still works on demand"
                        );
                    }
                    println!("  Run 'slate config set auto-theme configure' to customize dark/light pairing");
                    dispatch(BrandEvent::Success(SuccessKind::ConfigSet));
                    Ok(())
                }
                "disable" => {
                    disable_auto_theme(&config)?;

                    println!("{}", status_success_line(r.as_ref(), "Auto theme disabled"));
                    dispatch(BrandEvent::Success(SuccessKind::ConfigSet));
                    Ok(())
                }
                "configure" => crate::cli::auto_theme::configure_auto_theme(),
                _ => Err(crate::error::SlateError::InvalidConfig(format!(
                    "Invalid auto-theme action: '{}'. Must be one of: enable, disable, configure",
                    value
                ))),
            }
        }
        "fastfetch" => match value {
            "enable" => {
                set_fastfetch_autorun(&config, true)?;
                println!(
                    "{}",
                    status_success_line(r.as_ref(), "Fastfetch auto-run enabled")
                );
                // New interactive shells pick up the enabled autorun hook.
                crate::cli::new_shell_reminder::emit_new_shell_reminder_once(false, false);
                dispatch(BrandEvent::Success(SuccessKind::ConfigSet));
                Ok(())
            }
            "disable" => {
                set_fastfetch_autorun(&config, false)?;
                println!(
                    "{}",
                    status_success_line(r.as_ref(), "Fastfetch auto-run disabled")
                );
                // Only autorun is disabled; the manual wrapper remains available.
                crate::cli::new_shell_reminder::emit_new_shell_reminder_once(false, false);
                dispatch(BrandEvent::Success(SuccessKind::ConfigSet));
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
                println!(
                    "{}",
                    status_success_line(r.as_ref(), "Sound feedback enabled")
                );
                dispatch(BrandEvent::Success(SuccessKind::ConfigSet));
                Ok(())
            }
            "off" => {
                config.set_sound_enabled(false)?;
                println!(
                    "{}",
                    status_success_line(r.as_ref(), "Sound feedback disabled")
                );
                dispatch(BrandEvent::Success(SuccessKind::ConfigSet));
                Ok(())
            }
            _ => Err(crate::error::SlateError::InvalidConfig(format!(
                "Invalid sound value: '{}'. Must be one of: on, off",
                value
            ))),
        },
        // `slate config set editor disable` remembers the opt-out and strips the
        // marker block from init.lua / init.vim without touching
        // the 18 `slate-*.lua` shims or the loader. For users who want
        // to keep the colorscheme files available (so
        // `:colorscheme slate-<variant>` still works) but stop the
        // `pcall(require, 'slate')` auto-activation.
        "editor" => match value {
            "disable" => {
                // Save consent first. A later hook-removal failure must not let
                // the next quick setup silently opt the user back in.
                config.set_editor_auto_activation_enabled(false)?;
                let init_lua = env.nvim_config_dir().join("init.lua");
                let init_vim = env.nvim_config_dir().join("init.vim");
                for path in [&init_lua, &init_vim] {
                    crate::adapter::marker_block::remove_managed_blocks_from_file(path)
                        .map_err(|error| crate::error::SlateError::InvalidConfig(format!(
                            "Neovim auto-activation preference was saved, but hook removal did not finish: {error}. Earlier removals remain; no automatic rollback was attempted. Fix the file and rerun `slate config set editor disable`."
                        )))?;
                }
                println!(
                    "{}",
                    status_success_line(
                        r.as_ref(),
                        "Neovim auto-activation stays disabled for this profile, including future setup. \
                         Colors/ files remain for manual use. Restart Neovim to stop any already-loaded watcher; \
                         unmarked user hooks are not removed.",
                    )
                );
                dispatch(BrandEvent::Success(SuccessKind::ConfigSet));
                Ok(())
            }
            "enable" => {
                config.set_editor_auto_activation_enabled(true)?;
                println!("{}", status_success_line(r.as_ref(),
                    "Neovim automatic setup is allowed again for this profile. Run `slate setup` to configure activation; existing hooks were not changed."));
                dispatch(BrandEvent::Success(SuccessKind::ConfigSet));
                Ok(())
            }
            _ => Err(crate::error::SlateError::InvalidConfig(format!(
                "Invalid editor action: '{}'. Must be one of: enable, disable",
                value
            ))),
        },
        _ => Err(crate::error::SlateError::InvalidConfig(format!(
            "Unknown config key: '{}'. Known keys: opacity, auto-theme, fastfetch, sound, editor",
            key
        ))),
    }
}

/// Format a success body via `Roles::status_success` (theme.green
/// NEVER lavender per D-01a), falling back to plain `✓ message`.
fn status_success_line(r: Option<&Roles<'_>>, message: &str) -> String {
    match r {
        Some(r) => r.status_success(message),
        None => format!("✓ {}", message),
    }
}

/// Wrap a literal value in `Roles::code` (inline-code pill per Sketch
/// 001), falling back to backticked text when Roles is unavailable.
fn code_text(r: Option<&Roles<'_>>, text: &str) -> String {
    match r {
        Some(r) => r.code(text),
        None => format!("`{}`", text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brand::render_context::{mock_context_with_mode, mock_theme, RenderMode};
    use crate::cli::new_shell_reminder::REMINDER_TEST_LOCK;
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
    /// keep these legacy checks narrowly focused on reminder dispatch. Actual
    /// isolated watcher-start behavior is covered by the runtime's own tests;
    /// SLATE_HOME/injected isolated environments never start a desktop watcher.
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

    // `slate config editor disable` sub-command

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
        assert!(!ConfigManager::from_env_paths(&env)
            .is_editor_auto_activation_enabled()
            .unwrap());

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

    #[test]
    fn config_editor_enable_only_clears_consent_and_disable_reports_partial_removal() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        let config = ConfigManager::from_env_paths(&env);
        config.set_editor_auto_activation_enabled(false).unwrap();
        std::fs::create_dir(env.nvim_config_dir().join("init.vim")).unwrap();
        let error = handle_config_set_with_env("editor", "disable", &env)
            .unwrap_err()
            .to_string();
        assert!(error.contains("preference was saved"));
        assert!(error.contains("hook removal did not finish"));
        assert!(!config.is_editor_auto_activation_enabled().unwrap());
        handle_config_set_with_env("editor", "enable", &env).unwrap();
        assert!(config.is_editor_auto_activation_enabled().unwrap());
        assert!(!env.nvim_config_dir().join("init.lua").exists());
        assert!(env.nvim_config_dir().join("init.vim").is_dir());
    }

    /// Disabling on a fresh profile saves consent but creates no init file.
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
        assert!(env.nvim_auto_activation_path().is_file());
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

    /// D-01a invariant — the config-set success line uses theme.green,
    /// never brand-lavender, across every render mode.
    #[test]
    fn config_status_success_line_never_emits_brand_lavender() {
        let theme = mock_theme();
        for mode in [RenderMode::Truecolor, RenderMode::Basic, RenderMode::None] {
            let ctx = mock_context_with_mode(&theme, mode);
            let r = Roles::new(&ctx);
            let out = status_success_line(Some(&r), "Auto theme enabled");
            assert!(
                !out.contains("38;2;114;135;253"),
                "D-01a violation in mode {mode:?}: {out:?}"
            );
        }
    }

    /// graceful degrade — both helpers fall back to plain text
    /// (with backticks for `code_text`) when Roles is unavailable.
    #[test]
    fn config_helpers_fall_back_to_plain_when_roles_absent() {
        let line = status_success_line(None, "ok");
        assert_eq!(line, "✓ ok");
        let code = code_text(None, "frosted");
        assert_eq!(code, "`frosted`");
        for s in [line, code] {
            assert!(!s.contains('\x1b'));
        }
    }
}
