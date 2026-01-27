use crate::cli::config::{disable_auto_theme, enable_auto_theme};
use crate::config::ConfigManager;
use crate::error::Result;
use crate::theme::ThemeRegistry;

/// Handle bare `slate` invocation (single-entry guided flow)
pub fn handle() -> Result<()> {
    let config = ConfigManager::new()?;

    // First-time setup detection
    if !has_current_theme(&config)? {
        cliclack::intro("✦ Welcome to slate. Let's set it up.")?;
        crate::cli::setup::handle_with_env(
            false,
            false,
            None,
            &crate::env::SlateEnv::from_process()?,
        )?;
        // After setup, show hub and exit
    }

    show_hub_once(&config)
}

fn has_current_theme(config: &ConfigManager) -> Result<bool> {
    Ok(config.get_current_theme()?.is_some())
}

fn sync_auto_theme_toggle(config: &ConfigManager, enabled: bool) -> Result<()> {
    sync_auto_theme_toggle_with(
        config,
        enabled,
        enable_auto_theme,
        disable_auto_theme,
    )
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

    // Dashboard rendering with color hierarchy
    cliclack::log::info(format!("[1m{}[0m    {}", "Theme", current_theme.name))?;
    cliclack::log::info(format!("[1m{}[0m  {}", "Opacity", current_opacity))?;
    cliclack::log::info(format!(
        "[1m{}[0m    {}",
        "Font",
        current_font.unwrap_or_else(|| "Not configured".to_string())
    ))?;

    cliclack::log::remark("")?;

    // /Build single menu with high-frequency actions
    let mut menu_builder = cliclack::select("What would you like to do?");

    // Item 1: Switch Theme
    menu_builder = menu_builder.item("switch", "✦ Switch Theme", "");

    // Item 2: Change Font
    menu_builder = menu_builder.item("font", "✦ Change Font", "");

    // Item 3: More options (includes auto-theme toggle, restore, setup wizard)
    menu_builder = menu_builder.item("more", "◆ More options…", "");

    // Item 4: Quit
    menu_builder = menu_builder.item("quit", "○ Quit", "");

    // Render and handle selection (execute one action and exit)
    let selection = menu_builder.interact().map_err(|e| {
        if e.kind() == std::io::ErrorKind::Interrupted {
            crate::error::SlateError::UserCancelled
        } else {
            crate::error::SlateError::IOError(e)
        }
    })?;

    match selection {
        "switch" => {
            // Route to picker
            return crate::cli::picker::launch_picker(&env);
        }
        "font" => {
            // Delegate to font picker
            return crate::cli::font::handle_font(None);
        }
        "more" => {
            // More options submenu
            return handle_more_options(config);
        }
        "quit" => {
            cliclack::outro("✦ Done")?;
            return Ok(());
        }
        _ => {
            cliclack::outro("✦ Done")?;
            return Ok(());
        }
    }
}

/// More options submenu (looping menu, back to main hub not supported)
fn handle_more_options(config: &ConfigManager) -> Result<()> {
    loop {
        let auto_enabled = config.is_auto_theme_enabled()?;

        let selection = cliclack::select("More options")
            .item(
                "auto-theme",
                if auto_enabled {
                    "✦ Auto-Theme (enabled)"
                } else {
                    "✦ Auto-Theme (disabled)"
                },
                "",
            )
            .item("restore", "⏏ Restore from snapshot", "")
            .item("setup", "⚙ Setup Wizard", "")
            .item("quit", "○ Back", "")
            .interact()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::Interrupted {
                    crate::error::SlateError::UserCancelled
                } else {
                    crate::error::SlateError::IOError(e)
                }
            })?;

        match selection {
            "auto-theme" => {
                let new_state = !auto_enabled;
                sync_auto_theme_toggle(config, new_state)?;
                // After toggling, show updated state and menu again
            }
            "restore" => {
                // Delegate to restore command
                return crate::cli::restore::handle(&[]);
            }
            "setup" => {
                let env = crate::env::SlateEnv::from_process()?;
                crate::cli::setup::handle_with_env(false, false, None, &env)?;
                // After setup, exit
                cliclack::outro("✦ Done")?;
                return Ok(());
            }
            "quit" | _ => {
                cliclack::outro("✦ Done")?;
                return Ok(());
            }
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
}
