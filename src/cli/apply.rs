use crate::adapter::{SkipReason, ToolApplyResult, ToolApplyStatus, ToolRegistry};
use crate::config::ConfigManager;
use crate::env::SlateEnv;
use crate::error::Result;
use crate::opacity::OpacityPreset;
use crate::theme::ThemeVariant;
use std::collections::HashSet;

#[path = "apply/auto_pair.rs"]
mod auto_pair;
use auto_pair::AutoPairPolicy;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotPolicy {
    Create,
    Skip,
}

#[derive(Debug, Clone, Copy)]
pub struct ThemeApplyOptions<'a> {
    pub snapshot_policy: SnapshotPolicy,
    pub target_tools: Option<&'a [String]>,
}

#[derive(Debug, Clone, Copy)]
pub struct OpacityApplyOptions {
    pub persist_state: bool,
    pub reload_terminals: bool,
    /// Standalone config changes create a checkpoint. Imports and picker
    /// commits already own a wider recovery point; previews own a journal.
    pub snapshot_policy: SnapshotPolicy,
}

/// Coordinated result for a single theme application run.
#[derive(Debug)]
pub struct ThemeApplyReport {
    pub results: Vec<ToolApplyResult>,
    /// A required shared-file write failed after adapter execution. This is
    /// separate from adapter counts; callers must use `ensure_no_failures`.
    pub commit_failure: Option<ThemeCommitFailure>,
    /// Post-apply activation notices and nonfatal synchronization failures.
    /// Saved files are not rolled back; informational items are not errors.
    pub reload_warnings: Vec<ReloadWarning>,
    pub restore_point_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeCommitStage {
    ShellIntegration,
    CurrentTheme,
}

impl std::fmt::Display for ThemeCommitStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::ShellIntegration => "shared shell configuration",
            Self::CurrentTheme => "current theme tracking",
        })
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{stage}: {error}")]
pub struct ThemeCommitFailure {
    pub stage: ThemeCommitStage,
    #[source]
    pub error: crate::error::SlateError,
}

#[derive(Debug)]
pub struct ReloadWarning {
    pub informational: bool,
    pub tool_name: String,
    pub message: String,
}

impl ReloadWarning {
    fn display_message(&self, interactive: bool) -> Option<&str> {
        if interactive && self.informational {
            None
        } else {
            Some(&self.message)
        }
    }
}

impl ThemeApplyReport {
    fn failure_details(&self) -> Vec<String> {
        let mut failures: Vec<_> = self
            .results
            .iter()
            .filter_map(|result| match &result.status {
                ToolApplyStatus::Failed(err) => Some(format!("{}: {err}", result.tool_name)),
                _ => None,
            })
            .collect();
        if let Some(failure) = &self.commit_failure {
            failures.push(failure.to_string());
        }
        failures
    }

    pub fn ensure_no_failures(&self) -> Result<()> {
        let failures = self.failure_details();
        if failures.is_empty() {
            return Ok(());
        }
        let recovery = self
            .restore_point_id
            .as_ref()
            .map(|id| format!(" Inspect recovery with: slate restore {id} --dry-run."))
            .unwrap_or_default();
        Err(crate::error::SlateError::InvalidConfig(format!(
            "Theme application was incomplete ({} integration(s) succeeded): {}. Earlier file writes, if any, were not rolled back.{}",
            self.applied_count(),
            failures.join("; "),
            recovery,
        )))
    }
    pub fn applied_count(&self) -> usize {
        self.results
            .iter()
            .filter(|result| matches!(result.status, ToolApplyStatus::Applied))
            .count()
    }

    pub fn skipped_count(&self) -> usize {
        self.results
            .iter()
            .filter(|result| matches!(result.status, ToolApplyStatus::Skipped(_)))
            .count()
    }

    /// Failed adapters (including post-commit notifications), not shared commit
    /// failures. A nonzero count does not imply the old theme is still current.
    /// Use ensure_no_failures for the whole operation and its failure context.
    pub fn failed_count(&self) -> usize {
        self.results
            .iter()
            .filter(|result| matches!(result.status, ToolApplyStatus::Failed(_)))
            .count()
    }

    pub fn ghostty_applied(&self) -> bool {
        self.results.iter().any(|result| {
            result.tool_name == "ghostty" && matches!(result.status, ToolApplyStatus::Applied)
        })
    }
}

/// Coordinates adapter execution and only commits theme state after a real apply.
pub struct ThemeApplyCoordinator<'a> {
    env: &'a SlateEnv,
    snapshot_policy: SnapshotPolicy,
    checkpoint_opacity: bool,
    auto_pair_policy: AutoPairPolicy,
    defer_terminal_reload: bool,
}

impl<'a> ThemeApplyCoordinator<'a> {
    pub fn new(env: &'a SlateEnv) -> Self {
        Self::with_snapshot_policy(env, SnapshotPolicy::Create)
    }

    pub fn with_snapshot_policy(env: &'a SlateEnv, snapshot_policy: SnapshotPolicy) -> Self {
        Self {
            env,
            snapshot_policy,
            checkpoint_opacity: false,
            auto_pair_policy: AutoPairPolicy::RememberManualSelection,
            defer_terminal_reload: false,
        }
    }

    /// Picker commits also save opacity after the theme stage. Capture that
    /// stage's outputs now, before any part of the combined selection writes.
    pub(crate) fn including_opacity(mut self) -> Self {
        self.checkpoint_opacity = true;
        self
    }

    /// The picker publishes opacity next and owns the final terminal refresh.
    pub(crate) fn deferring_terminal_reload(mut self) -> Self {
        self.defer_terminal_reload = true;
        self
    }

