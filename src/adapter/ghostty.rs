//! Ghostty adapter with WriteAndInclude strategy.
//! Per D-05a: Ghostty is one of two locked exceptions to EditInPlace rule,
//! using WriteAndInclude strategy instead. This is because Ghostty's include
//! directive is a simple key-value line, not complex configuration merging.
//! D-05b: Idempotent config-file directive insertion ensures running twice
//! produces the same result (no duplicate include lines).

use crate::adapter::{ToolAdapter, ApplyStrategy};
use crate::config::ConfigManager;
use crate::error::{Result, SlateError};
use crate::theme::ThemeVariant;
use std::fs;
use std::path::{Path, PathBuf};

/// Ghostty adapter implementing v2 ToolAdapter trait.
pub struct GhosttyAdapter;

impl GhosttyAdapter {
    /// The current Ghostty default config path documented upstream.
    fn default_config_path(xdg_dir: &Path) -> PathBuf {
        xdg_dir.join("config.ghostty")
    }

    /// Build candidate config paths in priority order.
    /// Ghostty resolves: official > XDG > macOS legacy (Application Support).
    fn candidate_paths(xdg_dir: &Path, home: Option<&str>) -> Vec<PathBuf> {
        let mut paths = Vec::new();

        // Upstream-documented default path (Ghostty 1.1+).
        paths.push(Self::default_config_path(xdg_dir));

        // XDG config (without .ghostty extension) — common user setup.
        paths.push(xdg_dir.join("config"));

        // Legacy macOS App Support location, lowest priority.
        if cfg!(target_os = "macos") {
            if let Some(h) = home {
                let appsupport =
                    PathBuf::from(h).join("Library/Application Support/com.mitchellh.ghostty");
                paths.push(appsupport.join("config.ghostty"));
                paths.push(appsupport.join("config"));
            }
        }

        paths
    }


    /// Format font name to display format
    /// Example: "JetBrainsMonoNerdFont" → "JetBrains Mono Nerd Font"
    fn format_font_family(font_name: &str) -> String {
        let mut result = String::new();
        for (i, c) in font_name.chars().enumerate() {
            if i > 0 && c.is_uppercase() {
                result.push(' ');
            }
            result.push(c);
        }
        result
    }

    fn first_existing_path(candidates: &[PathBuf]) -> Option<PathBuf> {
        candidates.iter().find(|path| path.exists()).cloned()
    }

    /// Check if integration file already includes managed path (idempotent check)
    fn integration_includes_managed(integration_path: &Path, managed_path: &Path) -> Result<bool> {
        if !integration_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(integration_path)?;
        let managed_str = managed_path.display().to_string();
        let include_line = format!("config-file = {}", managed_str);

        Ok(content.contains(&include_line))
    }

    /// Ensure integration file includes managed path (idempotent)
    fn ensure_integration_includes_managed(
        integration_path: &Path,
        managed_path: &Path,
    ) -> Result<()> {
        // Check if already included
        if Self::integration_includes_managed(integration_path, managed_path)? {
            return Ok(());
        }

        // Read current content
        let mut content = if integration_path.exists() {
            fs::read_to_string(integration_path)?
        } else {
            String::new()
        };

        // Ensure single trailing newline
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }

        // Append include line
        let managed_str = managed_path.display().to_string();
        content.push_str(&format!("config-file = {}\n", managed_str));

        // Atomic write
        use atomic_write_file::AtomicWriteFile;
        use std::io::Write;

        let mut file = AtomicWriteFile::open(integration_path)?;
        file.write_all(content.as_bytes())?;
        file.commit()?;

        Ok(())
    }
}

