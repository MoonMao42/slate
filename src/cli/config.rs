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
    Ok(())
}

/// Handle `slate config set <key> <value>` command
pub fn handle_config_set(key: &str, value: &str) -> Result<()> {
    let env = SlateEnv::from_process()?;
    let config = ConfigManager::with_env(&env)?;
    let terminal = TerminalProfile::detect();

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
                    enable_auto_theme(&config)?;

                    println!("{} Auto theme enabled", Symbols::SUCCESS);
                    if terminal.watcher_shell_autostart_supported() {
                        println!("  Ghostty shell sessions can relaunch the watcher automatically");
                    } else {
                        println!(
                            "  Theme switching works now, but automatic relaunch after a restart is Ghostty-only"
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
        _ => Err(crate::error::SlateError::InvalidConfig(format!(
            "Unknown config key: '{}'. Known keys: opacity, auto-theme, fastfetch, sound",
            key
        ))),
    }
}
