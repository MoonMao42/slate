use crate::adapter::{SkipReason, ToolApplyResult, ToolApplyStatus};

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
    /// Whether font was successfully applied
    pub font_applied: bool,
    /// Whether theme was successfully applied
    pub theme_applied: bool,
    /// Per-adapter theme apply results
    pub theme_results: Vec<ToolApplyResult>,
    /// Non-fatal setup issues that still need user visibility
    pub issues: Vec<String>,
    /// Best-effort notes that should be visible but should not fail setup
    pub notices: Vec<String>,
    /// Overall success flag
    pub overall_success: bool,
}

impl ExecutionSummary {
    pub fn new() -> Self {
        Self {
            tool_results: Vec::new(),
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
        let mut output = String::new();

        // Summary counts
        let total = self.tool_results.len();
        let success = self.success_count();
        let failed = self.failure_count();

        if !self.overall_success {
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

        // Visibility guidance
        output.push_str("Visibility & Activation:\n\n");

        output.push_str("→ Available Now:\n");
        output.push_str("  • Homebrew finished installing the selected tools/apps\n");
        output.push_str("  • Newly installed CLIs can be available right away\n\n");

        output.push_str("→ Fresh Shell or Tab:\n");
        output.push_str("  • Starship prompt initialization\n");
        output.push_str("  • zsh-syntax-highlighting and shell init changes\n");
        output.push_str("  • PATH or environment updates that land on shell startup\n\n");

        output.push_str("→ New Terminal Window or Surface:\n");
        output.push_str("  • Ghostty/Alacritty window-level visuals and opacity changes\n");
        output.push_str("  • New tabs/windows often pick up terminal chrome changes first\n\n");

        output.push_str("→ Full App Restart May Still Be Required:\n");
        output.push_str("  • Font changes\n");
        output.push_str("  • Ghostty background opacity on macOS\n");
        output.push_str("  • Some terminal appearance settings depending on the app\n\n");

        output.push_str("→ Manual Font Pick In Some Apps:\n");
        output.push_str("  • Terminal.app and some other terminals do not let Slate switch the profile font automatically\n");
        output.push_str("  • If icons or powerline shapes still look wrong, choose a Nerd Font in that terminal's settings\n\n");

        // Retry guidance if there were failures
        if failed > 0 {
            output.push_str("Retry Failed Tools:\n\n");
            for tool_id in self.failed_tool_ids() {
                output.push_str(&format!("  slate setup --only {}\n", tool_id));
            }
            output.push('\n');
        }

        output.push_str("Open a fresh shell first, then restart the terminal app only if\n");
        output.push_str("   fonts or window visuals still look unchanged.\n");

        output
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
        output.push_str(&format!("Font applied: {}\n", self.font_applied));
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
        output.push_str(&format!("Overall success: {}\n", self.overall_success));

        output
    }
}

impl Default for ExecutionSummary {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_completion_message_format() {
        let mut summary = ExecutionSummary::new();
        summary.add_tool_result(ToolInstallResult {
            tool_id: "ghostty".to_string(),
            tool_label: "Ghostty".to_string(),
            status: InstallStatus::Success,
            error_message: None,
        });
        summary.font_applied = true;
        summary.overall_success = true;
        let message = summary.format_completion_message();
        assert!(message.contains("Setup Complete"));
        assert!(message.contains("Fresh Shell or Tab"));
        assert!(message.contains("Full App Restart May Still Be Required"));
        assert!(message.contains("Visibility & Activation"));
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
