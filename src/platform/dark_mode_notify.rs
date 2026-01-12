use crate::config::ConfigManager;
use crate::error::{Result, SlateError};
use std::fs;
use std::process::Command;
use tempfile::NamedTempFile;

const SWIFT_SOURCE: &str = include_str!("../../resources/dark-mode-notify.swift");

/// Get the path to the compiled binary
fn binary_path(config: &ConfigManager) -> Result<std::path::PathBuf> {
    let bin_dir = config.managed_dir("bin");
    Ok(bin_dir.join("slate-dark-mode-notify"))
}

/// Compile the Swift dark mode notifier binary.
/// Only recompiles if the binary doesn't exist.
pub fn ensure_binary(config: &ConfigManager) -> Result<std::path::PathBuf> {
    let bin_path = binary_path(config)?;

    if bin_path.exists() {
        return Ok(bin_path);
    }

    // Ensure parent directory exists
    if let Some(parent) = bin_path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            SlateError::LaunchdError(format!("Failed to create bin directory: {}", e))
        })?;
    }

    let parent = bin_path.parent().ok_or_else(|| {
        SlateError::LaunchdError("Compiled notifier path is missing a parent directory".to_string())
    })?;

    // Compile via unique temp files, then atomically publish the finished binary.
    let tmp_swift = NamedTempFile::new().map_err(|e| {
        SlateError::LaunchdError(format!("Failed to create Swift source temp file: {}", e))
    })?;
    fs::write(tmp_swift.path(), SWIFT_SOURCE)
        .map_err(|e| SlateError::LaunchdError(format!("Failed to write Swift source: {}", e)))?;

    let tmp_bin = NamedTempFile::new_in(parent).map_err(|e| {
        SlateError::LaunchdError(format!("Failed to create binary temp file: {}", e))
    })?;
    let tmp_bin_path = tmp_bin.path().to_path_buf();

    let output = Command::new("swiftc")
        .arg(tmp_swift.path())
        .arg("-o")
        .arg(&tmp_bin_path)
        .output()
        .map_err(|e| SlateError::LaunchdError(format!("Failed to run swiftc: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SlateError::LaunchdError(format!(
            "swiftc compilation failed: {}",
            stderr
        )));
    }

    tmp_bin.persist(&bin_path).map_err(|e| {
        SlateError::LaunchdError(format!(
            "Failed to install compiled notifier {}: {}",
            bin_path.display(),
            e
        ))
    })?;

    Ok(bin_path)
}

/// Remove the compiled binary
pub fn remove_binary(config: &ConfigManager) -> Result<()> {
    let bin_path = binary_path(config)?;
    if bin_path.exists() {
        fs::remove_file(&bin_path).ok();
    }
    Ok(())
}