    /// Automatic application and legacy restore reapplication consume saved
    /// preferences; they must not learn a new pair from a later desktop reading.
    pub(crate) fn preserving_auto_pair(mut self) -> Self {
        self.auto_pair_policy = AutoPairPolicy::Preserve;
        self
    }

    pub fn apply(&self, theme: &ThemeVariant) -> Result<ThemeApplyReport> {
        apply_theme_with_options(
            self.env,
            theme,
            ThemeApplyOptions {
                snapshot_policy: self.snapshot_policy,
                target_tools: None,
            },
            self.checkpoint_opacity,
            self.auto_pair_policy,
            self.defer_terminal_reload,
        )
    }

    pub fn apply_to_tools(
        &self,
        theme: &ThemeVariant,
        tool_names: &[String],
    ) -> Result<ThemeApplyReport> {
        apply_theme_with_options(
            self.env,
            theme,
            ThemeApplyOptions {
                snapshot_policy: self.snapshot_policy,
                target_tools: Some(tool_names),
            },
            self.checkpoint_opacity,
            self.auto_pair_policy,
            self.defer_terminal_reload,
        )
    }
}

pub fn log_apply_report(report: &ThemeApplyReport) {
    log_apply_report_with_summary(report, false);
}

/// A successfully printed menu summary already includes applied tools and the
/// recovery command. Never suppress failures, skips or reload warnings.
pub(crate) fn log_apply_report_with_summary(report: &ThemeApplyReport, summarized: bool) {
    if !summarized || report.applied_count() == 0 {
        if let Some(id) = &report.restore_point_id {
            eprintln!("Restore point: {id}");
        }
    }
    for result in &report.results {
        if !should_log_apply_result(result)
            || (summarized && matches!(result.status, ToolApplyStatus::Applied))
        {
            continue;
        }
        match &result.status {
            ToolApplyStatus::Applied => eprintln!("✓ {}", result.tool_name),
            ToolApplyStatus::Skipped(SkipReason::MissingIntegrationConfig) => {
                eprintln!("○ {}: missing integration config", result.tool_name)
            }
            ToolApplyStatus::Skipped(SkipReason::NotInstalled) => {}
            ToolApplyStatus::Skipped(SkipReason::ThemeNotCommitted) => {
                eprintln!(
                    "○ {}: theme was not committed; notification was not sent",
                    result.tool_name
                );
            }
            ToolApplyStatus::Failed(err) => eprintln!("❌ {}: {}", result.tool_name, err),
        }
    }
    log_apply_warnings(report);
    if let Some(failure) = &report.commit_failure {
        eprintln!("❌ {failure}");
    }
}

/// Interactive output omits routine activation info, never actual warnings.
/// Redirected output retains the full notices for diagnostics. Call after any
/// stderr redirection or interactive terminal guard has been dropped.
pub(crate) fn log_apply_warnings(report: &ThemeApplyReport) {
    log_apply_notices(report, false);
}

pub(crate) fn log_apply_errors(report: &ThemeApplyReport) {
    log_apply_notices(report, true);
}

fn log_apply_notices(report: &ThemeApplyReport, quiet: bool) {
    use std::io::IsTerminal;
    let interactive = std::io::stdout().is_terminal() && std::io::stderr().is_terminal();
    for warning in &report.reload_warnings {
        if quiet && warning.informational {
            continue;
        }
        let Some(message) = warning.display_message(interactive) else {
            continue;
        };
        let level = if warning.informational {
            "info"
        } else {
            "warning"
        };
        eprintln!("{level}: {}: {message}", warning.tool_name);
    }
}

fn should_log_apply_result(result: &ToolApplyResult) -> bool {
    // `ls_colors` is an internal shell-integration layer, not a user-selected
    // tool. Keep it in the apply report for state/reload decisions, but avoid
    // surfacing a pseudo-tool line in user-facing progress output.
    result.tool_name != "ls_colors"
}

fn apply_theme_with_options(
    env: &SlateEnv,
    theme: &ThemeVariant,
    options: ThemeApplyOptions<'_>,
    checkpoint_opacity: bool,
    auto_pair_policy: AutoPairPolicy,
    defer_terminal_reload: bool,
) -> Result<ThemeApplyReport> {
    apply_theme_with_registry(
        env,
        theme,
        options,
        checkpoint_opacity,
        auto_pair_policy,
        defer_terminal_reload,
        &ToolRegistry::default(),
    )
}

