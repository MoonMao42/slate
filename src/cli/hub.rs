use crate::cli::config::{disable_auto_theme, enable_auto_theme};
use crate::config::ConfigManager;
use crate::error::Result;
use crate::theme::ThemeRegistry;

/// Handle bare `slate` invocation — always show hub, never hijack with setup
pub fn handle() -> Result<()> {
    let config = ConfigManager::new()?;
    show_hub_once(&config)
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

    if was_enabled {
        config.disable_fastfetch_autorun()?;
    } else {
        config.enable_fastfetch_autorun()?;
    }

    if let Err(err) = config.refresh_shell_integration() {
        if was_enabled {
            let _ = config.enable_fastfetch_autorun();
        } else {
            let _ = config.disable_fastfetch_autorun();
        }
        return Err(err);
    }

    Ok(())
}

fn toggle_starship_from_preferences(config: &ConfigManager) -> Result<()> {
    let was_enabled = config.is_starship_enabled()?;
    config.set_starship_enabled(!was_enabled)?;

    if let Err(err) = config.refresh_shell_integration() {
        let _ = config.set_starship_enabled(was_enabled);
        let _ = config.refresh_shell_integration();
        return Err(err);
    }

    Ok(())
}

fn toggle_zsh_highlighting_from_preferences(config: &ConfigManager) -> Result<()> {
    let was_enabled = config.is_zsh_highlighting_enabled()?;
    config.set_zsh_highlighting_enabled(!was_enabled)?;

    if let Err(err) = config.refresh_shell_integration() {
        let _ = config.set_zsh_highlighting_enabled(was_enabled);
        let _ = config.refresh_shell_integration();
        return Err(err);
    }

    Ok(())
}

/// Single-entry guided flow - show state, present one action menu, execute, exit
fn show_hub_once(config: &ConfigManager) -> Result<()> {
    cliclack::intro("✦ slate")?;

    // Render dashboard once using cliclack info to preserve left border
    let registry = ThemeRegistry::new()?;
    let env = crate::env::SlateEnv::from_process()?;

    let current_theme = config
        .get_current_theme()?
        .and_then(|id| registry.get(&id).cloned())
        .unwrap_or_else(|| {
            registry
                .get("catppuccin-mocha")
                .cloned()
                .unwrap_or_else(|| {
                    // Fallback: get first theme from registry
                    registry.all().first().map(|t| (*t).clone()).unwrap()
                })
        });

    let current_font = config.get_current_font()?;
    let current_opacity = config
        .get_current_opacity()?
        .unwrap_or_else(|| "Solid".to_string());

    let is_first_run = config.get_current_theme()?.is_none();

    if is_first_run {
        cliclack::log::warning("No theme set yet. Run Setup to get started.")?;
    } else {
        cliclack::log::info(format!(
            "\x1b[1m{}\x1b[0m    {}",
            "Theme", current_theme.name
        ))?;
        cliclack::log::info(format!("\x1b[1m{}\x1b[0m  {}", "Opacity", current_opacity))?;
        cliclack::log::info(format!(
            "\x1b[1m{}\x1b[0m    {}",
            "Font",
            current_font.unwrap_or_else(|| "Not configured".to_string())
        ))?;
    }

    cliclack::log::remark("")?;

    let auto_enabled = config.is_auto_theme_enabled()?;
    let mut menu_builder = cliclack::select("What would you like to do?");

    if is_first_run {
        menu_builder = menu_builder.item("setup", "Run Setup", "get started in 30 seconds");
    }

    menu_builder = menu_builder.item("switch", "Switch Theme", "");
    menu_builder = menu_builder.item("font", "Change Font", "");

    menu_builder = menu_builder.item(
        "auto-theme",
        if auto_enabled {
            "Auto-Theme: On"
        } else {
            "Auto-Theme: Off"
        },
        "toggle, configure pairing",
    );

    menu_builder = menu_builder.item(
        "tools",
        "Configure Tools",
        "starship, highlighting, fastfetch",
    );

    menu_builder = menu_builder.item("restore", "Revert Changes", "restore from snapshot");

    if !is_first_run {
        menu_builder = menu_builder.item("setup", "Run Setup", "");
    }

    menu_builder = menu_builder.item("quit", "Quit", "");

    // Render and handle selection (execute one action and exit)
    let selection = menu_builder.interact().map_err(|e| {
        if e.kind() == std::io::ErrorKind::Interrupted {
            crate::error::SlateError::UserCancelled
        } else {
            crate::error::SlateError::IOError(e)
        }
    })?;

    match selection {
        "switch" => crate::cli::picker::launch_picker(&env),
        "font" => crate::cli::font::handle_font(None),
        "auto-theme" => {
            handle_auto_theme(config, auto_enabled)?;
            cliclack::outro("✦ Done")?;
            Ok(())
        }
        "tools" => {
            handle_tool_toggles(config)?;
            cliclack::outro("✦ Done")?;
            Ok(())
        }
        "restore" => crate::cli::restore::handle(None, false, None),
        "setup" => {
            crate::cli::setup::handle_with_env(false, false, None, &env)?;
            cliclack::outro("✦ Done")?;
            Ok(())
        }
        "quit" => {
            cliclack::outro("✦ Done")?;
            Ok(())
        }
        _ => {
            cliclack::outro("✦ Done")?;
            Ok(())
        }
    }
}

