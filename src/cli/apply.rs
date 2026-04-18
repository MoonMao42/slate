use crate::adapter::{SkipReason, ToolApplyResult, ToolApplyStatus, ToolRegistry};
use crate::config::ConfigManager;
use crate::env::SlateEnv;
use crate::error::Result;
use crate::opacity::OpacityPreset;
use crate::theme::ThemeVariant;
use std::collections::HashSet;

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
}

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
    snapshot_policy: SnapshotPolicy,
}

impl<'a> ThemeApplyCoordinator<'a> {
    pub fn new(env: &'a SlateEnv) -> Self {
        Self::with_snapshot_policy(env, SnapshotPolicy::Create)
    }

    pub fn with_snapshot_policy(env: &'a SlateEnv, snapshot_policy: SnapshotPolicy) -> Self {
        Self {
            env,
            snapshot_policy,
        }
    }

    pub fn apply(&self, theme: &ThemeVariant) -> Result<ThemeApplyReport> {
        apply_theme_with_options(
            self.env,
            theme,
            ThemeApplyOptions {
                snapshot_policy: self.snapshot_policy,
                target_tools: None,
            },
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
        )
    }
}

pub fn log_apply_report(report: &ThemeApplyReport) {
    for result in &report.results {
        if !should_log_apply_result(result) {
            continue;
        }
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

fn should_log_apply_result(result: &ToolApplyResult) -> bool {
    // `ls_colors` is an internal shell-integration layer, not a user-selected
    // tool. Keep it in the apply report for state/reload decisions, but avoid
    // surfacing a pseudo-tool line in user-facing progress output.
    result.tool_name != "ls_colors"
}

pub(crate) fn apply_theme_with_options(
    env: &SlateEnv,
    theme: &ThemeVariant,
    options: ThemeApplyOptions<'_>,
) -> Result<ThemeApplyReport> {
    theme.validate()?;

    if matches!(options.snapshot_policy, SnapshotPolicy::Create) {
        let config = ConfigManager::with_env(env)?;
        let current_theme = config.get_current_theme()?.unwrap_or_default();
        if !current_theme.is_empty() {
            let _ = crate::config::snapshot_current_state_with_env(env, &current_theme);
        }
    }

    let registry = ToolRegistry::default();
    let results = if let Some(target_tools) = options.target_tools {
        let selected: HashSet<String> = target_tools.iter().cloned().collect();
        registry.apply_theme_to_tools(theme, &selected)
    } else {
        registry.apply_theme_to_all(theme)
    };
    let report = ThemeApplyReport { results };

    let config = ConfigManager::with_env(env)?;
    if report.applied_count() == 0 {
        // No tool-specific config was written (no ghostty/alacritty/starship found). Still
        // emit the shared shell-integration env files so the slate loader sourced from
        // .zshrc / .bash_profile / fish conf.d doesn't reference a missing file. Skip the
        // `set_current_theme` + reload steps since there's nothing downstream to reload.
        config.write_shell_integration_file(theme)?;
        return Ok(report);
    }
    config.set_current_theme(&theme.id)?;
    config.write_shell_integration_file(theme)?;

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

    reload_theme_targets(&registry, &report);
    Ok(report)
}

pub(crate) fn apply_opacity(
    env: &SlateEnv,
    opacity: OpacityPreset,
    options: OpacityApplyOptions,
) -> Result<()> {
    let config = ConfigManager::with_env(env)?;
    if options.persist_state {
        config.set_current_opacity_preset(opacity)?;
    }

    crate::adapter::ghostty::write_opacity_config(env, opacity)?;
    crate::adapter::ghostty::write_blur_radius(env, opacity)?;
    crate::adapter::alacritty::write_opacity_config(env, opacity)?;
    crate::adapter::kitty::write_opacity_config(env, opacity)?;

    if options.reload_terminals {
        let registry = ToolRegistry::default();
        if let Some(adapter) = registry.get_adapter("ghostty") {
            let _ = adapter.reload();
        }
        crate::adapter::kitty::push_opacity_live(opacity);
    }

    Ok(())
}

pub(crate) fn preview_theme(
    env: &SlateEnv,
    theme: &ThemeVariant,
    opacity: OpacityPreset,
) -> Result<()> {
    theme.validate()?;

    let config = ConfigManager::with_env(env)?;
    let adapter_registry = ToolRegistry::default();
    // Live preview only touches adapters the user actually sees in this terminal window.
    // Rewriting starship/bat/delta on every picker keystroke is wasted IO and has no visible
    // effect until the user launches a new shell.
    let preview_targets: HashSet<String> = ["ghostty", "alacritty", "kitty"]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    let _ = adapter_registry.apply_theme_to_tools(theme, &preview_targets);

    apply_opacity(
        env,
        opacity,
        OpacityApplyOptions {
            persist_state: false,
            reload_terminals: false,
        },
    )?;

    apply_ghostty_live_preview_reload(&config, &adapter_registry);

    if let Some(kitty_adapter) = adapter_registry.get_adapter("kitty") {
        let _ = kitty_adapter.reload();
    }
    crate::adapter::kitty::push_opacity_live(opacity);

    Ok(())
}

fn reload_theme_targets(registry: &ToolRegistry, report: &ThemeApplyReport) {
    if report.ghostty_applied() {
        if let Some(adapter) = registry.get_adapter("ghostty") {
            let _ = adapter.reload();
        }
    }

    if report.results.iter().any(|result| {
        result.tool_name == "kitty" && matches!(result.status, ToolApplyStatus::Applied)
    }) {
        if let Some(adapter) = registry.get_adapter("kitty") {
            let _ = adapter.reload();
        }
    }
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
    std::env::var("TERM_PROGRAM")
        .map(|term_program| term_program.eq_ignore_ascii_case("ghostty"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::catppuccin;
    use tempfile::TempDir;

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
            },
        )
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(env.managed_file("current-opacity")).unwrap(),
            "clear"
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