fn apply_theme_with_registry(
    env: &SlateEnv,
    theme: &ThemeVariant,
    options: ThemeApplyOptions<'_>,
    checkpoint_opacity: bool,
    auto_pair_policy: AutoPairPolicy,
    defer_terminal_reload: bool,
    registry: &ToolRegistry,
) -> Result<ThemeApplyReport> {
    theme.validate()?;
    if matches!(options.snapshot_policy, SnapshotPolicy::Create) {
        crate::config::recovery_paths::validate_storage_paths(env, "Theme")?;
    }
    let _write_guard = crate::config::ConfigWriteGuard::acquire(env)?;
    // Initialize before any adapter writes so an initialization error cannot
    // discard an already-partial apply report.
    let config = ConfigManager::with_env(env)?;
    // Resolve required shared preferences before any adapter can modify files.
    let shell_files = config.prepare_shell_files(theme).map_err(|error| {
        crate::error::SlateError::InvalidConfig(format!(
            "Cannot prepare shared shell configuration: {error}. No theme files were written."
        ))
    })?;

    let selected: Option<HashSet<String>> = options
        .target_tools
        .map(|tools| tools.iter().cloned().collect());
    let prepared = registry.prepare_theme_with_env(env, selected.as_ref());

    let restore_point_id = if matches!(options.snapshot_policy, SnapshotPolicy::Create) {
        let mut files = crate::adapter::write_paths::shared_theme_paths(env);
        if checkpoint_opacity {
            files.insert(env.managed_file("current-opacity"));
            files.extend(crate::opacity::managed_paths(env));
        }
        for tool in prepared.ready_tools() {
            crate::adapter::write_paths::add_theme_paths(env, tool, &mut files)?;
        }
        let targets = crate::config::recovery_paths::targets(env, files, "Theme")?;
        let point = crate::config::snapshot_theme_targets_with_env(env, &targets)?;
        Some(point.id)
    } else {
        None
    };

    // Availability is not probed again after the checkpoint. A tool newly
    // appearing later cannot gain an uncaptured write target in this operation.
    let (prepared, notifications) = prepared.split_post_commit();
    let has_notifications = notifications.ready_tools().next().is_some();
    let nvim_will_be_notified = notifications.ready_tools().any(|tool| tool == "nvim");
    let results = prepared.apply_with_env(theme, env);
    let mut report = ThemeApplyReport {
        results,
        commit_failure: None,
        reload_warnings: Vec::new(),
        restore_point_id,
    };

    if report.failed_count() > 0 {
        // Adapters run independently, so successful writes remain available.
        // Do not declare a new global theme, update auto pairs, or notify Neovim
        // while the tools disagree. The report retains the safety snapshot ID.
        report.results.extend(notifications.skip_uncommitted());
        reload_theme_targets(env, registry, &mut report);
        return Ok(report);
    }

    // Shared shell files are required even when no adapters applied. Do not
    // advertise a new current theme until every required shared write succeeds.
    // This is ordered publication, not a multi-file transaction: earlier files
    // may already have changed, so retain their report and recovery point.
    if let Err(error) = config.publish_shell_files(shell_files) {
        report.commit_failure = Some(ThemeCommitFailure {
            stage: ThemeCommitStage::ShellIntegration,
            error,
        });
        report.results.extend(notifications.skip_uncommitted());
        reload_theme_targets(env, registry, &mut report);
        return Ok(report);
    }
    if report.applied_count() == 0 && !has_notifications {
        return Ok(report);
    }
    if let Err(error) = config.set_current_theme(&theme.id) {
        report.commit_failure = Some(ThemeCommitFailure {
            stage: ThemeCommitStage::CurrentTheme,
            error,
        });
        report.results.extend(notifications.skip_uncommitted());
        reload_theme_targets(env, registry, &mut report);
        return Ok(report);
    }

    if let Some(warning) = auto_pair_policy.record_after_commit(
        &config,
        &theme.id,
        crate::cli::auto_theme::detect_system_appearance,
    ) {
        report.reload_warnings.push(warning);
    }

    // Ready notification adapters run exactly once, only after current-theme
    // publication. A failed notification truthfully reports that the theme is
    // already saved; retry/restore is explicit, not an automatic second write.
    report
        .results
        .extend(notifications.notify_after_commit(theme, env));

    // Preserve the shared tail hook when Neovim was not a ready selected
    // adapter: an already-running editor may still watch this state file.
    // Do not duplicate or silently retry a selected Neovim notification.
    // One publication can yield multiple/coalesced filesystem events and is
    // not evidence that an editor received or rendered the theme.
    if !nvim_will_be_notified {
        if let Err(err) = crate::adapter::nvim::write_state_file(env, &theme.id) {
            report.reload_warnings.push(ReloadWarning {
                informational: false,
                tool_name: "nvim".into(),
                message: format!("Theme '{}' was already saved, but the shared editor notification failed: {err}", theme.id.escape_default()),
            });
        }
    }

    let scope = if defer_terminal_reload && report.failed_count() == 0 {
        ReloadScope::NonTerminal
    } else {
        ReloadScope::All
    };
    reload_theme_targets_with_scope(env, registry, &mut report, scope);
    Ok(report)
}

pub(crate) fn apply_opacity(
    env: &SlateEnv,
    opacity: OpacityPreset,
    options: OpacityApplyOptions,
) -> Result<()> {
    apply_opacity_with_effects(
        env,
        opacity,
        options,
        || {},
        || {
            let registry = ToolRegistry::default();
            reload_saved_opacity_with(
                || {
                    if let Some(adapter) = registry.get_adapter("ghostty") {
                        if adapter.is_installed_with_env(env)? {
                            adapter.reload_with_env(env)?;
                        }
                    }
                    Ok(())
                },
                || crate::adapter::kitty::try_push_opacity_live(opacity),
                |tool, error| {
                    eprintln!(
                        "warning: {tool}: Opacity files saved; live refresh was not confirmed: {}",
                        super::file_output::terminal_text(&error.to_string())
                    )
                },
            );
        },
    )
}

fn reload_saved_opacity_with(
    ghostty: impl FnOnce() -> Result<()>,
    kitty: impl FnOnce() -> Result<()>,
    mut warn: impl FnMut(&str, &crate::error::SlateError),
) {
    // Reload is best-effort after publication, not part of the file transaction.
    // A failure must not skip another terminal or suggest the saved files failed.
    if let Err(error) = ghostty() {
        warn("ghostty", &error);
    }
    if let Err(error) = kitty() {
        warn("kitty", &error);
    }
}

