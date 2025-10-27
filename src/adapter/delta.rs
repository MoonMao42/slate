use crate::adapter::ToolAdapter;
use crate::config::backup::create_backup;
use crate::error::{ThemeError, ThemeResult};
use crate::theme::Theme;
use atomic_write_file::AtomicWriteFile;
use regex::Regex;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

pub struct DeltaAdapter;

impl DeltaAdapter {
    /// Delta config lives at ~/.config/delta/config.gitconfig (XDG default)
    fn config_path_delta() -> ThemeResult<PathBuf> {
        let config_home = crate::adapter::xdg_config_home()?;
        Ok(config_home.join("delta").join("config.gitconfig"))
    }

    /// Managed include in ~/.gitconfig
    fn gitconfig_path() -> ThemeResult<PathBuf> {
        let home = std::env::var("HOME")
            .map_err(|_| ThemeError::Other("HOME not set".to_string()))?;
        Ok(PathBuf::from(home).join(".gitconfig"))
    }

    /// Build the managed include block that points to the delta config
    fn build_managed_block() -> String {
        "; -- START themectl managed block (do not edit) --\n\
         [include]\n\
         \tpath = ~/.config/delta/config.gitconfig\n\
         ; -- END themectl managed block --\n"
            .to_string()
    }

    /// Check if gitconfig contains delta-related config
    fn gitconfig_has_delta(content: &str) -> bool {
        content.contains("[delta]") || content.contains("path = ~/.config/delta/config.gitconfig")
    }

    /// Ensure delta config file parent directory exists
    fn ensure_delta_config_dir(config_path: &PathBuf) -> ThemeResult<()> {
        if let Some(parent) = config_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(|e| ThemeError::Io(e))?;
            }
        }
        Ok(())
    }

    /// Create a minimal delta config if it doesn't exist
    fn create_default_delta_config(config_path: &PathBuf) -> ThemeResult<()> {
        let default_config = "[delta]\n";
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

    /// Update gitconfig with managed include block
    fn update_gitconfig_with_include(gitconfig_path: &PathBuf) -> ThemeResult<()> {
        // Ensure parent directory exists
        if let Some(parent) = gitconfig_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(|e| ThemeError::Io(e))?;
            }
        }

        // Read current gitconfig if it exists
        let content = if gitconfig_path.exists() {
            fs::read_to_string(gitconfig_path).map_err(|e| ThemeError::Io(e))?
        } else {
            String::new()
        };

        // Build the managed block
        let managed_block = Self::build_managed_block();

        // Pattern to find existing managed block (multiline)
        let block_pattern = Regex::new(
            r#"(?m); -- START themectl managed block.*?; -- END themectl managed block--\n?"#,
        )
        .map_err(|e| {
            ThemeError::Other(format!("Invalid gitconfig block regex: {}", e))
        })?;

        // Replace or insert the managed block
        let new_content = if block_pattern.is_match(&content) {
            // Replace existing block
            block_pattern.replace(&content, managed_block.as_str()).to_string()
        } else {
            // Append new block at the end
            let mut result = content;
            if !result.is_empty() && !result.ends_with('\n') {
                result.push('\n');
            }
            result.push_str(&managed_block);
            result
        };

        // Atomic write gitconfig
        let mut file = AtomicWriteFile::open(gitconfig_path)
            .map_err(|e| ThemeError::WriteError {
                path: gitconfig_path.display().to_string(),
                reason: e.to_string(),
            })?;

        file.write_all(new_content.as_bytes())
            .map_err(|e| ThemeError::WriteError {
                path: gitconfig_path.display().to_string(),
                reason: e.to_string(),
            })?;

        file.commit().map_err(|e| ThemeError::WriteError {
            path: gitconfig_path.display().to_string(),
            reason: e.to_string(),
        })?;

        Ok(())
    }
}

impl ToolAdapter for DeltaAdapter {
    fn is_installed(&self) -> ThemeResult<bool> {
        // Check if delta binary exists
        let binary_exists = which::which("delta").is_ok();

        // Check if gitconfig references delta
        let gitconfig_has_delta = if let Ok(path) = Self::gitconfig_path() {
            if let Ok(content) = fs::read_to_string(&path) {
                Self::gitconfig_has_delta(&content)
            } else {
                false
            }
        } else {
            false
        };

        Ok(binary_exists || gitconfig_has_delta)
    }

    fn config_path(&self) -> ThemeResult<PathBuf> {
        Self::config_path_delta()
    }

    fn config_exists(&self) -> ThemeResult<bool> {
        let path = self.config_path()?;
        Ok(path.exists() && path.is_file())
    }

