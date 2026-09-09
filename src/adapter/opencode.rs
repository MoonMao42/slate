//! OpenCode adapter with EditInPlace strategy.
//! Detects opencode installation and configures transparent background
//! for Ghostty terminal by setting theme to "system" in tui.json.

use crate::adapter::{ApplyOutcome, ApplyStrategy, SkipReason, ToolAdapter};
use crate::config::ConfigManager;
use crate::detection;
use crate::env::SlateEnv;
use crate::error::Result;
use crate::theme::ThemeVariant;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) mod config;

/// OpenCode adapter implementing the ToolAdapter trait.
pub struct OpencodeAdapter;

impl OpencodeAdapter {
    pub(crate) const TUI_SCHEMA: &'static str = "https://opencode.ai/tui.json";

    /// Resolve the path to opencode's tui.json config file.
    pub(crate) fn tui_config_path(env: &SlateEnv) -> PathBuf {
        Self::tui_config_path_with_override(env, env.opencode_tui_config())
    }

    pub(crate) fn tui_config_path_with_override(
        env: &SlateEnv,
        override_path: Option<&Path>,
    ) -> PathBuf {
        if let Some(path) = override_path {
            return path.to_owned();
        }

        let json_path = Self::default_tui_json_path(env);
        let jsonc_path = Self::default_tui_jsonc_path(env);
        // A blocked preferred path must be reported, not silently bypassed.
        let genuinely_missing = |path: &std::path::Path| {
            matches!(fs::symlink_metadata(path), Err(e) if e.kind() == std::io::ErrorKind::NotFound)
                && crate::config::file_read::confirm_missing(path).is_ok()
        };
        if !genuinely_missing(&json_path) || genuinely_missing(&jsonc_path) {
            json_path
        } else {
            jsonc_path
        }
    }

