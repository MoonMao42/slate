use crate::config::ConfigManager;
use crate::design::symbols::Symbols;
use crate::env::SlateEnv;
use crate::error::Result;
use crate::opacity::OpacityPreset;
use crate::platform;

/// Handle `slate config set <key> <value>` command
pub fn handle_config_set(key: &str, value: &str) -> Result<()> {
    let env = SlateEnv::from_process()?;
    let config = ConfigManager::with_env(&env)?;

    match key {
        "opacity" => {
            // value ∈ {solid, frosted, clear}
            let preset = match value {
                "solid" => OpacityPreset::Solid,
                "frosted" => OpacityPreset::Frosted,
                "clear" => OpacityPreset::Clear,
                _ => {
                    return Err(crate::error::SlateError::InvalidConfig(
                        format!("Invalid opacity preset: '{}'. Must be one of: solid, frosted, clear", value)
                    ))
                }
            };

            // Write to ~/.config/slate/current-opacity
            config.set_current_opacity_preset(preset)?;

            println!("{} Opacity set to '{}'", Symbols::SUCCESS, value);
            Ok(())
        }
        "auto-theme" => {
            match value {
                "enable" => {
                    // Install launchd agent
                    platform::launchd::install_agent()?;

                    // Write [auto_theme].enabled = true
                    config.set_auto_theme_enabled(true)?;

                    println!("{} Auto theme enabled", Symbols::SUCCESS);
                    println!("  macOS appearance changes will automatically switch your terminal theme");
                    println!("  Run 'slate config set auto-theme configure' to customize dark/light pairing");
                    Ok(())
                }
                "disable" => {
                    // Unload launchd agent
                    platform::launchd::uninstall_agent()?;

                    // Write [auto_theme].enabled = false
                    config.set_auto_theme_enabled(false)?;

                    println!("{} Auto theme disabled", Symbols::SUCCESS);
                    Ok(())
                }
                "configure" => {
                    // Launch Configure Auto Theme two-step cliclack flow (reuse from)
                    crate::cli::auto_theme::configure_auto_theme()?;

                    // If auto-theme is now enabled, make sure agent is installed
                    if config.is_auto_theme_enabled()? {
                        platform::launchd::install_agent()?;
                    }

                    Ok(())
                }
                _ => {
                    Err(crate::error::SlateError::InvalidConfig(
                        format!("Invalid auto-theme action: '{}'. Must be one of: enable, disable, configure", value)
                    ))
                }
            }
        }
        _ => {
            Err(crate::error::SlateError::InvalidConfig(
                format!("Unknown config key: '{}'. Known keys: opacity, auto-theme", key)
            ))
        }
    }
}
