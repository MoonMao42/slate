use crate::brand::Language;
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

fn show_hub_menu(config: &ConfigManager) -> Result<()> {
    // Render dashboard header via cliclack
    cliclack::intro("✦ slate")?;

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
    cliclack::log::info(format!(
        "[1m{}[0m    {}",
        "Theme", current_theme.name
    ))?;
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
            crate::cli::picker::launch_picker(&env)
        }
        "font" => {
            // Delegate to font picker 
            crate::cli::font::handle_font(None)
        }
        "toggle-auto" => {
            // Toggle auto theme and re-render menu (loop)
            let new_state = !auto_enabled;
            config.set_auto_theme_enabled(new_state)?;
            show_hub_menu(config)
        }
        "status" => {
            // View status
            crate::cli::status::handle(&[])
        }
        "resume-auto" => {
            // Resume auto theme (apply the should-be theme)
            if let HubState::B(ref destination) = hub_state {
                crate::cli::theme::handle_theme(Some(destination.clone()), false)
            } else {
                Ok(())
            }
        }
        "prefs" => handle_preferences(),
        "quit" => {
            cliclack::outro("✦ Done")?;
            Ok(())
        }
        _ => Ok(()),
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


fn handle_preferences() -> Result<()> {
    // Preferences submenu reduced to 2 items + Back (no Reset, no auto, no font)
    // Font and auto are now top-level in the main menu 
    let config = ConfigManager::new()?;
    
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
            // Toggle fastfetch autorun marker file
            if config.has_fastfetch_autorun()? {
                config.disable_fastfetch_autorun()?;
            } else {
                config.enable_fastfetch_autorun()?;
            }
            // Re-render preferences menu with updated toggle state
            handle_preferences()
        }
        "setup" => {
            let env = crate::env::SlateEnv::from_process()?;
            crate::cli::setup::handle_with_env(false, false, None, &env)
        }
        "back" => {
            // Return to hub menu
            show_hub_menu(&config)
        }
        _ => Ok(()),
    }
}

