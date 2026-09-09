use crate::adapter::{SkipReason, ToolApplyResult, ToolApplyStatus};
use crate::detection::{TerminalKind, TerminalProfile};
mod completion;

/// Failure handling and result tracking for setup execution
/// Tracks which tools installed successfully, which failed, and provides retry guidance
/// Status of a tool installation attempt
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallStatus {
    Success,
    Failed,
    Skipped,
}

/// Result of a single tool installation
#[derive(Debug, Clone)]
pub struct ToolInstallResult {
    pub tool_id: String,
    pub tool_label: String,
    pub status: InstallStatus,
    pub error_message: Option<String>,
}

/// Summary of setup execution results
#[derive(Debug)]
pub struct ExecutionSummary {
    /// Per-tool results
    pub tool_results: Vec<ToolInstallResult>,
    /// Whether this run requested a font change.
    pub font_requested: bool,
    /// Whether the selected font was found or installed (not proof of activation).
    pub font_available: bool,
    /// Whether the selected font choice was successfully persisted.
    pub font_applied: bool,
    /// Whether theme was successfully applied
    pub theme_applied: bool,
    /// Per-adapter theme apply results
    pub theme_results: Vec<ToolApplyResult>,
    /// Issues that let independent steps continue but prevent overall success.
    pub issues: Vec<String>,
    /// Best-effort notes that should be visible but should not fail setup
    pub notices: Vec<String>,
    /// Compatibility mirror, refreshed by the executor/handler. Use
    /// `is_successful()` to derive the outcome after changing public fields.
    pub overall_success: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecoveryCategory {
    Network,
    Permissions,
    MissingDependency,
    UnsupportedEnvironment,
}

impl ExecutionSummary {
    pub fn new() -> Self {
        Self {
            tool_results: Vec::new(),
            font_requested: false,
            font_available: false,
            font_applied: false,
            theme_applied: false,
            theme_results: Vec::new(),
            issues: Vec::new(),
            notices: Vec::new(),
            overall_success: false,
        }
    }

    /// Add a tool result
    pub fn add_tool_result(&mut self, result: ToolInstallResult) {
        self.tool_results.push(result);
    }

    pub fn set_theme_results(&mut self, results: Vec<ToolApplyResult>) {
        self.theme_results = results;
    }

    pub fn add_issue(&mut self, issue: impl Into<String>) {
        self.issues.push(issue.into());
    }

    pub fn add_notice(&mut self, notice: impl Into<String>) {
        self.notices.push(notice.into());
    }

    pub fn is_successful(&self) -> bool {
        self.failure_count() == 0
            && (!self.font_requested || (self.font_available && self.font_applied))
            && self.theme_applied
            && self.theme_failure_count() == 0
            && self.missing_integration_skip_count() == 0
            && self.issues.is_empty()
    }

    pub fn refresh_outcome(&mut self) {
        self.overall_success = self.is_successful();
    }

    pub(crate) fn failure_summary(&self) -> String {
        let mut parts = Vec::new();
        if self.failure_count() > 0 {
            parts.push(format!(
                "{} tool installation(s) failed",
                self.failure_count()
            ));
        }
        if self.font_requested && !self.font_available {
            parts.push("selected font was not found or installed".into());
        } else if self.font_requested && !self.font_applied {
            parts.push("selected font choice was not saved".into());
        }
        if !self.theme_applied {
            parts.push("theme/shell setup did not finish".into());
        }
        if self.theme_failure_count() > 0 {
            parts.push(format!("{} adapter(s) failed", self.theme_failure_count()));
        }
        if self.missing_integration_skip_count() > 0 {
            parts.push(format!(
                "{} integration file(s) are still missing",
                self.missing_integration_skip_count()
            ));
        }
        if !self.issues.is_empty() {
            parts.push(format!(
                "{} setup issue(s) need attention",
                self.issues.len()
            ));
        }
        parts.join("; ")
    }

    /// Get count of successful installations
    pub fn success_count(&self) -> usize {
        self.tool_results
            .iter()
            .filter(|r| r.status == InstallStatus::Success)
            .count()
    }

