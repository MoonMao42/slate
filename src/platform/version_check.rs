//! Version detection and minimum-version gating for supported tools.
use super::process_output::{self, Completion, Limits};
use crate::error::Result;
use semver::Version;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

pub(crate) const PROBE_LIMITS: Limits = Limits {
    timeout: Duration::from_secs(2),
    max_output: 64 * 1024,
};

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
            // Keep the existing Neovim floor; prereleases retain their ordering.
            "nvim" => Some("0.8.0"),
            // Other tools: not yet gated (future expansion)
            _ => None,
        }
    }

    /// Check if a version string is supported for the given tool
    /// Returns Ok() if supported, Err with user-friendly message if not
    pub fn check_version(tool_id: &str, version_str: &str) -> Result<()> {
        let Some(min_version_str) = Self::min_version(tool_id) else {
            // Tool not in policy table; allow it
            return Ok(());
        };

        let version = Version::parse(version_str).map_err(|_| {
            crate::error::SlateError::PlatformError(format!(
                "Invalid semantic version for '{}': expected a complete MAJOR.MINOR.PATCH with optional prerelease/build metadata",
                tool_id.escape_default()
            ))
        })?;
        if Self::supports(tool_id, &version) {
            Ok(())
        } else {
            Err(crate::error::SlateError::PlatformError(format!(
                "Tool '{}' version {} is not supported. Minimum version required: {}. Please upgrade {} and try again.",
                tool_id.escape_default(), version_str.escape_default(), min_version_str, tool_id.escape_default()
            )))
        }
    }

    /// A parsed value cannot turn a malformed probe into an old-version skip.
    /// Compare SemVer precedence, ignoring build metadata. A prerelease above
    /// the floor is allowed; this is a lower bound, not a stable-only channel.
    pub(crate) fn supports(tool_id: &str, version: &Version) -> bool {
        Self::min_version(tool_id).is_none_or(|minimum| {
            let minimum =
                Version::parse(minimum).expect("minimum-version constants are valid SemVer");
            !version.cmp_precedence(&minimum).is_lt()
        })
    }
}

/// Detect via `tool --version`, with a post-spawn deadline and a combined output
/// cap. Incomplete output is never accepted as proof of an installed version.
pub fn detect_version(tool_id: &str) -> Result<String> {
    detect_with_limits(tool_id, PROBE_LIMITS)
}

/// Probe an already-resolved executable without another PATH lookup. Keep the
/// diagnostic label separate so non-UTF-8 paths need no lossy conversion.
pub(crate) fn detect_version_at(tool_id: &str, executable: &Path) -> Result<Version> {
    // PATH may contain an empty/relative directory. A bare `nvim` result must
    // still name the detected file rather than trigger Command's PATH search.
    let executable = std::path::absolute(executable)?;
    detect_at_with_limits(tool_id, &executable, PROBE_LIMITS)
}

fn detect_with_limits(tool_id: &str, limits: Limits) -> Result<String> {
    detect_at_with_limits(tool_id, Path::new(tool_id), limits).map(|version| version.to_string())
}

fn detect_at_with_limits(tool_id: &str, executable: &Path, limits: Limits) -> Result<Version> {
    let output = process_output::capture(Command::new(executable).arg("--version"), limits)
        .map_err(|error| {
            crate::error::SlateError::PlatformError(format!(
                "Could not start or safely read '{} --version': {}",
                tool_id.escape_default(),
                error
            ))
        })?;
    let failure = match output.completion {
        Completion::TimedOut => Some(format!("timed out after {} ms", limits.timeout.as_millis())),
        Completion::OutputLimit => Some(format!(
            "exceeded the {}-byte combined output limit",
            limits.max_output
        )),
        Completion::Exited(status) if !status.success() => {
            Some("returned a non-zero exit status".into())
        }
        Completion::Exited(_) => None,
    };
    if let Some(reason) = failure {
        return Err(crate::error::SlateError::PlatformError(format!(
            "Version probe for '{}' {reason}; its output was not accepted",
            tool_id.escape_default()
        )));
    }

    let stdout = std::str::from_utf8(&output.stdout).map_err(|_| {
        crate::error::SlateError::PlatformError(format!(
            "Version probe for '{}' returned invalid UTF-8",
            tool_id.escape_default()
        ))
    })?;

    extract_version_from_output(tool_id, stdout)
}

