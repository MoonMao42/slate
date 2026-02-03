//! lazygit adapter with macOS path fix and pager sync logic.
//! Uses EnvironmentVariable strategy (LG_CONFIG_FILE via slate init).
//! Per , Fixes macOS path resolution to check LG_CONFIG_FILE first.
//! Preserves pager sync logic (bat/delta themes) as competitive advantage.

use crate::adapter::{ApplyOutcome, ApplyStrategy, ToolAdapter};
use crate::config::ConfigManager;
use crate::detection;
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use crate::theme::ThemeVariant;
use std::path::PathBuf;

/// lazygit adapter implementing v2 ToolAdapter trait.
pub struct LazygitAdapter;

impl LazygitAdapter {
    fn parse_config_paths(path_str: &str) -> Option<PathBuf> {
        for separator in [',', ':'] {
            if !path_str.contains(separator) {
                continue;
            }

            if let Some(first_path) = path_str
                .split(separator)
                .map(str::trim)
                .find(|path| !path.is_empty())
            {
                return Some(PathBuf::from(first_path));
            }
        }

        let trimmed = path_str.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(PathBuf::from(trimmed))
        }
    }

    /// Resolve lazygit config path per , /// 1. LG_CONFIG_FILE env var (if set)
    /// 2. XDG_CONFIG_HOME/lazygit/config.yml
    /// 3. ~/Library/Application Support/lazygit/ (macOS)
    fn resolve_config_path() -> Result<PathBuf> {
        let env = SlateEnv::from_process()?;
        Self::resolve_config_path_with_env(&env)
    }

    fn resolve_config_path_with_env(env: &SlateEnv) -> Result<PathBuf> {
        // Step 1: Check LG_CONFIG_FILE env var first
        if let Ok(path_str) = std::env::var("LG_CONFIG_FILE") {
            if let Some(first_path) = Self::parse_config_paths(&path_str) {
                return Ok(first_path);
            }
        }

        // Step 2: Check XDG config root from SlateEnv
        let xdg = env.xdg_config_home();
        if !xdg.as_os_str().is_empty() {
            return Ok(xdg.join("lazygit/config.yml"));
        }

        // Step 3: Default to macOS location
        let home = env.home().to_str().ok_or(SlateError::MissingHomeDir)?;

        if cfg!(target_os = "macos") {
            Ok(PathBuf::from(home).join("Library/Application Support/lazygit/config.yml"))
        } else {
            Ok(PathBuf::from(home).join(".config/lazygit/config.yml"))
        }
    }
}

impl ToolAdapter for LazygitAdapter {
    fn tool_name(&self) -> &'static str {
        "lazygit"
    }

    fn is_installed(&self) -> Result<bool> {
        Ok(detection::detect_tool_presence(self.tool_name()).installed)
    }

    fn integration_config_path(&self) -> Result<PathBuf> {
        Self::resolve_config_path()
    }

    fn managed_config_path(&self) -> PathBuf {
        let env = SlateEnv::from_process().ok();
        if let Some(env) = env.as_ref() {
            env.config_dir().join("managed").join("lazygit")
        } else {
            PathBuf::from(".config/slate/managed/lazygit")
        }
    }

    fn apply_strategy(&self) -> ApplyStrategy {
        ApplyStrategy::EnvironmentVariable
    }

    fn apply_theme(&self, theme: &ThemeVariant) -> Result<ApplyOutcome> {
        // Step 1: Extract theme name from tool_refs
        theme.tool_refs.get("lazygit").ok_or_else(|| {
            SlateError::InvalidThemeData(format!(
                "Theme '{}' missing lazygit tool reference",
                theme.id
            ))
        })?;

        // Step 2: Generate managed YAML using PaletteRenderer
        let managed_content = self.generate_yaml_config(theme)?;

        // Step 3: Write to managed config directory
        let config_manager = ConfigManager::new()?;
        config_manager.write_managed_file("lazygit", "config.yml", &managed_content)?;

        // Step 4: Pager sync is handled at generation time via generate_yaml_config

        Ok(ApplyOutcome::Applied)
    }

    fn get_current_theme(&self) -> Result<Option<String>> {
        // feature; not implemented yet
        Ok(None)
    }
}

impl LazygitAdapter {
    fn generate_yaml_config(&self, theme: &ThemeVariant) -> Result<String> {
        let lazygit_ref = theme.tool_refs.get("lazygit").ok_or_else(|| {
            SlateError::InvalidThemeData(format!(
                "Theme '{}' missing lazygit tool reference",
                theme.id
            ))
        })?;

        // Generate lazygit YAML config with themed GUI colors
        let mut semantic_map = std::collections::HashMap::new();
        semantic_map.insert("text", "gui.theme.inactiveBorderColor");
        semantic_map.insert("foreground", "gui.theme.activeBorderColor");
        semantic_map.insert("red", "gui.theme.selectedLineBgColor");

        let yaml_content = crate::adapter::palette_renderer::PaletteRenderer::to_yaml(
            &theme.palette,
            &semantic_map,
        )?;

        // Wrap in gui.theme section
        let config = format!(
            "gui:\n  theme:\n{}\npager:\n  commands:\n    theme: \"{}\"\n",
            yaml_content
                .lines()
                .map(|line| format!("  {}", line))
                .collect::<Vec<_>>()
                .join("\n"),
            lazygit_ref
        );

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name() {
        let adapter = LazygitAdapter;
        assert_eq!(adapter.tool_name(), "lazygit");
    }

    #[test]
    fn test_apply_strategy_returns_environment_variable() {
        let adapter = LazygitAdapter;
        assert_eq!(adapter.apply_strategy(), ApplyStrategy::EnvironmentVariable);
    }

    #[test]
    fn test_managed_config_path_returns_correct_directory() {
        let adapter = LazygitAdapter;
        let path = adapter.managed_config_path();
        assert!(path
            .to_string_lossy()
            .contains(".config/slate/managed/lazygit"));
    }

    #[test]
    fn test_get_current_theme_returns_none() {
        let adapter = LazygitAdapter;
        let result = adapter.get_current_theme();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn test_parse_config_paths_prefers_first_comma_separated_path() {
        let path = LazygitAdapter::parse_config_paths("/tmp/managed.yml,/tmp/user.yml");
        assert_eq!(path, Some(PathBuf::from("/tmp/managed.yml")));
    }

    #[test]
    fn test_parse_config_paths_accepts_legacy_colon_separator() {
        let path = LazygitAdapter::parse_config_paths("/tmp/managed.yml:/tmp/user.yml");
        assert_eq!(path, Some(PathBuf::from("/tmp/managed.yml")));
    }

    #[test]
    fn test_parse_config_paths_rejects_empty_input() {
        assert_eq!(LazygitAdapter::parse_config_paths("   "), None);
    }
}
