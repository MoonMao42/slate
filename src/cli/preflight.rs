use crate::cli::font_selection::FontCatalog;
use crate::cli::tool_selection::{detect_installed_tools_with_env, ToolCatalog};
use crate::detection;
use crate::env::SlateEnv;
use crate::error::Result;
use std::process::Command;

/// Preflight checks before setup
#[derive(Debug, Clone)]
pub struct PreflightResult {
    pub checks: Vec<PreflightCheck>,
}

#[derive(Debug, Clone)]
pub struct PreflightCheck {
    pub name: String,
    pub description: String,
    pub passed: bool,
}

impl PreflightResult {
    /// Check if all required checks passed
    pub fn is_ready(&self) -> bool {
        // All checks must pass except optional ones
        self.checks
            .iter()
            .filter(|c| !c.name.starts_with("Optional:"))
            .all(|c| c.passed)
    }

    /// Format results for display
    pub fn format_for_display(&self) -> String {
        let mut output = String::from("✓ Preflight Checks\n");
        for check in &self.checks {
            let icon = if check.passed { "✓" } else { "✗" };
            output.push_str(&format!("{} {}: {}\n", icon, check.name, check.description));
        }
        output
    }
}

/// Run preflight checks
pub fn run_checks() -> Result<PreflightResult> {
    let env = SlateEnv::from_process()?;
    run_checks_with_env(&env)
}

/// Run preflight checks with injected SlateEnv.
pub fn run_checks_with_env(env: &SlateEnv) -> Result<PreflightResult> {
    let mut checks = Vec::new();

    // Check 1: Homebrew is installed (required for tool and font installation)
    checks.push(PreflightCheck {
        name: "Homebrew".to_string(),
        description: if is_homebrew_installed() {
            "installed".to_string()
        } else {
            "not found — install at https://brew.sh".to_string()
        },
        passed: is_homebrew_installed(),
    });

    // Check 2: Zsh is available
    checks.push(PreflightCheck {
        name: "Zsh".to_string(),
        description: if is_zsh_available() {
            "available".to_string()
        } else {
            "not found (required for shell integration)".to_string()
        },
        passed: is_zsh_available(),
    });

    // Check 3: Network reachable
    checks.push(PreflightCheck {
        name: "Network".to_string(),
        description: if check_network_reachable() {
            "reachable".to_string()
        } else {
            "check skipped (optional, required for tool downloads)".to_string()
        },
        passed: true, // Network is optional — user may be offline
    });

    // Check 4: Write permissions
    checks.push(PreflightCheck {
        name: "Write Permissions".to_string(),
        description: if check_write_permissions_with_env(env) {
            "can write to ~/.config".to_string()
        } else {
            "cannot write to ~/.config (required)".to_string()
        },
        passed: check_write_permissions_with_env(env),
    });

    // Check 5: Tools available
    let installed = detect_installed_tools_with_env(env);
    let tool_count = installed.values().filter(|&&v| v).count();

    checks.push(PreflightCheck {
        name: "Optional: Tools".to_string(),
        description: format!(
            "{} of {} tools already installed",
            tool_count,
            ToolCatalog::all_tools().len()
        ),
        passed: true, // Optional — user can install from scratch
    });

    // Check 6: Fonts available
    let fonts = FontCatalog::all_fonts();
    checks.push(PreflightCheck {
        name: "Optional: Fonts".to_string(),
        description: format!("{} fonts available", fonts.len()),
        passed: true, // Optional — defaults available
    });

    Ok(PreflightResult { checks })
}

/// Check if Homebrew is installed
pub fn is_homebrew_installed() -> bool {
    detection::homebrew_executable().is_some()
}

/// Check if Zsh is available
fn is_zsh_available() -> bool {
    match Command::new("which").arg("zsh").output() {
        Ok(status) => status.status.success(),
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
fn check_write_permissions_with_env(env: &SlateEnv) -> bool {
    // Try to write to ~/.config directory
    let config_dir = {
        let path = env.xdg_config_home().to_path_buf();
        let _ = std::fs::create_dir_all(&path);
        path
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
    fn test_preflight_is_ready_with_all_passing() {
        let result = PreflightResult {
            checks: vec![
                PreflightCheck {
                    name: "Test1".to_string(),
                    description: "passes".to_string(),
                    passed: true,
                },
                PreflightCheck {
                    name: "Test2".to_string(),
                    description: "passes".to_string(),
                    passed: true,
                },
            ],
        };
        assert!(result.is_ready());
    }

    #[test]
    fn test_preflight_is_ready_with_failures() {
        let result = PreflightResult {
            checks: vec![PreflightCheck {
                name: "Test1".to_string(),
                description: "fails".to_string(),
                passed: false,
            }],
        };
        assert!(!result.is_ready());
    }

    #[test]
    fn test_preflight_run_checks() {
        let result = run_checks().unwrap();
        assert!(!result.checks.is_empty());
    }
}