fn apply_opacity_with_effects(
    env: &SlateEnv,
    opacity: OpacityPreset,
    options: OpacityApplyOptions,
    after_checkpoint: impl FnOnce(),
    reload: impl FnOnce(),
) -> Result<()> {
    let _write_guard = if options.persist_state {
        Some(crate::config::ConfigWriteGuard::acquire(env)?)
    } else {
        None
    }; // Preview writes are already covered by their journal lock.
    let paths = crate::opacity::managed_paths(env).chain(
        options
            .persist_state
            .then(|| env.managed_file("current-opacity")),
    );
    let targets = crate::config::recovery_paths::targets(env, paths, "Opacity")?;
    if targets.len() != crate::opacity::MANAGED_FILES.len() + usize::from(options.persist_state) {
        return Err(crate::error::SlateError::InvalidConfig(
            "Opacity cancelled before applying settings: distinct terminal outputs resolve to the same file; separate their managed directories first.".into(),
        ));
    }
    let prepared = crate::opacity::PreparedOpacity::capture(env, opacity, options.persist_state)?;
    let changed = prepared.changed();
    let point =
        if changed && options.persist_state && options.snapshot_policy == SnapshotPolicy::Create {
            let point = crate::config::snapshot_opacity_targets_with_env(env, &targets)?;
            eprintln!("Pre-opacity recovery point: {}", point.id);
            eprintln!(
                "Inspect file recovery: slate restore {} --dry-run",
                point.id
            );
            Some(point)
        } else {
            None
        };
    if changed {
        after_checkpoint();
    }
    if let Err(error) = prepared.publish() {
        let recovery = point
            .as_ref()
            .map(|p| {
                format!(
                    " Inspect file recovery with: slate restore {} --dry-run.",
                    p.id
                )
            })
            .unwrap_or_default();
        return Err(crate::error::SlateError::InvalidConfig(format!(
            "Opacity application was incomplete: {error}. Earlier file writes, if any, were not rolled back.{recovery}"
        )));
    }

    // A repeated explicit request may still be useful for live state. Only the
    // disk work is skipped; retain the existing, session-gated reload behavior.
    if options.reload_terminals && env.session().can_reload_terminal() {
        reload();
    }

    Ok(())
}

pub(crate) fn preview_theme(
    env: &SlateEnv,
    theme: &ThemeVariant,
    opacity: OpacityPreset,
) -> Result<()> {
    theme.validate()?;

    // The picker can render an inline preview over SSH, but cannot preview
    // the client application's chrome by editing files on the remote host.
    if env.session().is_remote() {
        return Ok(());
    }

    let config = ConfigManager::with_env(env)?;
    let adapter_registry = ToolRegistry::default();
    // Live preview only touches adapters the user actually sees in this terminal window.
    // Rewriting starship/bat/delta on every picker keystroke is wasted IO and has no visible
    // effect until the user launches a new shell.
    // route through `apply_theme_to_tools_with_env` so the injected
    // `env` (e.g. a tempdir-backed `SlateEnv::with_home(...)` from integration
    // tests) actually reaches the preview-path adapters. This is what closes
    // the 19-08 "signature lie" at the `silent_preview_apply(&env, …)` boundary.
    let preview_targets: HashSet<String> = ["ghostty", "alacritty", "kitty"]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    let results =
        adapter_registry.apply_theme_to_tools_with_env(theme, env, Some(&preview_targets));
    let failures = ThemeApplyReport {
        results,
        commit_failure: None,
        reload_warnings: Vec::new(),
        restore_point_id: None,
    }
    .failure_details();
    if !failures.is_empty() {
        return Err(crate::error::SlateError::InvalidConfig(format!(
            "Live preview failed: {}",
            failures.join("; "),
        )));
    }

    apply_opacity(
        env,
        opacity,
        OpacityApplyOptions {
            persist_state: false,
            reload_terminals: false,
            snapshot_policy: SnapshotPolicy::Skip,
        },
    )?;

    if env.session().is_isolated() {
        return Ok(());
    }
    apply_ghostty_live_preview_reload(&config, &adapter_registry);

    if let Some(kitty_adapter) = adapter_registry.get_adapter("kitty") {
        let _ = kitty_adapter.reload();
    }
    crate::adapter::kitty::push_opacity_live(opacity);

    Ok(())
}

pub(super) fn reload_theme_targets(
    env: &SlateEnv,
    registry: &ToolRegistry,
    report: &mut ThemeApplyReport,
) {
    reload_theme_targets_with_scope(env, registry, report, ReloadScope::All);
}

#[derive(Clone, Copy)]
enum ReloadScope {
    All,
    NonTerminal,
    Terminal,
}

fn reload_theme_targets_with_scope(
    env: &SlateEnv,
    registry: &ToolRegistry,
    report: &mut ThemeApplyReport,
    scope: ReloadScope,
) {
    if report.results.iter().any(|result| {
        result.tool_name == "opencode" && matches!(result.status, ToolApplyStatus::Applied)
    }) {
        report.reload_warnings.push(ReloadWarning {
            informational: true,
            tool_name: "opencode".into(),
            message: "OpenCode TUI configuration saved with theme = system. Reopen OpenCode to check terminal-derived colors; Slate did not close or restart any running session. A new shell is not required for this file change. Live appearance is not verified.".into(),
        });
    }
    if report.results.iter().any(|result| {
        result.tool_name == "zellij" && matches!(result.status, ToolApplyStatus::Applied)
    }) {
        report.reload_warnings.push(ReloadWarning {
            informational: true,
            tool_name: "zellij".into(),
            message: "Zellij theme files saved. Static/dark/light choices follow Slate's saved theme. Native file watching may update a session, but no session was queried or commanded; layout/CLI overrides can still win. Start a new session with this config if needed.".into(),
        });
    }
    if report.results.iter().any(|result| {
        result.tool_name == "yazi" && matches!(result.status, ToolApplyStatus::Applied)
    }) {
        report.reload_warnings.push(ReloadWarning {
            informational: true,
            tool_name: "yazi".into(),
            message: "Flavor and code-preview colors saved; reopen Yazi. Personal theme.toml overrides still win. Both flavor slots follow Slate's saved theme; no running file manager was reloaded.".into(),
        });
    }
    if report.results.iter().any(|result| {
        result.tool_name == "btop" && matches!(result.status, ToolApplyStatus::Applied)
    }) {
        report.reload_warnings.push(ReloadWarning {
            informational: true,
            tool_name: "btop".into(),
            message: "Theme files saved; reopen btop to use them. A running btop may save its older theme on exit; reapply Slate afterward if needed.".into(),
        });
    }
    reload_applied_targets(env, registry, report, scope);
}

