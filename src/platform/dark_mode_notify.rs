use crate::config::ConfigManager;
use crate::error::{Result, SlateError};
use std::path::{Path, PathBuf};
use std::process::Stdio;

#[cfg(target_os = "linux")]
use crate::env::SlateEnv;

const PROCESS_PATTERN: &str = "slate-dark-mode-notify";
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
const WATCHER_UNAVAILABLE_MESSAGE: &str =
    "Auto-theme watcher is only available on macOS and portal-aware Linux today.";

fn binary_path(config: &ConfigManager) -> Result<PathBuf> {
    let bin_dir = config.managed_dir("bin");
    Ok(bin_dir.join(PROCESS_PATTERN))
}

fn watcher_stop_succeeded(status: &std::process::ExitStatus) -> bool {
    // pkill conventions: 0 = at least one process matched and was signalled; 1 = no matches
    // (which is fine — watcher already gone). Any other exit code is a real failure.
    if status.success() || status.code() == Some(1) {
        return true;
    }

    // Under some test harnesses and process-group setups, pkill itself terminates with
    // SIGTERM when slate is invoked inside a cascading-kill environment. Treat that as a
    // successful stop — the watcher was either already gone or signalled before pkill
    // could report a normal exit.
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if matches!(status.signal(), Some(libc::SIGTERM)) {
            return true;
        }
    }

    false
}

fn is_running_impl() -> Result<bool> {
    let status = std::process::Command::new("pgrep")
        .args(["-f", PROCESS_PATTERN])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| {
            SlateError::PlatformError(format!("Failed to check watcher process state: {}", e))
        })?;

    Ok(status.success())
}

fn stop_impl() -> Result<()> {
    let status = std::process::Command::new("pkill")
        .args(["-f", PROCESS_PATTERN])
        .status()
        .map_err(|e| SlateError::PlatformError(format!("Failed to stop watcher process: {}", e)))?;

    if watcher_stop_succeeded(&status) {
        return Ok(());
    }

    Err(SlateError::PlatformError(format!(
        "Watcher stop command exited with status {}",
        status
    )))
}

#[cfg(target_os = "linux")]
fn spawn_background_watcher(bin_path: &Path) -> Result<()> {
    std::process::Command::new(bin_path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| {
            SlateError::PlatformError(format!("Failed to start watcher process: {}", e))
        })?;

    Ok(())
}

#[cfg(target_os = "linux")]
fn apply_auto_theme_quiet() -> Result<()> {
    let env = SlateEnv::from_process()?;
    let config = ConfigManager::with_env(&env)?;
    let theme_id = crate::cli::auto_theme::resolve_auto_theme(&env, &config)?;
    let registry = crate::theme::ThemeRegistry::new()?;
    let theme = registry.get(&theme_id).ok_or_else(|| {
        SlateError::InvalidThemeData(format!("Auto-resolved theme '{}' not found", theme_id))
    })?;

    crate::cli::apply::ThemeApplyCoordinator::with_snapshot_policy(
        &env,
        crate::cli::apply::SnapshotPolicy::Skip,
    )
    .apply(theme)?;

    Ok(())
}

#[cfg(target_os = "macos")]
mod imp {
    use super::{
        binary_path, is_running_impl, run_watcher_loop_unsupported, stop_impl, ConfigManager, Path,
        PathBuf, Result, SlateError,
    };
    use std::fs;

    /// The watcher binary is embedded at compile time so it travels inside the slate executable.
    /// This eliminates the dependency on build-machine paths after distribution.
    const EMBEDDED_WATCHER: &[u8] = include_bytes!(env!("WATCHER_BINARY"));

    #[inline(never)]
    fn embedded_watcher_missing() -> bool {
        std::hint::black_box(EMBEDDED_WATCHER).is_empty()
    }

    fn binary_needs_refresh(bin_path: &Path) -> bool {
        if !bin_path.exists() {
            return true;
        }

        let current_exe = match std::env::current_exe() {
            Ok(path) => path,
            Err(_) => return false,
        };

        let bin_modified = fs::metadata(bin_path).and_then(|meta| meta.modified());
        let exe_modified = fs::metadata(current_exe).and_then(|meta| meta.modified());

        match (bin_modified, exe_modified) {
            (Ok(bin_time), Ok(exe_time)) => exe_time > bin_time,
            _ => false,
        }
    }

