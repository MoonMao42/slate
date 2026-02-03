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

/// Get the path to the pre-compiled binary from the build output.
fn build_time_binary_path() -> Result<PathBuf> {
    let path = env!("WATCHER_BINARY");
    if !Path::new(path).exists() {
        return Err(SlateError::PlatformError(
            "Pre-compiled watcher binary not found in build output".to_string(),
        ));
    }
    Ok(PathBuf::from(path))
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

/// Install the pre-compiled Swift dark mode notifier binary.
/// Copies the build-time compiled binary to the managed directory.
/// Refreshes when the installed binary is missing or older than the current slate binary.
pub fn ensure_binary(config: &ConfigManager) -> Result<PathBuf> {
    let bin_path = binary_path(config)?;

    if !binary_needs_refresh(&bin_path) {
        return Ok(bin_path);
    }

    // Get the pre-compiled binary from the build output
    let source_binary = build_time_binary_path()?;

    // Ensure parent directory exists
    if let Some(parent) = bin_path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            SlateError::PlatformError(format!("Failed to create bin directory: {}", e))
        })?;
    }

    // Copy the pre-compiled binary to the managed location
    fs::copy(&source_binary, &bin_path).map_err(|e| {
        SlateError::PlatformError(format!(
            "Failed to install watcher binary from {} to {}: {}",
            source_binary.display(),
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
        .args(["-qf", PROCESS_PATTERN])
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

    if status.success() || status.code() == Some(1) {
        return Ok(());
    }

    Err(SlateError::PlatformError(format!(
        "Watcher stop command exited with status {}",
        status
    )))
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