/// Read only the first nonempty header, never a dependency/compiler version on
/// another line. Known tool names must match; arbitrary executable paths retain
/// the generic `tool [version] v?X.Y.Z` / bare-version convention.
fn extract_version_from_output(tool_id: &str, output: &str) -> Result<Version> {
    let invalid = || {
        crate::error::SlateError::PlatformError(format!(
        "Could not read a complete semantic version from the first non-empty line of '{} --version' output",
        tool_id.escape_default()
    ))
    };
    let header = output
        .lines()
        .find(|line| !line.trim().is_empty())
        .ok_or_else(invalid)?;
    if header.chars().any(|c| c.is_control() && c != '\t') {
        return Err(invalid());
    }
    let mut words = header.split_ascii_whitespace();
    let first = words.next().ok_or_else(invalid)?;
    let name = Path::new(tool_id)
        .file_name()
        .and_then(|name| name.to_str());
    let known_name = name.filter(|name| VersionPolicy::min_version(name).is_some());
    if known_name.is_some_and(|name| !first.eq_ignore_ascii_case(name)) {
        return Err(invalid());
    }
    let bare_first = first.strip_prefix('v').unwrap_or(first);
    let candidate = if known_name.is_none() && bare_first.starts_with(|c: char| c.is_ascii_digit())
    {
        first
    } else {
        let next = words.next().ok_or_else(invalid)?;
        if next.eq_ignore_ascii_case("version") {
            words.next().ok_or_else(invalid)?
        } else {
            next
        }
    };
    // Do not strip punctuation, suffixes or invalid characters to salvage a
    // valid prefix. Preserve prerelease and build identifiers in the result.
    Version::parse(candidate.strip_prefix('v').unwrap_or(candidate)).map_err(|_| invalid())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn version_policy_rejects_malformed_versions() {
        for version in [
            "",
            "1",
            "1.2",
            "1.2.3.4",
            "01.8.0",
            "1.02.3",
            "1.2.03",
            "1.2.unknown",
            "18446744073709551616.0.0",
            "0.8.0-",
            "0.8.0+",
            "0.8.0-rc.01",
            "0.8.0+bad_metadata",
            "v0.8.0",
            "0.8.0\u{1b}",
        ] {
            assert!(
                VersionPolicy::check_version("nvim", version).is_err(),
                "accepted {version:?}"
            );
        }
    }

    #[test]
    fn version_policy_orders_prereleases_without_build_weight() {
        for tool in ["nvim", "ghostty", "alacritty"] {
            let floor = VersionPolicy::min_version(tool).unwrap();
            for suffix in ["-dev", "-rc.1", "-rc.1+build.3"] {
                assert!(
                    VersionPolicy::check_version(tool, &format!("{floor}{suffix}")).is_err(),
                    "{tool} {suffix}"
                );
            }
            for suffix in ["", "+build.3", "+0001"] {
                assert!(
                    VersionPolicy::check_version(tool, &format!("{floor}{suffix}")).is_ok(),
                    "{tool} {suffix}"
                );
            }
        }
        assert!(VersionPolicy::check_version("nvim", "0.8.1-dev+g123").is_ok());
        assert!(VersionPolicy::check_version("nvim", "4294967296.0.0").is_ok());
        // Unlisted tools still have no compatibility policy.
        assert!(VersionPolicy::check_version("unlisted", "not a version").is_ok());
    }

    #[test]
    fn version_header_preserves_complete_semver() {
        for (output, expected) in [
            (
                "NVIM v0.12.0-dev-123+gabc123\nLuaJIT 2.1.0",
                "0.12.0-dev-123+gabc123",
            ),
            (
                "Ghostty 1.3.2-dev+abc123\nBuild Config\nZig 0.15.2",
                "1.3.2-dev+abc123",
            ),
            ("Alacritty 0.12.0-rc.1 (abc123)", "0.12.0-rc.1"),
        ] {
            assert_eq!(
                extract_version_from_output("fixture", output)
                    .unwrap()
                    .to_string(),
                expected,
                "{output:?}"
            );
        }
    }

    #[test]
    fn version_header_does_not_promote_malformed_or_dependency_versions() {
        for output in [
            "NVIM v0.8\nLuaJIT 2.1.0",
            "NVIM PRIVATE_VERSION\nLuaJIT 2.1.0",
            "NVIM v0.8.0garbage\nLuaJIT 2.1.0",
            "NVIM v0.8.0+\nLuaJIT 2.1.0",
            "NVIM v0.8.0)\nLuaJIT 2.1.0",
            "NVIM v01.8.0\nLuaJIT 2.1.0",
            "NVIM v0.8.0.1\nLuaJIT 2.1.0",
            "NVIM v0.8.0-rc.01\nLuaJIT 2.1.0",
            "NVIM unknown (LuaJIT 2.1.0)",
            "warning: PRIVATE_VERSION\nNVIM v0.12.0",
        ] {
            let error = extract_version_from_output("nvim", output)
                .expect_err(output)
                .to_string();
            assert!(!error.contains("PRIVATE_VERSION"));
        }
    }

    fn probe_script(root: &std::path::Path, body: &str) -> std::path::PathBuf {
        let path = root.join("version-probe");
        std::fs::write(&path, format!("#!/bin/sh\n[ \"$#\" = 1 ] && [ \"$1\" = --version ] || exit 91\nif read -r value; then exit 92; fi\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[test]
    fn version_probe_accepts_only_complete_successful_stdout_and_omits_private_errors() {
        let td = tempfile::tempdir().unwrap();
        for (body, expected) in [
            ("printf 'NVIM v0.12.0\\nBuild fixture\\n'", Some("0.12.0")),
            (
                "printf 'NVIM v0.12.0-dev-123+gabc\\nLuaJIT 2.1.0\\n'",
                Some("0.12.0-dev-123+gabc"),
            ),
            ("printf 'NVIM PRIVATE_VERSION\\nLuaJIT 2.1.0\\n'", None),
            ("printf 'NVIM v0.8.0garbage\\nLuaJIT 2.1.0\\n'", None),
            (
                "printf 'tool 1.2.3\\n'; printf 'PRIVATE_STDERR\\033' >&2",
                Some("1.2.3"),
            ),
            (
                "printf 'PRIVATE_STDOUT\\033[31m\\n'; printf 'PRIVATE_STDERR' >&2",
                None,
            ),
            ("printf 'tool 1.2.3\\n'; exit 7", None),
            ("printf '\\377'", None),
            ("printf 'tool 1.2.3\\n' >&2", None),
        ] {
            let path = probe_script(td.path(), body);
            let result = detect_version(path.to_str().unwrap());
            if let Some(version) = expected {
                assert_eq!(result.unwrap(), version);
            } else {
                let error = result.unwrap_err().to_string();
                assert!(!error.contains("PRIVATE") && !error.contains('\u{1b}'));
                assert!(error.len() < 512);
            }
        }
        let path = td.path().join("absent\u{1b}");
        let error = detect_version(path.to_str().unwrap())
            .unwrap_err()
            .to_string();
        assert!(!error.contains('\u{1b}'));
    }

    #[test]
    fn version_probe_enforces_exact_combined_cap_and_rejects_valid_prefix_on_overflow() {
        let td = tempfile::tempdir().unwrap();
        let flood = "printf 'tool 1.2.3\\n'; i=0; while [ $i -lt 6000 ]; do printf 'PRIVATE_NOISE'; printf 'PRIVATE_NOISE' >&2; i=$((i+1)); done";
        let limits = Limits {
            timeout: Duration::from_secs(2),
            max_output: 32,
        };
        for (body, success) in [
            // 11 stdout bytes + 21 stderr bytes = the exact accepted cap.
            (
                "printf 'tool 1.2.3\\n'; printf '123456789012345678901' >&2",
                true,
            ),
            (
                "printf 'tool 1.2.3\\n'; printf '1234567890123456789012' >&2",
                false,
            ),
            (flood, false),
        ] {
            let path = probe_script(td.path(), body);
            let result = detect_with_limits(path.to_str().unwrap(), limits);
            assert_eq!(result.is_ok(), success);
            if !success {
                let error = result.unwrap_err().to_string();
                assert!(error.contains("combined output limit"));
                assert!(!error.contains("PRIVATE_NOISE"));
            }
        }
        let path = probe_script(td.path(), flood);
        let error = detect_version(path.to_str().unwrap())
            .unwrap_err()
            .to_string();
        assert!(error.contains("65536-byte combined output limit"));
        assert!(!error.contains("PRIVATE_NOISE"));
    }

    #[test]
    fn version_probe_rejects_timeout_even_after_a_valid_version_line() {
        let td = tempfile::tempdir().unwrap();
        let path = probe_script(td.path(), "printf 'tool 1.2.3\\n'; exec /bin/sleep 20");
        let start = std::time::Instant::now();
        let result = detect_with_limits(
            path.to_str().unwrap(),
            Limits {
                timeout: Duration::from_millis(150),
                max_output: 1024,
            },
        );
        assert!(result.unwrap_err().to_string().contains("timed out"));
        assert!(start.elapsed() < Duration::from_secs(4));
    }

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
            extract_version_from_output("ghostty", output)
                .unwrap()
                .to_string(),
            "1.2.3".to_string()
        );
    }

    #[test]
    fn test_extract_version_with_v_prefix() {
        let output = "starship v1.15.0";
        assert_eq!(
            extract_version_from_output("starship", output)
                .unwrap()
                .to_string(),
            "1.15.0".to_string()
        );
    }

    #[test]
    fn test_extract_version_multiline() {
        let output = "Alacritty 0.12.0\nsome other info";
        assert_eq!(
            extract_version_from_output("alacritty", output)
                .unwrap()
                .to_string(),
            "0.12.0".to_string()
        );
    }

    #[test]
    fn version_header_matches_known_tool_and_accepts_header_conventions() {
        for (tool, output, expected) in [
            ("nvim", "\n\tNVIM\tv0.8.0\r\nBuild type: Release", "0.8.0"),
            ("/private/bin/nvim", "NVIM v0.8.0+build.1", "0.8.0+build.1"),
            ("ghostty", "Ghostty 1.1.0", "1.1.0"),
            ("alacritty", "alacritty 0.12.0 (fixture hash)", "0.12.0"),
            (
                "fixture",
                "tool version v1.2.3-rc.1+build.2",
                "1.2.3-rc.1+build.2",
            ),
            ("fixture", "v1.2.3", "1.2.3"),
        ] {
            assert_eq!(
                extract_version_from_output(tool, output)
                    .unwrap()
                    .to_string(),
                expected
            );
        }
        for output in [
            "LuaJIT 2.1.0",
            "not-NVIM v0.8.0",
            "0.8.0",
            "NVIM\n0.8.0",
            "NVIM v0.8.0\u{1b}",
            "NVIM v0.8.0\rprivate",
        ] {
            assert!(
                extract_version_from_output("nvim", output).is_err(),
                "{output:?}"
            );
        }
        assert!(extract_version_from_output("fixture", "1.2 9.0.0").is_err());
    }
}
