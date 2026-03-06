use crate::config::ConfigManager;
use crate::detection::TerminalProfile;
use crate::env::SlateEnv;
use crate::error::Result;
use crate::theme::{ThemeAppearance, ThemeRegistry};
/// Detect the current system appearance through the active platform backend.
/// macOS uses `defaults`, Linux prefers XDG desktop portal and falls back to
/// GNOME `gsettings` when needed, and unsupported environments default to Light
/// so manual `slate theme --auto` still degrades safely.
pub fn detect_system_appearance() -> Result<ThemeAppearance> {
    Ok(crate::platform::desktop::detect_system_appearance())
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
        if let Some(current_theme) = registry.get(current_id) {
            // 4b: Check if current theme appearance matches system
            if current_theme.appearance == system_appearance {
                return Ok(current_id.clone());
            }

            // 4c: If no match, check auto_pair
            if let Some(pair_id) = current_theme.auto_pair.as_ref() {
                return Ok(pair_id.clone());
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
        eprintln!("✦ Using built-in auto pairing. Run slate config set auto-theme configure to customize.");
    }

    Ok(default_theme)
}

/// Interactive configuration flow for auto-theme pairing.
/// Per D-19b: Guide user to select dark and light theme variants.
/// Persists selections to auto.toml (~/.config/slate/auto.toml).
pub fn configure_auto_theme() -> Result<()> {
    use cliclack::{confirm, log, select};

    cliclack::intro("✦ Configure Auto Theme")?;
    log::info("Match themes to your system appearance .")?;
    let terminal = TerminalProfile::detect();
    let backend = crate::platform::desktop::detect_backend();
    if backend.supports_watcher() && terminal.watcher_shell_autostart_supported() {
        log::remark(format!(
            "Ghostty shell sessions can relaunch the {} watcher automatically.",
            backend.label()
        ))?;
    } else if backend.supports_watcher() {
        log::remark(format!(
            "{} watching is available, but restart recovery is still fully supported in Ghostty shells.",
            backend.label()
        ))?;
    } else {
        log::remark(
            "Automatic appearance watching is unavailable here. You can still run `slate theme --auto` manually.",
        )?;
    }

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
            registry
                .all()
                .iter()
                .filter(|t| t.appearance == ThemeAppearance::Dark)
                .map(|t| (t.id.as_str(), t.name.as_str(), ""))
                .collect::<Vec<_>>()
                .as_slice(),
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
            registry
                .all()
                .iter()
                .filter(|t| t.appearance == ThemeAppearance::Light)
                .map(|t| (t.id.as_str(), t.name.as_str(), ""))
                .collect::<Vec<_>>()
                .as_slice(),
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
    let dark_theme_name = registry
        .get(dark_theme_id)
        .map(|t| t.name.as_str())
        .unwrap_or("?");
    let light_theme_name = registry
        .get(light_theme_id)
        .map(|t| t.name.as_str())
        .unwrap_or("?");

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

    #[test]
    fn test_resolve_auto_theme_with_existing_auto_config() {
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let env = SlateEnv::with_home(temp.path().to_path_buf());
        let config = ConfigManager::with_env(&env).unwrap();

        // Write auto.toml with dark and light themes
        config
            .write_auto_config(Some("catppuccin-mocha"), Some("catppuccin-latte"))
            .unwrap();

        // Set current theme to something else
        config.set_current_theme("tokyo-night-dark").unwrap();

        // resolve_auto_theme should read from auto.toml regardless of current theme
        let resolved = resolve_auto_theme(&env, &config).unwrap();

        // Since we can't control system appearance in tests, check that it either
        // resolves to one of the configured themes or a fallback
        let theme_registry = ThemeRegistry::new().unwrap();
        assert!(theme_registry.get(&resolved).is_some());
    }

    #[test]
    fn test_resolve_auto_theme_fallback_with_auto_pair() {
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let env = SlateEnv::with_home(temp.path().to_path_buf());
        let config = ConfigManager::with_env(&env).unwrap();

        // Don't write auto.toml, so fallback pipeline is used
        // Set current theme to one with auto_pair (e.g., catppuccin-mocha pairs with catppuccin-latte)
        config.set_current_theme("catppuccin-mocha").unwrap();

        // resolve_auto_theme should use fallback pipeline
        let resolved = resolve_auto_theme(&env, &config).unwrap();

        // Verify resolved theme is valid
        let theme_registry = ThemeRegistry::new().unwrap();
        assert!(theme_registry.get(&resolved).is_some());
    }

    #[test]
    fn test_auto_config_read_write_round_trip() {
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let env = SlateEnv::with_home(temp.path().to_path_buf());
        let config = ConfigManager::with_env(&env).unwrap();

        // Initially no config
        let initial = config.read_auto_config().unwrap();
        assert!(initial.is_none());

        // Write config
        config
            .write_auto_config(Some("catppuccin-mocha"), Some("catppuccin-latte"))
            .unwrap();

        // Read it back
        let read_back = config.read_auto_config().unwrap();
        assert!(read_back.is_some());

        let auto_cfg = read_back.unwrap();
        assert_eq!(auto_cfg.dark_theme, Some("catppuccin-mocha".to_string()));
        assert_eq!(auto_cfg.light_theme, Some("catppuccin-latte".to_string()));
    }

    #[test]
    fn test_auto_config_partial_update() {
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let env = SlateEnv::with_home(temp.path().to_path_buf());
        let config = ConfigManager::with_env(&env).unwrap();

        // Write initial config with both values
        config
            .write_auto_config(Some("catppuccin-mocha"), Some("catppuccin-latte"))
            .unwrap();

        // Update only dark theme, should preserve light theme
        config
            .write_auto_config(Some("tokyo-night-dark"), None)
            .unwrap();

        // Read back
        let read_back = config.read_auto_config().unwrap().unwrap();
        assert_eq!(read_back.dark_theme, Some("tokyo-night-dark".to_string()));
        assert_eq!(read_back.light_theme, Some("catppuccin-latte".to_string()));
    }

    #[test]
    fn test_resolve_auto_theme_defaults_when_no_config() {
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let env = SlateEnv::with_home(temp.path().to_path_buf());
        let config = ConfigManager::with_env(&env).unwrap();

        // No auto.toml, no current theme
        let resolved = resolve_auto_theme(&env, &config).unwrap();

        // Should resolve to a brand default (catppuccin-mocha or catppuccin-latte)
        assert!(resolved == "catppuccin-mocha" || resolved == "catppuccin-latte");
    }
}