    pub fn ensure_binary(config: &ConfigManager) -> Result<PathBuf> {
        let bin_path = binary_path(config)?;

        if embedded_watcher_missing() {
            return Err(SlateError::PlatformError(
                "Auto-theme is not available: slate was built without swiftc (Xcode Command Line Tools). \
                 Install them with 'xcode-select --install' and rebuild slate."
                    .to_string(),
            ));
        }

        if !binary_needs_refresh(&bin_path) {
            return Ok(bin_path);
        }

        if let Some(parent) = bin_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                SlateError::PlatformError(format!("Failed to create bin directory: {}", e))
            })?;
        }

        fs::write(&bin_path, EMBEDDED_WATCHER).map_err(|e| {
            SlateError::PlatformError(format!(
                "Failed to write watcher binary to {}: {}",
                bin_path.display(),
                e
            ))
        })?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = fs::Permissions::from_mode(0o755);
            fs::set_permissions(&bin_path, perms).map_err(|e| {
                SlateError::PlatformError(format!(
                    "Failed to set executable permissions on watcher binary: {}",
                    e
                ))
            })?;
        }

        Ok(bin_path)
    }

    pub fn is_running() -> Result<bool> {
        is_running_impl()
    }

    pub fn stop() -> Result<()> {
        stop_impl()
    }

    pub fn start(config: &ConfigManager) -> Result<()> {
        if is_running()? {
            return Ok(());
        }

        let bin_path = binary_path(config)?;
        if !bin_path.exists() {
            return Err(SlateError::PlatformError(
                "Watcher binary not found. Run 'slate config set auto-theme enable' first."
                    .to_string(),
            ));
        }

        let slate_bin = std::env::current_exe()
            .ok()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "slate".to_string());

        std::process::Command::new(&bin_path)
            .args([&slate_bin, "theme", "--auto", "--quiet"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| {
                SlateError::PlatformError(format!("Failed to start watcher process: {}", e))
            })?;

        Ok(())
    }

    pub fn remove_binary(config: &ConfigManager) -> Result<()> {
        let bin_path = binary_path(config)?;
        if bin_path.exists() {
            fs::remove_file(&bin_path).ok();
        }
        Ok(())
    }

    pub fn run_watcher_loop() -> Result<()> {
        run_watcher_loop_unsupported("Swift watcher is embedded on macOS and should not be invoked through the hidden Rust watcher entrypoint.")
    }

    #[cfg(test)]
    mod tests {
        use super::super::watcher_stop_succeeded;

        #[test]
        fn test_binary_stop_succeeds_when_no_processes_match() {
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;

                let status = std::process::ExitStatus::from_raw(1 << 8);
                assert!(watcher_stop_succeeded(&status));
            }
        }

        #[test]
        fn test_watcher_stop_tolerates_sigterm_exit_status() {
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;

                let status = std::process::ExitStatus::from_raw(libc::SIGTERM);
                assert!(watcher_stop_succeeded(&status));
            }
        }
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use super::{
        apply_auto_theme_quiet, binary_path, is_running_impl, spawn_background_watcher, stop_impl,
        ConfigManager, PathBuf, Result, SlateError,
    };
    use std::fs;

    const LINUX_WATCHER_MESSAGE: &str =
        "Auto-theme watcher needs XDG desktop portal support or the GNOME gsettings fallback.";

    fn watcher_script_contents() -> Result<String> {
        let current_exe = std::env::current_exe().map_err(|err| {
            SlateError::PlatformError(format!(
                "Failed to resolve the current slate binary for watcher setup: {}",
                err
            ))
        })?;
        let quoted = crate::detection::shell_quote_path(&current_exe);
        Ok(format!(
            "#!/bin/sh\nexec {quoted} __watch-auto-theme\n",
            quoted = quoted
        ))
    }

    pub fn ensure_binary(config: &ConfigManager) -> Result<PathBuf> {
        if !crate::platform::desktop::detect_backend().supports_watcher() {
            return Err(SlateError::PlatformError(LINUX_WATCHER_MESSAGE.to_string()));
        }

        let bin_path = binary_path(config)?;
        if let Some(parent) = bin_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                SlateError::PlatformError(format!("Failed to create bin directory: {}", e))
            })?;
        }

        fs::write(&bin_path, watcher_script_contents()?).map_err(|e| {
            SlateError::PlatformError(format!(
                "Failed to write Linux watcher launcher to {}: {}",
                bin_path.display(),
                e
            ))
        })?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = fs::Permissions::from_mode(0o755);
            fs::set_permissions(&bin_path, perms).map_err(|e| {
                SlateError::PlatformError(format!(
                    "Failed to set executable permissions on Linux watcher launcher: {}",
                    e
                ))
            })?;
        }

        Ok(bin_path)
    }

    pub fn is_running() -> Result<bool> {
        is_running_impl()
    }

    pub fn stop() -> Result<()> {
        stop_impl()
    }

    pub fn start(config: &ConfigManager) -> Result<()> {
        if is_running()? {
            return Ok(());
        }

        let bin_path = ensure_binary(config)?;
        spawn_background_watcher(&bin_path)
    }

    pub fn remove_binary(config: &ConfigManager) -> Result<()> {
        let bin_path = binary_path(config)?;
        if bin_path.exists() {
            fs::remove_file(&bin_path).ok();
        }
        Ok(())
    }

    pub fn run_watcher_loop() -> Result<()> {
        crate::platform::desktop::watch_appearance_changes(|_| apply_auto_theme_quiet())
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
mod imp {
    use super::{
        run_watcher_loop_unsupported, ConfigManager, PathBuf, Result, SlateError,
        WATCHER_UNAVAILABLE_MESSAGE,
    };

    pub fn ensure_binary(_config: &ConfigManager) -> Result<PathBuf> {
        Err(SlateError::PlatformError(
            WATCHER_UNAVAILABLE_MESSAGE.to_string(),
        ))
    }

    pub fn is_running() -> Result<bool> {
        Ok(false)
    }

    pub fn stop() -> Result<()> {
        Ok(())
    }

    pub fn start(_config: &ConfigManager) -> Result<()> {
        Err(SlateError::PlatformError(
            WATCHER_UNAVAILABLE_MESSAGE.to_string(),
        ))
    }

    pub fn remove_binary(_config: &ConfigManager) -> Result<()> {
        Ok(())
    }

    pub fn run_watcher_loop() -> Result<()> {
        run_watcher_loop_unsupported(WATCHER_UNAVAILABLE_MESSAGE)
    }
}

#[cfg(not(target_os = "linux"))]
fn run_watcher_loop_unsupported(message: &str) -> Result<()> {
    Err(SlateError::PlatformError(message.to_string()))
}

pub use imp::{ensure_binary, is_running, remove_binary, run_watcher_loop, start, stop};
