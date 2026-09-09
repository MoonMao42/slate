use crate::config::{ConfigManager, ConfigWriteGuard};
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;

mod events;
mod inspection;
mod runtime;
pub use inspection::{
    inspect_installation, DirectoryAccess, InstallationInspection, InstallationState,
};
pub use runtime::{RuntimeInspection, RuntimeState};

const LAUNCHER: &str = "slate-dark-mode-notify";
const HELPER: &str = "slate-appearance-helper";

#[cfg(target_os = "macos")]
const EMBEDDED_WATCHER: &[u8] = include_bytes!(env!("WATCHER_BINARY"));

fn error(message: impl Into<String>) -> SlateError {
    SlateError::PlatformError(message.into())
}

fn installation_error(path: &std::path::Path, action: &str, cause: SlateError) -> SlateError {
    let path = path.to_string_lossy().escape_debug().to_string();
    let permission = matches!(&cause, SlateError::IOError(error) if error.kind() == std::io::ErrorKind::PermissionDenied);
    let detail = cause.to_string().escape_debug().to_string();
    error(format!(
        "Cannot {action} at {path}: {detail}.{}",
        if permission {
            " Inspect this path and its parent directory's ownership, permissions and ACL rules; Slate did not change permissions."
        } else {
            " Review this destination before retrying; earlier helper writes may remain."
        }
    ))
}

fn profile_env(env: &SlateEnv) -> Vec<(&'static str, std::ffi::OsString)> {
    vec![
        ("HOME", env.home().as_os_str().to_owned()),
        (
            "XDG_CONFIG_HOME",
            env.xdg_config_home().as_os_str().to_owned(),
        ),
        ("XDG_CACHE_HOME", env.cache_dir().as_os_str().to_owned()),
        (
            "ZDOTDIR",
            env.zshrc_path()
                .parent()
                .expect("zsh root")
                .as_os_str()
                .to_owned(),
        ),
        (
            "NVIM_APPNAME",
            env.nvim_config_dir()
                .strip_prefix(env.xdg_config_home())
                .expect("nvim profile")
                .as_os_str()
                .to_owned(),
        ),
    ]
}

fn launcher_contents(env: &SlateEnv) -> Result<String> {
    let mut script = String::from("#!/bin/sh\n# Slate watcher launcher v1\n");
    if env.session().is_isolated() {
        script.push_str(&format!(
            "export SLATE_HOME={}\n",
            crate::detection::shell_quote_path(env.home())
        ));
    } else {
        // A copied host launcher must not escape an explicitly isolated shell.
        script.push_str("if [ -n \"${SLATE_HOME:-}\" ]; then exit 0; fi\nunset SLATE_HOME\n");
    }
    for (key, value) in profile_env(env) {
        script.push_str(&format!(
            "export {key}={}\n",
            crate::detection::shell_quote_path(std::path::Path::new(&value))
        ));
    }
    script.push_str(&format!(
        "exec {} __watch-auto-theme\n",
        crate::detection::shell_quote_path(&std::env::current_exe()?)
    ));
    Ok(script)
}

/// Both platforms install a profile-bound Rust launcher. The macOS helper only
/// emits appearance events; it no longer applies themes itself.
pub fn ensure_binary(config: &ConfigManager) -> Result<PathBuf> {
    let env = config.environment();
    let previous = inspect_installation(env).launcher.state;
    if matches!(
        previous,
        InstallationState::Legacy | InstallationState::Unrecognized
    ) {
        eprintln!("warning: replacing a legacy or unrecognized managed watcher launcher. This does not stop untracked old processes. Review any known old watcher and run `slate doctor auto-theme` after the refresh.");
    }
    #[cfg(target_os = "macos")]
    if std::hint::black_box(EMBEDDED_WATCHER).is_empty() {
        return Err(error("Auto-theme is unavailable: build Slate with Xcode Command Line Tools to include the macOS appearance helper."));
    }
    #[cfg(not(target_os = "macos"))]
    if !crate::platform::desktop::detect_backend().supports_watcher() {
        return Err(error(
            "Auto-theme needs an XDG desktop portal or GNOME gsettings backend.",
        ));
    }
    let directory = config.managed_dir("bin");
    std::fs::create_dir_all(&directory).map_err(|cause| {
        installation_error(&directory, "prepare watcher directory", cause.into())
    })?;
    #[cfg(target_os = "macos")]
    crate::config::state_files::atomic_write_synced_mode(
        &directory.join(HELPER),
        EMBEDDED_WATCHER,
        Some(0o755),
    )
    .map_err(|cause| {
        installation_error(&directory.join(HELPER), "save appearance helper", cause)
    })?;
    let path = directory.join(LAUNCHER);
    crate::config::state_files::atomic_write_synced_mode(
        &path,
        launcher_contents(env)?.as_bytes(),
        Some(0o755),
    )
    .map_err(|cause| installation_error(&path, "save watcher launcher", cause))?;
    Ok(path)
}

