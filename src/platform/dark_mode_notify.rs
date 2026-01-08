use crate::config::ConfigManager;
use crate::error::{Result, SlateError};
use std::fs;
use std::process::Command;

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

    // Write Swift source to temp file and compile
    let tmp_swift = std::env::temp_dir().join("slate-dark-mode-notify.swift");
    fs::write(&tmp_swift, SWIFT_SOURCE).map_err(|e| {
        SlateError::LaunchdError(format!("Failed to write Swift source: {}", e))
    })?;

    let output = Command::new("swiftc")
        .args([
            tmp_swift.to_str().unwrap(),
            "-o",
            bin_path.to_str().unwrap(),
        ])
        .output()
        .map_err(|e| SlateError::LaunchdError(format!("Failed to run swiftc: {}", e)))?;

    // Clean up temp file
    let _ = fs::remove_file(&tmp_swift);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SlateError::LaunchdError(format!(
            "swiftc compilation failed: {}",
            stderr
        )));
    }

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