    /// Get count of failed installations
    pub fn failure_count(&self) -> usize {
        self.tool_results
            .iter()
            .filter(|r| r.status == InstallStatus::Failed)
            .count()
    }

    /// Get list of failed tool IDs for retry
    pub fn failed_tool_ids(&self) -> Vec<&str> {
        self.tool_results
            .iter()
            .filter(|r| r.status == InstallStatus::Failed)
            .map(|r| r.tool_id.as_str())
            .collect()
    }

    pub fn theme_failure_count(&self) -> usize {
        self.theme_results
            .iter()
            .filter(|result| matches!(result.status, ToolApplyStatus::Failed(_)))
            .count()
    }

    /// Count adapters that were actually updated during theme application.
    pub fn configured_count(&self) -> usize {
        self.theme_results
            .iter()
            .filter(|result| matches!(result.status, ToolApplyStatus::Applied))
            .count()
    }

    pub fn missing_integration_skip_count(&self) -> usize {
        self.theme_results
            .iter()
            .filter(|result| {
                matches!(
                    result.status,
                    ToolApplyStatus::Skipped(SkipReason::MissingIntegrationConfig)
                )
            })
            .count()
    }

    /// Format completion message with visibility guidance
    pub fn format_completion_message(&self) -> String {
        self.format_completion_message_for_terminal(&TerminalProfile::detect())
    }

    pub fn format_completion_message_for_terminal(&self, terminal: &TerminalProfile) -> String {
        use std::io::IsTerminal;
        if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
            return self.compact_completion(terminal, super::ui_language::output_language());
        }
        let mut output = String::new();

        // Summary counts
        let total = self.tool_results.len();
        let success = self.success_count();
        let failed = self.failure_count();

        if !self.is_successful() {
            output.push_str("✦ Setup finished with issues.\n\n");
        } else {
            output.push_str("✦ Setup Complete!\n\n");
        }

        if total > 0 {
            output.push_str("Tool Installation:\n");
            output.push_str(&format!("  ✓ {} installed\n", success));
            if failed > 0 {
                output.push_str(&format!("  ✗ {} failed\n", failed));
            }
            output.push('\n');
        }

        if !self.theme_results.is_empty() {
            let applied = self
                .theme_results
                .iter()
                .filter(|result| matches!(result.status, ToolApplyStatus::Applied))
                .count();
            let failed_theme = self.theme_failure_count();
            let missing_integration = self.missing_integration_skip_count();

            output.push_str("Configuration Apply:\n");
            output.push_str(&format!("  ✓ {} adapters updated\n", applied));
            if missing_integration > 0 {
                output.push_str(&format!(
                    "  ⚠ {} adapters still need an integration file\n",
                    missing_integration
                ));
            }
            if failed_theme > 0 {
                output.push_str(&format!(
                    "  ✗ {} adapters failed during apply\n",
                    failed_theme
                ));
            }
            output.push('\n');
        }

        if !self.issues.is_empty() {
            output.push_str("Issues To Check:\n");
            for issue in &self.issues {
                output.push_str(&format!("  • {}\n", issue));
            }
            output.push('\n');
        }

        if !self.notices.is_empty() {
            output.push_str("Notes:\n");
            for notice in &self.notices {
                output.push_str(&format!("  • {}\n", notice));
            }
            output.push('\n');
        }

        output.push_str("Current Terminal:\n");
        output.push_str(&format!(
            "  • {} — {}\n\n",
            terminal.display_name(),
            terminal.compatibility_summary()
        ));

        // Visibility guidance
        output.push_str("Visibility & Activation:\n\n");

        output.push_str("→ Confirmed On Disk:\n");
        if self.theme_applied {
            output.push_str(
                "  • Theme and shell integration completed; window appearance is not verified\n",
            );
        } else {
            output.push_str(
                "  • Theme/shell setup did not finish; some earlier file changes may remain\n",
            );
        }
        if success > 0 {
            output.push_str("  • Successful tool installation steps completed\n");
        }
        if self.font_applied {
            output.push_str("  • The selected font choice was saved; rendering is not verified\n");
        } else if self.font_available {
            output.push_str("  • The selected font is available, but its choice was not saved\n");
        }
        output.push('\n');

