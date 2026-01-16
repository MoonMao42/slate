use crate::brand::Language;
use crate::cli::config::{disable_auto_theme, enable_auto_theme};
use crate::config::ConfigManager;
use crate::error::Result;
use crate::theme::ThemeRegistry;

/// Handle bare `slate` invocation 
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
        // Return to hub after setup
    }

    show_hub_menu(&config)
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

fn show_hub_menu(config: &ConfigManager) -> Result<()> {
    cliclack::intro("✦ slate")?;

    loop {
        // Render dashboard using cliclack info to preserve left border
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
        cliclack::log::info(format!("[1m{}[0m    {}", "Theme", current_theme.name))?;
        cliclack::log::info(format!("[1m{}[0m  {}", "Opacity", current_opacity))?;
        cliclack::log::info(format!(
            "[1m{}[0m    {}",
            "Font",
            current_font.unwrap_or_else(|| "Not configured".to_string())
        ))?;

        cliclack::log::remark("")?;

        // Compute Hub state machine (A/B/C)
        let auto_enabled = config.is_auto_theme_enabled()?;
        let current_theme_id = config.get_current_theme()?;

        let hub_state = if auto_enabled {
            // Auto is enabled; check if current matches auto-resolved
            let should_be_theme = crate::cli::auto_theme::resolve_auto_theme(&env, config)?;
            if let Some(ref current_id) = current_theme_id {
                if current_id == &should_be_theme {
                    HubState::A
                } else {
                    HubState::B(should_be_theme)
                }
            } else {
                HubState::B(should_be_theme)
            }
        } else {
            HubState::C
        };

        // Build menu items (6 base + 1 conditional)
        let mut menu_builder = cliclack::select("What would you like to do?");

        // Item 1: Theme action (varies by state)
        let theme_label = match &hub_state {
            HubState::A => Language::HUB_PAUSE_AUTO_PICK,
            HubState::B(_) | HubState::C => Language::HUB_SWITCH_THEME,
        };
        menu_builder = menu_builder.item("switch", theme_label, "");

        // Item 2: Change Font
        menu_builder = menu_builder.item("font", Language::HUB_CHANGE_FONT, "");

        // Item 3: Toggle Auto Theme
        let auto_toggle_label = if auto_enabled {
            Language::HUB_TOGGLE_AUTO_ON
        } else {
            Language::HUB_TOGGLE_AUTO_OFF
        };
        menu_builder = menu_builder.item("toggle-auto", auto_toggle_label, "");

        // Item 4: View Status
        menu_builder = menu_builder.item("status", Language::HUB_VIEW_STATUS, "");

        // Conditional Item: Resume Auto (if State B)
        if let HubState::B(ref destination) = hub_state {
            let registry = ThemeRegistry::new()?;
            let dest_display = registry
                .get(destination)
                .map(|t| t.name.clone())
                .unwrap_or_else(|| destination.clone());
            menu_builder = menu_builder.item(
                "resume-auto",
                format!("⟲ Resume Auto ({})", dest_display),
                "",
            );
        }

        // Item 5: Preferences
        menu_builder = menu_builder.item("prefs", Language::HUB_PREFERENCES, "");

        // Item 6: Quit
        menu_builder = menu_builder.item("quit", Language::HUB_QUIT, "");

        // Render and handle selection
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
            "toggle-auto" => {
                let new_state = !auto_enabled;
                sync_auto_theme_toggle(config, new_state)?;
            }
            "status" => {
                // View status
                return crate::cli::status::handle(&[]);
            }
            "resume-auto" => {
                // Resume auto theme (apply the should-be theme)
                if let HubState::B(ref destination) = hub_state {
                    return crate::cli::theme::handle_theme(
                        Some(destination.clone()),
                        false,
                        false,
                    );
                }
            }
            "prefs" => match handle_preferences(config)? {
                PreferencesOutcome::BackToHub => {}
                PreferencesOutcome::ExitHub => return Ok(()),
            },
            "quit" => {
                cliclack::outro("✦ Done")?;
                return Ok(());
            }
            _ => return Ok(()),
        }
    }
}

/// Hub state machine : tracks auto-theme enabled state and theme sync status
#[derive(Debug)]
enum HubState {
    /// A: Auto-on and current == auto-resolved (synced)
    A,
    /// B: Auto-on and current != auto-resolved (out of sync); contains should-be theme ID
    B(String),
    /// C: Auto-off
    C,
}

enum PreferencesOutcome {
    BackToHub,
    ExitHub,
}

fn handle_preferences(config: &ConfigManager) -> Result<PreferencesOutcome> {
    // Preferences submenu reduced to 2 items + Back (no Reset, no auto, no font)
    // Font and auto are now top-level in the main menu 
    loop {
        let selection = cliclack::select("Preferences")
            .item(
                "fastfetch",
                if config.has_fastfetch_autorun()? {
                    Language::HUB_TOGGLE_FASTFETCH_ON
                } else {
                    Language::HUB_TOGGLE_FASTFETCH_OFF
                },
                "",
            )
            .item("setup", Language::HUB_RUN_SETUP, "")
            .item("back", Language::HUB_BACK, "")
            .interact()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::Interrupted {
                    crate::error::SlateError::UserCancelled
                } else {
                    crate::error::SlateError::IOError(e)
                }
            })?;

        match selection {
            "fastfetch" => {
                toggle_fastfetch_from_preferences(config)?;
            }
            "setup" => {
                let env = crate::env::SlateEnv::from_process()?;
                crate::cli::setup::handle_with_env(false, false, None, &env)?;
                return Ok(PreferencesOutcome::ExitHub);
            }
            "back" => return Ok(PreferencesOutcome::BackToHub),
            _ => return Ok(PreferencesOutcome::ExitHub),
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
