//! bat adapter for theme application via environment variables.
//! bat uses BAT_THEME environment variable, not file writing.
//! Managed config path is created for future use; apply_theme() returns Ok()
//! because actual export happens in shell init.

use crate::adapter::{ToolAdapter, ApplyStrategy};
use crate::error::{Result, SlateError};
use crate::theme::ThemeVariant;
use std::path::{Path, PathBuf};

/// bat adapter implementing v2 ToolAdapter trait.
pub struct BatAdapter;

impl BatAdapter {
    /// Pure path resolution: BAT_CONFIG_PATH → BAT_CONFIG_DIR/config → XDG default
    fn resolve_path(
        config_path: Option<&str>,
        config_dir: Option<&str>,
        config_home: &Path,
    ) -> PathBuf {
        if let Some(val) = config_path {
            if !val.is_empty() {
                return PathBuf::from(val);
            }
        }
        if let Some(val) = config_dir {
            if !val.is_empty() {
                return PathBuf::from(val).join("config");
            }
        }
        config_home.join("bat").join("config")
    }

    /// Get config home directory (XDG default)
    fn config_home() -> Result<PathBuf> {
        let home = std::env::var("HOME")
            .map_err(|_| SlateError::MissingHomeDir)?;
        Ok(PathBuf::from(home).join(".config"))
    }
}

impl ToolAdapter for BatAdapter {
    fn tool_name(&self) -> &'static str {
        "bat"
    }

    fn is_installed(&self) -> Result<bool> {
        // Check if binary exists in PATH
        let binary_exists = which::which("bat").is_ok();

        // Check if config directory exists
        let config_home = match Self::config_home() {
            Ok(home) => home,
            Err(_) => return Ok(binary_exists),
        };

        let config_dir = config_home.join("bat");
        let config_dir_exists = config_dir.exists();

        Ok(binary_exists || config_dir_exists)
    }

    fn integration_config_path(&self) -> Result<PathBuf> {
        let config_home = Self::config_home()?;
        Ok(Self::resolve_path(
            std::env::var("BAT_CONFIG_PATH").ok().as_deref(),
            std::env::var("BAT_CONFIG_DIR").ok().as_deref(),
            &config_home,
        ))
    }

    fn managed_config_path(&self) -> PathBuf {
        let home = std::env::var("HOME").ok();
        if let Some(h) = home {
            PathBuf::from(h).join(".config/slate/managed/bat")
        } else {
            PathBuf::from(".config/slate/managed/bat")
        }
    }

    fn apply_strategy(&self) -> ApplyStrategy {
        ApplyStrategy::EnvironmentVariable
    }

    fn apply_theme(&self, _theme: &ThemeVariant) -> Result<()> {
        // bat theme is applied via BAT_THEME environment variable
        // set by shell init. has no file writes for bat.
        // This method returns Ok() by design.
        Ok(())
    }

    fn reload(&self) -> Result<()> {
        // bat doesn't support hot-reload; manual restart required
        Err(SlateError::ReloadFailed(
            "bat".to_string(),
            "bat does not support hot-reload. Restart your terminal to apply theme.".to_string(),
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
        let adapter = BatAdapter;
        assert_eq!(adapter.tool_name(), "bat");
    }

    #[test]
    fn test_is_installed_with_binary_or_config_dir() {
        let adapter = BatAdapter;
        let result = adapter.is_installed();
        assert!(result.is_ok());
    }

    #[test]
    fn test_integration_config_path_resolves_via_priority() {
        let config_home = PathBuf::from("/home/user/.config");
        
        // Test BAT_CONFIG_PATH priority
        let path1 = BatAdapter::resolve_path(Some("/custom/bat-config"), None, &config_home);
        assert_eq!(path1, PathBuf::from("/custom/bat-config"));

        // Test BAT_CONFIG_DIR priority
        let path2 = BatAdapter::resolve_path(None, Some("/custom/bat-dir"), &config_home);
        assert_eq!(path2, PathBuf::from("/custom/bat-dir/config"));

        // Test default
        let path3 = BatAdapter::resolve_path(None, None, &config_home);
        assert_eq!(path3, PathBuf::from("/home/user/.config/bat/config"));
    }

    #[test]
    fn test_apply_strategy_returns_environment_variable() {
        let adapter = BatAdapter;
        assert_eq!(adapter.apply_strategy(), ApplyStrategy::EnvironmentVariable);
    }

    #[test]
    fn test_apply_theme_returns_ok_without_writing() {
        let adapter = BatAdapter;
        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();
        
        let result = adapter.apply_theme(&theme);
        assert!(result.is_ok());
    }

    #[test]
    fn test_managed_config_path_returns_correct_directory() {
        let adapter = BatAdapter;
        let path = adapter.managed_config_path();
        
        assert!(path.to_string_lossy().contains(".config/slate/managed/bat"));
    }

    #[test]
    fn test_get_current_theme_returns_none() {
        let adapter = BatAdapter;
        let result = adapter.get_current_theme();
        
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }
}