fn reload_applied_targets(
    env: &SlateEnv,
    registry: &ToolRegistry,
    report: &mut ThemeApplyReport,
    scope: ReloadScope,
) {
    reload_applied_targets_with(env, report, scope, |name| {
        registry
            .get_adapter(name)
            .map_or(Ok(()), |adapter| adapter.reload_with_env(env))
    });
}

fn reload_applied_targets_with(
    env: &SlateEnv,
    report: &mut ThemeApplyReport,
    scope: ReloadScope,
    mut reload: impl FnMut(&str) -> Result<()>,
) {
    if env.session().is_isolated() {
        return;
    }
    for result in &report.results {
        if !matches!(result.status, ToolApplyStatus::Applied)
            || !matches!(result.tool_name.as_str(), "ghostty" | "kitty" | "tmux")
        {
            continue;
        }
        let terminal = matches!(result.tool_name.as_str(), "ghostty" | "kitty");
        if matches!(scope, ReloadScope::NonTerminal) && terminal
            || matches!(scope, ReloadScope::Terminal) && !terminal
        {
            continue;
        }
        if env.session().is_remote() && result.tool_name != "tmux" {
            report.reload_warnings.push(ReloadWarning { informational: true, tool_name: result.tool_name.clone(),
                message: "Configuration saved on the remote host; SSH cannot reload your client terminal.".into() });
            continue;
        }
        if let Err(err) = reload(&result.tool_name) {
            report.reload_warnings.push(ReloadWarning {
                informational: matches!(&err, crate::error::SlateError::NoDefaultTmuxServer),
                tool_name: result.tool_name.clone(),
                message: err.to_string(),
            });
        }
    }
}

pub(crate) fn finish_picker_terminal_reload(env: &SlateEnv, report: &mut ThemeApplyReport) {
    // Ghostty rereads both files; Kitty reload reads the saved opacity as well
    // as colors. Do not follow it with another dynamic-opacity broadcast.
    reload_applied_targets(env, &ToolRegistry::default(), report, ReloadScope::Terminal);
}

fn apply_ghostty_live_preview_reload(config: &ConfigManager, registry: &ToolRegistry) {
    let Some(ghostty_adapter) = registry.get_adapter("ghostty") else {
        return;
    };

    if !is_ghostty() {
        return;
    }

    match config.is_live_preview_state_known() {
        Ok(true) => {
            if let Ok(enabled) = config.is_live_preview_enabled() {
                if enabled {
                    let _ = ghostty_adapter.reload();
                }
            }
        }
        Ok(false) => match ghostty_adapter.reload() {
            Ok(()) => {
                let _ = config.set_live_preview_enabled(true);
            }
            Err(_) => {
                let _ = config.set_live_preview_enabled(false);
            }
        },
        Err(_) => {
            let _ = ghostty_adapter.reload();
        }
    }
}

fn is_ghostty() -> bool {
    crate::session::SessionContext::from_process().can_reload_terminal()
        && std::env::var("TERM_PROGRAM")
            .map(|term_program| term_program.eq_ignore_ascii_case("ghostty"))
            .unwrap_or(false)
}

#[cfg(test)]
#[path = "apply/notification_tests.rs"]
mod notification_tests;

#[cfg(test)]
#[path = "apply/notice_tests.rs"]
mod notice_tests;