fn handle_auto_theme(config: &ConfigManager, currently_enabled: bool) -> Result<()> {
    let selection = cliclack::select("Auto-Theme")
        .item(
            "toggle",
            if currently_enabled {
                "Turn off"
            } else {
                "Turn on"
            },
            "",
        )
        .item("configure", "Configure Pairing", "choose dark/light themes")
        .item("back", "Back", "")
        .interact()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::Interrupted {
                crate::error::SlateError::UserCancelled
            } else {
                crate::error::SlateError::IOError(e)
            }
        })?;

    match selection {
        "toggle" => {
            let new_state = !currently_enabled;
            sync_auto_theme_toggle(config, new_state)?;
        }
        "configure" => {
            crate::cli::auto_theme::configure_auto_theme()?;
            if config.is_auto_theme_enabled()? {
                config.refresh_shell_integration()?;
                let _ = crate::platform::dark_mode_notify::stop();
                let _ = crate::platform::dark_mode_notify::start(config);
            }
        }
        _ => {}
    }

    Ok(())
}

fn handle_tool_toggles(config: &ConfigManager) -> Result<()> {
    loop {
        let starship_enabled = config.is_starship_enabled()?;
        let zsh_highlighting_enabled = config.is_zsh_highlighting_enabled()?;
        let fastfetch_enabled = config.has_fastfetch_autorun()?;

        let selection = cliclack::select("Configure Tools")
            .item(
                "starship",
                if starship_enabled {
                    "Starship: On"
                } else {
                    "Starship: Off"
                },
                "",
            )
            .item(
                "zsh-highlighting",
                if zsh_highlighting_enabled {
                    "Syntax Highlighting: On"
                } else {
                    "Syntax Highlighting: Off"
                },
                "",
            )
            .item(
                "fastfetch",
                if fastfetch_enabled {
                    "Fastfetch: On"
                } else {
                    "Fastfetch: Off"
                },
                "",
            )
            .item("back", "Back", "")
            .interact()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::Interrupted {
                    crate::error::SlateError::UserCancelled
                } else {
                    crate::error::SlateError::IOError(e)
                }
            })?;

        match selection {
            "starship" => {
                toggle_starship_from_preferences(config)?;
                cliclack::log::warning("Open a new terminal tab for changes to take effect.")?;
            }
            "zsh-highlighting" => {
                toggle_zsh_highlighting_from_preferences(config)?;
                cliclack::log::warning("Open a new terminal tab for changes to take effect.")?;
            }
            "fastfetch" => {
                toggle_fastfetch_from_preferences(config)?;
                cliclack::log::warning("Open a new terminal tab for changes to take effect.")?;
            }
            "back" => return Ok(()),
            _ => return Ok(()),
        }
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
        assert!(enabled_content.contains("if command -v fastfetch &> /dev/null; then"));
        assert!(enabled_content.contains("  fastfetch\n"));

        toggle_fastfetch_from_preferences(&config).unwrap();

        let disabled_content = fs::read_to_string(&shell_path).unwrap();
        assert!(!config.has_fastfetch_autorun().unwrap());
        assert!(!disabled_content.contains("if command -v fastfetch &> /dev/null; then"));
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
}
