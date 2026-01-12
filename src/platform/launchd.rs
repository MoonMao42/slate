use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use atomic_write_file::AtomicWriteFile;
use plist::{Dictionary, Value};
use std::fs;
use std::io::{Cursor, Write};
use std::path::PathBuf;
use std::process::Command;

const AGENT_LABEL: &str = "sh.slate.auto-theme";
const AGENT_FILENAME: &str = "sh.slate.auto-theme.plist";
const NOTIFICATION_KEY: &str = "AppleInterfaceThemeChangedNotification";

/// Get the path to the launchd plist file
fn agent_path() -> Result<PathBuf> {
    let env = SlateEnv::from_process()?;
    let home = env.home().to_path_buf();
    Ok(home.join("Library/LaunchAgents").join(AGENT_FILENAME))
}

/// Resolve the binary path for slate
/// Tries multiple paths in order:
/// 1. std::env::current_exe() (may be cargo or source build)
/// 2. /opt/homebrew/bin/slate (Apple Silicon Homebrew)
/// 3. /usr/local/bin/slate (Intel Homebrew)
pub fn resolve_binary_path() -> Result<String> {
    let exe = std::env::current_exe()?;
    Ok(exe.to_string_lossy().to_string())
}

/// Generate the plist content for the launchd agent
pub fn generate_plist(binary_path: &str) -> Result<String> {
    let mut plist_dict = Dictionary::new();

    // Label
    plist_dict.insert("Label".to_string(), Value::String(AGENT_LABEL.to_string()));

    // ProgramArguments: [binary_path, "theme", "--auto"]
    let program_args = vec![
        Value::String(binary_path.to_string()),
        Value::String("theme".to_string()),
        Value::String("--auto".to_string()),
    ];
    plist_dict.insert("ProgramArguments".to_string(), Value::Array(program_args));

    // WatchPaths: trigger when macOS writes appearance preference.
    // Switching dark/light mode updates this file immediately.
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let prefs_path = format!("{}/Library/Preferences/.GlobalPreferences.plist", home);
    plist_dict.insert(
        "WatchPaths".to_string(),
        Value::Array(vec![Value::String(prefs_path)]),
    );

    // Optional logging
    plist_dict.insert(
        "StandardOutPath".to_string(),
        Value::String("/var/tmp/slate-auto-theme.log".to_string()),
    );
    plist_dict.insert(
        "StandardErrorPath".to_string(),
        Value::String("/var/tmp/slate-auto-theme.err".to_string()),
    );

    // Serialize to XML using a buffer
    let mut buffer = Cursor::new(Vec::new());
    plist::to_writer_xml(&mut buffer, &plist_dict)
        .map_err(|e| SlateError::LaunchdError(format!("Failed to serialize plist: {}", e)))?;

    let plist_bytes = buffer.into_inner();
    let plist_content = String::from_utf8(plist_bytes).map_err(|e| {
        SlateError::LaunchdError(format!("Failed to convert plist to UTF-8: {}", e))
    })?;

    Ok(plist_content)
}

/// Install the launchd agent
pub fn install_agent() -> Result<()> {
    let path = agent_path()?;
    let binary = resolve_binary_path()?;
    let plist_content = generate_plist(&binary)?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            SlateError::LaunchdError(format!("Failed to create LaunchAgents directory: {}", e))
        })?;
    }

    // Atomic write: temp file → fsync → rename. Prevents launchctl from
    // seeing a half-written plist if it races with the bootstrap call.
    let mut file = AtomicWriteFile::open(&path).map_err(|e| {
        SlateError::LaunchdError(format!("Failed to open plist for atomic write: {}", e))
    })?;
    file.write_all(plist_content.as_bytes())
        .map_err(|e| SlateError::LaunchdError(format!("Failed to write plist: {}", e)))?;
    file.commit()
        .map_err(|e| SlateError::LaunchdError(format!("Failed to commit plist: {}", e)))?;

    // Unload any previously loaded instance before bootstrapping.
    launchctl_unload(AGENT_LABEL).ok();

    // Load agent; clean up plist on failure so retries start fresh.
    if let Err(e) = launchctl_load(AGENT_LABEL) {
        let _ = fs::remove_file(&path);
        return Err(e);
    }

    Ok(())
}

