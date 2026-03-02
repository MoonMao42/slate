use crate::adapter::{SkipReason, ToolApplyResult, ToolApplyStatus, ToolRegistry};
use crate::config::ConfigManager;
use crate::env::SlateEnv;
use crate::error::Result;
use crate::theme::ThemeVariant;
use std::collections::HashSet;

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
        self.apply_inner(theme, None)
    }

    pub fn apply_to_tools(
        &self,
        theme: &ThemeVariant,
        tool_names: &[String],
    ) -> Result<ThemeApplyReport> {
        let selected: HashSet<String> = tool_names.iter().cloned().collect();
        self.apply_inner(theme, Some(&selected))
    }

    fn apply_inner(
        &self,
        theme: &ThemeVariant,
        selected_tools: Option<&HashSet<String>>,
    ) -> Result<ThemeApplyReport> {
        theme.validate()?;

        let registry = ToolRegistry::default();
        let results = if let Some(selected_tools) = selected_tools {
            registry.apply_theme_to_tools(theme, selected_tools)
        } else {
            registry.apply_theme_to_all(theme)
        };
        let report = ThemeApplyReport { results };

        // Still persist theme and write shell integration even if no adapters applied.
        // This handles fresh users where config files were just created — the theme
        // data is written to managed/ files and shell integration (env.zsh) regardless.
        let config = ConfigManager::with_env(self.env)?;
        config.set_current_theme(&theme.id)?;
        config.write_shell_integration_file(theme)?;

        // If auto-theme is enabled, update the dark/light pairing to match
        // the user's manual selection for the current system appearance.
        if config.is_auto_theme_enabled().unwrap_or(false) {
            if let Ok(appearance) = crate::cli::auto_theme::detect_system_appearance() {
                match appearance {
                    crate::theme::ThemeAppearance::Dark => {
                        let _ = config.write_auto_config(Some(&theme.id), None);
                    }
                    crate::theme::ThemeAppearance::Light => {
                        let _ = config.write_auto_config(None, Some(&theme.id));
                    }
                }
            }
        }

        if report.ghostty_applied() {
            if let Some(adapter) = registry.get_adapter("ghostty") {
                let _ = adapter.reload();
            }
        }

        // Kitty needs explicit reload via kitten @ set-colors
        if report
            .results
            .iter()
            .any(|r| r.tool_name == "kitty" && matches!(r.status, ToolApplyStatus::Applied))
        {
            if let Some(adapter) = registry.get_adapter("kitty") {
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
    apply_theme_selection_for_tools_with_env(theme, env, None)
}

pub fn apply_theme_selection_for_tools_with_env(
    theme: &ThemeVariant,
    env: &SlateEnv,
    tool_names: Option<&[String]>,
) -> Result<ThemeApplyReport> {
    // Snapshot current state before switching themes
    let config = crate::config::ConfigManager::with_env(env)?;
    let current_theme = config.get_current_theme()?.unwrap_or_default();
    if !current_theme.is_empty() {
        let _ = crate::config::snapshot_current_state_with_env(env, &current_theme);
    }

    let coordinator = ThemeApplyCoordinator::new(env);
    let report = if let Some(tool_names) = tool_names {
        coordinator.apply_to_tools(theme, tool_names)?
    } else {
        coordinator.apply(theme)?
    };
    log_apply_report(&report);
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::{SkipReason, ToolApplyStatus};
    use crate::error::SlateError;
    use crate::theme::catppuccin;
    use tempfile::TempDir;

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

    #[test]
    fn test_apply_theme_selection_for_tools_with_empty_selection_skips_all_adapters() {
        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let theme = catppuccin::catppuccin_mocha().unwrap();
        let empty_selection: Vec<String> = Vec::new();

        let report =
            apply_theme_selection_for_tools_with_env(&theme, &env, Some(&empty_selection)).unwrap();

        assert!(report.results.is_empty());
    }
}
