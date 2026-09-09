//! The picker saves a theme and opacity as one user selection. Its mandatory
//! pre-theme checkpoint, not a remembered theme name, is the recovery source.
use crate::{
    cli::apply::{
        self, OpacityApplyOptions, SnapshotPolicy, ThemeApplyCoordinator, ThemeApplyReport,
    },
    config::{self, RestoreReceipt},
    env::SlateEnv,
    error::{Result, SlateError},
    opacity::OpacityPreset,
    theme::{ThemeRegistry, ThemeVariant},
};

/// Apply theme, then opacity, without stdout progress. Theme-stage failures retain
/// their recovery report; opacity-stage failures attempt file-only recovery of
/// this selection. Every failed selection still returns an error, even if its
/// files were recovered. Successful selections reload applied terminal targets
/// only after both file stages; nonfatal reload errors remain visible warnings.
/// The original-name/preset arguments remain for source
/// compatibility but never override the captured bytes, modes or prior absence.
pub fn silent_commit_apply(
    env: &SlateEnv,
    theme_id: &str,
    opacity: OpacityPreset,
    _original_theme_id: &str,
    _original_opacity: OpacityPreset,
) -> Result<()> {
    silent_commit_apply_with(
        env,
        theme_id,
        opacity,
        |env, theme| {
            ThemeApplyCoordinator::new(env)
                .including_opacity()
                .deferring_terminal_reload()
                .apply(theme)
        },
        apply::apply_opacity,
    )
}

fn silent_commit_apply_with(
    env: &SlateEnv,
    theme_id: &str,
    opacity: OpacityPreset,
    apply_theme: impl FnOnce(&SlateEnv, &ThemeVariant) -> Result<ThemeApplyReport>,
    apply_opacity: impl FnOnce(&SlateEnv, OpacityPreset, OpacityApplyOptions) -> Result<()>,
) -> Result<()> {
    silent_commit_apply_with_effects(
        env,
        theme_id,
        opacity,
        apply_theme,
        apply_opacity,
        apply::finish_picker_terminal_reload,
    )
}

fn silent_commit_apply_with_effects(
    env: &SlateEnv,
    theme_id: &str,
    opacity: OpacityPreset,
    apply_theme: impl FnOnce(&SlateEnv, &ThemeVariant) -> Result<ThemeApplyReport>,
    apply_opacity: impl FnOnce(&SlateEnv, OpacityPreset, OpacityApplyOptions) -> Result<()>,
    finish_reload: impl FnOnce(&SlateEnv, &mut ThemeApplyReport),
) -> Result<()> {
    let registry = ThemeRegistry::new()?;
    let theme = registry
        .get(theme_id)
        .ok_or_else(|| SlateError::InvalidThemeData(format!("Theme '{}' not found", theme_id)))?;
    // Retain the same lock across both writes and any recovery. A second Slate
    // writer cannot start between the failure and rollback's file plan.
    let _write_guard = config::ConfigWriteGuard::acquire(env)?;
    let mut report = apply_theme(env, theme)?;
    if let Err(error) = report.ensure_no_failures() {
        apply::log_apply_warnings(&report);
        return Err(error);
    }
    if let Err(error) = apply_opacity(
        env,
        opacity,
        OpacityApplyOptions {
            persist_state: true,
            reload_terminals: false,
            snapshot_policy: SnapshotPolicy::Skip,
        },
    ) {
        apply::log_apply_warnings(&report);
        let point_id = report.restore_point_id.as_deref();
        return Err(opacity_failure(
            error,
            point_id,
            recover_selection_files(env, point_id),
        ));
    }
    finish_reload(env, &mut report);
    apply::log_apply_warnings(&report);
    Ok(())
}

fn recover_selection_files(env: &SlateEnv, point_id: Option<&str>) -> Result<RestoreReceipt> {
    let id = point_id.ok_or_else(|| {
        SlateError::BackupFailed("The selection has no pre-theme recovery point".into())
    })?;
    let prepared = config::prepare_restore_with_env(env, id)?;
    let plan = prepared.plan();
    if plan.theme_name != "pre-theme" || plan.is_baseline || plan.may_regenerate_theme_files {
        return Err(SlateError::BackupFailed(
            "The selection's recovery point is not a file-only pre-theme checkpoint".into(),
        ));
    }
    // Recovery uses the existing bounded, revalidated restore engine. It saves
    // an undo checkpoint and returns per-file results; Ok is not proof that every
    // file succeeded. Never rerun adapters, learn pairs or guess absent settings.
    config::execute_prepared_restore(prepared)
}

fn opacity_failure(
    error: SlateError,
    point_id: Option<&str>,
    recovery: Result<RestoreReceipt>,
) -> SlateError {
    let hint = point_id
        .map(|id| {
            format!(" Inspect pre-selection file recovery with: slate restore {id} --dry-run.")
        })
        .unwrap_or_default();
    let outcome = match recovery {
        Ok(receipt) => {
            let status = if receipt.is_fully_successful() {
                "Pre-selection file bytes, permissions and prior absence were restored.".to_string()
            } else {
                let details = receipt.failed_results().into_iter().map(|result| {
                    format!("{}: {}", result.original_path.display(), result.error.as_deref().unwrap_or("file restore failed"))
                }).collect::<Vec<_>>().join("; ");
                format!("Automatic file recovery was incomplete ({} of {} file results succeeded). {details}", receipt.success_count(), receipt.results.len())
            };
            format!("{status} External caches and live application state were not rolled back; reload tools as needed. Inspect the state saved immediately before recovery (it may contain a partial selection): slate restore {} --dry-run.", receipt.pre_restore_point_id)
        }
        Err(recovery_error) => format!(
            "Automatic file recovery could not complete: {recovery_error}. Earlier writes may remain; no theme was regenerated as a fallback."
        ),
    };
    SlateError::Internal(format!(
        "Picker selection failed while applying opacity: {error}. {outcome}{hint}"
    ))
}

#[cfg(test)]
#[path = "commit_tests.rs"]
mod tests;