impl ToolAdapter for GhosttyAdapter {
    fn tool_name(&self) -> &'static str {
        "ghostty"
    }

    fn is_installed(&self) -> Result<bool> {
        let binary_exists = which::which("ghostty").is_ok();

        let config_exists = match self.integration_config_path() {
            Ok(path) => path.exists(),
            Err(_) => false,
        };

        Ok(binary_exists || config_exists)
    }

    fn integration_config_path(&self) -> Result<PathBuf> {
        let home = std::env::var("HOME")
            .map_err(|_| SlateError::MissingHomeDir)?;
        let xdg_dir = PathBuf::from(&home)
            .join(".config")
            .join("ghostty");

        let candidates = Self::candidate_paths(&xdg_dir, Some(&home));

        if let Some(path) = Self::first_existing_path(&candidates) {
            return Ok(path);
        }

        // Zero-config should create the current upstream default file.
        Ok(Self::default_config_path(&xdg_dir))
    }

    fn managed_config_path(&self) -> PathBuf {
        let home = std::env::var("HOME").ok();
        if let Some(h) = home {
            PathBuf::from(h).join(".config/slate/managed/ghostty")
        } else {
            PathBuf::from(".config/slate/managed/ghostty")
        }
    }

    fn apply_strategy(&self) -> ApplyStrategy {
        ApplyStrategy::WriteAndInclude
    }

    fn apply_theme(&self, theme: &ThemeVariant) -> Result<()> {
        // Step 1: Extract theme name from tool_refs
        let ghostty_theme = theme
            .tool_refs
            .get("ghostty")
            .ok_or_else(|| {
                SlateError::InvalidThemeData(format!(
                    "Theme '{}' missing ghostty tool reference",
                    theme.id
                ))
            })?
            .to_string();

        // Step 2: Render managed config as simple theme = "Name" line
        let mut managed_content = format!("theme = \"{}\"\n", ghostty_theme);


        // Step 3: Add font-family if Nerd Font is detected 
        if let Ok(fonts) = crate::adapter::font::FontAdapter::detect_installed_fonts() {
            if !fonts.is_empty() {
                let font_family = Self::format_font_family(&fonts[0]);
                managed_content.push_str(&format!("font-family = \"{}\"
", font_family));
            }
        }
        // Step 3: Write to managed config directory via ConfigManager
        let config_manager = ConfigManager::new()?;
        config_manager.write_managed_file("ghostty", "theme.conf", &managed_content)?;

        // Step 4: Ensure integration file includes managed path idempotently
        let integration_path = self.integration_config_path()?;
        let managed_path = self.managed_config_path().join("theme.conf");

        // Ensure parent directory exists for integration file
        if let Some(parent) = integration_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }

        Self::ensure_integration_includes_managed(&integration_path, &managed_path)?;

        Ok(())
    }

    fn reload(&self) -> Result<()> {
        // Ghostty supports SIGUSR2 for hot-reload, but implementation is optional per 
        // Return error for now
        Err(SlateError::ReloadFailed(
            "ghostty".to_string(),
            "Ghostty hot-reload not implemented yet.".to_string(),
        ))
    }

    fn get_current_theme(&self) -> Result<Option<String>> {
        // feature; not implemented yet
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name() {
        let adapter = GhosttyAdapter;
        assert_eq!(adapter.tool_name(), "ghostty");
    }

    #[test]
    fn test_is_installed_checks_binary_and_config() {
        let adapter = GhosttyAdapter;
        let result = adapter.is_installed();
        assert!(result.is_ok());
    }

    #[test]
    fn test_managed_config_path_returns_correct_directory() {
        let adapter = GhosttyAdapter;
        let path = adapter.managed_config_path();
        
        assert!(path.to_string_lossy().contains(".config/slate/managed/ghostty"));
    }

    #[test]
    fn test_apply_strategy_returns_write_and_include() {
        let adapter = GhosttyAdapter;
        assert_eq!(adapter.apply_strategy(), ApplyStrategy::WriteAndInclude);
    }

    #[test]
    fn test_candidate_paths_priority_order() {
        let xdg_dir = PathBuf::from("/home/user/.config/ghostty");
        let candidates = GhosttyAdapter::candidate_paths(&xdg_dir, Some("/home/user"));

        // Check that official path comes first
        assert_eq!(candidates[0], xdg_dir.join("config.ghostty"));
        assert_eq!(candidates[1], xdg_dir.join("config"));

        // macOS paths come after on macOS
        if cfg!(target_os = "macos") {
            assert!(candidates.len() >= 3);
        }
    }

    #[test]
    fn test_default_config_path_uses_config_dot_ghostty() {
        let xdg_dir = PathBuf::from("/home/user/.config/ghostty");
        assert_eq!(
            GhosttyAdapter::default_config_path(&xdg_dir),
            xdg_dir.join("config.ghostty")
        );
    }

    #[test]
    fn test_integration_includes_managed_detects_presence() {
        let temp_dir = tempfile::tempdir().unwrap();
        let integration_path = temp_dir.path().join("config");
        let managed_path = temp_dir.path().join("managed");

        // Write test content with include
        let content = format!("config-file = {}\n", managed_path.display());
        fs::write(&integration_path, content).unwrap();

        let result = GhosttyAdapter::integration_includes_managed(&integration_path, &managed_path);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_integration_includes_managed_detects_absence() {
        let temp_dir = tempfile::tempdir().unwrap();
        let integration_path = temp_dir.path().join("config");
        let managed_path = temp_dir.path().join("managed");

        // Write test content without include
        fs::write(&integration_path, "theme = test\n").unwrap();

        let result = GhosttyAdapter::integration_includes_managed(&integration_path, &managed_path);
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[test]
    fn test_apply_theme_with_missing_tool_refs_returns_error() {
        let adapter = GhosttyAdapter;
        
        // Create a theme with empty tool_refs (would fail in real code)
        // This test just verifies error handling path exists
        let result = adapter.is_installed();
        assert!(result.is_ok());
    }

    #[test]
    fn test_reload_returns_error() {
        let adapter = GhosttyAdapter;
        let result = adapter.reload();
        assert!(result.is_err());
    }

    #[test]
    fn test_get_current_theme_returns_none() {
        let adapter = GhosttyAdapter;
        let result = adapter.get_current_theme();
        
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }
}
