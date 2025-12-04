use crate::config::ConfigManager;
use crate::error::Result;
use crate::theme::ThemeRegistry;

/// Handle bare `slate` invocation 
pub fn handle() -> Result<()> {
    let config = ConfigManager::new()?;

    // First-time setup detection
    if !has_current_theme(&config)? {
        cliclack::intro("✦ Welcome to slate. Let's set it up.")?;
        crate::cli::setup::handle_with_env(false, false, None, &crate::env::SlateEnv::from_process()?)?;
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

    let current_theme = config.get_current_theme()?
        .and_then(|id| registry.get(&id).cloned())
        .unwrap_or_else(|| {
            registry.get("catppuccin-mocha")
                .cloned()
                .unwrap_or_else(|| {
                    // Fallback: get first theme from registry
                    registry.all().first().map(|t| (*t).clone()).unwrap()
                })
        });

    let current_font = config.get_current_font()?;
    let current_opacity = config.get_current_opacity()?
        .unwrap_or_else(|| "Solid".to_string());

    // Dashboard rendering with color hierarchy
    cliclack::log::info(format!(
        "\x1b[1m{}\x1b[0m    {}",
        "Theme",
        current_theme.name
    ))?;
    cliclack::log::info(format!(
        "\x1b[1m{}\x1b[0m  {}",
        "Opacity",
        current_opacity
    ))?;
    cliclack::log::info(format!(
        "\x1b[1m{}\x1b[0m    {}",
        "Font",
        current_font.unwrap_or_else(|| "Not configured".to_string())
    ))?;

    cliclack::log::remark("")?;

    // Present menu using correct cliclack API
    let selection = cliclack::select("What would you like to do?")
        .item("switch", "✦ Switch Theme", "")
        .item("status", "◆ View Status", "")
        .item("prefs", "⚙ Preferences…", "")
        .item("quit", "⏊ Quit", "")
        .interact()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::Interrupted {
                crate::error::SlateError::UserCancelled
            } else {
                crate::error::SlateError::IOError(e)
            }
        })?;

    match selection {
        "switch" => {
            // 06-04 wires the picker path.
            // For now, delegate to set command with no args (which will be picker in 06-04)
            crate::cli::set::handle(&[])
        }
        "status" => {
            // 06-02 wires the status path.
            crate::cli::status::handle(&[])
        }
        "prefs" => handle_preferences(),
        "quit" => {
            cliclack::outro("✦ Done")?;
            Ok(())
        }
        _ => Ok(()),
    }
}

fn handle_preferences() -> Result<()> {
    // Preferences submenu
    let selection = cliclack::select("Preferences")
        .item("font", "Change Font", "")
        .item("auto", "Configure Auto Theme", "")
        .item("setup", "Run Setup Wizard", "")
        .item("reset", "Reset", "")
        .item("back", "← Back", "")
        .interact()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::Interrupted {
                crate::error::SlateError::UserCancelled
            } else {
                crate::error::SlateError::IOError(e)
            }
        })?;

    match selection {
        "font" => {
            // 06-06 wires Change Font.
            cliclack::log::info("Font selection coming in .")?;
            Ok(())
        }
        "auto" => {
            // 06-05 owns the actual auto-theme implementation.
            cliclack::log::info("Configure Auto Theme is wired in 06-05.")?;
            Ok(())
        }
        "setup" => {
            let env = crate::env::SlateEnv::from_process()?;
            crate::cli::setup::handle_with_env(false, false, None, &env)
        }
        "reset" => crate::cli::restore::handle(&[]),
        "back" => {
            // Recursively show hub menu
            let config = ConfigManager::new()?;
            show_hub_menu(&config)
        }
        _ => Ok(()),
    }
}
