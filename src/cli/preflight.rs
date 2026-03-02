use crate::adapter::font::FontAdapter;
use crate::cli::font_selection::FontCatalog;
use crate::cli::tool_selection::{detect_installed_tools_with_env, ToolCatalog};
use crate::detection;
use crate::env::SlateEnv;
use crate::error::Result;

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
    pub blocking: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreflightScenario {
    GuidedSetup,
    QuickSetup,
    RetryInstall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NetworkExpectation {
    DownloadsLikely,
    LocalConfigOnly,
}

impl PreflightResult {
    /// Check if all required checks passed
    pub fn is_ready(&self) -> bool {
        self.checks.iter().filter(|c| c.blocking).all(|c| c.passed)
    }

    /// Format results for display
    pub fn format_for_display(&self) -> String {
        let mut output = String::from("✓ Preflight Checks\n");
        for check in &self.checks {
            let icon = if !check.blocking {
                "•"
            } else if check.passed {
                "✓"
            } else {
                "✗"
            };
            output.push_str(&format!("{} {}: {}\n", icon, check.name, check.description));
        }
        output
    }

    pub fn format_blocking_guidance(&self) -> String {
        let sections = self
            .checks
            .iter()
            .filter(|check| check.blocking && !check.passed)
            .map(format_blocking_section)
            .collect::<Vec<_>>();

        if sections.is_empty() {
            return String::new();
        }

        format!(
            "Setup paused until these blockers are fixed:\n\n{}\n\nAfter that, rerun `slate setup`.",
            sections.join("\n\n")
        )
    }
}

/// Run preflight checks
pub fn run_checks() -> Result<PreflightResult> {
    let env = SlateEnv::from_process()?;
    run_checks_with_env(&env)
}

/// Run preflight checks with injected SlateEnv.
pub fn run_checks_with_env(env: &SlateEnv) -> Result<PreflightResult> {
    run_checks_for_setup_with_env(env, PreflightScenario::GuidedSetup)
}

pub fn run_checks_for_setup_with_env(
    env: &SlateEnv,
    scenario: PreflightScenario,
) -> Result<PreflightResult> {
    let mut checks = Vec::new();
    let homebrew_installed = is_homebrew_installed();
    let zsh_available = is_zsh_available();
    let network_reachable = check_network_reachable();
    let write_permissions = check_write_permissions_with_env(env);
    let installed = detect_installed_tools_with_env(env);
    let tool_count = installed.values().filter(|p| p.installed).count();
    let installed_nerd_fonts = FontAdapter::detect_installed_nerd_fonts_with_env(env)
        .unwrap_or_default()
        .len();
    let network_expectation = infer_network_expectation(&installed, installed_nerd_fonts, scenario);

    // Check 1: Homebrew is installed (required for tool and font installation)
    checks.push(PreflightCheck {
        name: "Homebrew".to_string(),
        description: if homebrew_installed {
            "installed — primary install path is ready".to_string()
        } else {
            "missing — Slate uses Homebrew as the supported install path on a new Mac".to_string()
        },
        passed: homebrew_installed,
        blocking: true,
    });

    // Check 2: Zsh is available
    checks.push(PreflightCheck {
        name: "Zsh".to_string(),
        description: if zsh_available {
            "available — zsh shell integration can be written".to_string()
        } else {
            "missing — this release only supports shell integration for zsh".to_string()
        },
        passed: zsh_available,
        blocking: true,
    });

    // Check 3: Network reachable
    checks.push(PreflightCheck {
        name: "Network".to_string(),
        description: network_description(network_reachable, network_expectation),
        passed: true, // Network is optional — user may be offline
        blocking: false,
    });

    // Check 4: Write permissions
    checks.push(PreflightCheck {
        name: "Write Permissions".to_string(),
        description: if write_permissions {
            "ready — can write Slate-managed files under ~/.config".to_string()
        } else {
            "blocked — cannot write Slate-managed files under ~/.config".to_string()
        },
        passed: write_permissions,
        blocking: true,
    });

    // Check 5: Tools available
    checks.push(PreflightCheck {
        name: "Tools".to_string(),
        description: format!(
            "{} of {} managed tools already present",
            tool_count,
            ToolCatalog::all_tools().len()
        ),
        passed: true, // Optional — user can install from scratch
        blocking: false,
    });

    // Check 6: Fonts available
    checks.push(PreflightCheck {
        name: "Fonts".to_string(),
        description: if installed_nerd_fonts > 0 {
            format!(
                "{} supported Nerd Font(s) already installed",
                installed_nerd_fonts
            )
        } else {
            format!(
                "no supported Nerd Font detected yet — setup can install one from {} choices",
                FontCatalog::all_fonts().len()
            )
        },
        passed: true, // Optional — defaults available
        blocking: false,
    });

    Ok(PreflightResult { checks })
}

/// Check if Homebrew is installed
pub fn is_homebrew_installed() -> bool {
    detection::homebrew_executable().is_some()
}

/// Check if Zsh is available
fn is_zsh_available() -> bool {
    std::path::Path::new("/bin/zsh").exists() || detection::command_path("zsh").is_some()
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

fn infer_network_expectation(
    installed: &std::collections::HashMap<String, crate::detection::ToolPresence>,
    installed_nerd_fonts: usize,
    scenario: PreflightScenario,
) -> NetworkExpectation {
    let missing_installable_tool = ToolCatalog::installable_tools().into_iter().any(|tool| {
        !installed
            .get(tool.id)
            .map(|presence| presence.installed)
            .unwrap_or(false)
    });

    let quick_mode_missing_core = ["starship", "zsh-syntax-highlighting"]
        .iter()
        .any(|tool_id| {
            !installed
                .get(*tool_id)
                .map(|presence| presence.installed)
                .unwrap_or(false)
        });

    match scenario {
        PreflightScenario::RetryInstall => NetworkExpectation::DownloadsLikely,
        PreflightScenario::QuickSetup => {
            if quick_mode_missing_core || installed_nerd_fonts == 0 {
                NetworkExpectation::DownloadsLikely
            } else {
                NetworkExpectation::LocalConfigOnly
            }
        }
        PreflightScenario::GuidedSetup => {
            if missing_installable_tool || installed_nerd_fonts == 0 {
                NetworkExpectation::DownloadsLikely
            } else {
                NetworkExpectation::LocalConfigOnly
            }
        }
    }
}

fn network_description(reachable: bool, expectation: NetworkExpectation) -> String {
    match (reachable, expectation) {
        (true, NetworkExpectation::DownloadsLikely) => {
            "reachable — installs, font downloads, and release fallback are available".to_string()
        }
        (true, NetworkExpectation::LocalConfigOnly) => {
            "reachable — downloads are available if this run ends up needing them".to_string()
        }
        (false, NetworkExpectation::DownloadsLikely) => {
            "offline — installs or font downloads may fail, but already-installed tools can still be configured".to_string()
        }
        (false, NetworkExpectation::LocalConfigOnly) => {
            "offline — no download looks necessary for this run, so local config can still continue".to_string()
        }
    }
}

fn format_blocking_section(check: &PreflightCheck) -> String {
    match check.name.as_str() {
        "Homebrew" => [
            "Homebrew",
            "  What happened: Homebrew is missing, so Slate has no supported install path on this Mac yet.",
            "  Completed: Preflight ran and no config was changed.",
            "  Not completed: Slate cannot install tools or Nerd Fonts until Homebrew exists.",
            "  Next: Install Homebrew from https://brew.sh, then rerun `slate setup`.",
        ]
        .join("\n"),
        "Zsh" => [
            "Zsh",
            "  What happened: zsh was not found in this environment.",
            "  Completed: Slate checked the rest of the machine and did not modify your files.",
            "  Not completed: shell integration and Ghostty watcher hooks stay zsh-only in this release.",
            "  Next: Install or enable zsh, then rerun `slate setup`.",
        ]
        .join("\n"),
        "Write Permissions" => [
            "Write Permissions",
            "  What happened: Slate could not write under ~/.config for this user.",
            "  Completed: Preflight finished and nothing was partially written.",
            "  Not completed: managed config files, shell integration, and snapshots cannot be created.",
            "  Next: Fix ownership or permissions for ~/.config, then rerun `slate setup`.",
        ]
        .join("\n"),
        _ => format!(
            "{}\n  What happened: {}\n  Completed: Preflight ran safely.\n  Not completed: Setup cannot continue yet.\n  Next: Fix the blocker above, then rerun `slate setup`.",
            check.name, check.description
        ),
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
                    blocking: true,
                },
                PreflightCheck {
                    name: "Test2".to_string(),
                    description: "passes".to_string(),
                    passed: true,
                    blocking: true,
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
                blocking: true,
            }],
        };
        assert!(!result.is_ready());
    }

    #[test]
    fn test_preflight_run_checks() {
        let result = run_checks().unwrap();
        assert!(!result.checks.is_empty());
    }

    #[test]
    fn test_network_description_changes_with_download_need() {
        let offline_local = network_description(false, NetworkExpectation::LocalConfigOnly);
        let offline_downloads = network_description(false, NetworkExpectation::DownloadsLikely);

        assert!(offline_local.contains("no download looks necessary"));
        assert!(offline_downloads.contains("font downloads may fail"));
    }

    #[test]
    fn test_format_blocking_guidance_is_actionable() {
        let result = PreflightResult {
            checks: vec![PreflightCheck {
                name: "Homebrew".to_string(),
                description: "missing".to_string(),
                passed: false,
                blocking: true,
            }],
        };

        let message = result.format_blocking_guidance();
        assert!(message.contains("Setup paused"));
        assert!(message.contains("Homebrew"));
        assert!(message.contains("https://brew.sh"));
    }
}