/// Uninstall the launchd agent
pub fn uninstall_agent() -> Result<()> {
    // Unload agent (soft fail if not loaded)
    launchctl_unload(AGENT_LABEL).ok();

    // Delete plist file
    let path = agent_path()?;
    if path.exists() {
        fs::remove_file(&path).ok();
    }

    Ok(())
}

/// Check if the agent is currently loaded
pub fn check_agent_loaded() -> Result<bool> {
    let output = Command::new("launchctl")
        .args(["list", AGENT_LABEL])
        .output()
        .map_err(|e| SlateError::LaunchdError(format!("Failed to run launchctl list: {}", e)))?;

    Ok(output.status.success())
}

/// Load the agent via launchctl
fn launchctl_load(_label: &str) -> Result<()> {
    let uid = unsafe { libc::getuid() };
    let plist_path = agent_path()?;
    let plist_path_str = plist_path.to_string_lossy().to_string();

    let output = Command::new("launchctl")
        .args(["bootstrap", &format!("gui/{}", uid), &plist_path_str])
        .output()
        .map_err(|e| {
            SlateError::LaunchdError(format!("Failed to run launchctl bootstrap: {}", e))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SlateError::LaunchdError(format!(
            "launchctl bootstrap failed: {}",
            stderr
        )));
    }

    Ok(())
}

/// Unload the agent via launchctl
fn launchctl_unload(label: &str) -> Result<()> {
    let uid = unsafe { libc::getuid() };
    Command::new("launchctl")
        .args(["bootout", &format!("gui/{}/{}", uid, label)])
        .output()
        .map_err(|e| SlateError::LaunchdError(format!("Failed to run launchctl bootout: {}", e)))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_plist_valid_xml() {
        let binary_path = "/opt/homebrew/bin/slate";
        let result = generate_plist(binary_path);

        assert!(result.is_ok());
        let plist_content = result.unwrap();

        // Verify XML structure
        assert!(plist_content.contains("<?xml"));
        assert!(plist_content.contains(AGENT_LABEL));
        assert!(plist_content.contains("theme"));
        assert!(plist_content.contains("--auto"));
    }

    #[test]
    fn test_generate_plist_contains_program_args() {
        let binary_path = "/opt/homebrew/bin/slate";
        let plist_content = generate_plist(binary_path).unwrap();

        assert!(plist_content.contains("ProgramArguments"));
        assert!(plist_content.contains("/opt/homebrew/bin/slate"));
    }

    #[test]
    fn test_generate_plist_contains_watch_paths() {
        let binary_path = "/usr/local/bin/slate";
        let plist_content = generate_plist(binary_path).unwrap();

        assert!(plist_content.contains("WatchPaths"));
        assert!(plist_content.contains(".GlobalPreferences.plist"));
    }

    #[test]
    fn test_binary_path_resolution() {
        let result = resolve_binary_path();
        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(!path.is_empty());
    }

    #[test]
    fn test_plist_roundtrip() {
        let binary_path = "/opt/homebrew/bin/slate";
        let plist_content = generate_plist(binary_path).unwrap();

        // Try to parse the generated plist as bytes
        let plist_bytes = plist_content.as_bytes();
        let parsed: std::result::Result<Dictionary, _> = plist::from_bytes(plist_bytes);
        assert!(parsed.is_ok(), "Generated plist should be valid");

        let dict = parsed.unwrap();
        assert_eq!(
            dict.get("Label").and_then(|v| v.as_string()),
            Some(AGENT_LABEL)
        );
    }
}
