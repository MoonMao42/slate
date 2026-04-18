//! bat adapter for theme application via environment variables.
//!
//! bat uses BAT_THEME environment variable, not file writing.
//! Managed config path is created for future use; apply_theme() returns Ok(())
//! because actual export happens in shell init.

use crate::adapter::{ApplyOutcome, ApplyStrategy, ToolAdapter};
use crate::detection;
use crate::env::SlateEnv;
use crate::error::Result;
use crate::theme::ThemeVariant;
use std::path::{Path, PathBuf};

/// bat adapter implementing the ToolAdapter trait.
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
}

impl ToolAdapter for BatAdapter {
    fn tool_name(&self) -> &'static str {
        "bat"
    }

    fn is_installed(&self) -> Result<bool> {
        Ok(detection::detect_tool_presence(self.tool_name()).installed)
    }

    fn integration_config_path(&self) -> Result<PathBuf> {
        let env = SlateEnv::from_process()?;
        self.integration_config_path_with_env(&env)
    }

    fn managed_config_path(&self) -> PathBuf {
        let env = SlateEnv::from_process().ok();
        self.managed_config_path_with_env(env.as_ref())
    }

    fn apply_strategy(&self) -> ApplyStrategy {
        ApplyStrategy::EnvironmentVariable
    }

    fn apply_theme(&self, _theme: &ThemeVariant) -> Result<ApplyOutcome> {
        // bat uses BAT_THEME env var, not file writes.
        // env.zsh exports this during shell init.
        // This method is no-op; write happens in shell integration.
        // env var changes only take effect in a fresh shell.
        Ok(ApplyOutcome::applied_needs_new_shell())
    }
}

/// Helper methods using injected SlateEnv (for testing)
impl BatAdapter {
    pub fn integration_config_path_with_env(&self, env: &SlateEnv) -> Result<PathBuf> {
        let config_home = env.xdg_config_home().to_path_buf();
        let config_path = std::env::var("BAT_CONFIG_PATH").ok();
        let config_dir = std::env::var("BAT_CONFIG_DIR").ok();

        Ok(Self::resolve_path(
            config_path.as_deref(),
            config_dir.as_deref(),
            &config_home,
        ))
    }

    pub fn managed_config_path_with_env(&self, env: Option<&SlateEnv>) -> PathBuf {
        if let Some(e) = env {
            let config_dir = e.config_dir();
            config_dir.join("managed").join("bat")
        } else {
            PathBuf::from(".config/slate/managed/bat")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bat_adapter_tool_name() {
        let adapter = BatAdapter;
        assert_eq!(adapter.tool_name(), "bat");
    }

    #[test]
    fn test_bat_apply_strategy() {
        let adapter = BatAdapter;
        assert_eq!(adapter.apply_strategy(), ApplyStrategy::EnvironmentVariable);
    }

    #[test]
    fn test_bat_resolve_path_with_explicit_path() {
        let result =
            BatAdapter::resolve_path(Some("/explicit/path"), None, &PathBuf::from("/config"));
        assert_eq!(result, PathBuf::from("/explicit/path"));
    }

    #[test]
    fn test_bat_resolve_path_with_dir() {
        let result = BatAdapter::resolve_path(None, Some("/bat/dir"), &PathBuf::from("/config"));
        assert_eq!(result, PathBuf::from("/bat/dir/config"));
    }

    #[test]
    fn test_bat_resolve_path_with_default() {
        let result = BatAdapter::resolve_path(None, None, &PathBuf::from("/config"));
        assert_eq!(result, PathBuf::from("/config/bat/config"));
    }

    #[test]
    fn test_bat_integration_config_path_with_env() {
        let tempdir = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let adapter = BatAdapter;

        let path = adapter.integration_config_path_with_env(&env).unwrap();
        assert!(path.ends_with("bat/config"));
    }

    #[test]
    fn test_bat_managed_config_path_with_env() {
        let tempdir = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let adapter = BatAdapter;

        let path = adapter.managed_config_path_with_env(Some(&env));
        assert!(path.ends_with("slate/managed/bat"));
    }
}
