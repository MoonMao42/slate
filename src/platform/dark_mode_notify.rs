use crate::config::ConfigManager;
use crate::error::{Result, SlateError};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::NamedTempFile;

const SWIFT_SOURCE: &str = include_str!("../../resources/dark-mode-notify.swift");
const PROCESS_PATTERN: &str = "slate-dark-mode-notify";

/// Get the path to the compiled binary
fn binary_path(config: &ConfigManager) -> Result<PathBuf> {
    let bin_dir = config.managed_dir("bin");
    Ok(bin_dir.join("slate-dark-mode-notify"))
}

fn binary_needs_rebuild(bin_path: &Path) -> bool {
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

/// Compile the Swift dark mode notifier binary.
/// Recompiles when the watcher is missing or older than the current slate binary.
pub fn ensure_binary(config: &ConfigManager) -> Result<PathBuf> {
    let bin_path = binary_path(config)?;

    if !binary_needs_rebuild(&bin_path) {
        return Ok(bin_path);
    }

    // Ensure parent directory exists
    if let Some(parent) = bin_path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            SlateError::PlatformError(format!("Failed to create bin directory: {}", e))
        })?;
    }

    let parent = bin_path.parent().ok_or_else(|| {
        SlateError::PlatformError("Compiled notifier path is missing a parent directory".to_string())
    })?;

    // Compile via unique temp files, then atomically publish the finished binary.
    let tmp_swift = NamedTempFile::new().map_err(|e| {
        SlateError::PlatformError(format!("Failed to create Swift source temp file: {}", e))
    })?;
    fs::write(tmp_swift.path(), SWIFT_SOURCE)
        .map_err(|e| SlateError::PlatformError(format!("Failed to write Swift source: {}", e)))?;

    let tmp_bin = NamedTempFile::new_in(parent).map_err(|e| {
        SlateError::PlatformError(format!("Failed to create binary temp file: {}", e))
    })?;
    let tmp_bin_path = tmp_bin.path().to_path_buf();

    let output = Command::new("swiftc")
        .arg(tmp_swift.path())
        .arg("-o")
        .arg(&tmp_bin_path)
        .output()
        .map_err(|e| SlateError::PlatformError(format!("Failed to run swiftc: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SlateError::PlatformError(format!(
            "swiftc compilation failed: {}",
            stderr
        )));
    }

    tmp_bin.persist(&bin_path).map_err(|e| {
        SlateError::PlatformError(format!(
            "Failed to install compiled notifier {}: {}",
            bin_path.display(),
            e
        ))
    })?;

    Ok(bin_path)
}

/// Check whether the watcher process is currently running.
pub fn is_running() -> Result<bool> {
    let status = Command::new("pgrep")
        .args(["-qf", PROCESS_PATTERN])
        .status()
        .map_err(|e| SlateError::PlatformError(format!("Failed to check watcher process state: {}", e)))?;

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

/// Remove the compiled binary
pub fn remove_binary(config: &ConfigManager) -> Result<()> {
    let bin_path = binary_path(config)?;
    if bin_path.exists() {
        fs::remove_file(&bin_path).ok();
    }
    Ok(())
}
