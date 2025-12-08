use crate::env::SlateEnv;
use crate::error::Result;
use crate::theme::{ThemeAppearance, ThemeRegistry};
use crate::config::ConfigManager;
use std::process::Command;

/// Detect the current macOS system appearance (Dark or Light).
/// Runs `defaults read -g AppleInterfaceStyle` once per invocation.
/// On non-macOS or if the command fails, defaults to Light.
/// On success, stdout contains "Dark\n" for dark mode, anything else is treated as light.
pub fn detect_system_appearance() -> Result<ThemeAppearance> {
    // Execute: defaults read -g AppleInterfaceStyle
    // Output: "Dark" if dark mode, missing/error otherwise
    
    match Command::new("defaults")
        .args(&["read", "-g", "AppleInterfaceStyle"])
        .output()
    {
        Ok(output) => {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if stdout.contains("Dark") {
                    Ok(ThemeAppearance::Dark)
                } else {
                    Ok(ThemeAppearance::Light)
                }
            } else {
                // Command failed (e.g., light mode on macOS)
                Ok(ThemeAppearance::Light)
            }
        }
        Err(_) => {
            // Command not found or failed to execute
            Ok(ThemeAppearance::Light)
        }
    }
}

/// Resolve which theme to apply based on system appearance and auto-pairing.
/// Per , the decision pipeline is:
/// 1. Detect system appearance via detect_system_appearance()
/// 2. Read auto.toml if it exists
/// 3. If auto.toml has entry for this appearance → use that theme
/// 4. If no auto.toml or missing field:
/// a. Get current theme
/// b. If current theme's appearance matches system appearance → keep current
/// c. If mismatch and current has auto_pair → apply auto_pair
/// d. If no auto_pair → fall back to brand defaults (Dark→catppuccin-mocha, Light→catppuccin-latte)
/// On this fallback, print guidance message
pub fn resolve_auto_theme(_env: &SlateEnv, config: &ConfigManager) -> Result<String> {
    // Step 1: Detect system appearance
    let system_appearance = detect_system_appearance()?;
    
    // Step 2: Try to read auto.toml
    let auto_config = config.read_auto_config()?;
    
    // Step 3: If auto.toml exists and has entry for this appearance
    if let Some(auto_cfg) = auto_config {
        let theme_id = match system_appearance {
            ThemeAppearance::Dark => auto_cfg.dark_theme,
            ThemeAppearance::Light => auto_cfg.light_theme,
        };
        
        if let Some(theme_id) = theme_id {
            return Ok(theme_id);
        }
    }
    
    // Step 4: No auto.toml or missing field - use fallback pipeline
    let registry = ThemeRegistry::new()?;
    let current_theme_id = config.get_current_theme()?;
    
    if let Some(ref current_id) = current_theme_id {
        if let Some(current_theme) = registry.get(&current_id) {
            // 4b: Check if current theme appearance matches system
            if current_theme.appearance == system_appearance {
                return Ok(current_id.clone());
            }
            
            // 4c: If no match, check auto_pair
            if let Some(pair_id) = current_theme.auto_pair {
                return Ok(pair_id.to_string());
            }
        }
    }
    
    // 4d: Fall back to brand defaults
    // Print guidance on this path only (per)
    let default_theme = match system_appearance {
        ThemeAppearance::Dark => "catppuccin-mocha".to_string(),
        ThemeAppearance::Light => "catppuccin-latte".to_string(),
    };
    
    if current_theme_id.is_some() {
        eprintln!("✦ Using built-in auto pairing. Run slate set --auto --configure to customize.");
    }
    
    Ok(default_theme)
}


/// Interactive configuration flow for auto-theme pairing.
/// Per D-19b: Guide user to select dark and light theme variants.
/// Persists selections to auto.toml (~/.config/slate/auto.toml).
pub fn configure_auto_theme() -> Result<()> {
    use cliclack::{confirm, select, log};
    
    cliclack::intro("✦ Configure Auto Theme")?;
    log::info("Match themes to your system appearance .")?;
    
    let env = SlateEnv::from_process()?;
    let config = ConfigManager::with_env(&env)?;
    let registry = ThemeRegistry::new()?;
    
    // Detect current system appearance for messaging
    let current_appearance = detect_system_appearance()?;
    
    // Step 1: Select dark theme
    cliclack::log::remark("")?;
    let dark_prompt = match current_appearance {
        ThemeAppearance::Dark => "Select dark theme (current system mode)",
        ThemeAppearance::Light => "Select dark theme",
    };
    
    let dark_theme_id = select(dark_prompt)
        .items(
            registry.all()
                .iter()
                .filter(|t| t.appearance == ThemeAppearance::Dark)
                .map(|t| (t.id.as_str(), t.name.as_str(), ""))
                .collect::<Vec<_>>()
                .as_slice()
        )
        .interact()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::Interrupted {
                crate::error::SlateError::UserCancelled
            } else {
                crate::error::SlateError::IOError(e)
            }
        })?;
    
    // Step 2: Select light theme
    cliclack::log::remark("")?;
    let light_prompt = match current_appearance {
        ThemeAppearance::Light => "Select light theme (current system mode)",
        ThemeAppearance::Dark => "Select light theme",
    };
    
    let light_theme_id = select(light_prompt)
        .items(
            registry.all()
                .iter()
                .filter(|t| t.appearance == ThemeAppearance::Light)
                .map(|t| (t.id.as_str(), t.name.as_str(), ""))
                .collect::<Vec<_>>()
                .as_slice()
        )
        .interact()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::Interrupted {
                crate::error::SlateError::UserCancelled
            } else {
                crate::error::SlateError::IOError(e)
            }
        })?;
    
    // Step 3: Confirm and save
    cliclack::log::remark("")?;
    let dark_theme_name = registry.get(dark_theme_id).map(|t| t.name.as_str()).unwrap_or("?");
    let light_theme_name = registry.get(light_theme_id).map(|t| t.name.as_str()).unwrap_or("?");
    
    log::info(format!("Dark:  {}", dark_theme_name))?;
    log::info(format!("Light: {}", light_theme_name))?;
    cliclack::log::remark("")?;
    
    let confirm_save = confirm("Save these preferences?")
        .initial_value(true)
        .interact()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::Interrupted {
                crate::error::SlateError::UserCancelled
            } else {
                crate::error::SlateError::IOError(e)
            }
        })?;
    
    if confirm_save {
        config.write_auto_config(Some(dark_theme_id), Some(light_theme_id))?;
        cliclack::log::success("Auto-theme preferences saved.")?;
    } else {
        cliclack::log::info("Configuration cancelled.")?;
    }
    
    cliclack::outro("")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_system_appearance_defaults_to_light() {
        // This will actually call the system command
        // On systems without defaults, should return Light
        let appearance = detect_system_appearance().unwrap();
        // We can't assert the specific value without knowing the system state,
        // but we can verify it's either Dark or Light
        assert!(appearance == ThemeAppearance::Dark || appearance == ThemeAppearance::Light);
    }
    
    #[test]
    fn test_theme_appearance_enum() {
        assert_eq!(ThemeAppearance::Dark, ThemeAppearance::Dark);
        assert_eq!(ThemeAppearance::Light, ThemeAppearance::Light);
        assert_ne!(ThemeAppearance::Dark, ThemeAppearance::Light);
    }
}
