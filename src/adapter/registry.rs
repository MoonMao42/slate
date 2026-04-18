use crate::adapter::{ApplyOutcome, ApplyStrategy, SkipReason, ToolAdapter};
use crate::error::Result;
use crate::theme::ThemeVariant;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};

/// Aggregated status for one adapter after a coordinated theme apply run.
#[derive(Debug)]
pub enum ToolApplyStatus {
    Applied,
    Skipped(SkipReason),
    Failed(crate::error::SlateError),
}

/// Structured adapter result emitted by ToolRegistry.
///
/// `requires_new_shell` is captured from `ApplyOutcome::Applied` when the adapter
/// succeeds; it is `false` for `Skipped` / `Failed` outcomes since no change was
/// made that would need a new shell. Plan 16-04 aggregates this field across the
/// per-run result set to drive the Phase 16 UX-01 new-terminal reminder.
#[derive(Debug)]
pub struct ToolApplyResult {
    pub tool_name: String,
    pub status: ToolApplyStatus,
    pub requires_new_shell: bool,
}

/// Registry for all tool adapters.
/// Manages adapter instances and coordinates theme application across tools.
pub struct ToolRegistry {
    adapters: Vec<Box<dyn ToolAdapter>>,
}

impl ToolRegistry {
    /// Create empty registry
    pub fn new() -> Self {
        Self {
            adapters: Vec::new(),
        }
    }

    /// Register an adapter.
    /// Called during initialization for each supported tool.
    pub fn register(&mut self, adapter: Box<dyn ToolAdapter>) {
        self.adapters.push(adapter);
    }

    /// Get all registered adapters
    pub fn adapters(&self) -> &[Box<dyn ToolAdapter>] {
        &self.adapters
    }

    /// Find adapter by tool name
    pub fn get_adapter(&self, tool_name: &str) -> Option<&dyn ToolAdapter> {
        self.adapters
            .iter()
            .find(|a| a.tool_name() == tool_name)
            .map(|a| a.as_ref())
    }

    /// Detect which registered tools are installed
    /// Returns map of tool_name → is_installed
    pub fn detect_installed(&self) -> HashMap<String, bool> {
        let mut result = HashMap::new();
        for adapter in &self.adapters {
            let installed = adapter.is_installed().unwrap_or(false);
            result.insert(adapter.tool_name().to_string(), installed);
        }
        result
    }

    /// Apply theme to all registered tools.
    /// Returns structured results for each themeable adapter.
    /// Per research: partial failure pattern (apply to others even if one fails).
    /// Detect-and-install adapters are not theme targets and are skipped.
    pub fn apply_theme_to_all(&self, theme: &ThemeVariant) -> Vec<ToolApplyResult> {
        self.apply_theme_with_filter(theme, None)
    }

    /// Apply a theme only to the adapters explicitly selected by the caller.
    pub fn apply_theme_to_tools(
        &self,
        theme: &ThemeVariant,
        tool_names: &HashSet<String>,
    ) -> Vec<ToolApplyResult> {
        self.apply_theme_with_filter(theme, Some(tool_names))
    }

    fn apply_theme_with_filter(
        &self,
        theme: &ThemeVariant,
        allowed_tools: Option<&HashSet<String>>,
    ) -> Vec<ToolApplyResult> {
        self.adapters
            .par_iter()
            .filter(|adapter| adapter.apply_strategy() != ApplyStrategy::DetectAndInstall)
            .filter(|adapter| {
                allowed_tools.is_none_or(|allowed| allowed.contains(adapter.tool_name()))
            })
            .map(|adapter| {
                let tool_name = adapter.tool_name().to_string();
                let (status, requires_new_shell) = match adapter.is_installed() {
                    Ok(false) => (ToolApplyStatus::Skipped(SkipReason::NotInstalled), false),
                    Ok(true) => match adapter.apply_theme(theme) {
                        Ok(ApplyOutcome::Applied { requires_new_shell }) => {
                            (ToolApplyStatus::Applied, requires_new_shell)
                        }
                        Ok(ApplyOutcome::Skipped(reason)) => {
                            (ToolApplyStatus::Skipped(reason), false)
                        }
                        Err(err) => (ToolApplyStatus::Failed(err), false),
                    },
                    Err(err) => (ToolApplyStatus::Failed(err), false),
                };

                ToolApplyResult {
                    tool_name,
                    status,
                    requires_new_shell,
                }
            })
            .collect()
    }

