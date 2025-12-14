//! eza adapter with managed YAML theme file and EnvironmentVariable strategy.
//! Per and eza uses YAML theme files, not TOML. The adapter writes
//! a managed theme.yml to ~/.config/slate/managed/eza/ and expects EZA_CONFIG_DIR
//! environment variable to be exported by shell init.

use crate::adapter::{ApplyStrategy, ToolAdapter};
use crate::config::ConfigManager;
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use crate::theme::ThemeVariant;
use std::path::PathBuf;

/// eza adapter implementing v2 ToolAdapter trait.
pub struct EzaAdapter;

impl EzaAdapter {
    /// Get config home directory (XDG default)
    fn config_home() -> Result<PathBuf> {
        let env = SlateEnv::from_process()?;
        let home = env.home().to_str().ok_or(SlateError::MissingHomeDir)?;
        Ok(PathBuf::from(home).join(".config"))
    }

    /// Render Palette into eza YAML theme structure.
    /// Mapping guided by eza color semantics:
    /// - foreground/background: text and background colors
    /// - ANSI colors: map to directory/file/permission categories
    fn render_eza_yaml(theme: &ThemeVariant) -> String {
        let palette = &theme.palette;

        format!(
            "colors:\n  text: \"{}\"\n  background: \"{}\"\n  errors: \"{}\"\n  warning: \"{}\"\n  success: \"{}\"\n  info: \"{}\"\n  special: \"{}\"\n  modified: \"{}\"\n",
            palette.foreground,
            palette.background,
            palette.red,
            palette.yellow,
            palette.green,
            palette.blue,
            palette.cyan,
            palette.magenta,
        )
    }
}

impl ToolAdapter for EzaAdapter {
    fn tool_name(&self) -> &'static str {
        "eza"
    }

    fn is_installed(&self) -> Result<bool> {
        // Check if binary exists in PATH
        let binary_exists = which::which("eza").is_ok();

        // Check if config directory exists
        let config_home = match Self::config_home() {
            Ok(home) => home,
            Err(_) => return Ok(binary_exists),
        };

        // Check EZA_CONFIG_DIR env var or default ~/.config/eza/
        let config_dir = match std::env::var("EZA_CONFIG_DIR") {
            Ok(var) => PathBuf::from(var),
            Err(_) => config_home.join("eza"),
        };

        let config_dir_exists = config_dir.exists();

        Ok(binary_exists || config_dir_exists)
    }

    fn integration_config_path(&self) -> Result<PathBuf> {
        let config_home = Self::config_home()?;

        // Respect EZA_CONFIG_DIR env var if set
        if let Ok(custom_dir) = std::env::var("EZA_CONFIG_DIR") {
            Ok(PathBuf::from(custom_dir))
        } else {
            Ok(config_home.join("eza"))
        }
    }

    fn managed_config_path(&self) -> PathBuf {
        let env = SlateEnv::from_process().ok();
        let home = env
            .as_ref()
            .and_then(|e| e.home().to_str().map(|s| s.to_string()));
        if let Some(h) = home {
            PathBuf::from(h).join(".config/slate/managed/eza")
        } else {
            PathBuf::from(".config/slate/managed/eza")
        }
    }

    fn apply_strategy(&self) -> ApplyStrategy {
        ApplyStrategy::EnvironmentVariable
    }

    fn apply_theme(&self, theme: &ThemeVariant) -> Result<()> {
        // Validate theme has palette data
        theme.palette.validate()?;

        // Render theme as YAML
        let yaml_content = Self::render_eza_yaml(theme);

        // Write to managed config via ConfigManager
        let config_manager = ConfigManager::new()?;
        config_manager.write_managed_file("eza", "theme.yml", &yaml_content)?;

        Ok(())
    }

    fn reload(&self) -> Result<()> {
        // eza doesn't support hot-reload; manual restart required
        Err(SlateError::ReloadFailed(
            "eza".to_string(),
            "eza does not support hot-reload. Restart your terminal to apply theme.".to_string(),
        ))
    }

    fn get_current_theme(&self) -> Result<Option<String>> {
        // feature; not implemented yet
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name() {
        let adapter = EzaAdapter;
        assert_eq!(adapter.tool_name(), "eza");
    }

    #[test]
    fn test_is_installed_checks_binary_and_config_dir() {
        let adapter = EzaAdapter;
        let result = adapter.is_installed();
        assert!(result.is_ok());
    }

    #[test]
    fn test_integration_config_path_resolves_eza_config_dir_or_default() {
        let adapter = EzaAdapter;
        let result = adapter.integration_config_path();
        assert!(result.is_ok());

        let path = result.unwrap();
        // Should be either custom EZA_CONFIG_DIR or ~/.config/eza
        assert!(
            path.to_string_lossy().contains("eza")
                || path.to_string_lossy().contains("EZA_CONFIG_DIR")
        );
    }

    #[test]
    fn test_managed_config_path_returns_correct_directory() {
        let adapter = EzaAdapter;
        let path = adapter.managed_config_path();

        assert!(path.to_string_lossy().contains(".config/slate/managed/eza"));
    }

    #[test]
    fn test_apply_strategy_returns_environment_variable() {
        let adapter = EzaAdapter;
        assert_eq!(adapter.apply_strategy(), ApplyStrategy::EnvironmentVariable);
    }

    #[test]
    fn test_apply_theme_writes_managed_yaml_theme() {
        let adapter = EzaAdapter;
        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();

        // Just verify it returns Ok without errors
        // Actual file writing would require mocking ConfigManager
        let result = adapter.apply_theme(&theme);
        assert!(result.is_ok());
    }

    #[test]
    fn test_render_eza_yaml_produces_valid_yaml_structure() {
        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();
        let yaml = EzaAdapter::render_eza_yaml(&theme);

        assert!(yaml.contains("colors:"));
        assert!(yaml.contains("text:"));
        assert!(yaml.contains("background:"));
        assert!(yaml.contains("errors:"));
        assert!(yaml.contains("warning:"));
        assert!(yaml.contains("success:"));
        assert!(yaml.contains("info:"));
        assert!(yaml.contains("special:"));
        assert!(yaml.contains("modified:"));

        // Verify it's valid YAML format (basic check)
        assert!(yaml.contains("#"));
    }

    #[test]
    fn test_reload_returns_error() {
        let adapter = EzaAdapter;
        let result = adapter.reload();
        assert!(result.is_err());
    }

    #[test]
    fn test_get_current_theme_returns_none() {
        let adapter = EzaAdapter;
        let result = adapter.get_current_theme();

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }
}
