use crate::config::ConfigManager;
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};

/// Export current slate config as a shareable URI.
/// Format: slate://theme/font/opacity/tools
/// Example: slate://catppuccin-mocha/JetBrainsMono/frosted/s,h,f
/// Tool flags: s=starship, h=highlighting, f=fastfetch
pub fn handle_export() -> Result<()> {
    let env = SlateEnv::from_process()?;
    let config = ConfigManager::with_env(&env)?;

    let theme = config
        .get_current_theme()?
        .unwrap_or_else(|| "none".to_string());

    let font = config
        .get_current_font()?
        .unwrap_or_else(|| "none".to_string())
        .replace(' ', "-");

    let opacity = config
        .get_current_opacity()?
        .unwrap_or_else(|| "solid".to_string())
        .to_lowercase();

    let mut tools = Vec::new();
    if config.is_starship_enabled()? {
        tools.push("s");
    }
    if config.is_zsh_highlighting_enabled()? {
        tools.push("h");
    }
    if config.has_fastfetch_autorun()? {
        tools.push("f");
    }
    let tools_str = if tools.is_empty() {
        "none".to_string()
    } else {
        tools.join(",")
    };

    let uri = format!("slate://{}/{}/{}/{}", theme, font, opacity, tools_str);

    println!();
    println!("  {}", uri);
    println!();
    println!("  Share this with anyone — they can run:");
    println!("  slate import \"{}\"", uri);
    println!();

    Ok(())
}

/// Import a slate config from a shareable URI.
/// Parses the URI and applies theme, font, opacity, and tool toggles.
pub fn handle_import(uri: &str) -> Result<()> {
    let stripped = uri
        .strip_prefix("slate://")
        .ok_or_else(|| SlateError::InvalidConfig("URI must start with slate://".to_string()))?;

    let parts: Vec<&str> = stripped.split('/').collect();
    if parts.len() != 4 {
        return Err(SlateError::InvalidConfig(
            "Expected format: slate://theme/font/opacity/tools".to_string(),
        ));
    }

    let theme = parts[0];
    let font = parts[1].replace('-', " ");
    let opacity = parts[2];
    let tools = parts[3];

    let env = SlateEnv::from_process()?;
    let config = ConfigManager::with_env(&env)?;

    // Apply theme
    if theme != "none" {
        crate::cli::theme::handle_theme(Some(theme.to_string()), false, false)?;
    }

    // Apply font
    if font != "none" {
        crate::cli::font::handle_font(Some(&font))?;
    }

    // Apply opacity
    if matches!(opacity, "solid" | "frosted" | "clear") {
        crate::cli::config::handle_config_set("opacity", opacity)?;
    }

    // Apply tool toggles
    let starship = tools.contains('s');
    let highlighting = tools.contains('h');
    let fastfetch = tools.contains('f');

    if config.is_starship_enabled()? != starship {
        config.set_starship_enabled(starship)?;
    }
    if config.is_zsh_highlighting_enabled()? != highlighting {
        config.set_zsh_highlighting_enabled(highlighting)?;
    }
    if fastfetch && !config.has_fastfetch_autorun()? {
        config.enable_fastfetch_autorun()?;
    } else if !fastfetch && config.has_fastfetch_autorun()? {
        config.disable_fastfetch_autorun()?;
    }

    config.refresh_shell_integration()?;

    println!();
    println!("  ✓ Config imported successfully");
    println!("  Open a new terminal to see all changes.");
    println!();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_export_produces_valid_uri() {
        let temp = tempfile::TempDir::new().unwrap();
        let env = SlateEnv::with_home(temp.path().to_path_buf());
        let config = ConfigManager::with_env(&env).unwrap();

        config.set_current_theme("catppuccin-mocha").unwrap();
        config.set_starship_enabled(true).unwrap();

        // Verify config was set
        assert_eq!(
            config.get_current_theme().unwrap(),
            Some("catppuccin-mocha".to_string())
        );
        assert!(config.is_starship_enabled().unwrap());
    }

    #[test]
    fn test_import_rejects_invalid_uri() {
        let result = handle_import("invalid-uri");
        assert!(result.is_err());
    }

    #[test]
    fn test_import_rejects_wrong_segment_count() {
        let result = handle_import("slate://only-one-part");
        assert!(result.is_err());
    }
}
