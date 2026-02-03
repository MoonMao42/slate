use crate::adapter::{SkipReason, ToolApplyResult, ToolApplyStatus, ToolRegistry};
use crate::config::ConfigManager;
use crate::env::SlateEnv;
use crate::error::Result;
use crate::theme::ThemeVariant;

/// Coordinated result for a single theme application run.
#[derive(Debug)]
pub struct ThemeApplyReport {
    pub results: Vec<ToolApplyResult>,
}

impl ThemeApplyReport {
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
}

impl<'a> ThemeApplyCoordinator<'a> {
    pub fn new(env: &'a SlateEnv) -> Self {
        Self { env }
    }

    pub fn apply(&self, theme: &ThemeVariant) -> Result<ThemeApplyReport> {
        theme.validate()?;

        let registry = ToolRegistry::default();
        let results = registry.apply_theme_to_all(theme);
        let report = ThemeApplyReport { results };

        // Still persist theme and write shell integration even if no adapters applied.
        // This handles fresh users where config files were just created — the theme
        // data is written to managed/ files and shell integration (env.zsh) regardless.
        let config = ConfigManager::with_env(self.env)?;
        config.set_current_theme(&theme.id)?;
        config.write_shell_integration_file(theme)?;

        if report.ghostty_applied() {
            if let Some(adapter) = registry.get_adapter("ghostty") {
                let _ = adapter.reload();
            }
        }

        Ok(report)
    }
}

pub fn log_apply_report(report: &ThemeApplyReport) {
    for result in &report.results {
        match &result.status {
            ToolApplyStatus::Applied => eprintln!("✓ {}", result.tool_name),
            ToolApplyStatus::Skipped(SkipReason::MissingIntegrationConfig) => {
                eprintln!("○ {}: missing integration config", result.tool_name)
            }
            ToolApplyStatus::Skipped(SkipReason::NotInstalled) => {}
            ToolApplyStatus::Failed(err) => eprintln!("❌ {}: {}", result.tool_name, err),
        }
    }
}

pub fn apply_theme_selection(theme: &ThemeVariant) -> Result<ThemeApplyReport> {
    let env = SlateEnv::from_process()?;
    apply_theme_selection_with_env(theme, &env)
}

pub fn apply_theme_selection_with_env(
    theme: &ThemeVariant,
    env: &SlateEnv,
) -> Result<ThemeApplyReport> {
    // Snapshot current state before switching themes
    let config = crate::config::ConfigManager::with_env(env)?;
    let current_theme = config.get_current_theme()?.unwrap_or_default();
    if !current_theme.is_empty() {
        let _ = crate::config::snapshot_current_state_with_env(env, &current_theme);
    }

    let report = ThemeApplyCoordinator::new(env).apply(theme)?;
    log_apply_report(&report);
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::{SkipReason, ToolApplyStatus};
    use crate::error::SlateError;

    #[test]
    fn test_report_counts_statuses() {
        let report = ThemeApplyReport {
            results: vec![
                ToolApplyResult {
                    tool_name: "ghostty".to_string(),
                    status: ToolApplyStatus::Applied,
                },
                ToolApplyResult {
                    tool_name: "alacritty".to_string(),
                    status: ToolApplyStatus::Skipped(SkipReason::MissingIntegrationConfig),
                },
                ToolApplyResult {
                    tool_name: "starship".to_string(),
                    status: ToolApplyStatus::Failed(SlateError::Internal("boom".to_string())),
                },
            ],
        };

        assert_eq!(report.applied_count(), 1);
        assert_eq!(report.skipped_count(), 1);
        assert_eq!(report.failed_count(), 1);
        assert!(report.ghostty_applied());
    }
}
