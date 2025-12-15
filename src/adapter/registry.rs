use crate::adapter::{ApplyStrategy, ToolAdapter};
use crate::error::Result;
use crate::theme::ThemeVariant;
use rayon::prelude::*;
use std::collections::HashMap;

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

    /// Apply theme to all registered tools (ignores if tool not installed).
    /// Returns results for each tool: Ok or Err.
    /// Per research: partial failure pattern (apply to others even if one fails).
    /// Detect-and-install adapters are not theme targets and are skipped.
    pub fn apply_theme_to_all(&self, theme: &ThemeVariant) -> HashMap<String, Result<()>> {
        let results: HashMap<String, Result<()>> = self.adapters
            .par_iter()
            .filter(|adapter| {
                adapter.apply_strategy() != ApplyStrategy::DetectAndInstall
                    && adapter.is_installed().unwrap_or(false)
            })
            .map(|adapter| {
                (adapter.tool_name().to_string(), adapter.apply_theme(theme))
            })
            .collect();

        results
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
    }

    impl ToolAdapter for MockAdapter {
        fn tool_name(&self) -> &'static str {
            self.name
        }

        fn is_installed(&self) -> Result<bool> {
            Ok(true)
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

        fn apply_theme(&self, _theme: &ThemeVariant) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn test_registry_register_and_retrieve() {
        let mut registry = ToolRegistry::new();
        let adapter = Box::new(MockAdapter {
            name: "test_tool",
            strategy: crate::adapter::ApplyStrategy::WriteAndInclude,
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
        });
        registry.register(adapter);

        let installed = registry.detect_installed();
        assert_eq!(installed.get("test_tool"), Some(&true));
    }

    #[test]
    fn test_registry_default() {
        let registry = ToolRegistry::default();
        assert_eq!(registry.adapters().len(), 11);
    }

    #[test]
    fn test_apply_theme_skips_detect_and_install_adapters() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(MockAdapter {
            name: "themeable",
            strategy: crate::adapter::ApplyStrategy::WriteAndInclude,
        }));
        registry.register(Box::new(MockAdapter {
            name: "detector",
            strategy: crate::adapter::ApplyStrategy::DetectAndInstall,
        }));

        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();
        let results = registry.apply_theme_to_all(&theme);

        assert!(results.contains_key("themeable"));
        assert!(!results.contains_key("detector"));
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
