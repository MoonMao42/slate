use crate::adapter::ToolAdapter;
use crate::config::backup::create_backup;
use crate::error::{ThemeError, ThemeResult};
use crate::theme::Theme;
use atomic_write_file::AtomicWriteFile;
use regex::Regex;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub struct GhosttyAdapter;

impl GhosttyAdapter {
    /// The current Ghostty default config path documented upstream.
    fn default_config_path(xdg_dir: &Path) -> PathBuf {
        xdg_dir.join("config.ghostty")
    }

    /// Build candidate config paths in priority order.
    /// Prefer the current upstream default first, then known legacy paths.
    fn candidate_paths(xdg_dir: &Path, home: Option<&str>) -> Vec<PathBuf> {
        let mut paths = Vec::new();

        // Upstream-documented default path.
        paths.push(Self::default_config_path(xdg_dir));

        // Legacy macOS App Support location, if present on older setups.
        if cfg!(target_os = "macos") {
            if let Some(h) = home {
                let appsupport = PathBuf::from(h)
                    .join("Library/Application Support/com.mitchellh.ghostty");
                paths.push(appsupport.join("config.ghostty"));
                paths.push(appsupport.join("config"));
            }
        }

        // Legacy XDG location used by earlier revisions of this project.
        paths.push(xdg_dir.join("config"));
        paths
    }

    fn first_existing_path(candidates: &[PathBuf]) -> Option<PathBuf> {
        candidates.iter().find(|path| path.exists()).cloned()
    }
}

impl ToolAdapter for GhosttyAdapter {
    fn is_installed(&self) -> ThemeResult<bool> {
        let binary_exists = which::which("ghostty").is_ok();
        let config_exists = match self.config_path() {
            Ok(path) => path.exists(),
            Err(_) => false,
        };
        Ok(binary_exists || config_exists)
    }

    fn config_path(&self) -> ThemeResult<PathBuf> {
        let xdg_dir = crate::adapter::xdg_config_home()?.join("ghostty");
        let home = std::env::var("HOME").ok();
        let candidates = Self::candidate_paths(&xdg_dir, home.as_deref());

        if let Some(path) = Self::first_existing_path(&candidates) {
            return Ok(path);
        }

        // Zero-config should create the current upstream default file.
        Ok(Self::default_config_path(&xdg_dir))
    }

    fn config_exists(&self) -> ThemeResult<bool> {
        let path = self.config_path()?;
        Ok(path.exists() && path.is_file())
    }

