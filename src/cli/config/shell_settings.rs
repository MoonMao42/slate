//! File recovery and native lifecycle are separate stages. Never reset a saved
//! preference blindly after a later refresh/start/stop failure.
use crate::{
    config::{
        recovery_paths,
        shell_change::{PreparedShellPreference, ShellPreference},
        ConfigManager, ConfigWriteGuard,
    },
    error::{Result, SlateError},
    platform,
};

pub(super) fn apply(config: &ConfigManager, preference: ShellPreference) -> Result<()> {
    apply_impl(config, preference, false)
}

pub(super) fn apply_menu(config: &ConfigManager, preference: ShellPreference) -> Result<()> {
    apply_impl(config, preference, true)
}

fn apply_impl(config: &ConfigManager, preference: ShellPreference, compact: bool) -> Result<()> {
    apply_with_notice(
        config,
        preference,
        compact,
        |config| {
            if matches!(preference, ShellPreference::AutoTheme(true)) {
                platform::dark_mode_notify::ensure_binary(config)?;
            }
            Ok(())
        },
        |config| match preference {
            ShellPreference::Fastfetch(_)
            | ShellPreference::Starship(_)
            | ShellPreference::Highlighting(_) => Ok(()),
            ShellPreference::AutoTheme(true) => platform::dark_mode_notify::start(config),
            ShellPreference::AutoTheme(false) => {
                platform::dark_mode_notify::stop_with_env(config.environment())?;
                platform::dark_mode_notify::remove_binary(config)
            }
        },
    )
}

#[cfg(test)]
fn apply_with(
    config: &ConfigManager,
    preference: ShellPreference,
    before_files: impl FnOnce(&ConfigManager) -> Result<()>,
    after_files: impl FnOnce(&ConfigManager) -> Result<()>,
) -> Result<()> {
    apply_with_notice(config, preference, false, before_files, after_files)
}

fn checkpoint_notice(id: &str, compact: bool) -> String {
    if compact {
        format!("已创建恢复点 · 查看改动：slate restore {id} --dry-run")
    } else {
        format!(
            "Pre-config recovery point: {id}\nInspect file recovery: slate restore {id} --dry-run"
        )
    }
}

fn apply_with_notice(
    config: &ConfigManager,
    preference: ShellPreference,
    compact: bool,
    before_files: impl FnOnce(&ConfigManager) -> Result<()>,
    after_files: impl FnOnce(&ConfigManager) -> Result<()>,
) -> Result<()> {
    let env = config.environment();
    let _guard = ConfigWriteGuard::acquire(env)?;
    let plan = PreparedShellPreference::capture(env, preference)?;
    let native_files = matches!(preference, ShellPreference::AutoTheme(_));
    let mut paths = plan.paths();
    if native_files {
        paths.extend(
            ["slate-dark-mode-notify", "slate-appearance-helper"]
                .into_iter()
                .map(|name| env.managed_file(&format!("managed/bin/{name}"))),
        );
    }
    let point = if plan.changed() || native_files {
        let count = paths.len();
        let targets = recovery_paths::targets(env, paths, "Configuration")?;
        if targets.len() != count {
            return Err(SlateError::InvalidConfig(
                "Configuration paths overlap; separate their managed directories before retrying."
                    .into(),
            ));
        }
        let point = crate::config::snapshot_config_targets_with_env(env, &targets)?;
        eprintln!("{}", checkpoint_notice(&point.id, compact));
        Some(point)
    } else {
        None
    };
    let recovery = point
        .as_ref()
        .map(|point| {
            format!(
                " Inspect file recovery with: slate restore {} --dry-run.",
                point.id
            )
        })
        .unwrap_or_default();
    plan.verify().map_err(|error| SlateError::InvalidConfig(format!(
        "Configuration changed after checkpoint creation: {error}. No helper or preference update was started.{recovery}"
    )))?;
    before_files(config).map_err(|error| SlateError::InvalidConfig(format!(
        "Configuration helper preparation failed before saving the preference: {error}. Helper files may have changed; no automatic file or process rollback was attempted.{recovery}"
    )))?;
    plan.publish().map_err(|error| SlateError::InvalidConfig(format!(
        "Configuration file update was incomplete: {error}. The preference is published last; earlier generated/helper files may have changed. No automatic overwrite of later edits was attempted.{recovery}"
    )))?;
    after_files(config).map_err(|error| SlateError::InvalidConfig(format!(
        "Configuration preference and shell files were saved, but watcher lifecycle did not finish: {error}. No automatic preference reset was attempted. Inspect `slate doctor auto-theme`; file recovery does not restore processes.{recovery}"
    )))
}

#[cfg(test)]
mod tests;
