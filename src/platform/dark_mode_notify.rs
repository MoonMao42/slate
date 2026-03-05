use crate::config::ConfigManager;
use crate::error::{Result, SlateError};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const PROCESS_PATTERN: &str = "slate-dark-mode-notify";

/// Get the path where the binary should be installed in the managed config directory.
fn binary_path(config: &ConfigManager) -> Result<PathBuf> {
    let bin_dir = config.managed_dir("bin");
    Ok(bin_dir.join("slate-dark-mode-notify"))
}

/// The watcher binary is embedded at compile time so it travels inside the slate executable.
/// This eliminates the dependency on build-machine paths after distribution.
const EMBEDDED_WATCHER: &[u8] = include_bytes!(env!("WATCHER_BINARY"));

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

/// Install the pre-compiled Swift dark mode notifier binary.
/// Copies the build-time compiled binary to the managed directory.
/// Refreshes when the installed binary is missing or older than the current slate binary.
pub fn ensure_binary(config: &ConfigManager) -> Result<PathBuf> {
    let bin_path = binary_path(config)?;

    // Guard: if swiftc was missing at build time, the embedded binary is an empty stub
    if EMBEDDED_WATCHER.is_empty() {
        return Err(SlateError::PlatformError(
            "Auto-theme is not available: slate was built without swiftc (Xcode Command Line Tools). \
             Install them with 'xcode-select --install' and rebuild slate."
                .to_string(),
        ));
    }

    if !binary_needs_refresh(&bin_path) {
        return Ok(bin_path);
    }

    // Ensure parent directory exists
    if let Some(parent) = bin_path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            SlateError::PlatformError(format!("Failed to create bin directory: {}", e))
        })?;
    }

    // Write the embedded watcher binary to the managed location
    fs::write(&bin_path, EMBEDDED_WATCHER).map_err(|e| {
        SlateError::PlatformError(format!(
            "Failed to write watcher binary to {}: {}",
            bin_path.display(),
            e
        ))
    })?;

    // Ensure the binary is executable
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

/// Check whether the watcher process is currently running.
pub fn is_running() -> Result<bool> {
    let status = Command::new("pgrep")
        .args(["-f", PROCESS_PATTERN])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|e| {
            SlateError::PlatformError(format!("Failed to check watcher process state: {}", e))
        })?;

    Ok(status.success())
}

/// Stop any running watcher process.
pub fn stop() -> Result<()> {
    let status = Command::new("pkill")
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

fn watcher_stop_succeeded(status: &std::process::ExitStatus) -> bool {
    if status.success() || status.code() == Some(1) {
        return true;
    }

    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;

        if matches!(status.signal(), Some(libc::SIGTERM)) {
            return true;
        }
    }

    false
}

/// Start the watcher process in the background if not already running.
/// The watcher calls `slate theme --auto --quiet` on each macOS appearance change.
pub fn start(config: &ConfigManager) -> Result<()> {
    if is_running()? {
        return Ok(());
    }

    let bin_path = binary_path(config)?;
    if !bin_path.exists() {
        return Err(SlateError::PlatformError(
            "Watcher binary not found. Run 'slate config set auto-theme enable' first.".to_string(),
        ));
    }

    let slate_bin = std::env::current_exe()
        .ok()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "slate".to_string());

    Command::new(&bin_path)
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

/// Remove the compiled binary
pub fn remove_binary(config: &ConfigManager) -> Result<()> {
    let bin_path = binary_path(config)?;
    if bin_path.exists() {
        fs::remove_file(&bin_path).ok();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::watcher_stop_succeeded;

    #[test]
    fn test_watcher_stop_succeeds_when_no_processes_match() {
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
