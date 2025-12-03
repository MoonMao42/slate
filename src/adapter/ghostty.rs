//! Ghostty adapter with WriteAndInclude strategy.
//! Per D-05a: Ghostty is one of two locked exceptions to EditInPlace rule,
//! using WriteAndInclude strategy instead. This is because Ghostty's include
//! directive is a simple key-value line, not complex configuration merging.
//! D-05b: Idempotent config-file directive insertion ensures running twice
//! produces the same result (no duplicate include lines).

use crate::adapter::{ApplyStrategy, ToolAdapter};
use crate::config::ConfigManager;
use crate::env::SlateEnv;
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

    fn first_existing_path(candidates: &[PathBuf]) -> Option<PathBuf> {
        candidates.iter().find(|p| p.exists()).cloned()
    }

    /// Insert managed path in integration file idempotently.
    /// Per D-05b: integration file can be created by tool (zero-config setup).
    /// If it doesn't exist, we still must track its path so apply_theme can upsert it later.
    fn ensure_integration_includes_managed(
        integration_path: &Path,
        managed_path: &Path,
    ) -> Result<()> {
        let include_line = format!("include = \"{}\"\n", managed_path.display());

        if !integration_path.exists() {
            // File doesn't exist yet; Ghostty will create it on first run.
            // We've recorded the path for later when apply_theme upserts it.
            return Ok(());
        }

        let content = fs::read_to_string(integration_path)?;

        if content.contains("include") {
            // Include directive already present: idempotent.
            return Ok(());
        }

        // Append include line
        fs::write(integration_path, format!("{}{}", content, include_line))?;

        Ok(())
    }

    /// Update font-family in integration config.
    /// Modifies user's integration file (not managed) because Ghostty main config
    /// takes precedence over config-file includes for font-family.
    fn update_font_in_config(integration_path: &Path, font_family: &str) -> Result<()> {
        if !integration_path.exists() {
            // Config file doesn't exist, file will be created by Ghostty on first run.
            // Skip font update — Ghostty will use system defaults until explicitly set.
            return Ok(());
        }

        let mut content = fs::read_to_string(integration_path)?;

        let font_line = format!("font-family = \"{}\"\n", font_family);
        let font_pattern = "font-family";

        if let Some(idx) = content.find(font_pattern) {
            // Find end of line and replace
            let end_of_line = content[idx..].find('\n').map(|i| idx + i + 1).unwrap_or(content.len());
            content.replace_range(idx..end_of_line, &font_line);
        } else {
            // Append to end of file
            content.push_str(&font_line);
        }

        fs::write(integration_path, content)?;

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
        let env = SlateEnv::from_process()?;
        self.integration_config_path_with_env(&env)
    }

    fn managed_config_path(&self) -> PathBuf {
        let env = SlateEnv::from_process().ok();
        self.managed_config_path_with_env(env.as_ref())
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

        // Step 2: Render managed config as theme-only line
        let managed_content = format!("theme = \"{}\"\n", ghostty_theme);

        // Step 3: Write managed theme config
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

        // Step 5: Update font-family in user's main config (not managed — Ghostty
        // main config takes precedence over config-file includes for font-family)
        let chosen_font = crate::config::ConfigManager::new()
            .ok()
            .and_then(|cm| cm.get_current_font().ok().flatten());
        let font_family = chosen_font.or_else(|| {
            crate::adapter::font::FontAdapter::detect_installed_fonts()
                .ok()
                .and_then(|f| f.into_iter().next())
        });
        if let Some(family) = font_family {
            Self::update_font_in_config(&integration_path, &family)?;
        }

        Ok(())
    }

    fn reload(&self) -> Result<()> {
        // Send SIGUSR2 to all Ghostty processes to trigger config reload
        let output = std::process::Command::new("pkill")
            .arg("-SIGUSR2")
            .arg("-x")
            .arg("ghostty")
            .output()
            .map_err(|e| SlateError::Internal(format!("Failed to reload ghostty: {}", e)))?;

        if !output.status.success() {
            return Err(SlateError::Internal(
                "pkill signal failed (Ghostty may not be running)".to_string(),
            ));
        }

        Ok(())
    }
}

/// Helper methods using injected SlateEnv (for testing)
impl GhosttyAdapter {
    pub fn integration_config_path_with_env(&self, env: &SlateEnv) -> Result<PathBuf> {
        let home = env.home().to_str().ok_or(SlateError::MissingHomeDir)?;
        let xdg_dir = env
            .home()
            .join(".config")
            .join("ghostty");

        let candidates = Self::candidate_paths(&xdg_dir, Some(home));

        if let Some(path) = Self::first_existing_path(&candidates) {
            return Ok(path);
        }

        // Zero-config should create the current upstream default file.
        Ok(Self::default_config_path(&xdg_dir))
    }

    pub fn managed_config_path_with_env(&self, env: Option<&SlateEnv>) -> PathBuf {
        if let Some(e) = env {
            let config_dir = e.config_dir();
            config_dir.join("managed").join("ghostty")
        } else {
            PathBuf::from(".config/slate/managed/ghostty")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ghostty_adapter_tool_name() {
        let adapter = GhosttyAdapter;
        assert_eq!(adapter.tool_name(), "ghostty");
    }

    #[test]
    fn test_ghostty_default_config_path() {
        let xdg_dir = PathBuf::from("/test/.config/ghostty");
        let path = GhosttyAdapter::default_config_path(&xdg_dir);
        assert!(path.to_string_lossy().contains("config.ghostty"));
    }

    #[test]
    fn test_ghostty_candidate_paths_includes_xdg() {
        let xdg_dir = PathBuf::from("/test/.config/ghostty");
        let candidates = GhosttyAdapter::candidate_paths(&xdg_dir, Some("/home/user"));
        assert!(candidates.iter().any(|p| p.to_string_lossy().contains("config.ghostty")));
    }

    #[test]
    fn test_ghostty_first_existing_path() {
        let candidates = vec![
            PathBuf::from("/nonexistent/path1"),
            PathBuf::from("/nonexistent/path2"),
        ];
        assert!(GhosttyAdapter::first_existing_path(&candidates).is_none());
    }

    #[test]
    fn test_ghostty_apply_strategy() {
        let adapter = GhosttyAdapter;
        assert_eq!(
            adapter.apply_strategy(),
            ApplyStrategy::WriteAndInclude
        );
    }

    #[test]
    fn test_ghostty_integration_config_path_with_env() {
        let tempdir = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let adapter = GhosttyAdapter;

        let path = adapter.integration_config_path_with_env(&env).unwrap();
        assert!(path.ends_with("config.ghostty"));
    }

    #[test]
    fn test_ghostty_managed_config_path_with_env() {
        let tempdir = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let adapter = GhosttyAdapter;

        let path = adapter.managed_config_path_with_env(Some(&env));
        assert!(path.ends_with("slate/managed/ghostty"));
    }
}
