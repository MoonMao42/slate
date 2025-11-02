use crate::adapter::ToolAdapter;
use crate::config::backup::{create_backup, create_backup_with_session, BackupSession};
use crate::error::{ThemeError, ThemeResult};
use crate::theme::Theme;
use atomic_write_file::AtomicWriteFile;
use regex::Regex;
use serde_yaml::{Mapping, Value};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

pub struct LazygitAdapter;

impl LazygitAdapter {
    /// Lazygit config lives at ~/.config/lazygit/config.yml (XDG default)
    fn config_path_impl() -> ThemeResult<PathBuf> {
        let config_home = crate::adapter::xdg_config_home()?;
        Ok(config_home.join("lazygit").join("config.yml"))
    }

    /// Ensure lazygit config directory exists
    fn ensure_config_dir(config_path: &PathBuf) -> ThemeResult<()> {
        if let Some(parent) = config_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(|e| ThemeError::Io(e))?;
            }
        }
        Ok(())
    }

    /// Create a minimal lazygit config if it doesn't exist
    fn create_default_config(config_path: &PathBuf) -> ThemeResult<()> {
        let default_config = "gui:\n";
        let mut file = AtomicWriteFile::open(config_path).map_err(|e| ThemeError::WriteError {
            path: config_path.display().to_string(),
            reason: e.to_string(),
        })?;

        file.write_all(default_config.as_bytes())
            .map_err(|e| ThemeError::WriteError {
                path: config_path.display().to_string(),
                reason: e.to_string(),
            })?;

        file.commit().map_err(|e| ThemeError::WriteError {
            path: config_path.display().to_string(),
            reason: e.to_string(),
        })?;

        Ok(())
    }

    /// Extract lazygit theme identifier from tool_overrides
    fn get_lazygit_theme_id(theme: &Theme) -> ThemeResult<String> {
        theme
            .colors
            .tool_overrides
            .get("lazygit")
            .ok_or_else(|| {
                ThemeError::Other(format!("No lazygit theme override for {}", theme.name))
            })
            .map(|s| s.to_string())
    }

    /// Apply theme to the gui.theme field in the YAML config
    fn apply_theme_to_yaml(
        config_path: &PathBuf,
        theme_id: &str,
        bat_theme: &str,
        delta_theme: &str,
    ) -> ThemeResult<()> {
        // Read existing config or start fresh
        let content = if config_path.exists() {
            fs::read_to_string(&config_path).map_err(|e| ThemeError::Io(e))?
        } else {
            "gui:\n".to_string()
        };

        let has_pager_integration = Self::has_pager_integration(&content);

        // Parse YAML
        let mut doc: Value = serde_yaml::from_str(&content).map_err(|e| {
            ThemeError::Other(format!("Failed to parse lazygit config YAML: {}", e))
        })?;

        // Ensure 'gui' is a mapping at the root
        if !doc.is_mapping() {
            doc = Value::Mapping(Mapping::new());
        }

        let gui_value = if let Some(gui) = doc.get_mut("gui") {
            gui
        } else {
            let mapping = doc
                .as_mapping_mut()
                .ok_or_else(|| ThemeError::Other("Failed to access root mapping".to_string()))?;
            mapping.insert(
                Value::String("gui".to_string()),
                Value::Mapping(Mapping::new()),
            );
            &mut doc["gui"]
        };

        // Ensure gui is a mapping
        if !gui_value.is_mapping() {
            *gui_value = Value::Mapping(Mapping::new());
        }

        // Update gui.theme
        gui_value["theme"] = Value::String(theme_id.to_string());

        if has_pager_integration {
            Self::sync_pager_commands(&mut doc, bat_theme, delta_theme, false)?;
        }

        // Serialize back to YAML
        let new_content = serde_yaml::to_string(&doc)
            .map_err(|e| ThemeError::Other(format!("Failed to serialize lazygit config: {}", e)))?;

        // Atomic write
        let mut file = AtomicWriteFile::open(&config_path).map_err(|e| ThemeError::WriteError {
            path: config_path.display().to_string(),
            reason: e.to_string(),
        })?;

        file.write_all(new_content.as_bytes())
            .map_err(|e| ThemeError::WriteError {
                path: config_path.display().to_string(),
                reason: e.to_string(),
            })?;

        file.commit().map_err(|e| ThemeError::WriteError {
            path: config_path.display().to_string(),
            reason: e.to_string(),
        })?;

        Ok(())
    }

    /// Check if existing config has a pager-related integration
    fn has_pager_integration(content: &str) -> bool {
        content.contains("pager") && (content.contains("bat") || content.contains("delta"))
    }

    fn sync_pager_command(
        command: &str,
        bat_theme: &str,
        delta_theme: &str,
    ) -> ThemeResult<String> {
        let bat_pattern = Regex::new(r#"--theme(?:=|\s+)(?:"[^"\n]*"|'[^'\n]*'|[^"'#\s\n]+)"#)
            .map_err(|e| ThemeError::Other(format!("Invalid built-in lazygit bat regex: {}", e)))?;
        let delta_pattern = Regex::new(
            r#"--syntax-theme(?:=|\s+)(?:"[^"\n]*"|'[^'\n]*'|[^"'#\s\n]+)"#,
        )
        .map_err(|e| ThemeError::Other(format!("Invalid built-in lazygit delta regex: {}", e)))?;

        let mut updated = command.to_string();

        if updated.contains("bat") {
            if bat_pattern.is_match(&updated) {
                updated = bat_pattern
                    .replace(&updated, format!(r#"--theme="{}""#, bat_theme))
                    .to_string();
            } else {
                updated.push_str(&format!(r#" --theme="{}""#, bat_theme));
            }
        }

        if updated.contains("delta") {
            if delta_pattern.is_match(&updated) {
                updated = delta_pattern
                    .replace(&updated, format!(r#"--syntax-theme="{}""#, delta_theme))
                    .to_string();
            } else {
                updated.push_str(&format!(r#" --syntax-theme="{}""#, delta_theme));
            }
        }

        Ok(updated)
    }

    fn sync_pager_commands(
        value: &mut Value,
        bat_theme: &str,
        delta_theme: &str,
        in_pager_context: bool,
    ) -> ThemeResult<bool> {
        match value {
            Value::Mapping(map) => {
                let mut changed = false;
                for (key, child) in map.iter_mut() {
                    let key_name = key.as_str().unwrap_or("");
                    let child_in_pager = in_pager_context || matches!(key_name, "pager" | "paging");

                    if matches!(key_name, "command" | "cmd" | "pager") {
                        if let Value::String(command) = child {
                            let updated =
                                Self::sync_pager_command(command, bat_theme, delta_theme)?;
                            if updated != *command {
                                *command = updated;
                                changed = true;
                            }
                            continue;
                        }
                    }

                    changed |=
                        Self::sync_pager_commands(child, bat_theme, delta_theme, child_in_pager)?;
                }
                Ok(changed)
            }
            Value::Sequence(seq) => {
                let mut changed = false;
                for child in seq.iter_mut() {
                    changed |=
                        Self::sync_pager_commands(child, bat_theme, delta_theme, in_pager_context)?;
                }
                Ok(changed)
            }
            Value::String(command) if in_pager_context => {
                let updated = Self::sync_pager_command(command, bat_theme, delta_theme)?;
                let changed = updated != *command;
                if changed {
                    *command = updated;
                }
                Ok(changed)
            }
            _ => Ok(false),
        }
    }

    fn get_tool_override(theme: &Theme, tool: &str) -> ThemeResult<String> {
        theme
            .colors
            .tool_overrides
            .get(tool)
            .ok_or_else(|| {
                ThemeError::Other(format!("No {} theme override for {}", tool, theme.name))
            })
            .map(|s| s.to_string())
    }
}

impl ToolAdapter for LazygitAdapter {
    fn is_installed(&self) -> ThemeResult<bool> {
        let binary_exists = which::which("lazygit").is_ok();

        let config_exists = match self.config_path() {
            Ok(path) => path.exists(),
            Err(_) => false,
        };

        Ok(binary_exists || config_exists)
    }

    fn config_path(&self) -> ThemeResult<PathBuf> {
        Self::config_path_impl()
    }

    fn config_exists(&self) -> ThemeResult<bool> {
        let path = self.config_path()?;
        Ok(path.exists() && path.is_file())
    }

    fn apply_theme(&self, theme: &Theme, session: Option<&BackupSession>) -> ThemeResult<()> {
        let config_path = self.config_path()?;

        Self::ensure_config_dir(&config_path)?;

        if !config_path.exists() {
            Self::create_default_config(&config_path)?;
        }

        let canonical_path =
            fs::canonicalize(&config_path).map_err(|_e| ThemeError::SymlinkError {
                path: config_path.display().to_string(),
            })?;

        if let Some(sess) = session {
            // Manifest-backed backup with persisted metadata
            let _restore_entry = create_backup_with_session("lazygit", "lazygit", sess, &canonical_path)?;
        } else {
            // Legacy backup without session
            let _backup_info = create_backup("lazygit", &theme.name, &canonical_path)?;
        }

        let theme_id = Self::get_lazygit_theme_id(theme)?;
        let bat_theme = Self::get_tool_override(theme, "bat")?;
        let delta_theme = Self::get_tool_override(theme, "delta")?;

        Self::apply_theme_to_yaml(&canonical_path, &theme_id, &bat_theme, &delta_theme)?;

        Ok(())
    }

    fn get_current_theme(&self) -> ThemeResult<Option<String>> {
        if !self.config_exists()? {
            return Ok(None);
        }

        let path = self.config_path()?;
        let content = fs::read_to_string(&path).map_err(|e| ThemeError::Io(e))?;

        let doc: Value = serde_yaml::from_str(&content)
            .map_err(|e| ThemeError::Other(format!("Failed to parse lazygit config: {}", e)))?;

        if let Some(gui) = doc.get("gui") {
            if let Some(theme_value) = gui.get("theme") {
                if let Some(theme_str) = theme_value.as_str() {
                    return Ok(Some(theme_str.to_string()));
                }
            }
        }

        Ok(None)
    }

    fn tool_name(&self) -> &'static str {
        "lazygit"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lazygit_tool_name() {
        let adapter = LazygitAdapter;
        assert_eq!(adapter.tool_name(), "lazygit");
    }

    #[test]
    fn test_lazygit_config_path_uses_xdg() {
        let path = LazygitAdapter::config_path_impl().unwrap();
        assert!(path.ends_with("lazygit/config.yml"));
    }

    #[test]
    fn test_lazygit_create_default_config() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("config.yml");

        LazygitAdapter::create_default_config(&config_path).unwrap();

        assert!(config_path.exists());
        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("gui:"));
    }

    #[test]
    fn test_lazygit_apply_theme_to_yaml_create_new() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("config.yml");

        LazygitAdapter::create_default_config(&config_path).unwrap();

        LazygitAdapter::apply_theme_to_yaml(&config_path, "dracula", "Dracula", "Dracula").unwrap();

        let content = fs::read_to_string(&config_path).unwrap();
        let doc: Value = serde_yaml::from_str(&content).unwrap();

        assert_eq!(doc["gui"]["theme"], Value::String("dracula".to_string()));
    }

    #[test]
    fn test_lazygit_apply_theme_to_yaml_update_existing() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("config.yml");

        let initial_yaml = "gui:\n  theme: \"light\"\n  nerdFontsVersion: \"3\"\n";
        fs::write(&config_path, initial_yaml).unwrap();

        LazygitAdapter::apply_theme_to_yaml(
            &config_path,
            "catppuccin-mocha",
            "Catppuccin Mocha",
            "Catppuccin Mocha",
        )
        .unwrap();

        let content = fs::read_to_string(&config_path).unwrap();
        let doc: Value = serde_yaml::from_str(&content).unwrap();

        assert_eq!(
            doc["gui"]["theme"],
            Value::String("catppuccin-mocha".to_string())
        );
        assert_eq!(
            doc["gui"]["nerdFontsVersion"],
            Value::String("3".to_string())
        );
    }

    #[test]
    fn test_lazygit_has_pager_integration_with_bat() {
        let content = "[pager]\ncommand = bat --style=plain\n";
        assert!(LazygitAdapter::has_pager_integration(content));
    }

    #[test]
    fn test_lazygit_has_pager_integration_with_delta() {
        let content = "pager:\n  command: delta\n";
        assert!(LazygitAdapter::has_pager_integration(content));
    }

    #[test]
    fn test_lazygit_has_pager_integration_missing() {
        let content = "gui:\n  theme: dracula\n  nerdFontsVersion: 3\n";
        assert!(!LazygitAdapter::has_pager_integration(content));
    }

    #[test]
    fn test_lazygit_parse_yaml_and_extract_theme() {
        let content = "gui:\n  theme: tokyo-night\n  nerdFontsVersion: \"3\"\n";
        let doc: Value = serde_yaml::from_str(content).unwrap();

        if let Some(gui) = doc.get("gui") {
            if let Some(theme_value) = gui.get("theme") {
                assert_eq!(theme_value.as_str(), Some("tokyo-night"));
            }
        }
    }

    #[test]
    fn test_lazygit_preserve_other_gui_settings() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("config.yml");

        let initial_yaml =
            "gui:\n  theme: old-theme\n  nerdFontsVersion: \"3\"\n  window:\n    width: 200\n";
        fs::write(&config_path, initial_yaml).unwrap();

        LazygitAdapter::apply_theme_to_yaml(&config_path, "new-theme", "Dracula", "Dracula")
            .unwrap();

        let content = fs::read_to_string(&config_path).unwrap();
        let doc: Value = serde_yaml::from_str(&content).unwrap();

        assert_eq!(doc["gui"]["theme"], Value::String("new-theme".to_string()));
        assert_eq!(
            doc["gui"]["nerdFontsVersion"],
            Value::String("3".to_string())
        );
        assert_eq!(doc["gui"]["window"]["width"], Value::Number(200.into()));
    }

    #[test]
    fn test_lazygit_minimal_yaml_initialization() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("config.yml");

        LazygitAdapter::create_default_config(&config_path).unwrap();

        let content = fs::read_to_string(&config_path).unwrap();
        let result: Result<Value, _> = serde_yaml::from_str(&content);
        assert!(result.is_ok());
    }

    #[test]
    fn test_lazygit_syncs_existing_delta_pager_theme() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("config.yml");

        let initial_yaml = "gui:\n  theme: old-theme\ngit:\n  paging:\n    pager: delta --paging=never --syntax-theme=\"Dracula\"\n";
        fs::write(&config_path, initial_yaml).unwrap();

        LazygitAdapter::apply_theme_to_yaml(
            &config_path,
            "catppuccin-mocha",
            "Catppuccin Mocha",
            "Catppuccin Mocha",
        )
        .unwrap();

        let content = fs::read_to_string(&config_path).unwrap();
        let doc: Value = serde_yaml::from_str(&content).unwrap();

        assert_eq!(
            doc["gui"]["theme"],
            Value::String("catppuccin-mocha".to_string())
        );
        assert_eq!(
            doc["git"]["paging"]["pager"],
            Value::String(r#"delta --paging=never --syntax-theme="Catppuccin Mocha""#.to_string())
        );
    }

    #[test]
    fn test_lazygit_syncs_existing_bat_pager_theme() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("config.yml");

        let initial_yaml =
            "gui:\n  theme: old-theme\ngit:\n  paging:\n    pager: bat --style=plain\n";
        fs::write(&config_path, initial_yaml).unwrap();

        LazygitAdapter::apply_theme_to_yaml(&config_path, "nord", "Nord", "Nord").unwrap();

        let content = fs::read_to_string(&config_path).unwrap();
        let doc: Value = serde_yaml::from_str(&content).unwrap();

        assert_eq!(doc["gui"]["theme"], Value::String("nord".to_string()));
        assert_eq!(
            doc["git"]["paging"]["pager"],
            Value::String(r#"bat --style=plain --theme="Nord""#.to_string())
        );
    }
}