pub fn is_running() -> Result<bool> {
    is_running_with_env(&SlateEnv::from_process()?)
}

pub fn is_running_with_env(env: &SlateEnv) -> Result<bool> {
    runtime::Profile::new(env)?.is_running()
}

pub fn stop() -> Result<()> {
    stop_with_env(&SlateEnv::from_process()?)
}

pub fn stop_with_env(env: &SlateEnv) -> Result<()> {
    if env.session().is_isolated() {
        return Ok(());
    }
    runtime::Profile::new(env)?.stop()
}

pub fn start(config: &ConfigManager) -> Result<()> {
    let env = config.environment();
    if env.session().is_isolated() {
        return Ok(());
    }
    if !config.is_auto_theme_enabled()? {
        return Ok(());
    }
    let mut command = Command::new(std::env::current_exe()?);
    command.arg("__watch-auto-theme").env_remove("SLATE_HOME");
    for (key, value) in profile_env(env) {
        command.env(key, value);
    }
    runtime::Profile::new(env)?.start(&mut command)
}

pub fn remove_binary(config: &ConfigManager) -> Result<()> {
    for name in [LAUNCHER, HELPER] {
        match std::fs::remove_file(config.managed_dir("bin").join(name)) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(err.into()),
        }
    }
    Ok(())
}

pub fn run_watcher_loop() -> Result<()> {
    let env = SlateEnv::from_process()?;
    if env.session().is_isolated() {
        return Ok(());
    }
    run_with_events(
        &env,
        || events::Source::new(&env),
        || apply_auto_theme_quiet_with_env(&env),
    )
}

fn run_with_events(
    env: &SlateEnv,
    source: impl FnOnce() -> Result<events::Source>,
    mut apply: impl FnMut() -> Result<bool>,
) -> Result<()> {
    let profile = runtime::Profile::new(env)?;
    let Some(lease) = profile.claim()? else {
        return Ok(());
    };
    let result = run_claimed(env, &lease, source, &mut apply);
    if let Err(err) = lease.finish(if result.is_ok() {
        runtime::ExitKind::Stopped
    } else {
        runtime::ExitKind::Failed
    }) {
        eprintln!("Could not record watcher exit: {err}");
    }
    result
}

fn run_claimed(
    env: &SlateEnv,
    lease: &runtime::Lease,
    source: impl FnOnce() -> Result<events::Source>,
    apply: &mut impl FnMut() -> Result<bool>,
) -> Result<()> {
    // An old shell hook may race disable/clean: recheck under the lifetime lock.
    if !ConfigManager::from_env_paths(env).is_auto_theme_enabled()? {
        return Ok(());
    }
    let source = source()?;
    lease.ready()?;
    let mut pending = true;
    loop {
        if lease.should_stop()? || !ConfigManager::from_env_paths(env).is_auto_theme_enabled()? {
            return Ok(());
        }
        match source.events.recv_timeout(Duration::from_millis(200)) {
            Ok(events::Event::Changed) => pending = true,
            Ok(events::Event::Failed(message)) => return Err(error(message)),
            Err(RecvTimeoutError::Disconnected) => {
                return Err(error("Appearance event source closed"))
            }
            Err(RecvTimeoutError::Timeout) => {}
        }
        if pending {
            match apply() {
                Ok(applied) => pending = !applied,
                Err(err) => {
                    eprintln!("Auto-theme event failed: {err}");
                    pending = false;
                }
            }
        }
    }
}

fn apply_auto_theme_quiet_with_env(env: &SlateEnv) -> Result<bool> {
    let _write_guard = match ConfigWriteGuard::acquire(env) {
        Ok(guard) => guard,
        Err(SlateError::ConfigurationBusy | SlateError::PreviewRecoveryPending) => {
            return Ok(false)
        }
        Err(err) => return Err(err),
    };
    let config = ConfigManager::from_env_paths(env);
    if !config.is_auto_theme_enabled()? {
        return Ok(true);
    }
    let theme_id = crate::cli::auto_theme::resolve_auto_theme(env, &config)?;
    let registry = crate::theme::ThemeRegistry::new()?;
    let theme = registry
        .get(&theme_id)
        .ok_or_else(|| error(format!("Auto-resolved theme '{theme_id}' not found")))?;
    let report = crate::cli::apply::ThemeApplyCoordinator::with_snapshot_policy(
        env,
        crate::cli::apply::SnapshotPolicy::Skip,
    )
    .preserving_auto_pair()
    .apply(theme)?;
    crate::cli::apply::log_apply_warnings(&report);
    report.ensure_no_failures()?;
    Ok(true)
}

#[cfg(test)]
mod tests;

#[cfg(all(test, target_os = "macos"))]
mod apply_tests;
