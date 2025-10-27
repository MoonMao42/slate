use crate::error::{ThemeError, ThemeResult};
use crate::theme::Theme;
use std::path::PathBuf;

/// Resolve XDG config home from env vars (pure function, testable)
pub(crate) fn resolve_xdg_config_home(xdg: Option<&str>, home: Option<&str>) -> ThemeResult<PathBuf> {
    if let Some(val) = xdg {
        if !val.is_empty() {
            return Ok(PathBuf::from(val));
        }
    }
    let home = home
        .ok_or_else(|| ThemeError::Other("Cannot determine HOME directory".to_string()))?;
    Ok(PathBuf::from(home).join(".config"))
}

/// Resolve XDG config home from real environment
pub(crate) fn xdg_config_home() -> ThemeResult<PathBuf> {
    resolve_xdg_config_home(
        std::env::var("XDG_CONFIG_HOME").ok().as_deref(),
        std::env::var("HOME").ok().as_deref(),
    )
}

/// Trait that all tool adapters must implement.
/// Each adapter handles tool-specific config detection and modification.
pub trait ToolAdapter: Send + Sync {
    /// Check if the tool is installed (binary in PATH or config file exists).
    /// Returns true if tool is available, false otherwise.
    /// Returns error if check itself fails (e.g., permission denied).
    fn is_installed(&self) -> ThemeResult<bool>;

    /// Get the canonical path to the tool's config file.
    /// Resolves symlinks to ensure we get the actual target.
    fn config_path(&self) -> ThemeResult<PathBuf>;

    /// Check if the config file exists and is readable.
    fn config_exists(&self) -> ThemeResult<bool>;

    /// Apply a theme to the tool's configuration.
    /// Must create a backup before modification and use atomic writes.
    fn apply_theme(&self, theme: &Theme) -> ThemeResult<()>;

    /// Get the current theme name from the tool's config (optional, for phase 2 status command).
    /// Returns None if theme cannot be determined.
    fn get_current_theme(&self) -> ThemeResult<Option<String>>;

    /// Get the tool's identifier (e.g., "ghostty", "starship", "bat").
    fn tool_name(&self) -> &'static str;
}

/// Struct that tracks results of applying themes to multiple tools.
/// Reports per-tool success/failure status.
#[derive(Debug, Clone)]
pub struct ApplyThemeResult {
    pub successful: Vec<String>,
    pub failed: Vec<(String, String)>, // tool name + error description
}

impl ApplyThemeResult {
    /// Create a new empty result tracker.
    pub fn new() -> Self {
        Self {
            successful: Vec::new(),
            failed: Vec::new(),
        }
    }

    /// Add a successful tool.
    pub fn add_success(&mut self, tool_name: String) {
        self.successful.push(tool_name);
    }

    /// Add a failed tool with error description.
    pub fn add_failure(&mut self, tool_name: String, error: String) {
        self.failed.push((tool_name, error));
    }

    /// Count of tools that succeeded.
    pub fn count_successful(&self) -> usize {
        self.successful.len()
    }

    /// Count of tools that failed.
    pub fn count_failed(&self) -> usize {
        self.failed.len()
    }

    /// True if no failures occurred (partial or total success).
    pub fn is_success(&self) -> bool {
        self.failed.is_empty()
    }

    /// True if at least one tool succeeded.
    pub fn has_success(&self) -> bool {
        !self.successful.is_empty()
    }
}

impl Default for ApplyThemeResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Registry managing all tool adapters.
/// Provides detection and batch theme application.
pub struct ToolRegistry {
    adapters: Vec<Box<dyn ToolAdapter>>,
}