    fn apply_theme(&self, theme: &Theme) -> ThemeResult<()> {
        // Get canonical path (resolve symlinks)
        let config_path = self.config_path()?;
        let canonical_path = fs::canonicalize(&config_path)
            .map_err(|_e| ThemeError::SymlinkError {
                path: config_path.display().to_string(),
            })?;

        // Create backup before modification (SAFE-04)
        let _backup_info = create_backup("ghostty", &theme.name, &canonical_path)?;

        // Read current config
        let content = fs::read_to_string(&canonical_path)
            .map_err(|e| ThemeError::Io(e))?;

        // Get the Ghostty theme name from tool_overrides
        let ghostty_theme = theme
            .colors
            .tool_overrides
            .get("ghostty")
            .ok_or_else(|| ThemeError::Other(
                format!("No Ghostty theme override for {}", theme.name)
            ))?
            .to_string();

        // Use regex to replace or create the theme line
        let theme_pattern = Regex::new(r#"(?m)^\s*theme\s*=\s*["\']?.*?["\']?\s*$"#)
            .map_err(|e| ThemeError::Other(format!("Regex error: {}", e)))?;

        let new_content = if theme_pattern.is_match(&content) {
            // Replace existing theme line
            theme_pattern
                .replace(&content, format!(r#"theme = "{}""#, ghostty_theme))
                .to_string()
        } else {
            // Create new theme line at the end
            let mut new = content;
            if !new.ends_with('\n') {
                new.push('\n');
            }
            new.push_str(&format!(r#"theme = "{}""#, ghostty_theme));
            new.push('\n');
            new
        };

        // Atomic write
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
        
        file.commit()
            .map_err(|e| ThemeError::WriteError {
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
        let content = fs::read_to_string(&path)
            .map_err(|e| ThemeError::Io(e))?;

        let theme_pattern = Regex::new(r#"^\s*theme\s*=\s*["\'](.+?)["\']"#)
            .map_err(|e| ThemeError::Other(format!("Regex error: {}", e)))?;

        if let Some(caps) = theme_pattern.captures(&content) {
            if let Some(theme_name) = caps.get(1) {
                return Ok(Some(theme_name.as_str().to_string()));
            }
        }

        Ok(None)
    }

    fn tool_name(&self) -> &'static str {
        "ghostty"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ghostty_tool_name() {
        let adapter = GhosttyAdapter;
        assert_eq!(adapter.tool_name(), "ghostty");
    }

    #[test]
    fn test_ghostty_candidate_paths_includes_all_locations() {
        let xdg_dir = PathBuf::from("/home/user/.config/ghostty");
        let candidates = GhosttyAdapter::candidate_paths(&xdg_dir, Some("/home/user"));

        assert!(candidates.contains(&xdg_dir.join("config.ghostty")));
        assert!(candidates.contains(&xdg_dir.join("config")));
        assert_eq!(candidates[0], xdg_dir.join("config.ghostty"));

        // macOS legacy paths are included on macOS, but official default stays first.
        if cfg!(target_os = "macos") {
            let appsupport = PathBuf::from("/home/user/Library/Application Support/com.mitchellh.ghostty");
            assert!(candidates.contains(&appsupport.join("config.ghostty")));
            assert!(candidates.contains(&appsupport.join("config")));
        }
    }

    #[test]
    fn test_ghostty_candidate_paths_without_home() {
        let xdg_dir = PathBuf::from("/home/user/.config/ghostty");
        let candidates = GhosttyAdapter::candidate_paths(&xdg_dir, None);
        assert!(candidates.contains(&xdg_dir.join("config.ghostty")));
        assert!(candidates.len() == 2);
    }

    #[test]
    fn test_ghostty_default_config_path_uses_config_dot_ghostty() {
        let xdg_dir = PathBuf::from("/home/user/.config/ghostty");
        assert_eq!(
            GhosttyAdapter::default_config_path(&xdg_dir),
            xdg_dir.join("config.ghostty")
        );
    }

    #[test]
    fn test_ghostty_prefers_config_dot_ghostty_over_legacy_config() {
        let temp_dir = tempfile::tempdir().unwrap();
        let xdg_dir = temp_dir.path().join("ghostty");
        std::fs::create_dir_all(&xdg_dir).unwrap();

        let legacy_path = xdg_dir.join("config");
        let official_path = xdg_dir.join("config.ghostty");
        std::fs::write(&legacy_path, "theme = \"Wrong\"\n").unwrap();
        std::fs::write(&official_path, "theme = \"Right\"\n").unwrap();

        let selected = GhosttyAdapter::first_existing_path(
            &GhosttyAdapter::candidate_paths(&xdg_dir, Some(temp_dir.path().to_str().unwrap())),
        )
        .unwrap();

        assert_eq!(selected, official_path);
    }

    #[test]
    fn test_ghostty_replace_existing_theme() {
        let content = "font-family = monospace\ntheme = \"Dracula\"\nfont-size = 12\n";
        
        let theme_pattern = Regex::new(r#"(?m)^\s*theme\s*=\s*["\']?.*?["\']?\s*$"#).unwrap();
        let new_content = theme_pattern
            .replace(content, r#"theme = "Catppuccin Mocha""#)
            .to_string();
        
        assert!(new_content.contains(r#"theme = "Catppuccin Mocha""#));
        assert!(!new_content.contains("Dracula"));
    }

    #[test]
    fn test_ghostty_theme_detection() {
        let content = r#"theme = "Tokyo Night""#;
        
        let theme_pattern = Regex::new(r#"^\s*theme\s*=\s*["\'](.+?)["\']"#).unwrap();
        
        if let Some(caps) = theme_pattern.captures(content) {
            if let Some(theme_name) = caps.get(1) {
                assert_eq!(theme_name.as_str(), "Tokyo Night");
            }
        }
    }

    #[test]
    fn test_ghostty_add_missing_theme() {
        let content = "font-family = monospace\nfont-size = 12\n";
        
        let theme_pattern = Regex::new(r#"(?m)^\s*theme\s*=\s*["\']?.*?["\']?\s*$"#).unwrap();
        
        let new_content = if theme_pattern.is_match(content) {
            theme_pattern
                .replace(content, r#"theme = "Catppuccin Mocha""#)
                .to_string()
        } else {
            let mut new = content.to_string();
            if !new.ends_with('\n') {
                new.push('\n');
            }
            new.push_str(r#"theme = "Catppuccin Mocha""#);
            new.push('\n');
            new
        };
        
        assert!(new_content.contains(r#"theme = "Catppuccin Mocha""#));
    }
}
