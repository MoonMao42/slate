/// Version detection and minimum-version gating for supported tools
use crate::error::Result;
use std::process::Command;

/// Minimum supported versions for high-confidence tools
pub struct VersionPolicy;

impl VersionPolicy {
    /// Get minimum supported version for a tool
    pub fn min_version(tool_id: &str) -> Option<&'static str> {
        match tool_id {
            // Ghostty: 1.1.0+
            "ghostty" => Some("1.1.0"),
            // Alacritty: 0.12.0+
            "alacritty" => Some("0.12.0"),
            // Neovim: 0.8.0+ (nvim_set_hl API + vim.uv baseline; Phase 17 D-01).
            "nvim" => Some("0.8.0"),
            // Other tools: not yet gated (future expansion)
            _ => None,
        }
    }

    /// Check if a version string is supported for the given tool
    /// Returns Ok(()) if supported, Err with user-friendly message if not
    pub fn check_version(tool_id: &str, version_str: &str) -> Result<()> {
        let Some(min_version_str) = Self::min_version(tool_id) else {
            // Tool not in policy table; allow it
            return Ok(());
        };

        // Simple version string comparison for semver-like strings
        // Format: "X.Y.Z"
        if Self::is_version_supported(version_str, min_version_str) {
            Ok(())
        } else {
            Err(crate::error::SlateError::PlatformError(format!(
                "Tool '{}' version {} is not supported. Minimum version required: {}. Please upgrade {} and try again.",
                tool_id, version_str, min_version_str, tool_id
            )))
        }
    }

    /// Compare two version strings
    fn is_version_supported(current: &str, minimum: &str) -> bool {
        let current_parts: Vec<&str> = current.split('.').collect();
        let minimum_parts: Vec<&str> = minimum.split('.').collect();

        for i in 0..3.min(current_parts.len().min(minimum_parts.len())) {
            let curr = current_parts[i].parse::<u32>().unwrap_or(0);
            let min = minimum_parts[i].parse::<u32>().unwrap_or(0);

            if curr > min {
                return true;
            }
            if curr < min {
                return false;
            }
            // Equal, continue to next
        }

        // All compared parts are equal, versions match minimum
        true
    }
}

/// Detect tool version via `tool --version`
pub fn detect_version(tool_id: &str) -> Result<String> {
    let output = Command::new(tool_id)
        .arg("--version")
        .output()
        .map_err(|e| {
            crate::error::SlateError::PlatformError(format!(
                "Failed to run '{} --version': {}",
                tool_id, e
            ))
        })?;

    if !output.status.success() {
        return Err(crate::error::SlateError::PlatformError(format!(
            "'{}' returned non-zero exit code",
            tool_id
        )));
    }

    let stdout = String::from_utf8(output.stdout).map_err(|e| {
        crate::error::SlateError::PlatformError(format!("Invalid UTF-8 output: {}", e))
    })?;

    // Extract version: common patterns are "tool 1.2.3", "tool version 1.2.3", etc.
    extract_version_from_output(&stdout)
}

/// Extract semver from version output
/// Handles common patterns like:
/// "ghostty 1.2.3 (abc123)"
/// "Alacritty 0.12.0"
/// "starship v1.2.3"
fn extract_version_from_output(output: &str) -> Result<String> {
    let output = output.trim();

    // Try to find a pattern like "v?X.Y.Z" or "X.Y.Z"
    for word in output.split_whitespace() {
        let word_clean = word
            .trim_end_matches(|c: char| !c.is_ascii_digit() && c != '.')
            .trim_end_matches('(');

        // Check if this looks like a version (e.g., 1.2.3)
        if is_version_string(word_clean) {
            return Ok(word_clean.to_string());
        }

        // Try stripping leading 'v'
        if let Some(stripped) = word_clean.strip_prefix('v') {
            if is_version_string(stripped) {
                return Ok(stripped.to_string());
            }
        }
    }

    Err(crate::error::SlateError::PlatformError(format!(
        "Could not extract version from output: {}",
        output
    )))
}

/// Check if a string looks like a version (simple check for X.Y.Z pattern)
fn is_version_string(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() < 2 {
        return false;
    }
    parts
        .iter()
        .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_policy_ghostty_min() {
        assert_eq!(VersionPolicy::min_version("ghostty"), Some("1.1.0"));
    }

    #[test]
    fn test_version_policy_alacritty_min() {
        assert_eq!(VersionPolicy::min_version("alacritty"), Some("0.12.0"));
    }

    #[test]
    fn version_policy_nvim_min_is_0_8() {
        assert_eq!(VersionPolicy::min_version("nvim"), Some("0.8.0"));
    }

    #[test]
    fn check_version_accepts_nvim_0_12() {
        assert!(VersionPolicy::check_version("nvim", "0.12.0").is_ok());
    }

    #[test]
    fn check_version_accepts_nvim_0_8_floor() {
        assert!(VersionPolicy::check_version("nvim", "0.8.0").is_ok());
    }

    #[test]
    fn check_version_rejects_nvim_0_7() {
        assert!(VersionPolicy::check_version("nvim", "0.7.2").is_err());
    }

    #[test]
    fn test_check_version_supported() {
        assert!(VersionPolicy::check_version("ghostty", "1.2.0").is_ok());
        assert!(VersionPolicy::check_version("ghostty", "1.1.0").is_ok());
    }

    #[test]
    fn test_check_version_unsupported() {
        let result = VersionPolicy::check_version("ghostty", "1.0.9");
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_version_standard() {
        let output = "ghostty 1.2.3 (abc123)";
        assert_eq!(
            extract_version_from_output(output).unwrap(),
            "1.2.3".to_string()
        );
    }

    #[test]
    fn test_extract_version_with_v_prefix() {
        let output = "starship v1.15.0";
        assert_eq!(
            extract_version_from_output(output).unwrap(),
            "1.15.0".to_string()
        );
    }

    #[test]
    fn test_extract_version_multiline() {
        let output = "Alacritty 0.12.0\nsome other info";
        assert_eq!(
            extract_version_from_output(output).unwrap(),
            "0.12.0".to_string()
        );
    }

    #[test]
    fn test_is_version_string() {
        assert!(is_version_string("1.2.3"));
        assert!(is_version_string("0.12.0"));
        assert!(is_version_string("1.2"));
        assert!(!is_version_string("abc"));
    }
}
