/// Pre-flight checks before setup execution
/// Verifies environment readiness: Homebrew, network, write permissions

use crate::error::Result;
use std::path::Path;
use std::process::Command;

/// Environment readiness status
#[derive(Debug, Clone)]
pub struct PreflightResult {
    pub homebrew_available: bool,
    pub network_reachable: bool,
    pub write_permissions_ok: bool,
    pub blockers: Vec<String>,
}

impl PreflightResult {
    /// Check if all critical checks pass
    pub fn is_ready(&self) -> bool {
        self.blockers.is_empty()
    }

    /// Format results for display
    pub fn format_for_display(&self) -> String {
        let mut output = String::new();
        output.push_str("✦ Pre-flight Checks:\n\n");

        let brew_status = if self.homebrew_available { "✓" } else { "✗" };
        output.push_str(&format!("  {} Homebrew available\n", brew_status));

        let network_status = if self.network_reachable { "✓" } else { "✗" };
        output.push_str(&format!("  {} Network reachable\n", network_status));

        let write_status = if self.write_permissions_ok { "✓" } else { "✗" };
        output.push_str(&format!("  {} Write permissions OK\n", write_status));

        if !self.blockers.is_empty() {
            output.push_str("\n⚠ Issues found:\n");
            for blocker in &self.blockers {
                output.push_str(&format!("  • {}\n", blocker));
            }
        }

        output
    }
}

/// Run all pre-flight checks
pub fn run_checks() -> Result<PreflightResult> {
    let mut result = PreflightResult {
        homebrew_available: false,
        network_reachable: false,
        write_permissions_ok: false,
        blockers: Vec::new(),
    };

    // Check 1: Homebrew availability
    result.homebrew_available = check_homebrew_available();
    if !result.homebrew_available {
        result.blockers.push(
            "Homebrew not found. Install from https://brew.sh".to_string()
        );
    }

    // Check 2: Network reachability (only if brew is available)
    if result.homebrew_available {
        result.network_reachable = check_network_reachable();
        if !result.network_reachable {
            result.blockers.push(
                "Network unreachable. Cannot download packages.".to_string()
            );
        }
    }

    // Check 3: Write permissions in config directory
    result.write_permissions_ok = check_write_permissions();
    if !result.write_permissions_ok {
        result.blockers.push(
            "No write permissions in ~/.config directory.".to_string()
        );
    }

    Ok(result)
}

/// Check if Homebrew is available in PATH
fn check_homebrew_available() -> bool {
    match Command::new("brew")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
    {
        Ok(status) => status.success(),
        Err(_) => false,
    }
}

/// Check if network is reachable (simple DNS check)
fn check_network_reachable() -> bool {
    // Try to resolve Homebrew formulae host
    match std::net::ToSocketAddrs::to_socket_addrs("formulae.brew.sh:443") {
        Ok(mut addrs) => addrs.next().is_some(),
        Err(_) => false,
    }
}

/// Check if we can write to config directory
fn check_write_permissions() -> bool {
    // Try to write to ~/.config directory
    let config_dir = match std::env::var("HOME") {
        Ok(home) => {
            let path = Path::new(&home).join(".config");
            // Create if doesn't exist
            let _ = std::fs::create_dir_all(&path);
            path
        }
        Err(_) => return false,
    };

    // Try to create a temp file to verify write access
    use std::fs::File;
    use std::io::Write;

    let temp_path = config_dir.join(".slate_preflight_test");
    match File::create(&temp_path) {
        Ok(mut file) => {
            let _ = file.write_all(b"test");
            let _ = std::fs::remove_file(&temp_path);
            true
        }
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preflight_result_is_ready() {
        let mut result = PreflightResult {
            homebrew_available: true,
            network_reachable: true,
            write_permissions_ok: true,
            blockers: Vec::new(),
        };
        assert!(result.is_ready());

        result.blockers.push("test blocker".to_string());
        assert!(!result.is_ready());
    }

    #[test]
    fn test_preflight_format_display() {
        let result = PreflightResult {
            homebrew_available: true,
            network_reachable: true,
            write_permissions_ok: true,
            blockers: vec!["Test issue".to_string()],
        };
        let output = result.format_for_display();
        assert!(output.contains("Pre-flight Checks"));
        assert!(output.contains("Issues found"));
        assert!(output.contains("Test issue"));
    }

    #[test]
    fn test_check_write_permissions() {
        // This should pass in normal environments
        let can_write = check_write_permissions();
        // We don't assert true/false here because it depends on env
        // Just verify it doesn't panic
        let _ = can_write;
    }
}
