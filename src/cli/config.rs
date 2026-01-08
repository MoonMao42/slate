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
                    return Err(crate::error::SlateError::InvalidConfig(format!(
                        "Invalid opacity preset: '{}'. Must be one of: solid, frosted, clear",
                        value
                    )))
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
                    // Compile the Swift dark mode notifier binary
                    platform::dark_mode_notify::ensure_binary(&config)?;

                    config.set_auto_theme_enabled(true)?;
                    config.refresh_shell_integration()?;

                    println!("{} Auto theme enabled", Symbols::SUCCESS);
                    println!(
                        "  New shell sessions will auto-switch theme on macOS appearance change"
                    );
                    println!("  Run 'slate config set auto-theme configure' to customize dark/light pairing");
                    Ok(())
                }
                "disable" => {
                    config.set_auto_theme_enabled(false)?;
                    config.refresh_shell_integration()?;
                    platform::dark_mode_notify::remove_binary(&config)?;

                    println!("{} Auto theme disabled", Symbols::SUCCESS);
                    Ok(())
                }
                "configure" => {
                    // Launch Configure Auto Theme two-step cliclack flow (reuse from)
                    crate::cli::auto_theme::configure_auto_theme()?;

                    // If auto-theme is now enabled, regenerate shell integration
                    if config.is_auto_theme_enabled()? {
                        config.refresh_shell_integration()?;
                    }

                    Ok(())
                }
                _ => Err(crate::error::SlateError::InvalidConfig(format!(
                    "Invalid auto-theme action: '{}'. Must be one of: enable, disable, configure",
                    value
                ))),
            }
        }
        "fastfetch" => {
            match value {
                "enable" => {
                    config.enable_fastfetch_autorun()?;
                    config.refresh_shell_integration()?;
                    println!("{} Fastfetch auto-run enabled", Symbols::SUCCESS);
                    Ok(())
                }
                "disable" => {
                    config.disable_fastfetch_autorun()?;
                    config.refresh_shell_integration()?;
                    println!("{} Fastfetch auto-run disabled", Symbols::SUCCESS);
                    Ok(())
                }
                _ => Err(crate::error::SlateError::InvalidConfig(format!(
                    "Invalid fastfetch action: '{}'. Must be one of: enable, disable",
                    value
                ))),
            }
        }
        _ => Err(crate::error::SlateError::InvalidConfig(format!(
            "Unknown config key: '{}'. Known keys: opacity, auto-theme, fastfetch",
            key
        ))),
    }
}