        if !terminal.session().can_reload_terminal() {
            output.push_str("→ File-Only Session:\n");
            if terminal.session().is_remote() {
                output.push_str("  • These changes concern tools on this host; configure the client terminal's font and opacity locally\n");
            } else {
                output.push_str("  • This isolated profile does not establish changes to host terminal windows\n");
            }
            output.push_str(
                "  • Live font, opacity and window appearance are not verified by this receipt\n\n",
            );
        } else {
            output.push_str("→ Fresh Shell or Tab:\n");
            output.push_str("  • Starship prompt initialization\n");
            output.push_str("  • zsh-syntax-highlighting and shell init changes\n");
            output.push_str("  • PATH or environment updates that land on shell startup\n\n");

            output.push_str("→ New Terminal Window or Surface:\n");
            match terminal.kind() {
                TerminalKind::Ghostty => {
                    output.push_str("  • Ghostty chrome, opacity, and frosted glass usually show up after a new tab or window\n");
                }
                TerminalKind::Kitty => {
                    output.push_str(
                        "  • Open a new Kitty window if colors did not reload immediately\n",
                    );
                }
                TerminalKind::Alacritty => {
                    output.push_str("  • Open a new Alacritty window if colors or opacity did not reload immediately\n");
                }
                TerminalKind::TerminalApp => {
                    output.push_str("  • Open a new Terminal.app tab after setup so shell startup changes are loaded cleanly\n");
                }
                TerminalKind::Unknown => {
                    output.push_str("  • Open a fresh terminal tab or window if your app does not hot-reload config changes\n");
                }
            }
            output.push('\n');

            output.push_str("→ Manual Follow-Up:\n");
            match terminal.kind() {
                TerminalKind::Ghostty => {
                    output.push_str(
                        "  • If the font still looks unchanged, fully restart Ghostty once\n",
                    );
                }
                TerminalKind::Kitty => {
                    output.push_str("  • If glyphs still look wrong, verify your chosen Nerd Font is available to Kitty\n");
                }
                TerminalKind::Alacritty => {
                    output.push_str("  • If glyphs still look wrong, verify your chosen Nerd Font is available to Alacritty\n");
                }
                TerminalKind::TerminalApp => {
                    output.push_str(
                        "  • Choose your Nerd Font in Terminal.app Settings > Profiles > Text\n",
                    );
                    output.push_str(
                        "  • If icons still look wrong, reopen the profile after switching fonts\n",
                    );
                }
                TerminalKind::Unknown => {
                    output.push_str("  • If icons still look wrong, pick a Nerd Font in your terminal's font settings\n");
                }
            }
            output.push('\n');

            output.push_str("→ Not Supported In This Terminal:\n");
            match terminal.kind() {
                TerminalKind::Ghostty => {
                    output.push_str(
                        "  • Nothing major is gated here — Ghostty gets the full Slate path\n",
                    );
                }
                TerminalKind::Kitty => {
                    output.push_str(
                    "  • Frosted/blurred backgrounds and watcher auto-relaunch remain Ghostty-only\n",
                );
                }
                TerminalKind::Alacritty => {
                    output.push_str(
                    "  • Frosted/blurred backgrounds and watcher auto-relaunch remain Ghostty-only\n",
                );
                }
                TerminalKind::TerminalApp => {
                    output.push_str(
                    "  • Slate cannot auto-pick Terminal.app profile fonts or enable frosted backgrounds\n",
                );
                    output.push_str(
                    "  • Auto-theme recovery after a restart is not guaranteed outside Ghostty shell sessions\n",
                );
                }
                TerminalKind::Unknown => {
                    output.push_str(
                    "  • Terminal-specific visuals are best-effort only and depend on the app you are using\n",
                );
                }
            }
            output.push('\n');
        }
        let recovery_sections = self.recovery_sections();
        if !recovery_sections.is_empty() {
            output.push_str("Recovery Paths:\n\n");
            output.push_str(&recovery_sections.join("\n\n"));
            output.push('\n');
        }