    pub(crate) fn tui_config_paths(env: &SlateEnv) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        let mut destinations = std::collections::BTreeSet::new();
        for path in env
            .opencode_tui_config()
            .map(Path::to_owned)
            .into_iter()
            .chain([
                Self::default_tui_json_path(env),
                Self::default_tui_jsonc_path(env),
            ])
        {
            let destination = crate::config::file_read::directory_alias_target(&path)
                .unwrap_or_else(|| path.clone());
            if !paths.contains(&path) && destinations.insert(destination) {
                paths.push(path);
            }
        }
        paths
    }

    fn default_tui_json_path(env: &SlateEnv) -> PathBuf {
        env.xdg_config_home().join("opencode").join("tui.json")
    }

    fn default_tui_jsonc_path(env: &SlateEnv) -> PathBuf {
        env.xdg_config_home().join("opencode").join("tui.jsonc")
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
        Ok(Self::tui_config_path(&env))
    }

    fn managed_config_path(&self) -> PathBuf {
        let env = SlateEnv::from_process().ok();
        if let Some(env) = env.as_ref() {
            env.config_dir().join("managed").join("opencode")
        } else {
            PathBuf::from(".config/slate/managed/opencode")
        }
    }

    fn apply_strategy(&self) -> ApplyStrategy {
        ApplyStrategy::EditInPlace
    }

    fn apply_theme(&self, _theme: &ThemeVariant) -> Result<ApplyOutcome> {
        let env = SlateEnv::from_process()?;
        self.apply_theme_with_env(_theme, &env)
    }

    fn apply_theme_with_env(&self, _theme: &ThemeVariant, env: &SlateEnv) -> Result<ApplyOutcome> {
        env.validate_opencode_tui_config()?;
        let override_path = env.opencode_tui_config();
        let config_path = Self::tui_config_path_with_override(env, override_path);

        // Avoid creating a default OpenCode config for users who have the
        // binary installed but have never initialized OpenCode's config dir.
        if override_path.is_none()
            && matches!(fs::symlink_metadata(Self::config_dir(env)), Err(e) if e.kind() == std::io::ErrorKind::NotFound)
            && crate::config::file_read::confirm_missing(&Self::config_dir(env)).is_ok()
        {
            return Ok(ApplyOutcome::Skipped(SkipReason::MissingIntegrationConfig));
        }

        // Prepare and validate before backup/cache creation. Already connected
        // documents are byte-for-byte no-ops, including their missing schema.
        let prepared = config::Prepared::read(&config_path)?;
        if !prepared.changed() {
            return Ok(ApplyOutcome::Applied {
                requires_new_shell: false,
            });
        }
        prepared.verify()?;
        if let Some(original) = prepared.original_bytes() {
            ConfigManager::from_env_paths(env).backup_captured_file(&config_path, original)?;
        }
        prepared.publish()?;

        // Only the TUI file changed, not shell initialization or environment.
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
    fn opencode_candidates_deduplicate_directory_aliases_but_keep_final_links() {
        use std::os::unix::fs::symlink;
        for case in ["missing", "present", "final-link"] {
            let td = tempfile::tempdir().unwrap();
            let config = td.path().join(".config/opencode");
            fs::create_dir_all(&config).unwrap();
            let selected = if case == "final-link" {
                fs::write(config.join("tui.json"), "{}").unwrap();
                let link = td.path().join("linked.json");
                symlink(config.join("tui.json"), &link).unwrap();
                link
            } else {
                symlink(&config, td.path().join("alias")).unwrap();
                if case == "present" {
                    fs::write(config.join("tui.json"), "{}").unwrap();
                }
                td.path().join("alias/tui.json")
            };
            let env = SlateEnv::from_vars(|key| match key {
                "HOME" => Some(td.path().as_os_str().to_owned()),
                "OPENCODE_TUI_CONFIG" => Some(selected.as_os_str().to_owned()),
                _ => None,
            })
            .unwrap();
            let paths = OpencodeAdapter::tui_config_paths(&env);
            assert_eq!(paths.len(), if case == "final-link" { 3 } else { 2 });
            assert_eq!(paths[0], selected);
        }
    }

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
    fn test_tui_config_path() {
        let tempdir = tempfile::TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let path = OpencodeAdapter::tui_config_path_with_override(&env, None);
        assert!(path.ends_with("opencode/tui.json"));
    }

    #[test]
    fn test_tui_config_path_prefers_existing_jsonc_when_json_missing() {
        let tempdir = tempfile::TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let jsonc_path = tempdir.path().join(".config/opencode/tui.jsonc");
        fs::create_dir_all(jsonc_path.parent().unwrap()).unwrap();
        fs::write(&jsonc_path, "{}").unwrap();

        let path = OpencodeAdapter::tui_config_path_with_override(&env, None);
        assert!(path.ends_with("opencode/tui.jsonc"));
    }

    #[test]
    fn test_tui_config_path_honors_override() {
        let tempdir = tempfile::TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let override_path = tempdir.path().join("custom/tui.jsonc");

        let path = OpencodeAdapter::tui_config_path_with_override(&env, Some(&override_path));
        assert_eq!(path, override_path);
    }

    #[test]
    fn test_config_dir() {
        let tempdir = tempfile::TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let path = OpencodeAdapter::config_dir(&env);
        assert!(path.ends_with("opencode"));
    }

    #[test]
    fn test_apply_theme_skips_if_already_system() {
        let tempdir = tempfile::TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());

        // Create config directory
        let config_dir = tempdir.path().join(".config/opencode");
        fs::create_dir_all(&config_dir).unwrap();

        // Create tui.json with theme already set to system
        let tui_path = config_dir.join("tui.json");
        fs::write(&tui_path, r#"{"theme": "system"}"#).unwrap();

        let adapter = OpencodeAdapter;
        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();
        let outcome = adapter.apply_theme_with_env(&theme, &env).unwrap();

        assert!(matches!(outcome, ApplyOutcome::Applied { .. }));
    }

    #[test]
    fn test_apply_theme_sets_system_theme() {
        let tempdir = tempfile::TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());

        // Create config directory
        let config_dir = tempdir.path().join(".config/opencode");
        fs::create_dir_all(&config_dir).unwrap();

        // Create tui.json with different theme
        let tui_path = config_dir.join("tui.json");
        fs::write(&tui_path, r#"{"theme": "catppuccin-mocha"}"#).unwrap();

        let adapter = OpencodeAdapter;
        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();
        let outcome = adapter.apply_theme_with_env(&theme, &env).unwrap();

        assert!(matches!(outcome, ApplyOutcome::Applied { .. }));

        // Verify theme was changed
        let content = fs::read_to_string(&tui_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["theme"], "system");
    }

    #[test]
    fn test_apply_theme_creates_tui_json_if_missing() {
        let tempdir = tempfile::TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());

        // Create config directory but no tui.json
        let config_dir = tempdir.path().join(".config/opencode");
        fs::create_dir_all(&config_dir).unwrap();

        let adapter = OpencodeAdapter;
        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();
        let outcome = adapter.apply_theme_with_env(&theme, &env).unwrap();

        assert!(matches!(outcome, ApplyOutcome::Applied { .. }));

        // Verify tui.json was created
        let tui_path = config_dir.join("tui.json");
        assert!(tui_path.exists());
        let content = fs::read_to_string(&tui_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["theme"], "system");
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
        assert!(path.to_string_lossy().contains("managed/opencode"));
    }
}
