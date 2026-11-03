//! OpenCode palette publication with lossless CLI theme selection.

use crate::adapter::{ApplyOutcome, ApplyStrategy, SkipReason, ToolAdapter};
use crate::config::ConfigManager;
use crate::detection;
use crate::env::SlateEnv;
use crate::error::Result;
use crate::theme::ThemeVariant;
use std::fs;
use std::path::PathBuf;

pub(crate) mod config;
pub(crate) mod palette;

/// OpenCode adapter implementing the ToolAdapter trait.
pub struct OpencodeAdapter;

impl OpencodeAdapter {
    pub fn theme_path(env: &SlateEnv) -> PathBuf {
        env.xdg_config_home()
            .join("opencode/themes/slate-sync.json")
    }

    pub(crate) const CLI_SCHEMA: &'static str = "https://opencode.ai/v2/cli.json";

    pub(crate) fn cli_config_path(env: &SlateEnv) -> PathBuf {
        env.xdg_config_home().join("opencode/cli.json")
    }

    pub(crate) fn cli_config_paths(env: &SlateEnv) -> Vec<PathBuf> {
        vec![Self::cli_config_path(env)]
    }

    /// Resolve the path to opencode's config directory.
    fn config_dir(env: &SlateEnv) -> PathBuf {
        env.xdg_config_home().join("opencode")
    }
}

impl ToolAdapter for OpencodeAdapter {
    fn tool_name(&self) -> &'static str {
        "opencode"
    }

    fn is_installed(&self) -> Result<bool> {
        Ok(detection::detect_tool_presence(self.tool_name()).installed)
    }

    fn is_installed_with_env(&self, env: &SlateEnv) -> Result<bool> {
        Ok(detection::detect_tool_presence_with_env(self.tool_name(), env).installed)
    }

    fn integration_config_path(&self) -> Result<PathBuf> {
        let env = SlateEnv::from_process()?;
        Ok(Self::cli_config_path(&env))
    }

    fn managed_config_path(&self) -> PathBuf {
        let env = SlateEnv::from_process().ok();
        if let Some(env) = env.as_ref() {
            Self::theme_path(env)
        } else {
            PathBuf::from(".config/opencode/themes/slate-sync.json")
        }
    }

    fn apply_strategy(&self) -> ApplyStrategy {
        ApplyStrategy::EditInPlace
    }

    fn apply_theme(&self, _theme: &ThemeVariant) -> Result<ApplyOutcome> {
        let env = SlateEnv::from_process()?;
        self.apply_theme_with_env(_theme, &env)
    }

    fn apply_theme_with_env(&self, theme: &ThemeVariant, env: &SlateEnv) -> Result<ApplyOutcome> {
        let config_path = Self::cli_config_path(env);

        // Avoid creating a default OpenCode config for users who have the
        // binary installed but have never initialized OpenCode's config dir.
        if matches!(fs::symlink_metadata(Self::config_dir(env)), Err(e) if e.kind() == std::io::ErrorKind::NotFound)
            && crate::config::file_read::confirm_missing(&Self::config_dir(env)).is_ok()
        {
            return Ok(ApplyOutcome::Skipped(SkipReason::MissingIntegrationConfig));
        }

        let theme_path = Self::theme_path(env);
        crate::config::recovery_paths::validate_file_path(env, &config_path, "OpenCode")?;
        crate::config::recovery_paths::validate_file_path(env, &theme_path, "OpenCode theme")?;
        let prepared = config::Prepared::read(&config_path)?;
        let asset = config::Prepared::asset(&theme_path, palette::render(theme)?.into_bytes())?;
        prepared.verify()?;
        asset.verify()?;
        if prepared.changed() {
            if let Some(original) = prepared.original_bytes() {
                ConfigManager::from_env_paths(env).backup_captured_file(&config_path, original)?;
            }
        }
        asset.publish()?;
        prepared.publish()?;

        // Theme files are read on the next OpenCode launch.
        // The coordinator reports app-specific reopening guidance separately.
        Ok(ApplyOutcome::applied_no_shell())
    }

    fn reload(&self) -> Result<()> {
        // Slate does not command a running OpenCode process.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name() {
        let adapter = OpencodeAdapter;
        assert_eq!(adapter.tool_name(), "opencode");
    }

    #[test]
    fn test_apply_strategy() {
        let adapter = OpencodeAdapter;
        assert_eq!(adapter.apply_strategy(), ApplyStrategy::EditInPlace);
    }

    #[test]
    fn test_config_dir() {
        let tempdir = tempfile::TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let path = OpencodeAdapter::config_dir(&env);
        assert!(path.ends_with("opencode"));
    }

    #[test]
    fn test_apply_theme_keeps_existing_theme_reference() {
        let tempdir = tempfile::TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());

        // Create config directory
        let config_dir = tempdir.path().join(".config/opencode");
        fs::create_dir_all(&config_dir).unwrap();

        // Create a connected CLI file; the palette may still need publishing.
        let cli_path = config_dir.join("cli.json");
        fs::write(&cli_path, r#"{"theme": {"name":"slate-sync"}}"#).unwrap();

        let adapter = OpencodeAdapter;
        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();
        let outcome = adapter.apply_theme_with_env(&theme, &env).unwrap();

        assert!(matches!(outcome, ApplyOutcome::Applied { .. }));
    }

    #[test]
    fn test_apply_theme_sets_slate_theme() {
        let tempdir = tempfile::TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());

        // Create config directory
        let config_dir = tempdir.path().join(".config/opencode");
        fs::create_dir_all(&config_dir).unwrap();

        // Create cli.json with different theme
        let cli_path = config_dir.join("cli.json");
        fs::write(&cli_path, r#"{"theme": {"name":"catppuccin-mocha"}}"#).unwrap();

        let adapter = OpencodeAdapter;
        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();
        let outcome = adapter.apply_theme_with_env(&theme, &env).unwrap();

        assert!(matches!(outcome, ApplyOutcome::Applied { .. }));

        // Verify theme was changed
        let content = fs::read_to_string(&cli_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["theme"]["name"], "slate-sync");
    }

    #[test]
    fn test_apply_theme_creates_cli_json_if_missing() {
        let tempdir = tempfile::TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());

        // Create config directory but no cli.json
        let config_dir = tempdir.path().join(".config/opencode");
        fs::create_dir_all(&config_dir).unwrap();

        let adapter = OpencodeAdapter;
        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();
        let outcome = adapter.apply_theme_with_env(&theme, &env).unwrap();

        assert!(matches!(outcome, ApplyOutcome::Applied { .. }));

        // Verify cli.json was created
        let cli_path = config_dir.join("cli.json");
        assert!(cli_path.exists());
        let content = fs::read_to_string(&cli_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["theme"]["name"], "slate-sync");
    }

    #[test]
    fn test_apply_theme_skips_if_no_config_dir() {
        let tempdir = tempfile::TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());

        // Don't create config directory

        let adapter = OpencodeAdapter;
        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();
        let outcome = adapter.apply_theme_with_env(&theme, &env).unwrap();

        assert!(matches!(outcome, ApplyOutcome::Skipped(_)));
    }

    #[test]
    fn test_managed_config_path() {
        let adapter = OpencodeAdapter;
        let path = adapter.managed_config_path();
        assert!(path
            .to_string_lossy()
            .ends_with("opencode/themes/slate-sync.json"));
    }
}