        // Retry guidance if there were failures
        if failed > 0 {
            output.push_str("Retry Failed Tools:\n\n");
            for tool_id in self.failed_tool_ids() {
                output.push_str(&format!("  slate setup --only {}\n", tool_id));
            }
            output.push('\n');
        }

        if terminal.session().can_reload_terminal() {
            output.push_str("Open a fresh shell first, then restart the terminal app only if\n");
            output.push_str("   fonts or window visuals still look unchanged.\n");
        }

        output
    }

    fn completed_summary(&self) -> String {
        let mut parts = Vec::new();

        if self.success_count() > 0 {
            parts.push(format!("{} tool install(s) finished", self.success_count()));
        }
        if self.configured_count() > 0 {
            parts.push(format!(
                "{} integration(s) were updated",
                self.configured_count()
            ));
        }
        if self.theme_applied {
            parts.push("theme files were written".to_string());
        }
        if self.font_applied {
            parts.push("the selected font choice was saved".to_string());
        }

        if parts.is_empty() {
            "No successful installation or configuration step was confirmed; inspect the issues for possible partial writes.".to_string()
        } else {
            parts.join("; ")
        }
    }

    fn remaining_summary(&self) -> String {
        let mut parts = Vec::new();

        if self.failure_count() > 0 {
            parts.push(format!(
                "{} install(s) still need a retry",
                self.failure_count()
            ));
        }
        if self.theme_failure_count() > 0 {
            parts.push(format!(
                "{} integration(s) failed during theme apply",
                self.theme_failure_count()
            ));
        }
        if self.missing_integration_skip_count() > 0 {
            parts.push(format!(
                "{} integration file(s) still need to exist before Slate can manage them",
                self.missing_integration_skip_count()
            ));
        }
        if self.font_requested && !self.font_available {
            parts.push("the selected font still needs to be available".into());
        } else if self.font_requested && !self.font_applied {
            parts.push("the selected font choice still needs to be saved".into());
        }
        if !self.theme_applied {
            parts.push("theme/shell setup did not finish".into());
        }
        if self.is_successful() {
            "Nothing else is blocked right now.".to_string()
        } else if parts.is_empty() {
            "Some setup steps still need attention.".to_string()
        } else {
            parts.join("; ")
        }
    }

    fn all_issue_messages(&self) -> Vec<String> {
        self.tool_results
            .iter()
            .filter_map(|result| result.error_message.clone())
            .chain(self.issues.iter().cloned())
            .chain(self.notices.iter().cloned())
            .collect()
    }

    fn recovery_sections(&self) -> Vec<String> {
        let mut sections = Vec::new();
        let completed = self.completed_summary();
        let remaining = self.remaining_summary();

        for category in [
            RecoveryCategory::Network,
            RecoveryCategory::Permissions,
            RecoveryCategory::MissingDependency,
            RecoveryCategory::UnsupportedEnvironment,
        ] {
            let matching = self
                .all_issue_messages()
                .into_iter()
                .filter(|message| classify_recovery_category(message) == Some(category))
                .collect::<Vec<_>>();

            if matching.is_empty() {
                continue;
            }

            sections.push(format!(
                "{}\n  What happened: {}\n  Completed: {}\n  Not completed: {}\n  Next: {}",
                recovery_title(category),
                summarize_recovery_messages(&matching),
                completed.as_str(),
                remaining.as_str(),
                recovery_next_step(category, self)
            ));
        }

        sections
    }

