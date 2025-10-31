use crate::adapter::ToolAdapter;
use crate::config::backup::create_backup;
use crate::error::{ThemeError, ThemeResult};
use crate::theme::Theme;
use atomic_write_file::AtomicWriteFile;
use regex::Regex;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

pub struct BatAdapter;

impl BatAdapter {
    /// Pure path resolution: BAT_CONFIG_PATH → BAT_CONFIG_DIR/config → XDG default (no global state)
    fn resolve_path(
        config_path: Option<&str>,
        config_dir: Option<&str>,
        config_home: &std::path::Path,
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
    fn is_installed(&self) -> ThemeResult<bool> {
        // Check if binary exists in PATH
        let binary_exists = which::which("bat").is_ok();

        // Check if config file exists
        let config_exists = match self.config_path() {
            Ok(path) => path.exists(),
            Err(_) => false,
        };

        // Tool is installed if either binary OR config exists
        Ok(binary_exists || config_exists)
    }

    fn config_path(&self) -> ThemeResult<PathBuf> {
        let config_home = crate::adapter::xdg_config_home()?;
        Ok(Self::resolve_path(
            std::env::var("BAT_CONFIG_PATH").ok().as_deref(),
            std::env::var("BAT_CONFIG_DIR").ok().as_deref(),
            &config_home,
        ))
    }

    fn config_exists(&self) -> ThemeResult<bool> {
        let path = self.config_path()?;
        Ok(path.exists() && path.is_file())
    }

    fn apply_theme(&self, theme: &Theme) -> ThemeResult<()> {
        // Get canonical path (resolve symlinks)
        let config_path = self.config_path()?;
        let canonical_path =
            fs::canonicalize(&config_path).map_err(|_e| ThemeError::SymlinkError {
                path: config_path.display().to_string(),
            })?;

        // Create backup before modification (SAFE-04)
        let _backup_info = create_backup("bat", &theme.name, &canonical_path)?;

        // Read current config
        let content = fs::read_to_string(&canonical_path).map_err(|e| ThemeError::Io(e))?;

        // Get the bat theme name from tool_overrides
        let bat_theme = theme
            .colors
            .tool_overrides
            .get("bat")
            .ok_or_else(|| ThemeError::Other(format!("No bat theme override for {}", theme.name)))?
            .to_string();

        // Use regex to replace or create the --theme flag
        // Use (?m) for multiline mode so ^ and $ match line boundaries
        let theme_pattern = Regex::new(
            r#"(?m)^\s*--theme\s*=\s*(?:"[^"\n]*"|'[^'\n]*'|[^"'#\n]+)\s*$"#,
        )
        .map_err(|e| ThemeError::Other(format!("Invalid built-in bat theme regex: {}", e)))?;

        let new_content = if theme_pattern.is_match(&content) {
            // Replace existing --theme line
            theme_pattern
                .replace(&content, format!(r#"--theme="{}""#, bat_theme))
                .to_string()
        } else {
            // Create new --theme line at the end
            let mut new = content;
            if !new.ends_with('\n') {
                new.push('\n');
            }
            new.push_str(&format!(r#"--theme="{}""#, bat_theme));
            new.push('\n');
            new
        };

        // Atomic write
        let mut file =
            AtomicWriteFile::open(&canonical_path).map_err(|e| ThemeError::WriteError {
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

        Ok(())
    }

    fn get_current_theme(&self) -> ThemeResult<Option<String>> {
        if !self.config_exists()? {
            return Ok(None);
        }

        let path = self.config_path()?;
        let content = fs::read_to_string(&path).map_err(|e| ThemeError::Io(e))?;

        let theme_pattern = Regex::new(
            r#"^\s*--theme\s*=\s*(?:"([^"\n]*)"|'([^'\n]*)'|([^"'#\s\n]+))"#,
        )
        .map_err(|e| ThemeError::Other(format!("Invalid built-in bat read regex: {}", e)))?;

        if let Some(caps) = theme_pattern.captures(&content) {
            if let Some(theme_name) = caps.get(1).or_else(|| caps.get(2)).or_else(|| caps.get(3)) {
                return Ok(Some(theme_name.as_str().to_string()));
            }
        }

        Ok(None)
    }

    fn tool_name(&self) -> &'static str {
        "bat"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bat_tool_name() {
        let adapter = BatAdapter;
        assert_eq!(adapter.tool_name(), "bat");
    }

    #[test]
    fn test_bat_resolve_path_config_path_override() {
        let config_home = PathBuf::from("/home/user/.config");
        assert_eq!(
            BatAdapter::resolve_path(Some("/custom/bat-config"), None, &config_home),
            PathBuf::from("/custom/bat-config")
        );
    }

    #[test]
    fn test_bat_resolve_path_config_dir_override() {
        let config_home = PathBuf::from("/home/user/.config");
        assert_eq!(
            BatAdapter::resolve_path(None, Some("/custom/bat-dir"), &config_home),
            PathBuf::from("/custom/bat-dir/config")
        );
    }

    #[test]
    fn test_bat_resolve_path_config_path_takes_priority() {
        let config_home = PathBuf::from("/home/user/.config");
        assert_eq!(
            BatAdapter::resolve_path(Some("/direct/config"), Some("/dir/bat"), &config_home),
            PathBuf::from("/direct/config")
        );
    }

    #[test]
    fn test_bat_resolve_path_empty_env_uses_default() {
        let config_home = PathBuf::from("/home/user/.config");
        assert_eq!(
            BatAdapter::resolve_path(Some(""), Some(""), &config_home),
            PathBuf::from("/home/user/.config/bat/config")
        );
    }

    #[test]
    fn test_bat_resolve_path_default_xdg() {
        let config_home = PathBuf::from("/home/user/.config");
        assert_eq!(
            BatAdapter::resolve_path(None, None, &config_home),
            PathBuf::from("/home/user/.config/bat/config")
        );
    }

    #[test]
    fn test_bat_replace_existing_theme() {
        let content = "--paging=always\n--theme=\"Dracula\"\n--color=always\n";

        let theme_pattern =
            Regex::new(r#"(?m)^\s*--theme\s*=\s*(?:"[^"\n]*"|'[^'\n]*'|[^"'#\n]+)\s*$"#).unwrap();
        let new_content = theme_pattern
            .replace(content, r#"--theme="Catppuccin Mocha""#)
            .to_string();

        assert!(new_content.contains(r#"--theme="Catppuccin Mocha""#));
        assert!(!new_content.contains("Dracula"));
    }

    #[test]
    fn test_bat_theme_detection() {
        let content = r#"--theme="Tokyo Night""#;

        let theme_pattern =
            Regex::new(r#"^\s*--theme\s*=\s*(?:"([^"\n]*)"|'([^'\n]*)'|([^"'#\s\n]+))"#).unwrap();

        if let Some(caps) = theme_pattern.captures(content) {
            if let Some(theme_name) = caps.get(1).or_else(|| caps.get(2)).or_else(|| caps.get(3)) {
                assert_eq!(theme_name.as_str(), "Tokyo Night");
            }
        }
    }

    #[test]
    fn test_bat_add_missing_theme() {
        let content = "--paging=always\n--color=always\n";

        let theme_pattern =
            Regex::new(r#"(?m)^\s*--theme\s*=\s*(?:"[^"\n]*"|'[^'\n]*'|[^"'#\n]+)\s*$"#).unwrap();

        let new_content = if theme_pattern.is_match(content) {
            theme_pattern
                .replace(content, r#"--theme="Catppuccin Mocha""#)
                .to_string()
        } else {
            let mut new = content.to_string();
            if !new.ends_with('\n') {
                new.push('\n');
            }
            new.push_str(r#"--theme="Catppuccin Mocha""#);
            new.push('\n');
            new
        };

        assert!(new_content.contains(r#"--theme="Catppuccin Mocha""#));
    }

    #[test]
    fn test_bat_comment_handling() {
        let content = "# bat config file\n--paging=always\n--theme=\"Old\"\n# end config\n";

        let theme_pattern =
            Regex::new(r#"(?m)^\s*--theme\s*=\s*(?:"[^"\n]*"|'[^'\n]*'|[^"'#\n]+)\s*$"#).unwrap();
        let new_content = theme_pattern
            .replace(content, r#"--theme="New""#)
            .to_string();

        assert!(new_content.contains("# bat config file"));
        assert!(new_content.contains("# end config"));
        assert!(new_content.contains(r#"--theme="New""#));
    }

    #[test]
    fn test_bat_pattern_rejects_mismatched_quotes() {
        let theme_pattern =
            Regex::new(r#"(?m)^\s*--theme\s*=\s*(?:"[^"\n]*"|'[^'\n]*'|[^"'#\n]+)\s*$"#).unwrap();
        assert!(!theme_pattern.is_match(r#"--theme="Dracula"#));
        assert!(!theme_pattern.is_match("--theme='Dracula\""));
    }
}