    fn apply_theme(&self, theme: &Theme) -> ThemeResult<()> {
        let delta_config_path = self.config_path()?;

        // Ensure the delta config directory exists
        Self::ensure_delta_config_dir(&delta_config_path)?;

        // If delta config doesn't exist, create a default one
        if !delta_config_path.exists() {
            Self::create_default_delta_config(&delta_config_path)?;
        }

        // Get canonical path (resolve symlinks)
        let canonical_path = fs::canonicalize(&delta_config_path)
            .map_err(|_e| ThemeError::SymlinkError {
                path: delta_config_path.display().to_string(),
            })?;

        // Create backup before modification (SAFE-04)
        let _backup_info = create_backup("delta", &theme.name, &canonical_path)?;

        // Read current delta config
        let content = fs::read_to_string(&canonical_path)
            .map_err(|e| ThemeError::Io(e))?;

        // Get the delta theme name from tool_overrides
        let delta_theme = theme
            .colors
            .tool_overrides
            .get("delta")
            .ok_or_else(|| {
                ThemeError::Other(format!("No delta theme override for {}", theme.name))
            })?
            .to_string();

        // Use regex to replace or create the syntax-theme line
        // Pattern: syntax-theme = "value" or syntax-theme = value (with optional spaces/quotes)
        let theme_pattern = Regex::new(
            r#"(?m)^\s*syntax-theme\s*=\s*(?:"[^"\n]*"|'[^'\n]*'|[^"'#\n]+)\s*$"#,
        )
        .map_err(|e| ThemeError::Other(format!("Invalid delta theme regex: {}", e)))?;

        let new_content = if theme_pattern.is_match(&content) {
            // Replace existing syntax-theme line
            theme_pattern
                .replace(&content, format!(r#"syntax-theme = "{}""#, delta_theme))
                .to_string()
        } else {
            // Create new syntax-theme line at the end of [delta] section or at end of file
            let mut new = content;

            // Ensure we have a [delta] section
            if !new.contains("[delta]") {
                // No [delta] section, append one
                if !new.ends_with('\n') {
                    new.push('\n');
                }
                new.push_str("[delta]\n");
            }

            // Add syntax-theme line at the end
            if !new.ends_with('\n') {
                new.push('\n');
            }
            new.push_str(&format!(r#"syntax-theme = "{}""#, delta_theme));
            new.push('\n');
            new
        };

        // Atomic write delta config
        let mut file = AtomicWriteFile::open(&canonical_path)
            .map_err(|e| ThemeError::WriteError {
                path: canonical_path.display().to_string(),
                reason: e.to_string(),
            })?;

        file.write_all(new_content.as_bytes())
            .map_err(|e| ThemeError::WriteError {
                path: canonical_path.display().to_string(),
                reason: e.to_string(),
            })?;

        file.commit().map_err(|e| ThemeError::WriteError {
            path: canonical_path.display().to_string(),
            reason: e.to_string(),
        })?;

        // Now update gitconfig with managed include block
        let gitconfig_path = Self::gitconfig_path()?;
        Self::update_gitconfig_with_include(&gitconfig_path)?;

        Ok(())
    }

    fn get_current_theme(&self) -> ThemeResult<Option<String>> {
        if !self.config_exists()? {
            return Ok(None);
        }

        let path = self.config_path()?;
        let content = fs::read_to_string(&path).map_err(|e| ThemeError::Io(e))?;

        let theme_pattern =
            Regex::new(r#"^\s*syntax-theme\s*=\s*(?:"([^"\n]*)"|'([^'\n]*)'|([^"'#\s\n]+))"#)
                .map_err(|e| ThemeError::Other(format!("Invalid delta read regex: {}", e)))?;

        if let Some(caps) = theme_pattern.captures(&content) {
            if let Some(theme_name) = caps
                .get(1)
                .or_else(|| caps.get(2))
                .or_else(|| caps.get(3))
            {
                return Ok(Some(theme_name.as_str().to_string()));
            }
        }

        Ok(None)
    }

    fn tool_name(&self) -> &'static str {
        "delta"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delta_tool_name() {
        let adapter = DeltaAdapter;
        assert_eq!(adapter.tool_name(), "delta");
    }

    #[test]
    fn test_delta_build_managed_block() {
        let block = DeltaAdapter::build_managed_block();
        assert!(block.contains("START themectl managed block"));
        assert!(block.contains("END themectl managed block"));
        assert!(block.contains("[include]"));
        assert!(block.contains("path = ~/.config/delta/config.gitconfig"));
    }

    #[test]
    fn test_delta_gitconfig_has_delta_with_section() {
        let content = "[user]\nname = Test\n[delta]\n";
        assert!(DeltaAdapter::gitconfig_has_delta(content));
    }

    #[test]
    fn test_delta_gitconfig_has_delta_with_path() {
        let content = "[user]\nname = Test\n[include]\npath = ~/.config/delta/config.gitconfig\n";
        assert!(DeltaAdapter::gitconfig_has_delta(content));
    }

    #[test]
    fn test_delta_gitconfig_no_delta() {
        let content = "[user]\nname = Test\n[core]\npager = less\n";
        assert!(!DeltaAdapter::gitconfig_has_delta(content));
    }

    #[test]
    fn test_delta_replace_existing_theme() {
        let content = "[delta]\nsyntax-theme = \"Dracula\"\n";

        let theme_pattern = Regex::new(
            r#"(?m)^\s*syntax-theme\s*=\s*(?:"[^"\n]*"|'[^'\n]*'|[^"'#\n]+)\s*$"#,
        )
        .unwrap();
        let new_content = theme_pattern
            .replace(content, r#"syntax-theme = "Catppuccin Mocha""#)
            .to_string();

        assert!(new_content.contains(r#"syntax-theme = "Catppuccin Mocha""#));
        assert!(!new_content.contains("Dracula"));
    }

    #[test]
    fn test_delta_theme_detection() {
        let content = r#"[delta]
syntax-theme = "Tokyo Night""#;

        let theme_pattern =
            Regex::new(r#"^\s*syntax-theme\s*=\s*(?:"([^"\n]*)"|'([^'\n]*)'|([^"'#\s\n]+))"#)
                .unwrap();

        if let Some(caps) = theme_pattern.captures(content) {
            if let Some(theme_name) = caps.get(1).or_else(|| caps.get(2)).or_else(|| caps.get(3)) {
                assert_eq!(theme_name.as_str(), "Tokyo Night");
            }
        }
    }

    #[test]
    fn test_delta_add_missing_theme() {
        let content = "[delta]\ncolor = true\n";

        let theme_pattern = Regex::new(
            r#"(?m)^\s*syntax-theme\s*=\s*(?:"[^"\n]*"|'[^'\n]*'|[^"'#\n]+)\s*$"#,
        )
        .unwrap();

        let new_content = if theme_pattern.is_match(content) {
            theme_pattern
                .replace(content, r#"syntax-theme = "Catppuccin Mocha""#)
                .to_string()
        } else {
            let mut new = content.to_string();
            if !new.ends_with('\n') {
                new.push('\n');
            }
            new.push_str(r#"syntax-theme = "Catppuccin Mocha""#);
            new.push('\n');
            new
        };

        assert!(new_content.contains(r#"syntax-theme = "Catppuccin Mocha""#));
    }

    #[test]
    fn test_gitconfig_managed_block_replacement() {
        let old_gitconfig = "[user]\nname = Test\n; -- START themectl managed block (do not edit) --\n[include]\npath = ~/.config/delta/config.gitconfig\n; -- END themectl managed block --\n[core]\npager = delta\n";
        let new_block = DeltaAdapter::build_managed_block();

        let block_pattern = Regex::new(
            r#"(?m); -- START themectl managed block.*?; -- END themectl managed block--\n?"#,
        )
        .unwrap();

        let new_gitconfig = block_pattern.replace(&old_gitconfig, new_block.as_str()).to_string();

        assert!(new_gitconfig.contains("START themectl managed block"));
        assert!(new_gitconfig.contains("END themectl managed block"));
        assert!(new_gitconfig.contains("[user]"));
        assert!(new_gitconfig.contains("[core]"));
    }

    #[test]
    fn test_gitconfig_managed_block_append() {
        let old_gitconfig = "[user]\nname = Test\n[core]\npager = delta\n";
        let new_block = DeltaAdapter::build_managed_block();

        let block_pattern = Regex::new(
            r#"(?m); -- START themectl managed block.*?; -- END themectl managed block--\n?"#,
        )
        .unwrap();

        let new_gitconfig = if block_pattern.is_match(&old_gitconfig) {
            block_pattern.replace(&old_gitconfig, new_block.as_str()).to_string()
        } else {
            let mut result = old_gitconfig.to_string();
            if !result.ends_with('\n') {
                result.push('\n');
            }
            result.push_str(&new_block);
            result
        };

        assert!(new_gitconfig.contains("START themectl managed block"));
        assert!(new_gitconfig.contains("END themectl managed block"));
        assert!(new_gitconfig.contains("[user]"));
    }
}