    /// Format a detailed summary for logging
    pub fn format_detailed_summary(&self) -> String {
        let mut output = String::new();
        output.push_str("=== Setup Execution Summary ===\n\n");

        for result in &self.tool_results {
            let status_str = match result.status {
                InstallStatus::Success => "✓ Success",
                InstallStatus::Failed => "✗ Failed",
                InstallStatus::Skipped => "◆ Skipped",
            };

            output.push_str(&format!("{}: {}\n", status_str, result.tool_label));

            if let Some(error) = &result.error_message {
                output.push_str(&format!("  Error: {}\n", error));
            }
        }

        output.push('\n');
        output.push_str(&format!("Font requested: {}\n", self.font_requested));
        output.push_str(&format!("Font available: {}\n", self.font_available));
        output.push_str(&format!("Font choice saved: {}\n", self.font_applied));
        output.push_str(&format!("Theme applied: {}\n", self.theme_applied));
        output.push_str(&format!(
            "Theme apply failures: {}\n",
            self.theme_failure_count()
        ));
        output.push_str(&format!(
            "Missing integration skips: {}\n",
            self.missing_integration_skip_count()
        ));
        output.push_str(&format!("Issues: {}\n", self.issues.len()));
        output.push_str(&format!("Notices: {}\n", self.notices.len()));
        output.push_str(&format!("Overall success: {}\n", self.is_successful()));

        output
    }
}

impl Default for ExecutionSummary {
    fn default() -> Self {
        Self::new()
    }
}

fn classify_recovery_category(message: &str) -> Option<RecoveryCategory> {
    let lower = message.to_ascii_lowercase();

    if lower.contains("network unreachable")
        || lower.contains("could not resolve host")
        || lower.contains("couldn't connect")
        || lower.contains("download failed")
        || lower.contains("failed to download")
        || lower.contains("offline")
    {
        Some(RecoveryCategory::Network)
    } else if lower.contains("permission denied")
        || lower.contains("not writable")
        || lower.contains("no write access")
        || lower.contains("cannot write")
    {
        Some(RecoveryCategory::Permissions)
    } else if lower.contains("homebrew was not found")
        || lower.contains("xcode command line tools")
        || lower.contains("swiftc")
        || lower.contains("zsh was not found")
        || lower.contains("zsh is not installed")
        || lower.contains("command not found")
        || lower.contains("no such file or directory")
    {
        Some(RecoveryCategory::MissingDependency)
    } else if lower.contains("ghostty-only")
        || lower.contains("terminal.app")
        || lower.contains("unsupported")
    {
        Some(RecoveryCategory::UnsupportedEnvironment)
    } else {
        None
    }
}

fn summarize_recovery_messages(messages: &[String]) -> String {
    messages
        .iter()
        .map(|message| message.trim())
        .filter(|message| !message.is_empty())
        .take(2)
        .collect::<Vec<_>>()
        .join("; ")
}

fn recovery_title(category: RecoveryCategory) -> &'static str {
    match category {
        RecoveryCategory::Network => "Network / Downloads",
        RecoveryCategory::Permissions => "Permissions / Shared Homebrew",
        RecoveryCategory::MissingDependency => "Missing Dependency",
        RecoveryCategory::UnsupportedEnvironment => "Unsupported Shell / Terminal",
    }
}