#[cfg(test)]
#[path = "apply/reload_tests.rs"]
mod reload_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::catppuccin;
    use tempfile::TempDir;

    #[test]
    fn activation_instructions_are_informational_not_reload_failures() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        let mut report = ThemeApplyReport {
            results: ["opencode", "btop", "yazi", "zellij"]
                .into_iter()
                .map(|tool| ToolApplyResult {
                    tool_name: tool.into(),
                    status: ToolApplyStatus::Applied,
                    requires_new_shell: false,
                })
                .collect(),
            commit_failure: None,
            reload_warnings: Vec::new(),
            restore_point_id: None,
        };
        reload_theme_targets(&env, &ToolRegistry::default(), &mut report);
        assert_eq!(report.reload_warnings.len(), 4);
        assert!(report
            .reload_warnings
            .iter()
            .all(|notice| notice.informational));
        for notice in &report.reload_warnings {
            assert_eq!(notice.display_message(true), None);
            assert_eq!(notice.display_message(false), Some(notice.message.as_str()));
        }
        let failure = ReloadWarning {
            informational: false,
            tool_name: "tmux".into(),
            message: "Reload failed; inspect the session before retrying".into(),
        };
        assert_eq!(
            failure.display_message(true),
            Some(failure.message.as_str())
        );
        assert_eq!(
            failure.display_message(false),
            Some(failure.message.as_str())
        );
        assert_eq!(std::fs::read_dir(td.path()).unwrap().count(), 0);
    }

    #[test]
    fn theme_checkpoint_picker_includes_the_later_opacity_stage_and_its_absence() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        let _config = ConfigManager::with_env(&env).unwrap();
        std::fs::write(env.managed_file("current-opacity"), b"solid\n").unwrap();
        let theme = catppuccin::catppuccin_mocha().unwrap();
        let report = ThemeApplyCoordinator::new(&env)
            .including_opacity()
            .apply_to_tools(&theme, &["ls_colors".into()])
            .unwrap();
        report.ensure_no_failures().unwrap();
        let id = report.restore_point_id.as_ref().unwrap();
        let point = crate::config::get_restore_point_with_env(&env, id).unwrap();
        for path in crate::opacity::managed_paths(&env) {
            assert!(point.entries.iter().any(|entry| entry.original_path == path
                && entry.original_state == crate::config::OriginalFileState::Absent));
        }
        apply_opacity(
            &env,
            OpacityPreset::Frosted,
            OpacityApplyOptions {
                persist_state: true,
                reload_terminals: false,
                snapshot_policy: SnapshotPolicy::Skip,
            },
        )
        .unwrap();
        assert_ne!(
            std::fs::read(env.managed_file("current-opacity")).unwrap(),
            b"solid\n"
        );
        assert!(crate::config::execute_restore_with_env(&env, id)
            .unwrap()
            .is_fully_successful());
        assert_eq!(
            std::fs::read(env.managed_file("current-opacity")).unwrap(),
            b"solid\n"
        );
        assert!(crate::opacity::managed_paths(&env)
            .into_iter()
            .all(|path| !path.exists()));
    }

    fn managed_tool_dir(env: &SlateEnv, tool: &str) -> std::path::PathBuf {
        env.config_dir().join("managed").join(tool)
    }

    fn count_restore_points(path: &std::path::Path) -> usize {
        std::fs::read_dir(path)
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|entry| entry.path().is_dir())
                    .count()
            })
            .unwrap_or(0)
    }

    #[test]
    fn test_report_counts_statuses() {
        let report = ThemeApplyReport {
            commit_failure: None,
            reload_warnings: Vec::new(),
            restore_point_id: None,
            results: vec![
                ToolApplyResult {
                    tool_name: "ghostty".to_string(),
                    status: ToolApplyStatus::Applied,
                    requires_new_shell: false,
                },
                ToolApplyResult {
                    tool_name: "alacritty".to_string(),
                    status: ToolApplyStatus::Skipped(SkipReason::MissingIntegrationConfig),
                    requires_new_shell: false,
                },
                ToolApplyResult {
                    tool_name: "starship".to_string(),
                    status: ToolApplyStatus::Failed(crate::error::SlateError::Internal(
                        "boom".to_string(),
                    )),
                    requires_new_shell: false,
                },
            ],
        };

        assert_eq!(report.applied_count(), 1);
        assert_eq!(report.skipped_count(), 1);
        assert_eq!(report.failed_count(), 1);
        assert!(report.ghostty_applied());
    }

    #[test]
    fn test_apply_report_skips_internal_ls_colors_adapter() {
        let result = ToolApplyResult {
            tool_name: "ls_colors".to_string(),
            status: ToolApplyStatus::Applied,
            requires_new_shell: true,
        };

        assert!(
            !should_log_apply_result(&result),
            "internal ls_colors layer should not print as a pseudo-tool in apply logs"
        );
    }

    #[test]
    fn test_apply_opacity_without_persisting_state_skips_current_file() {
        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());

        apply_opacity(
            &env,
            OpacityPreset::Frosted,
            OpacityApplyOptions {
                persist_state: false,
                reload_terminals: false,
                snapshot_policy: SnapshotPolicy::Skip,
            },
        )
        .unwrap();

        assert!(!env.managed_file("current-opacity").exists());
        assert!(managed_tool_dir(&env, "ghostty")
            .join("opacity.conf")
            .exists());
        assert!(managed_tool_dir(&env, "ghostty").join("blur.conf").exists());
        assert!(managed_tool_dir(&env, "alacritty")
            .join("opacity.toml")
            .exists());
        assert!(managed_tool_dir(&env, "kitty")
            .join("opacity.conf")
            .exists());
    }

    #[test]
    fn test_apply_opacity_with_persisting_state_writes_current_file() {
        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());

        apply_opacity(
            &env,
            OpacityPreset::Clear,
            OpacityApplyOptions {
                persist_state: true,
                reload_terminals: false,
                snapshot_policy: SnapshotPolicy::Skip,
            },
        )
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(env.managed_file("current-opacity")).unwrap(),
            "clear"
        );
    }

    #[test]
    fn opacity_failure_does_not_advance_the_saved_preset() {
        use std::os::unix::fs::symlink;
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        let config = ConfigManager::with_env(&env).unwrap();
        config
            .set_current_opacity_preset(OpacityPreset::Solid)
            .unwrap();
        let outside = td.path().join("private-original.conf");
        std::fs::write(&outside, "background_opacity 1\n").unwrap();
        let target = config.managed_dir("kitty").join("opacity.conf");
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        symlink(&outside, &target).unwrap();
        assert!(apply_opacity(
            &env,
            OpacityPreset::Clear,
            OpacityApplyOptions {
                persist_state: true,
                reload_terminals: false,
                snapshot_policy: SnapshotPolicy::Skip,
            }
        )
        .is_err());
        assert_eq!(
            config.get_current_opacity_preset().unwrap(),
            OpacityPreset::Solid
        );
        assert_eq!(
            std::fs::read_to_string(&outside).unwrap(),
            "background_opacity 1\n"
        );
    }

    #[test]
    fn opacity_late_failure_retains_checkpoint_and_never_reloads() {
        use std::os::unix::fs::symlink;
        for name in ["managed/kitty/opacity.conf", "current-opacity"] {
            let td = TempDir::new().unwrap();
            let env = SlateEnv::with_home(td.path().to_owned());
            let config = ConfigManager::with_env(&env).unwrap();
            config
                .set_current_opacity_preset(OpacityPreset::Solid)
                .unwrap();
            let target = env.managed_file(name);
            let external = td.path().join("private-original");
            std::fs::write(&external, "solid").unwrap();
            let error = apply_opacity_with_effects(
                &env,
                OpacityPreset::Clear,
                OpacityApplyOptions {
                    persist_state: true,
                    reload_terminals: true,
                    snapshot_policy: SnapshotPolicy::Create,
                },
                || {
                    // A deterministic late edit after the real checkpoint, before
                    // publication. Never race a background editor in this test.
                    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
                    if target.exists() {
                        std::fs::remove_file(&target).unwrap();
                    }
                    symlink(&external, &target).unwrap();
                },
                || panic!("failed opacity must not reload any terminal"),
            )
            .unwrap_err()
            .to_string();
            let points = crate::config::list_restore_points_with_env(&env).unwrap();
            assert_eq!(points.len(), 1);
            let point = &points[0];
            assert_eq!(point.theme_name, "pre-opacity");
            assert!(!point.reapplies_theme());
            assert!(
                error.contains(&format!("slate restore {} --dry-run", point.id)),
                "{error}"
            );
            assert!(
                error.contains(if name == "current-opacity" {
                    "saved opacity"
                } else {
                    "Kitty opacity"
                }),
                "{error}"
            );
            assert_eq!(
                config.get_current_opacity_preset().unwrap(),
                OpacityPreset::Solid
            );
            assert_eq!(std::fs::read_to_string(&external).unwrap(), "solid");
            assert!(env.managed_file("managed/ghostty/opacity.conf").exists());
            std::fs::remove_file(&target).unwrap(); // Repair private link before restore.
            assert!(crate::config::execute_restore_with_env(&env, &point.id)
                .unwrap()
                .is_fully_successful());
            assert_eq!(
                config.get_current_opacity_preset().unwrap(),
                OpacityPreset::Solid
            );
            assert!(!env.managed_file("managed/ghostty/opacity.conf").exists());
        }
    }

    #[test]
    fn opacity_reload_uses_captured_session_and_preview_never_saves_state() {
        for (session, requested, expected) in [
            ("isolated", true, false),
            ("remote", true, false),
            ("local", false, false),
            ("local", true, true),
        ] {
            let td = TempDir::new().unwrap();
            let env = if session == "isolated" {
                SlateEnv::with_home(td.path().to_owned())
            } else {
                SlateEnv::from_vars(|key| match key {
                    "HOME" => Some(td.path().as_os_str().to_owned()),
                    "SSH_CONNECTION" if session == "remote" => {
                        Some("captured remote session".into())
                    }
                    _ => None,
                })
                .unwrap()
            };
            let config = ConfigManager::with_env(&env).unwrap();
            config
                .set_current_opacity_preset(OpacityPreset::Solid)
                .unwrap();
            let reloaded = std::cell::Cell::new(false);
            apply_opacity_with_effects(
                &env,
                OpacityPreset::Clear,
                OpacityApplyOptions {
                    persist_state: false,
                    reload_terminals: requested,
                    snapshot_policy: SnapshotPolicy::Create,
                },
                || {},
                || reloaded.set(true),
            )
            .unwrap();
            assert_eq!(reloaded.get(), expected, "{session}");
            assert_eq!(
                config.get_current_opacity_preset().unwrap(),
                OpacityPreset::Solid
            );
            assert!(crate::config::list_restore_points_with_env(&env)
                .unwrap()
                .is_empty());
        }
    }

    #[test]
    fn opacity_noop_keeps_requested_reload_without_checkpoint_or_writes() {
        let td = TempDir::new().unwrap();
        let env =
            SlateEnv::from_vars(|key| (key == "HOME").then(|| td.path().as_os_str().to_owned()))
                .unwrap();
        let options = OpacityApplyOptions {
            persist_state: true,
            reload_terminals: true,
            snapshot_policy: SnapshotPolicy::Create,
        };
        apply_opacity_with_effects(&env, OpacityPreset::Frosted, options, || {}, || {}).unwrap();
        let reloaded = std::cell::Cell::new(false);
        apply_opacity_with_effects(
            &env,
            OpacityPreset::Frosted,
            options,
            || panic!("no-op must not enter checkpoint/write effects"),
            || reloaded.set(true),
        )
        .unwrap();
        assert!(reloaded.get());
        assert_eq!(
            crate::config::list_restore_points_with_env(&env)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn test_theme_apply_with_skip_snapshot_keeps_restore_directory_empty() {
        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let config = ConfigManager::with_env(&env).unwrap();
        config.set_current_theme("catppuccin-mocha").unwrap();

        let theme = catppuccin::catppuccin_latte().unwrap();
        let coordinator = ThemeApplyCoordinator::with_snapshot_policy(&env, SnapshotPolicy::Skip);

        coordinator.apply(&theme).unwrap();
        coordinator.apply(&theme).unwrap();

        assert_eq!(
            count_restore_points(&env.slate_cache_dir().join("backups")),
            0
        );
    }

    #[test]
    fn test_theme_apply_with_create_snapshot_records_previous_theme() {
        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let config = ConfigManager::with_env(&env).unwrap();
        config.set_current_theme("catppuccin-mocha").unwrap();

        let theme = catppuccin::catppuccin_latte().unwrap();
        ThemeApplyCoordinator::new(&env).apply(&theme).unwrap();

        assert_eq!(
            count_restore_points(&env.slate_cache_dir().join("backups")),
            1
        );
    }

    #[test]
    fn slate_theme_set_writes_nvim_state_file_on_successful_apply() {
        // Contract: after ThemeApplyCoordinator::apply succeeds (at least one
        // adapter returns Applied), the shared coordinator writes the nvim
        // state file at `<home>/.cache/slate/current_theme.lua` containing the
        // applied variant id. The state file is what triggers the nvim loader's
        // vim.uv.fs_event watcher in any running nvim.
        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let theme = catppuccin::catppuccin_mocha().unwrap();

        let report = ThemeApplyCoordinator::with_snapshot_policy(&env, SnapshotPolicy::Skip)
            .apply_to_tools(&theme, &["ls_colors".into()])
            .unwrap();
        report.ensure_no_failures().unwrap();
        assert!(
            report.applied_count() >= 1,
            "precondition: at least one adapter must apply (ls_colors is always-on)"
        );

        let state = tempdir.path().join(".cache/slate/current_theme.lua");
        assert!(
            state.is_file(),
            "shared coordinator must write nvim state file at {:?}",
            state
        );
        let got = std::fs::read_to_string(&state).unwrap();
        assert!(
            got.contains(&theme.id),
            "state file must contain applied variant id {:?}, got {:?}",
            theme.id,
            got
        );
    }

    #[test]
    fn slate_theme_set_no_state_file_when_no_adapter_applied() {
        // Contract: if applied_count == 0 (e.g., only a non-existent tool was
        // targeted), the coordinator must NOT write an orphan state file. This
        // prevents running nvim instances from hot-reloading to a theme that
        // nothing else applied.
        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let theme = catppuccin::catppuccin_mocha().unwrap();

        let coordinator = ThemeApplyCoordinator::with_snapshot_policy(&env, SnapshotPolicy::Skip);
        let unreachable_target = vec!["definitely-not-a-real-tool".to_string()];
        let report = coordinator
            .apply_to_tools(&theme, &unreachable_target)
            .unwrap();
        assert_eq!(report.applied_count(), 0);

        let state = tempdir.path().join(".cache/slate/current_theme.lua");
        assert!(
            !state.exists(),
            "no orphan state file must be written when no adapter applied; found {:?}",
            state
        );
    }

    /// contract: `preview_theme(&env, ...)` must route through
    /// `ToolRegistry::apply_theme_to_tools_with_env` so the 3 preview-path
    /// adapters (Ghostty, Alacritty, Kitty) honor the injected env. With a
    /// tempdir-backed env, managed writes must land inside the tempdir and
    /// the host's real `~/.config/slate/managed/*` must NOT be touched. This
    /// is the end-to-end proof that the `silent_preview_apply(&env, ...)`
    /// signature is no longer a lie.
    #[test]
    fn preview_theme_routes_injected_env_all_the_way_to_adapters() {
        use std::io::Write;

        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());

        // Pre-create Ghostty integration config inside tempdir so the apply
        // path doesn't short-circuit with MissingIntegrationConfig. This is
        // the "live preview on Ghostty" scenario — picker navigation with a
        // real Ghostty session running.
        let ghostty_integration = tempdir.path().join(".config/ghostty/config.ghostty");
        std::fs::create_dir_all(ghostty_integration.parent().unwrap()).unwrap();
        let mut file = std::fs::File::create(&ghostty_integration).unwrap();
        writeln!(file, "# managed by slate").unwrap();
        drop(file);

        let theme = catppuccin::catppuccin_mocha().unwrap();

        // Route through the actual `preview_theme` entry point — this is
        // what `silent_preview_apply` calls.
        preview_theme(&env, &theme, OpacityPreset::Solid).unwrap();

        // Managed ghostty/theme.conf MUST live inside the tempdir.
        let managed_ghostty_theme = tempdir
            .path()
            .join(".config/slate/managed/ghostty/theme.conf");
        assert!(
            managed_ghostty_theme.exists(),
            "preview_theme must write managed ghostty theme.conf inside tempdir at {:?}",
            managed_ghostty_theme
        );

        // Integration config inside tempdir must reference the tempdir managed path.
        let integration_content = std::fs::read_to_string(&ghostty_integration).unwrap();
        assert!(
            integration_content.contains(&managed_ghostty_theme.display().to_string()),
            "ghostty integration config must include the tempdir-scoped managed theme.conf, got:\n{}",
            integration_content
        );
    }

    #[test]
    fn test_apply_does_not_commit_current_when_no_adapter_applied() {
        // Targeting a non-existent adapter produces an empty result list, i.e. applied_count==0.
        // The guard must keep the previous current_theme untouched so `slate status` does not
        // advertise a theme that nothing actually applied.
        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let config = ConfigManager::with_env(&env).unwrap();
        config.set_current_theme("catppuccin-mocha").unwrap();

        let theme = catppuccin::catppuccin_latte().unwrap();
        let coordinator = ThemeApplyCoordinator::with_snapshot_policy(&env, SnapshotPolicy::Skip);
        let unreachable_target = vec!["definitely-not-a-real-tool".to_string()];
        let report = coordinator
            .apply_to_tools(&theme, &unreachable_target)
            .unwrap();

        assert_eq!(report.applied_count(), 0);
        assert_eq!(
            config.get_current_theme().unwrap().unwrap_or_default(),
            "catppuccin-mocha",
            "current theme should be preserved when no adapter applied the new theme"
        );
    }
}