impl ToolRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            adapters: Vec::new(),
        }
    }

    /// Register an adapter.
    pub fn register(&mut self, adapter: Box<dyn ToolAdapter>) {
        self.adapters.push(adapter);
    }

    /// Get all registered adapters.
    pub fn adapters(&self) -> &[Box<dyn ToolAdapter>] {
        &self.adapters
    }

    /// Detect which tools are installed.
    /// Returns list of installed tool names.
    pub fn detect_installed(&self) -> ThemeResult<Vec<String>> {
        let mut installed = Vec::new();
        for adapter in &self.adapters {
            match adapter.is_installed() {
                Ok(true) => {
                    installed.push(adapter.tool_name().to_string());
                }
                Ok(false) => {
                    // Tool not installed, skip
                }
                Err(e) => {
                    // Log detection error but continue
                    eprintln!("Warning: Failed to detect {}: {}", adapter.tool_name(), e);
                }
            }
        }
        Ok(installed)
    }

    /// Apply a theme to all installed tools.
    /// Returns per-tool status (successful + failed).
    /// Implements SAFE-07: graceful partial failure.
    /// If one tool fails, others still apply.
    pub fn apply_theme_to_all(&self, theme: &Theme) -> ThemeResult<ApplyThemeResult> {
        apply_all_tools_with_fallback(&self.adapters, theme)
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Apply theme to all adapters with graceful fallback.
/// If one adapter fails, continue with the rest.
/// Implements SAFE-07 partial failure handling.
fn apply_all_tools_with_fallback(
    adapters: &[Box<dyn ToolAdapter>],
    theme: &Theme,
) -> ThemeResult<ApplyThemeResult> {
    let mut result = ApplyThemeResult::new();

    for adapter in adapters {
        match adapter.is_installed() {
            Ok(true) => {
                match adapter.apply_theme(theme) {
                    Ok(()) => {
                        result.add_success(adapter.tool_name().to_string());
                    }
                    Err(e) => {
                        result.add_failure(adapter.tool_name().to_string(), e.to_string());
                    }
                }
            }
            Ok(false) => {
                // Tool not installed, skip
            }
            Err(e) => {
                result.add_failure(
                    adapter.tool_name().to_string(),
                    format!("Detection failed: {}", e),
                );
            }
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ThemeError;
    use crate::theme::get_theme;

    /// Mock adapter for testing.
    struct MockAdapter {
        name: &'static str,
        installed: bool,
        should_fail: bool,
    }

    impl MockAdapter {
        fn new(name: &'static str, installed: bool, should_fail: bool) -> Box<Self> {
            Box::new(Self {
                name,
                installed,
                should_fail,
            })
        }
    }

    impl ToolAdapter for MockAdapter {
        fn is_installed(&self) -> ThemeResult<bool> {
            Ok(self.installed)
        }

        fn config_path(&self) -> ThemeResult<PathBuf> {
            Ok(PathBuf::from(format!("/home/user/.config/{}/config", self.name)))
        }

        fn config_exists(&self) -> ThemeResult<bool> {
            Ok(self.installed)
        }

        fn apply_theme(&self, _theme: &Theme) -> ThemeResult<()> {
            if self.should_fail {
                Err(ThemeError::WriteError {
                    path: "test".to_string(),
                    reason: "Test failure".to_string(),
                })
            } else {
                Ok(())
            }
        }

        fn get_current_theme(&self) -> ThemeResult<Option<String>> {
            Ok(None)
        }

        fn tool_name(&self) -> &'static str {
            self.name
        }
    }

    #[test]
    fn test_adapter_trait_object() {
        let mock: Box<dyn ToolAdapter> = MockAdapter::new("test", true, false);
        assert!(mock.is_installed().unwrap());
        assert_eq!(mock.tool_name(), "test");
    }

    #[test]
    fn test_registry_detection() {
        let mut registry = ToolRegistry::new();
        registry.register(MockAdapter::new("ghostty", true, false));
        registry.register(MockAdapter::new("starship", true, false));
        registry.register(MockAdapter::new("bat", false, false));

        let detected = registry.detect_installed().unwrap();
        assert_eq!(detected.len(), 2);
        assert!(detected.contains(&"ghostty".to_string()));
        assert!(detected.contains(&"starship".to_string()));
        assert!(!detected.contains(&"bat".to_string()));
    }

    #[test]
    fn test_partial_failure() {
        let mut registry = ToolRegistry::new();
        registry.register(MockAdapter::new("ghostty", true, false));
        registry.register(MockAdapter::new("starship", true, true)); // This one fails
        registry.register(MockAdapter::new("bat", true, false));

        let theme = get_theme("catppuccin-mocha").unwrap();
        let result = registry.apply_theme_to_all(&theme).unwrap();

        // Should have 2 successes (ghostty, bat) and 1 failure (starship)
        assert_eq!(result.count_successful(), 2);
        assert_eq!(result.count_failed(), 1);
        assert!(!result.is_success()); // Has failures
        assert!(result.has_success()); // But also has successes

        // Verify which ones succeeded/failed
        assert!(result.successful.contains(&"ghostty".to_string()));
        assert!(result.successful.contains(&"bat".to_string()));
        assert_eq!(result.failed[0].0, "starship");
    }

    #[test]
    fn test_result_tracking() {
        let mut result = ApplyThemeResult::new();
        assert!(result.is_success());
        assert!(!result.has_success());

        result.add_success("ghostty".to_string());
        assert!(result.is_success());
        assert!(result.has_success());
        assert_eq!(result.count_successful(), 1);

        result.add_failure("starship".to_string(), "Config not found".to_string());
        assert!(!result.is_success());
        assert!(result.has_success());
        assert_eq!(result.count_failed(), 1);
    }

    #[test]
    fn test_empty_registry() {
        let registry = ToolRegistry::new();
        let theme = get_theme("dracula").unwrap();
        let result = registry.apply_theme_to_all(&theme).unwrap();

        assert_eq!(result.count_successful(), 0);
        assert_eq!(result.count_failed(), 0);
        assert!(result.is_success());
    }
}

pub mod ghostty;
pub use ghostty::GhosttyAdapter;
pub mod starship;
pub use starship::StarshipAdapter;
pub mod bat;
pub use bat::BatAdapter;
pub mod delta;
pub use delta::DeltaAdapter;
pub mod lazygit;
pub use lazygit::LazygitAdapter;