fn recovery_next_step(category: RecoveryCategory, summary: &ExecutionSummary) -> String {
    let retry_command = if summary.failure_count() > 0 && summary.failure_count() == 1 {
        format!(
            "rerun `slate setup --only {}`",
            summary.failed_tool_ids()[0]
        )
    } else {
        "rerun `slate setup`".to_string()
    };

    match category {
        RecoveryCategory::Network => {
            format!("Reconnect to the network, then {}.", retry_command)
        }
        RecoveryCategory::Permissions => format!(
            "Use a writable Homebrew setup or ask the primary Homebrew owner/admin to install the blocked package, then {}.",
            retry_command
        ),
        RecoveryCategory::MissingDependency => format!(
            "Install the missing dependency mentioned above, then {}.",
            retry_command
        ),
        RecoveryCategory::UnsupportedEnvironment => {
            "Use zsh for shell integration and Ghostty for the full Slate experience.".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_outcome_is_derived_from_required_stages_not_cached_success() {
        for case in [
            "complete",
            "notice",
            "install",
            "font_absent",
            "font_unsaved",
            "theme",
            "adapter",
            "missing",
            "late_issue",
        ] {
            let mut summary = ExecutionSummary::new();
            summary.theme_applied = true;
            summary.overall_success = true; // Deliberately stale for failure cases.
            match case {
                "complete" => {}
                "notice" => summary.add_notice("Optional helper unavailable"),
                "install" => summary.add_tool_result(ToolInstallResult {
                    tool_id: "bat".into(),
                    tool_label: "bat".into(),
                    status: InstallStatus::Failed,
                    error_message: Some("download failed".into()),
                }),
                "font_absent" => {
                    summary.font_requested = true;
                    summary.font_applied = true;
                }
                "font_unsaved" => {
                    summary.font_requested = true;
                    summary.font_available = true;
                }
                "theme" => summary.theme_applied = false,
                "adapter" | "missing" => summary.set_theme_results(vec![ToolApplyResult {
                    tool_name: "ghostty".into(),
                    requires_new_shell: false,
                    status: if case == "adapter" {
                        ToolApplyStatus::Failed(crate::error::SlateError::Internal(
                            "write failed".into(),
                        ))
                    } else {
                        ToolApplyStatus::Skipped(SkipReason::MissingIntegrationConfig)
                    },
                }]),
                "late_issue" => summary.add_issue("Neovim activation failed"),
                _ => unreachable!(),
            }
            let expected = matches!(case, "complete" | "notice");
            assert_eq!(summary.is_successful(), expected, "{case}");
            let report = summary.format_completion_message_for_terminal(
                &TerminalProfile::from_env_vars(Some("ghostty"), None),
            );
            assert_eq!(
                report.contains("Setup Complete!"),
                expected,
                "{case}: {report}"
            );
            assert!(!report.contains("Already Live"));
            if !expected {
                assert!(!summary.failure_summary().is_empty(), "{case}");
            }
            summary.refresh_outcome();
            assert_eq!(summary.overall_success, expected, "{case}");
        }
    }

    #[test]
    fn setup_outcome_file_only_receipts_do_not_promise_host_window_activation() {
        for session in [
            crate::session::SessionContext::isolated(),
            crate::session::SessionContext::from_vars(|key| {
                (key == "SSH_CONNECTION").then(|| "private".into())
            }),
        ] {
            let terminal =
                TerminalProfile::from_env_vars(Some("ghostty"), None).with_session(session);
            let mut summary = ExecutionSummary::new();
            summary.theme_applied = true;
            summary.font_requested = true;
            summary.font_available = true;
            summary.font_applied = true;
            let report = summary.format_completion_message_for_terminal(&terminal);
            assert!(report.contains("Setup Complete!"));
            assert!(report.contains("File-Only Session"));
            assert!(report.contains("rendering is not verified"));
            assert!(!report.contains("Nothing major is gated"));
            assert!(!report.contains("fully restart Ghostty"));
        }
        let report = ExecutionSummary::new()
            .format_completion_message_for_terminal(&TerminalProfile::from_env_vars(None, None));
        assert!(report.contains("Theme/shell setup did not finish"));
        assert!(!report.contains("Theme files and managed config were written"));
    }

    #[test]
    fn test_execution_summary_counts() {
        let mut summary = ExecutionSummary::new();
        summary.add_tool_result(ToolInstallResult {
            tool_id: "ghostty".to_string(),
            tool_label: "Ghostty".to_string(),
            status: InstallStatus::Success,
            error_message: None,
        });
        summary.add_tool_result(ToolInstallResult {
            tool_id: "starship".to_string(),
            tool_label: "Starship".to_string(),
            status: InstallStatus::Failed,
            error_message: Some("Already installed".to_string()),
        });

        assert_eq!(summary.success_count(), 1);
        assert_eq!(summary.failure_count(), 1);
    }

    #[test]
    fn test_notices_render_separately_from_issues() {
        let mut summary = ExecutionSummary::new();
        summary.add_notice("Optional preset font bundle had partial misses");

        let message = summary.format_completion_message();
        assert!(message.contains("Notes:"));
        assert!(message.contains("Optional preset font bundle had partial misses"));
        assert!(!message
            .contains("Issues To Check:\n  • Optional preset font bundle had partial misses"));
    }

    #[test]
    fn test_failed_tool_ids() {
        let mut summary = ExecutionSummary::new();
        summary.add_tool_result(ToolInstallResult {
            tool_id: "ghostty".to_string(),
            tool_label: "Ghostty".to_string(),
            status: InstallStatus::Failed,
            error_message: None,
        });
        summary.add_tool_result(ToolInstallResult {
            tool_id: "starship".to_string(),
            tool_label: "Starship".to_string(),
            status: InstallStatus::Success,
            error_message: None,
        });

        let failed_ids = summary.failed_tool_ids();
        assert_eq!(failed_ids.len(), 1);
        assert_eq!(failed_ids[0], "ghostty");
    }

    #[test]
    fn test_configured_count_uses_applied_adapter_results() {
        let mut summary = ExecutionSummary::new();
        summary.add_tool_result(ToolInstallResult {
            tool_id: "starship".to_string(),
            tool_label: "Starship".to_string(),
            status: InstallStatus::Success,
            error_message: None,
        });
        summary.set_theme_results(vec![
            ToolApplyResult {
                tool_name: "bat".to_string(),
                status: ToolApplyStatus::Applied,
                requires_new_shell: true,
            },
            ToolApplyResult {
                tool_name: "delta".to_string(),
                status: ToolApplyStatus::Applied,
                requires_new_shell: false,
            },
            ToolApplyResult {
                tool_name: "ghostty".to_string(),
                status: ToolApplyStatus::Skipped(SkipReason::MissingIntegrationConfig),
                requires_new_shell: false,
            },
        ]);

        assert_eq!(summary.success_count(), 1);
        assert_eq!(summary.configured_count(), 2);
    }

    #[test]
    fn test_completion_message_format() {
        let mut summary = ExecutionSummary::new();
        summary.add_tool_result(ToolInstallResult {
            tool_id: "ghostty".to_string(),
            tool_label: "Ghostty".to_string(),
            status: InstallStatus::Success,
            error_message: None,
        });
        summary.font_applied = true;
        summary.theme_applied = true;
        summary.overall_success = true;
        let message = summary.format_completion_message_for_terminal(
            &TerminalProfile::from_env_vars(Some("ghostty"), None),
        );
        assert!(message.contains("Setup Complete"));
        assert!(message.contains("Current Terminal"));
        assert!(message.contains("Fresh Shell or Tab"));
        assert!(message.contains("Manual Follow-Up"));
        assert!(message.contains("Visibility & Activation"));
    }

    #[test]
    fn test_completion_message_mentions_terminal_app_limits() {
        let summary = ExecutionSummary::new();
        let message = summary.format_completion_message_for_terminal(
            &TerminalProfile::from_env_vars(Some("Apple_Terminal"), None),
        );
        assert!(message.contains("Terminal.app"));
        assert!(message.contains("Choose your Nerd Font"));
        assert!(message.contains("Not Supported In This Terminal"));
    }

    #[test]
    fn test_recovery_paths_group_network_failures() {
        let mut summary = ExecutionSummary::new();
        summary.add_tool_result(ToolInstallResult {
            tool_id: "starship".to_string(),
            tool_label: "Starship".to_string(),
            status: InstallStatus::Failed,
            error_message: Some(
                "starship — network unreachable. Check your connection and retry.".to_string(),
            ),
        });

        let message = summary.format_completion_message_for_terminal(
            &TerminalProfile::from_env_vars(Some("ghostty"), None),
        );
        assert!(message.contains("Recovery Paths"));
        assert!(message.contains("Network / Downloads"));
        assert!(message.contains("Reconnect to the network"));
    }

    #[test]
    fn test_detailed_summary_format() {
        let mut summary = ExecutionSummary::new();
        summary.add_tool_result(ToolInstallResult {
            tool_id: "test_tool".to_string(),
            tool_label: "Test Tool".to_string(),
            status: InstallStatus::Success,
            error_message: None,
        });
        summary.overall_success = true;

        let detailed = summary.format_detailed_summary();
        assert!(detailed.contains("Setup Execution Summary"));
        assert!(detailed.contains("Success"));
        assert!(detailed.contains("Test Tool"));
    }
}
