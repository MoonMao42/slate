//! fastfetch adapter with JSONC config generation and Apple logo theming.
//! Implements EnvironmentVariable strategy.
//! Generates managed JSONC config with themed colors while preserving Apple logo.

use crate::adapter::{ApplyOutcome, ApplyStrategy, ToolAdapter};
use crate::config::ConfigManager;
use crate::detection;
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use crate::theme::ThemeVariant;
use std::path::PathBuf;

/// fastfetch adapter implementing the ToolAdapter trait.
pub struct FastfetchAdapter;

impl FastfetchAdapter {
    /// Get config home directory (XDG default)
    fn config_home() -> Result<PathBuf> {
        let env = SlateEnv::from_process()?;
        Ok(env.xdg_config_home().to_path_buf())
    }
}

impl ToolAdapter for FastfetchAdapter {
    fn tool_name(&self) -> &'static str {
        "fastfetch"
    }

    fn is_installed(&self) -> Result<bool> {
        Ok(detection::detect_tool_presence(self.tool_name()).installed)
    }

    fn integration_config_path(&self) -> Result<PathBuf> {
        let config_home = Self::config_home()?;
        Ok(config_home.join("fastfetch/config.jsonc"))
    }

    fn managed_config_path(&self) -> PathBuf {
        let env = SlateEnv::from_process().ok();
        if let Some(env) = env.as_ref() {
            env.config_dir().join("managed").join("fastfetch")
        } else {
            PathBuf::from(".config/slate/managed/fastfetch")
        }
    }

    fn apply_strategy(&self) -> ApplyStrategy {
        ApplyStrategy::EnvironmentVariable
    }

    fn apply_theme(&self, theme: &ThemeVariant) -> Result<ApplyOutcome> {
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

        // fastfetch is invoked at shell startup via the managed wrapper;
        // updated colors are visible the next time a shell runs it.
        Ok(ApplyOutcome::applied_needs_new_shell())
    }

    fn get_current_theme(&self) -> Result<Option<String>> {
        // feature; not implemented yet
        Ok(None)
    }
}

impl FastfetchAdapter {
    pub fn generate_jsonc_config(&self, theme: &ThemeVariant) -> Result<String> {
        use crate::adapter::palette_renderer::PaletteRenderer;
        use serde_json::json;

        let palette = &theme.palette;

        // Use subtext color for keys (muted), accent for separators (subtle pop)
        let key_hex = palette.subtext1.as_deref().unwrap_or(&palette.foreground);
        let (r_key, g_key, b_key) = PaletteRenderer::hex_to_rgb(key_hex)?;
        let (r_acc, g_acc, b_acc) = PaletteRenderer::hex_to_rgb(&palette.blue)?;
        let (r_fg, g_fg, b_fg) = PaletteRenderer::hex_to_rgb(&palette.foreground)?;

        let color_keys = format!("38;2;{};{};{}", r_key, g_key, b_key);
        let color_separator = format!("38;2;{};{};{}", r_acc, g_acc, b_acc);
        let color_output = format!("38;2;{};{};{}", r_fg, g_fg, b_fg);

        let config = json!({
            "$schema": "https://github.com/fastfetch-cli/fastfetch/raw/dev/doc/json_schema.json",
            "logo": {
                "type": "builtin",
                "source": if cfg!(target_os = "macos") { "apple_small" } else { "auto" },
                "padding": { "top": 1 }
            },
            "display": {
                "separator": " ",
                "color": {
                    "keys": color_keys,
                    "separator": color_separator,
                    "output": color_output
                }
            },
            "modules": [
                { "type": "title" },
                { "type": "separator" },
                { "type": "os" },
                { "type": "kernel" },
                { "type": "uptime" },
                { "type": "terminal" },
                { "type": "shell" },
                { "type": "cpu" },
                { "type": "memory" },
                { "type": "break" },
                { "type": "colors" }
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
        assert!(path
            .to_string_lossy()
            .contains(".config/slate/managed/fastfetch"));
    }

    #[test]
    fn test_get_current_theme_returns_none() {
        let adapter = FastfetchAdapter;
        let result = adapter.get_current_theme();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }
}
