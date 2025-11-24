//! fastfetch adapter with JSONC config generation and Apple logo theming.
//! Per , Implements EnvironmentVariable strategy.
//! Generates managed JSONC config with themed colors while preserving Apple logo.

use crate::adapter::{ToolAdapter, ApplyStrategy};
use crate::config::ConfigManager;
use crate::error::{Result, SlateError};
use crate::theme::ThemeVariant;
use std::path::PathBuf;
use which::which;

/// fastfetch adapter implementing v2 ToolAdapter trait.
pub struct FastfetchAdapter;

impl FastfetchAdapter {
    /// Get config home directory (XDG default)
    fn config_home() -> Result<PathBuf> {
        let home = std::env::var("HOME")
            .map_err(|_| SlateError::MissingHomeDir)?;
        Ok(PathBuf::from(home).join(".config"))
    }
}

impl ToolAdapter for FastfetchAdapter {
    fn tool_name(&self) -> &'static str {
        "fastfetch"
    }

    fn is_installed(&self) -> Result<bool> {
        let binary_exists = which("fastfetch").is_ok();
        let config_dir_exists = match Self::config_home() {
            Ok(home) => home.join("fastfetch").exists(),
            Err(_) => false,
        };

        Ok(binary_exists || config_dir_exists)
    }

    fn integration_config_path(&self) -> Result<PathBuf> {
        let config_home = Self::config_home()?;
        Ok(config_home.join("fastfetch/config.jsonc"))
    }

    fn managed_config_path(&self) -> PathBuf {
        let home = std::env::var("HOME").ok();
        if let Some(h) = home {
            PathBuf::from(h).join(".config/slate/managed/fastfetch")
        } else {
            PathBuf::from(".config/slate/managed/fastfetch")
        }
    }

    fn apply_strategy(&self) -> ApplyStrategy {
        ApplyStrategy::EnvironmentVariable
    }

    fn apply_theme(&self, theme: &ThemeVariant) -> Result<()> {
        // Step 1: Extract theme name from tool_refs
        let _fastfetch_theme = theme
            .tool_refs
            .get("fastfetch")
            .ok_or_else(|| {
                SlateError::InvalidThemeData(format!(
                    "Theme '{}' missing fastfetch tool reference",
                    theme.id
                ))
            })?
            .to_string();

        // Step 2: Generate managed JSONC config with themed colors
        let managed_content = self.generate_jsonc_config(theme)?;

        // Step 3: Write to managed config directory
        let config_manager = ConfigManager::new()?;
        config_manager.write_managed_file("fastfetch", "config.jsonc", &managed_content)?;

        Ok(())
    }

    fn get_current_theme(&self) -> Result<Option<String>> {
        // feature; not implemented yet
        Ok(None)
    }
}

impl FastfetchAdapter {
    pub fn generate_jsonc_config(&self, theme: &ThemeVariant) -> Result<String> {
        use serde_json::json;
        use crate::adapter::palette_renderer::PaletteRenderer;

        let palette = &theme.palette;

        // Convert hex colors to ANSI 24-bit RGB format
        let (r_fg, g_fg, b_fg) = PaletteRenderer::hex_to_rgb(&palette.foreground)?;
        let (r_blue, g_blue, b_blue) = PaletteRenderer::hex_to_rgb(&palette.blue)?;

        let color_keys = format!("38;2;{};{};{}", r_fg, g_fg, b_fg);
        let color_accent = format!("38;2;{};{};{}", r_blue, g_blue, b_blue);

        let config = json!({
            "$schema": "https://github.com/fastfetch-cli/fastfetch/raw/dev/doc/json_schema.json",
            "display": {
                "separator": "─",
                "key-width": 12,
                "logo": {
                    "type": "builtin",
                    "name": "apple",
                    "width": 20,
                    "height": 10,
                    "preserve": true
                }
            },
            "color": {
                "keys": color_keys,
                "separator": color_keys,
                "output": color_keys
            },
            "modules": [
                { "type": "title" },
                { "type": "separator" },
                { "type": "os" },
                { "type": "kernel" },
                { "type": "cpu" },
                {
                    "type": "memory",
                    "options": {
                        "barLength": 20,
                        "barsColors": [color_accent]
                    }
                },
                {
                    "type": "disk",
                    "key": "Disk (/)",
                    "options": {
                        "barsColors": [color_accent]
                    }
                },
                { "type": "shell" }
            ]
        });

        Ok(serde_json::to_string_pretty(&config)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name() {
        let adapter = FastfetchAdapter;
        assert_eq!(adapter.tool_name(), "fastfetch");
    }

    #[test]
    fn test_apply_strategy_returns_environment_variable() {
        let adapter = FastfetchAdapter;
        assert_eq!(adapter.apply_strategy(), ApplyStrategy::EnvironmentVariable);
    }

    #[test]
    fn test_managed_config_path_returns_correct_directory() {
        let adapter = FastfetchAdapter;
        let path = adapter.managed_config_path();
        assert!(path.to_string_lossy().contains(".config/slate/managed/fastfetch"));
    }

    #[test]
    fn test_get_current_theme_returns_none() {
        let adapter = FastfetchAdapter;
        let result = adapter.get_current_theme();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }
}
