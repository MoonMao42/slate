//! OpenCode adapter with EditInPlace strategy.
//! Detects opencode installation and configures transparent background
//! for Ghostty terminal by setting theme to "system" in tui.json.

use crate::adapter::{ApplyOutcome, ApplyStrategy, SkipReason, ToolAdapter};
use crate::config::ConfigManager;
use crate::detection;
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use crate::theme::ThemeVariant;
use std::fs;
use std::path::{Path, PathBuf};

/// OpenCode adapter implementing the ToolAdapter trait.
pub struct OpencodeAdapter;

impl OpencodeAdapter {
    pub(crate) const TUI_SCHEMA: &'static str = "https://opencode.ai/tui.json";

    fn process_tui_config_override_for_env(env: &SlateEnv) -> Option<String> {
        let process_env = SlateEnv::from_process().ok()?;
        if process_env.home() != env.home() {
            return None;
        }

        std::env::var("OPENCODE_TUI_CONFIG")
            .ok()
            .filter(|value| !value.trim().is_empty())
    }

    /// Resolve the path to opencode's tui.json config file.
    pub(crate) fn tui_config_path(env: &SlateEnv) -> PathBuf {
        let override_path = Self::process_tui_config_override_for_env(env);
        Self::tui_config_path_with_override(env, override_path.as_deref())
    }

    pub(crate) fn tui_config_path_with_override(
        env: &SlateEnv,
        override_path: Option<&str>,
    ) -> PathBuf {
        if let Some(path) = override_path.filter(|value| !value.trim().is_empty()) {
            return PathBuf::from(path);
        }

        let json_path = Self::default_tui_json_path(env);
        let jsonc_path = Self::default_tui_jsonc_path(env);
        if json_path.exists() || !jsonc_path.exists() {
            json_path
        } else {
            jsonc_path
        }
    }

    pub(crate) fn tui_config_paths(env: &SlateEnv) -> Vec<PathBuf> {
        let override_path = Self::process_tui_config_override_for_env(env);
        let mut paths = Vec::new();
        if let Some(path) = override_path.as_deref() {
            paths.push(PathBuf::from(path));
        }

        for path in [
            Self::default_tui_json_path(env),
            Self::default_tui_jsonc_path(env),
        ] {
            if !paths.iter().any(|existing| existing == &path) {
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

    fn strip_jsonc_comments(content: &str) -> String {
        let mut output = String::with_capacity(content.len());
        let mut chars = content.chars().peekable();
        let mut in_string = false;
        let mut escape = false;
        let mut in_line_comment = false;
        let mut in_block_comment = false;

        while let Some(ch) = chars.next() {
            if in_line_comment {
                if ch == '\n' {
                    in_line_comment = false;
                    output.push(ch);
                }
                continue;
            }

            if in_block_comment {
                if ch == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    in_block_comment = false;
                }
                continue;
            }

            if in_string {
                output.push(ch);
                if escape {
                    escape = false;
                } else if ch == '\\' {
                    escape = true;
                } else if ch == '"' {
                    in_string = false;
                }
                continue;
            }

            if ch == '"' {
                in_string = true;
                output.push(ch);
            } else if ch == '/' && chars.peek() == Some(&'/') {
                chars.next();
                in_line_comment = true;
            } else if ch == '/' && chars.peek() == Some(&'*') {
                chars.next();
                in_block_comment = true;
            } else {
                output.push(ch);
            }
        }

        output
    }

    fn remove_jsonc_trailing_commas(content: &str) -> String {
        let chars: Vec<char> = content.chars().collect();
        let mut output = String::with_capacity(content.len());
        let mut i = 0;
        let mut in_string = false;
        let mut escape = false;

        while i < chars.len() {
            let ch = chars[i];

            if in_string {
                output.push(ch);
                if escape {
                    escape = false;
                } else if ch == '\\' {
                    escape = true;
                } else if ch == '"' {
                    in_string = false;
                }
                i += 1;
                continue;
            }

            if ch == '"' {
                in_string = true;
                output.push(ch);
                i += 1;
                continue;
            }

            if ch == ',' {
                let mut j = i + 1;
                while j < chars.len() && chars[j].is_whitespace() {
                    j += 1;
                }
                if j < chars.len() && (chars[j] == '}' || chars[j] == ']') {
                    i += 1;
                    continue;
                }
            }

            output.push(ch);
            i += 1;
        }

        output
    }

    pub(crate) fn parse_tui_config(content: &str, path: &Path) -> Result<serde_json::Value> {
        match serde_json::from_str(content) {
            Ok(value) => Ok(value),
            Err(json_err) => {
                let jsonc =
                    Self::remove_jsonc_trailing_commas(&Self::strip_jsonc_comments(content));
                serde_json::from_str(&jsonc).map_err(|jsonc_err| {
                    SlateError::ConfigReadError(
                        path.display().to_string(),
                        format!("invalid JSON/JSONC: {}; {}", json_err, jsonc_err),
                    )
                })
            }
        }
    }

    /// Read existing tui.json or create a default one.
    /// Returns the parsed JSON value.
    pub(crate) fn read_or_create_tui_config(path: &Path) -> Result<serde_json::Value> {
        if path.exists() {
            let content = fs::read_to_string(path).map_err(|e| {
                SlateError::ConfigReadError(path.display().to_string(), e.to_string())
            })?;
            let value = Self::parse_tui_config(&content, path)?;
            if !value.is_object() {
                return Err(SlateError::InvalidConfig(format!(
                    "OpenCode TUI config at {} must be a JSON object",
                    path.display()
                )));
            }
            Ok(value)
        } else {
            Ok(serde_json::json!({}))
        }
    }

    /// Write the tui.json config with theme set to "system".
    pub(crate) fn write_tui_config(path: &Path, mut config: serde_json::Value) -> Result<()> {
        if !config.is_object() {
            return Err(SlateError::InvalidConfig(format!(
                "OpenCode TUI config at {} must be a JSON object",
                path.display()
            )));
        }

        // Set theme to "system" for transparent background support
        config["theme"] = serde_json::json!("system");

        // Add schema reference if not present
        if config.get("$schema").is_none() {
            config["$schema"] = serde_json::json!(Self::TUI_SCHEMA);
        }

        let content = serde_json::to_string_pretty(&config).map_err(|e| {
            SlateError::ConfigWriteError(path.display().to_string(), format!("JSON error: {}", e))
        })?;

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                SlateError::ConfigWriteError(
                    path.display().to_string(),
                    format!("failed to create directory: {}", e),
                )
            })?;
        }

