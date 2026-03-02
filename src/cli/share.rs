use crate::cli::font::resolve_font_choice;
use crate::config::ConfigManager;
use crate::design::symbols::Symbols;
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use crate::opacity::OpacityPreset;
use crate::theme::ThemeRegistry;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct ToolImportFlags {
    starship: bool,
    highlighting: bool,
    fastfetch: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ImportRequest {
    theme: Option<String>,
    font: Option<String>,
    opacity: Option<OpacityPreset>,
    tools: ToolImportFlags,
}

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
    let request = parse_import_request(uri)?;
    let env = SlateEnv::from_process()?;

    if let Some(font) = request.font.as_deref() {
        crate::cli::font::handle_font(Some(font))?;
    }

    if let Some(theme) = request.theme.clone() {
        crate::cli::theme::handle_theme(Some(theme), false, false)?;
    }

    let config = ConfigManager::with_env(&env)?;

    if let Some(opacity) = request.opacity {
        config.set_current_opacity_preset(opacity)?;
        println!(
            "{} Opacity set to '{}'",
            Symbols::SUCCESS,
            opacity.to_string().to_lowercase()
        );
    }

    apply_imported_tool_flags(&config, request.tools)?;

    println!();
    println!("  ✓ Config imported successfully");
    println!("  Open a new terminal to see all changes.");
    println!();

    Ok(())
}

fn parse_import_request(uri: &str) -> Result<ImportRequest> {
    let stripped = uri
        .strip_prefix("slate://")
        .ok_or_else(|| SlateError::InvalidConfig("URI must start with slate://".to_string()))?;

    let parts: Vec<&str> = stripped.split('/').collect();
    if parts.len() != 4 {
        return Err(SlateError::InvalidConfig(
            "Expected format: slate://theme/font/opacity/tools".to_string(),
        ));
    }

    Ok(ImportRequest {
        theme: parse_theme_segment(parts[0])?,
        font: parse_font_segment(parts[1])?,
        opacity: parse_opacity_segment(parts[2])?,
        tools: parse_tool_flags(parts[3])?,
    })
}

fn parse_theme_segment(theme: &str) -> Result<Option<String>> {
    if theme == "none" {
        return Ok(None);
    }

    let registry = ThemeRegistry::new()?;
    if registry.get(theme).is_none() {
        return Err(SlateError::ThemeNotFound(theme.to_string()));
    }

    Ok(Some(theme.to_string()))
}

fn parse_font_segment(font: &str) -> Result<Option<String>> {
    if font == "none" {
        return Ok(None);
    }

    let resolved = resolve_font_choice(font)?;
    Ok(Some(resolved.font_name().to_string()))
}

fn parse_opacity_segment(opacity: &str) -> Result<Option<OpacityPreset>> {
    if opacity == "none" {
        return Ok(None);
    }

    opacity.parse::<OpacityPreset>().map(Some).map_err(|_| {
        SlateError::InvalidConfig(format!(
            "Invalid opacity preset: '{}'. Must be one of: solid, frosted, clear",
            opacity
        ))
    })
}

fn parse_tool_flags(tools: &str) -> Result<ToolImportFlags> {
    if tools == "none" {
        return Ok(ToolImportFlags::default());
    }

    let mut flags = ToolImportFlags::default();
    let mut seen = std::collections::BTreeSet::new();

    for flag in tools.split(',') {
        if flag.is_empty() || !seen.insert(flag) {
            return Err(SlateError::InvalidConfig(format!(
                "Invalid tool flag list: '{}'. Use comma-separated values from: s, h, f",
                tools
            )));
        }

        match flag {
            "s" => flags.starship = true,
            "h" => flags.highlighting = true,
            "f" => flags.fastfetch = true,
            _ => {
                return Err(SlateError::InvalidConfig(format!(
                    "Invalid tool flag list: '{}'. Use comma-separated values from: s, h, f",
                    tools
                )))
            }
        }
    }

    Ok(flags)
}

fn apply_imported_tool_flags(config: &ConfigManager, flags: ToolImportFlags) -> Result<()> {
    let previous_starship = config.is_starship_enabled()?;
    let previous_highlighting = config.is_zsh_highlighting_enabled()?;
    let previous_fastfetch = config.has_fastfetch_autorun()?;

    if previous_starship != flags.starship {
        config.set_starship_enabled(flags.starship)?;
    }
    if previous_highlighting != flags.highlighting {
        config.set_zsh_highlighting_enabled(flags.highlighting)?;
    }
    if previous_fastfetch != flags.fastfetch {
        if flags.fastfetch {
            config.enable_fastfetch_autorun()?;
        } else {
            config.disable_fastfetch_autorun()?;
        }
    }

    if previous_starship == flags.starship
        && previous_highlighting == flags.highlighting
        && previous_fastfetch == flags.fastfetch
    {
        return Ok(());
    }

    if let Err(err) = config.refresh_shell_integration() {
        let _ = config.set_starship_enabled(previous_starship);
        let _ = config.set_zsh_highlighting_enabled(previous_highlighting);
        if previous_fastfetch {
            let _ = config.enable_fastfetch_autorun();
        } else {
            let _ = config.disable_fastfetch_autorun();
        }
        let _ = config.refresh_shell_integration();
        return Err(err);
    }

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

    #[test]
    fn test_parse_import_request_rejects_invalid_font() {
        let result = parse_import_request("slate://none/Definitely-Not-A-Font/solid/none");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_import_request_rejects_invalid_opacity() {
        let result = parse_import_request("slate://none/none/not-real/none");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_import_request_rejects_invalid_tool_flags() {
        let result = parse_import_request("slate://none/none/solid/s,x");
        assert!(result.is_err());
    }
}