    /// Reload all adapters that support hot-reload
    pub fn reload_all(&self) -> HashMap<String, Result<()>> {
        let mut results = HashMap::new();
        for adapter in &self.adapters {
            let tool_name = adapter.tool_name().to_string();
            let result = adapter.reload();
            results.insert(tool_name, result);
        }
        results
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        let mut registry = Self::new();
        // Register all 11 adapters in default instance
        registry.register(Box::new(crate::adapter::GhosttyAdapter));
        registry.register(Box::new(crate::adapter::AlacrittyAdapter));
        registry.register(Box::new(crate::adapter::KittyAdapter));
        registry.register(Box::new(crate::adapter::StarshipAdapter));
        registry.register(Box::new(crate::adapter::BatAdapter));
        registry.register(Box::new(crate::adapter::DeltaAdapter));
        registry.register(Box::new(crate::adapter::EzaAdapter));
        registry.register(Box::new(crate::adapter::LazygitAdapter));
        registry.register(Box::new(crate::adapter::FastfetchAdapter));
        registry.register(Box::new(crate::adapter::ZshHighlightAdapter));
        registry.register(Box::new(crate::adapter::TmuxAdapter));
        registry.register(Box::new(crate::adapter::FontAdapter));
        registry
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mock adapter for testing
    struct MockAdapter {
        name: &'static str,
        strategy: crate::adapter::ApplyStrategy,
        installed: bool,
    }

    impl ToolAdapter for MockAdapter {
        fn tool_name(&self) -> &'static str {
            self.name
        }

        fn is_installed(&self) -> Result<bool> {
            Ok(self.installed)
        }

        fn integration_config_path(&self) -> Result<std::path::PathBuf> {
            Ok(std::path::PathBuf::from("/tmp/config"))
        }

        fn managed_config_path(&self) -> std::path::PathBuf {
            std::path::PathBuf::from("/tmp/managed")
        }

        fn apply_strategy(&self) -> crate::adapter::ApplyStrategy {
            self.strategy
        }

        fn apply_theme(&self, _theme: &ThemeVariant) -> Result<ApplyOutcome> {
            Ok(ApplyOutcome::Applied {
                requires_new_shell: false,
            })
        }
    }

    #[test]
    fn test_registry_register_and_retrieve() {
        let mut registry = ToolRegistry::new();
        let adapter = Box::new(MockAdapter {
            name: "test_tool",
            strategy: crate::adapter::ApplyStrategy::WriteAndInclude,
            installed: true,
        });
        registry.register(adapter);

        assert_eq!(registry.adapters().len(), 1);
        assert!(registry.get_adapter("test_tool").is_some());
        assert!(registry.get_adapter("unknown").is_none());
    }

    #[test]
    fn test_detect_installed() {
        let mut registry = ToolRegistry::new();
        let adapter = Box::new(MockAdapter {
            name: "test_tool",
            strategy: crate::adapter::ApplyStrategy::WriteAndInclude,
            installed: true,
        });
        registry.register(adapter);

        let installed = registry.detect_installed();
        assert_eq!(installed.get("test_tool"), Some(&true));
    }

    #[test]
    fn test_registry_default() {
        let registry = ToolRegistry::default();
        assert_eq!(registry.adapters().len(), 12);
    }

    #[test]
    fn test_apply_theme_skips_detect_and_install_adapters() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(MockAdapter {
            name: "themeable",
            strategy: crate::adapter::ApplyStrategy::WriteAndInclude,
            installed: true,
        }));
        registry.register(Box::new(MockAdapter {
            name: "detector",
            strategy: crate::adapter::ApplyStrategy::DetectAndInstall,
            installed: true,
        }));

        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();
        let results = registry.apply_theme_to_all(&theme);

        assert!(results.iter().any(|result| result.tool_name == "themeable"));
        assert!(!results.iter().any(|result| result.tool_name == "detector"));
    }

    #[test]
    fn test_apply_theme_to_tools_only_runs_selected_adapters() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(MockAdapter {
            name: "themeable",
            strategy: crate::adapter::ApplyStrategy::WriteAndInclude,
            installed: true,
        }));
        registry.register(Box::new(MockAdapter {
            name: "ignored",
            strategy: crate::adapter::ApplyStrategy::WriteAndInclude,
            installed: true,
        }));

        let selected = HashSet::from(["themeable".to_string()]);
        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();
        let results = registry.apply_theme_to_tools(&theme, &selected);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].tool_name, "themeable");
        assert!(matches!(results[0].status, ToolApplyStatus::Applied));
    }
}

#[cfg(test)]
mod registry_extended_tests {
    use super::*;
    use crate::adapter::{AlacrittyAdapter, DeltaAdapter, TmuxAdapter};

    #[test]
    fn test_registry_with_new_adapters() {
        let mut registry = ToolRegistry::new();

        registry.register(Box::new(AlacrittyAdapter));
        registry.register(Box::new(DeltaAdapter));
        registry.register(Box::new(TmuxAdapter));

        assert_eq!(registry.adapters().len(), 3);
        assert!(registry.get_adapter("alacritty").is_some());
        assert!(registry.get_adapter("delta").is_some());
        assert!(registry.get_adapter("tmux").is_some());
    }
}
