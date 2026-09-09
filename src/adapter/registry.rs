use crate::adapter::{ApplyOutcome, ApplyStrategy, SkipReason, ToolAdapter};
use crate::env::SlateEnv;
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
/// `requires_new_shell` is captured from `ApplyOutcome::Applied` when the adapter
/// succeeds; it is `false` for `Skipped` / `Failed` outcomes. Only confirmed
/// applications drive the new-shell reminder; failures may still have partial
/// file effects, including a saved global theme before notification failure.
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

/// Resolve availability once, before the coordinator captures potential writes.
/// The same ready/skip/failure decisions drive execution after the checkpoint.
pub(crate) struct PreparedThemeApply<'a> {
    targets: Vec<PreparedTarget<'a>>,
}

struct PreparedTarget<'a> {
    adapter: &'a dyn ToolAdapter,
    installed: Result<bool>,
}

impl PreparedThemeApply<'_> {
    /// Explicit tool sync requires every requested adapter to be ready. Keep
    /// compatibility/probe errors distinct from a changed review or a success.
    pub(crate) fn ensure_all_ready(&self) -> Result<()> {
        for target in &self.targets {
            let name = target.adapter.tool_name();
            let reason = match &target.installed {
                Ok(true) => continue,
                Ok(false) => "not available or not supported by this adapter".to_owned(),
                Err(error) => format!("readiness check failed: {error}"),
            };
            let guidance = if name == "nvim" {
                " Use `slate doctor nvim --check-version` to inspect Neovim."
            } else {
                " Check installation and compatibility before syncing."
            };
            return Err(crate::error::SlateError::InvalidConfig(format!(
                "{name}: {reason}. No adapter ran.{guidance}"
            )));
        }
        Ok(())
    }

    pub(crate) fn ready_tools(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.targets
            .iter()
            .filter(|target| matches!(target.installed, Ok(true)))
            .map(|target| target.adapter.tool_name())
    }

    pub(crate) fn apply_with_env(
        self,
        theme: &ThemeVariant,
        env: &SlateEnv,
    ) -> Vec<ToolApplyResult> {
        self.apply_with(|adapter| adapter.apply_theme_with_env(theme, env))
    }

    /// Only ready notifications are deferred. Missing tools and probe failures
    /// retain their pre-commit result and cannot bypass the global failure gate.
    pub(crate) fn split_post_commit(self) -> (Self, Self) {
        let (notifications, regular) = self.targets.into_iter().partition(|target| {
            matches!(target.installed, Ok(true)) && target.adapter.is_post_commit_notification()
        });
        (
            Self { targets: regular },
            Self {
                targets: notifications,
            },
        )
    }

    pub(crate) fn skip_uncommitted(self) -> Vec<ToolApplyResult> {
        self.apply_with(|_| Ok(ApplyOutcome::Skipped(SkipReason::ThemeNotCommitted)))
    }

    pub(crate) fn notify_after_commit(
        self,
        theme: &ThemeVariant,
        env: &SlateEnv,
    ) -> Vec<ToolApplyResult> {
        self.apply_with(|adapter| {
            adapter.apply_theme_with_env(theme, env).map_err(|error| {
                crate::error::SlateError::ConfigWriteError(
                    format!("{} post-commit notification", adapter.tool_name()),
                    format!("Theme '{}' was already saved, but notification failed: {error}. The saved theme was not rolled back", theme.id.escape_default()),
                )
            })
        })
    }

    fn apply_with(
        self,
        apply_call: impl Fn(&dyn ToolAdapter) -> Result<ApplyOutcome> + Sync,
    ) -> Vec<ToolApplyResult> {
        // Only preview-owned worker calls inherit write attribution. Unrelated
        // threads and ordinary theme application remain outside that scope.
        let preview_context = crate::config::preview_write::context();
        self.targets
            .into_par_iter()
            .map(|target| {
                let _preview = preview_context.as_ref().map(|context| context.enter());
                let tool_name = target.adapter.tool_name().to_owned();
                let (status, requires_new_shell) = match target.installed {
                    Ok(false) => (ToolApplyStatus::Skipped(SkipReason::NotInstalled), false),
                    Ok(true) => match apply_call(target.adapter) {
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

    /// Env-injecting variant of [`apply_theme_to_tools`] / [`apply_theme_to_all`].
    /// Dispatches through [`ToolAdapter::apply_theme_with_env`] so the four
    /// preview-path adapters (Ghostty, Alacritty, Kitty, Starship) resolve all
    /// paths via the injected `env` instead of `SlateEnv::from_process()`. The
    /// other 10 adapters inherit the trait's default implementation (which
    /// simply delegates to `apply_theme`), so their behavior is unchanged.
    /// Pass `allowed_tools = None` to apply to every themeable adapter (the
    /// equivalent of `apply_theme_to_all`); pass `Some(&set)` to restrict to a
    /// caller-selected subset (the equivalent of `apply_theme_to_tools`).
    pub fn apply_theme_to_tools_with_env(
        &self,
        theme: &ThemeVariant,
        env: &SlateEnv,
        allowed_tools: Option<&HashSet<String>>,
    ) -> Vec<ToolApplyResult> {
        self.apply_theme_with_filter_env(theme, env, allowed_tools)
    }

    fn apply_theme_with_filter(
        &self,
        theme: &ThemeVariant,
        allowed_tools: Option<&HashSet<String>>,
    ) -> Vec<ToolApplyResult> {
        self.apply_theme_with_filter_inner(
            allowed_tools,
            |adapter| adapter.is_installed(),
            |adapter| adapter.apply_theme(theme),
        )
    }

    fn apply_theme_with_filter_env(
        &self,
        theme: &ThemeVariant,
        env: &SlateEnv,
        allowed_tools: Option<&HashSet<String>>,
    ) -> Vec<ToolApplyResult> {
        self.apply_theme_with_filter_inner(
            allowed_tools,
            |adapter| adapter.is_installed_with_env(env),
            |adapter| adapter.apply_theme_with_env(theme, env),
        )
    }

    /// Shared body for the two public apply-with-filter variants.
    /// WR-03 (review): the non-env and env-aware paths are identical
    /// aside from which `ToolAdapter` method they invoke. Any future change to
    /// the filter predicate, `ToolApplyResult` shape, or error-to-status
    /// mapping only needs to land here. `apply_call` is the per-adapter hook
    /// that selects `apply_theme` vs `apply_theme_with_env`; it must be `Fn +
    /// Sync` because rayon parallelises the map. `ToolAdapter: Send + Sync`
    /// already makes adapter handles thread-safe.
    fn apply_theme_with_filter_inner<I, F>(
        &self,
        allowed_tools: Option<&HashSet<String>>,
        is_installed_call: I,
        apply_call: F,
    ) -> Vec<ToolApplyResult>
    where
        I: Fn(&dyn ToolAdapter) -> Result<bool> + Sync,
        F: Fn(&dyn ToolAdapter) -> Result<ApplyOutcome> + Sync,
    {
        self.prepare_with_filter(allowed_tools, is_installed_call)
            .apply_with(apply_call)
    }

    pub(crate) fn prepare_theme_with_env(
        &self,
        env: &SlateEnv,
        allowed_tools: Option<&HashSet<String>>,
    ) -> PreparedThemeApply<'_> {
        self.prepare_with_filter(allowed_tools, |adapter| adapter.is_installed_with_env(env))
    }

    fn prepare_with_filter(
        &self,
        allowed_tools: Option<&HashSet<String>>,
        is_installed_call: impl Fn(&dyn ToolAdapter) -> Result<bool> + Sync,
    ) -> PreparedThemeApply<'_> {
        let targets = self
            .adapters
            .par_iter()
            .filter(|adapter| adapter.apply_strategy() != ApplyStrategy::DetectAndInstall)
            .filter(|adapter| {
                allowed_tools.is_none_or(|allowed| allowed.contains(adapter.tool_name()))
            })
            .map(|adapter| PreparedTarget {
                adapter: adapter.as_ref(),
                installed: is_installed_call(adapter.as_ref()),
            })
            .collect();
        PreparedThemeApply { targets }
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

/// Aggregate `requires_new_shell` across a batch of adapter results.
/// / D-D6: returns `true` iff at least one result is a successful
/// apply (`ToolApplyStatus::Applied`) **and** carries `requires_new_shell ==
/// true`. `Failed` and `Skipped` results never contribute — the aggregator
/// reflects changes that actually landed, so it only counts successes.
/// Intended consumer: the four CLI command handlers (`setup`, `theme`, `font`,
/// `config`) in . Each handler reads this bool once at the end of
/// its run and decides whether to emit the platform-aware new-terminal
/// reminder. Keeping the aggregator as a free function (not a method on
/// `ToolRegistry` / `ToolApplyResult`) matches RESEARCH §Pattern 5 Option A
/// and keeps call sites to a single one-liner.
pub fn requires_new_shell(results: &[ToolApplyResult]) -> bool {
    results
        .iter()
        .any(|r| matches!(r.status, ToolApplyStatus::Applied) && r.requires_new_shell)
}

impl Default for ToolRegistry {
    fn default() -> Self {
        let mut registry = Self::new();
        // Shared catalog for theme application and availability reporting.
        registry.register(Box::new(crate::adapter::GhosttyAdapter));
        registry.register(Box::new(crate::adapter::AlacrittyAdapter));
        registry.register(Box::new(crate::adapter::KittyAdapter));
        registry.register(Box::new(crate::adapter::StarshipAdapter));
        registry.register(Box::new(crate::adapter::BatAdapter));
        registry.register(Box::new(crate::adapter::BtopAdapter));
        registry.register(Box::new(crate::adapter::YaziAdapter));
        registry.register(Box::new(crate::adapter::ZellijAdapter));
        registry.register(Box::new(crate::adapter::DeltaAdapter));
        registry.register(Box::new(crate::adapter::EzaAdapter));
        registry.register(Box::new(crate::adapter::LazygitAdapter));
        registry.register(Box::new(crate::adapter::FastfetchAdapter));
        registry.register(Box::new(crate::adapter::LsColorsAdapter));
        registry.register(Box::new(crate::adapter::ZshHighlightAdapter));
        registry.register(Box::new(crate::adapter::TmuxAdapter));
        registry.register(Box::new(crate::adapter::FontAdapter));
        registry.register(Box::new(crate::adapter::NvimAdapter));
        registry.register(Box::new(crate::adapter::OpencodeAdapter));
        registry
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepared_theme_apply_probes_once_and_retains_ready_missing_and_failed_states() {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        let mut registry = ToolRegistry::new();
        for (name, strategy) in [
            ("ready", ApplyStrategy::EnvironmentVariable),
            ("missing", ApplyStrategy::EnvironmentVariable),
            ("failed", ApplyStrategy::EnvironmentVariable),
            ("unselected", ApplyStrategy::EnvironmentVariable),
            ("installer", ApplyStrategy::DetectAndInstall),
        ] {
            registry.register(Box::new(MockAdapter {
                name,
                strategy,
                installed: true,
            }));
        }
        let selection = ["ready", "missing", "failed", "installer"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        let probes = AtomicUsize::new(0);
        let installed = AtomicBool::new(true);
        let prepared = registry.prepare_with_filter(Some(&selection), |adapter| {
            probes.fetch_add(1, Ordering::SeqCst);
            match adapter.tool_name() {
                "ready" => Ok(installed.load(Ordering::SeqCst)),
                "missing" => Ok(false),
                "failed" => Err(crate::error::SlateError::Internal("probe failure".into())),
                _ => panic!("unselected/install-only adapter must not be probed"),
            }
        });
        assert_eq!(probes.load(Ordering::SeqCst), 3);
        assert_eq!(prepared.ready_tools().collect::<Vec<_>>(), ["ready"]);
        installed.store(false, Ordering::SeqCst);
        let calls = AtomicUsize::new(0);
        let results = prepared.apply_with(|adapter| {
            assert_eq!(adapter.tool_name(), "ready");
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(ApplyOutcome::applied_needs_new_shell())
        });
        assert_eq!(probes.load(Ordering::SeqCst), 3);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(matches!(results[0].status, ToolApplyStatus::Applied));
        assert!(results[0].requires_new_shell);
        assert!(matches!(
            results[1].status,
            ToolApplyStatus::Skipped(SkipReason::NotInstalled)
        ));
        assert!(
            matches!(&results[2].status, ToolApplyStatus::Failed(error) if error.to_string().contains("probe failure"))
        );
    }

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
        assert_eq!(registry.adapters().len(), 18);
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

#[cfg(test)]
mod requires_new_shell_tests {
    //! Task 2: D-D6-compliant aggregator — only successful
    //! `Applied` results with `requires_new_shell == true` contribute;
    //! `Failed` / `Skipped` never do.

    use super::*;
    use crate::adapter::SkipReason;
    use crate::error::SlateError;

    fn applied(name: &str, needs_new_shell: bool) -> ToolApplyResult {
        ToolApplyResult {
            tool_name: name.to_string(),
            status: ToolApplyStatus::Applied,
            requires_new_shell: needs_new_shell,
        }
    }

    fn failed(name: &str, needs_new_shell: bool) -> ToolApplyResult {
        ToolApplyResult {
            tool_name: name.to_string(),
            status: ToolApplyStatus::Failed(SlateError::Internal("test failure".into())),
            requires_new_shell: needs_new_shell,
        }
    }

    fn skipped(name: &str, needs_new_shell: bool) -> ToolApplyResult {
        ToolApplyResult {
            tool_name: name.to_string(),
            status: ToolApplyStatus::Skipped(SkipReason::NotInstalled),
            requires_new_shell: needs_new_shell,
        }
    }

    #[test]
    fn requires_new_shell_true_when_any_applied_with_flag() {
        let results = vec![applied("bat", true)];
        assert!(requires_new_shell(&results));
    }

    #[test]
    fn requires_new_shell_false_when_all_applied_without_flag() {
        let results = vec![
            applied("ghostty", false),
            applied("alacritty", false),
            applied("kitty", false),
        ];
        assert!(!requires_new_shell(&results));
    }

    #[test]
    fn requires_new_shell_ignores_failed_adapters() {
        // D-D6: failures never contribute even if the failed adapter set the
        // signal bool to true.
        let results = vec![failed("bat", true)];
        assert!(!requires_new_shell(&results));
    }

    #[test]
    fn requires_new_shell_ignores_skipped_adapters() {
        let results = vec![skipped("ls_colors", true)];
        assert!(!requires_new_shell(&results));
    }

    #[test]
    fn requires_new_shell_mixed_succeed_and_fail() {
        let results = vec![applied("bat", true), failed("tmux", false)];
        assert!(requires_new_shell(&results));
    }

    #[test]
    fn requires_new_shell_empty_vec_is_false() {
        let results: Vec<ToolApplyResult> = Vec::new();
        assert!(!requires_new_shell(&results));
    }
}
