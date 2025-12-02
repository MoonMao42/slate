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
#[derive(Debug, Clone)]
pub struct ExecutionSummary {
    /// Per-tool results
    pub tool_results: Vec<ToolInstallResult>,
    /// Whether font was successfully applied
    pub font_applied: bool,
    /// Whether theme was successfully applied
    pub theme_applied: bool,
    /// Overall success flag
    pub overall_success: bool,
}

impl ExecutionSummary {
    pub fn new() -> Self {
        Self {
            tool_results: Vec::new(),
            font_applied: false,
            theme_applied: false,
            overall_success: false,
        }
    }

    /// Add a tool result
    pub fn add_tool_result(&mut self, result: ToolInstallResult) {
        self.tool_results.push(result);
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

    /// Format completion message with visibility guidance
    pub fn format_completion_message(&self) -> String {
        let mut output = String::new();
        output.push_str("✦ Setup Complete!\n\n");

        // Summary counts
        let total = self.tool_results.len();
        let success = self.success_count();
        let failed = self.failure_count();

        if total > 0 {
            output.push_str("📦 Tool Installation:\n");
            output.push_str(&format!("  ✓ {} installed\n", success));
            if failed > 0 {
                output.push_str(&format!("  ✗ {} failed\n", failed));
            }
            output.push('\n');
        }

        // Visibility guidance
        output.push_str("📍 Visibility & Activation:\n\n");

        output.push_str("→ Available Now:\n");
        output.push_str("  • Homebrew finished installing the selected tools/apps\n");
        output.push_str("  • Newly installed CLIs can be available right away\n\n");

        output.push_str("→ Fresh Shell or Tab:\n");
        output.push_str("  • Starship prompt initialization\n");
        output.push_str("  • zsh-syntax-highlighting and shell init changes\n");
        output.push_str("  • PATH or environment updates that land on shell startup\n\n");

        output.push_str("→ New Terminal Window or Surface:\n");
        output.push_str("  • Ghostty/Alacritty padding and similar window-level visuals\n");
        output.push_str("  • New tabs/windows often pick up terminal chrome changes first\n\n");

        output.push_str("→ Full App Restart May Still Be Required:\n");
        output.push_str("  • Font changes\n");
        output.push_str("  • Ghostty background opacity on macOS\n");
        output.push_str("  • Some terminal appearance settings depending on the app\n\n");

        // Retry guidance if there were failures
        if failed > 0 {
            output.push_str("🔄 Retry Failed Tools:\n\n");
            for tool_id in self.failed_tool_ids() {
                output.push_str(&format!("  slate setup --only {}\n", tool_id));
            }
            output.push('\n');
        }

        output.push_str("🎨 Open a fresh shell first, then restart the terminal app only if\n");
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