        crate::config::atomic_write_synced(path, content.as_bytes())
            .map_err(|e| SlateError::ConfigWriteError(path.display().to_string(), e.to_string()))?;

        Ok(())
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
        let override_path = Self::process_tui_config_override_for_env(env);
        let config_path = Self::tui_config_path_with_override(env, override_path.as_deref());

        // Avoid creating a default OpenCode config for users who have the
        // binary installed but have never initialized OpenCode's config dir.
        if override_path.is_none() && !Self::config_dir(env).exists() {
            return Ok(ApplyOutcome::Skipped(SkipReason::MissingIntegrationConfig));
        }

        // Backup before modification
        let config_manager = ConfigManager::with_env(env)?;
        if config_path.exists() {
            let _backup_path = config_manager.backup_file(&config_path)?;
        }

        // Read existing config or create new one
        let config = Self::read_or_create_tui_config(&config_path)?;

        // Check if already set to system theme
        if let Some(theme) = config.get("theme").and_then(|v| v.as_str()) {
            if theme == "system" {
                return Ok(ApplyOutcome::Applied {
                    requires_new_shell: false,
                });
            }
        }

        // Write updated config
        Self::write_tui_config(&config_path, config)?;

        // OpenCode doesn't support hot-reload, needs restart
        Ok(ApplyOutcome::applied_needs_new_shell())
    }

    fn reload(&self) -> Result<()> {
        // OpenCode doesn't support hot-reload
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

        let path = OpencodeAdapter::tui_config_path_with_override(
            &env,
            Some(override_path.to_str().unwrap()),
        );
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
    fn test_read_or_create_tui_config_new() {
        let tempdir = tempfile::TempDir::new().unwrap();
        let path = tempdir.path().join("tui.json");
        let config = OpencodeAdapter::read_or_create_tui_config(&path).unwrap();
        assert!(config.is_object());
        assert!(config.as_object().unwrap().is_empty());
    }

    #[test]
    fn test_read_or_create_tui_config_existing() {
        let tempdir = tempfile::TempDir::new().unwrap();
        let path = tempdir.path().join("tui.json");
        fs::write(&path, r#"{"scroll_speed": 5}"#).unwrap();
        let config = OpencodeAdapter::read_or_create_tui_config(&path).unwrap();
        assert_eq!(config["scroll_speed"], 5);
    }

    #[test]
    fn test_read_or_create_tui_config_accepts_jsonc() {
        let tempdir = tempfile::TempDir::new().unwrap();
        let path = tempdir.path().join("tui.jsonc");
        fs::write(
            &path,
            r#"{
                // user comment
                "scroll_speed": 5,
            }"#,
        )
        .unwrap();
        let config = OpencodeAdapter::read_or_create_tui_config(&path).unwrap();
        assert_eq!(config["scroll_speed"], 5);
    }

    #[test]
    fn test_write_tui_config() {
        let tempdir = tempfile::TempDir::new().unwrap();
        let path = tempdir.path().join("tui.json");
        let config = serde_json::json!({});
        OpencodeAdapter::write_tui_config(&path, config).unwrap();

        let content = fs::read_to_string(&path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["theme"], "system");
        assert_eq!(parsed["$schema"], "https://opencode.ai/tui.json");
    }

    #[test]
    fn test_write_tui_config_preserves_existing() {
        let tempdir = tempfile::TempDir::new().unwrap();
        let path = tempdir.path().join("tui.json");
        let config = serde_json::json!({
            "scroll_speed": 5,
            "mouse": false
        });
        OpencodeAdapter::write_tui_config(&path, config).unwrap();

        let content = fs::read_to_string(&path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["theme"], "system");
        assert_eq!(parsed["scroll_speed"], 5);
        assert_eq!(parsed["mouse"], false);
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
