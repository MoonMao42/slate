pub use crate::cli::apply::{
    log_apply_report, SnapshotPolicy, ThemeApplyCoordinator, ThemeApplyReport,
};
use crate::env::SlateEnv;
use crate::error::Result;
use crate::theme::ThemeVariant;

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
    use crate::cli::apply::ThemeApplyReport;
    use crate::theme::catppuccin;
    use crate::{cli::theme_apply::apply_theme_selection_for_tools_with_env, env::SlateEnv};
    use tempfile::TempDir;

    #[test]
    fn test_apply_theme_selection_for_tools_with_empty_selection_skips_all_adapters() {
        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let theme = catppuccin::catppuccin_mocha().unwrap();
        let empty_selection: Vec<String> = Vec::new();

        let report: ThemeApplyReport =
            apply_theme_selection_for_tools_with_env(&theme, &env, Some(&empty_selection)).unwrap();

        assert!(report.results.is_empty());
    }
}
