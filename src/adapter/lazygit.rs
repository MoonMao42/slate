use crate::adapter::ToolAdapter;
use crate::config::backup::create_backup;
use crate::error::{ThemeError, ThemeResult};
use crate::theme::Theme;
use atomic_write_file::AtomicWriteFile;
use serde_yaml::{Value, Mapping};
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
    ) -> ThemeResult<()> {
        // Read existing config or start fresh
        let content = if config_path.exists() {
            fs::read_to_string(&config_path)
                .map_err(|e| ThemeError::Io(e))?
        } else {
            "gui:\n".to_string()
        };

        // Parse YAML
        let mut doc: Value = serde_yaml::from_str(&content)
            .map_err(|e| ThemeError::Other(format!("Failed to parse lazygit config YAML: {}", e)))?;

        // Ensure 'gui' is a mapping at the root
        if !doc.is_mapping() {
            doc = Value::Mapping(Mapping::new());
        }

        let gui_value = if let Some(gui) = doc.get_mut("gui") {
            gui
        } else {
            let mapping = doc.as_mapping_mut()
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

        // Serialize back to YAML
        let new_content = serde_yaml::to_string(&doc)
            .map_err(|e| ThemeError::Other(format!("Failed to serialize lazygit config: {}", e)))?;

        // Atomic write
        let mut file = AtomicWriteFile::open(&config_path)
            .map_err(|e| ThemeError::WriteError {
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

    fn apply_theme(&self, theme: &Theme) -> ThemeResult<()> {
        let config_path = self.config_path()?;

        Self::ensure_config_dir(&config_path)?;

        if !config_path.exists() {
            Self::create_default_config(&config_path)?;
        }

        let canonical_path = fs::canonicalize(&config_path)
            .map_err(|_e| ThemeError::SymlinkError {
                path: config_path.display().to_string(),
            })?;

        let _backup_info = create_backup("lazygit", &theme.name, &canonical_path)?;

        let theme_id = Self::get_lazygit_theme_id(theme)?;

        let current_content = fs::read_to_string(&canonical_path)
            .map_err(|e| ThemeError::Io(e))?;

        let _has_pager = Self::has_pager_integration(&current_content);

        Self::apply_theme_to_yaml(&canonical_path, &theme_id)?;

        Ok(())
    }

    fn get_current_theme(&self) -> ThemeResult<Option<String>> {
        if !self.config_exists()? {
            return Ok(None);
        }

        let path = self.config_path()?;
        let content = fs::read_to_string(&path)
            .map_err(|e| ThemeError::Io(e))?;

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

        LazygitAdapter::apply_theme_to_yaml(&config_path, "dracula").unwrap();

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

        LazygitAdapter::apply_theme_to_yaml(&config_path, "catppuccin-mocha").unwrap();

        let content = fs::read_to_string(&config_path).unwrap();
        let doc: Value = serde_yaml::from_str(&content).unwrap();

        assert_eq!(doc["gui"]["theme"], Value::String("catppuccin-mocha".to_string()));
        assert_eq!(doc["gui"]["nerdFontsVersion"], Value::String("3".to_string()));
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

        let initial_yaml = "gui:\n  theme: old-theme\n  nerdFontsVersion: \"3\"\n  window:\n    width: 200\n";
        fs::write(&config_path, initial_yaml).unwrap();

        LazygitAdapter::apply_theme_to_yaml(&config_path, "new-theme").unwrap();

        let content = fs::read_to_string(&config_path).unwrap();
        let doc: Value = serde_yaml::from_str(&content).unwrap();

        assert_eq!(doc["gui"]["theme"], Value::String("new-theme".to_string()));
        assert_eq!(doc["gui"]["nerdFontsVersion"], Value::String("3".to_string()));
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
}
