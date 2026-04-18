use crate::adapter::font::FontAdapter;
use crate::brand::language::Language;
use crate::cli::font_selection::FontCatalog;
use crate::cli::tool_selection::{detect_installed_tools_with_env, ToolCatalog};
use crate::env::SlateEnv;
use crate::error::Result;
use crate::platform::capabilities::{detect_capabilities, CapabilityReport, SupportLevel};

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
    /// Re-run against an existing slate install: no new downloads, just refresh config.
    /// Relaxes Package Manager blocking — no brew/apt required when nothing needs installing.
    ConfigOnlyReconfigure,
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
        let mut output = String::from(Language::PREFLIGHT_HEADER);
        output.push('\n');
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
    let network_reachable = check_network_reachable();
    let write_permissions = check_write_permissions_with_env(env);
    let installed = detect_installed_tools_with_env(env);
    let tool_count = installed.values().filter(|p| p.installed).count();
    let installed_nerd_fonts = FontAdapter::detect_installed_nerd_fonts_with_env(env)
        .unwrap_or_default()
        .len();
    let network_expectation = infer_network_expectation(&installed, installed_nerd_fonts, scenario);
    let capabilities = detect_capabilities();
    let terminal_features = crate::detection::TerminalProfile::detect().feature_summary();

    checks.push(capability_preflight_check("OS", &capabilities.os, true));
    checks.push(capability_preflight_check("Arch", &capabilities.arch, true));
    checks.push(capability_preflight_check(
        "Shell",
        &capabilities.shell,
        true,
    ));
    checks.push(capability_preflight_check(
        "Package Manager",
        &capabilities.package_manager,
        downloads_require_package_manager(network_expectation),
    ));
    checks.push(capability_preflight_check(
        "Desktop Appearance",
        &capabilities.desktop_appearance,
        false,
    ));
    checks.push(capability_preflight_check(
        "Share Capture",
        &capabilities.share_capture,
        false,
    ));
    checks.push(capability_preflight_check(
        "Terminal",
        &capabilities.terminal,
        false,
    ));

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

    // Advisory checks keep setup honest without blocking local-only runs.
    checks.push(PreflightCheck {
        name: "Network".to_string(),
        description: network_description(network_reachable, network_expectation),
        passed: true, // Network is optional — user may be offline
        blocking: false,
    });

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

    checks.push(PreflightCheck {
        name: "Fonts".to_string(),
        description: fonts_description(installed_nerd_fonts, &capabilities.font_platform),
        passed: true, // Optional — defaults available
        blocking: false,
    });

    checks.push(PreflightCheck {
        name: "Terminal Features".to_string(),
        description: format!(
            "this terminal's live-reload {} · preview {} · font {}",
            terminal_features.reload, terminal_features.live_preview, terminal_features.font_apply
        ),
        passed: true,
        blocking: false,
    });

    Ok(PreflightResult { checks })
}

/// Check if network is reachable (simple DNS check)
fn check_network_reachable() -> bool {
    // GitHub Releases and Nerd Fonts downloads are common to both supported platforms.
    match std::net::ToSocketAddrs::to_socket_addrs("github.com:443") {
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
        PreflightScenario::ConfigOnlyReconfigure => NetworkExpectation::LocalConfigOnly,
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

fn downloads_require_package_manager(expectation: NetworkExpectation) -> bool {
    matches!(expectation, NetworkExpectation::DownloadsLikely)
}

fn capability_preflight_check(
    name: &str,
    report: &CapabilityReport,
    blocking_on_failure: bool,
) -> PreflightCheck {
    let passed = capability_allows_setup(report);

    PreflightCheck {
        name: name.to_string(),
        description: format_capability_description(report),
        passed,
        blocking: blocking_on_failure,
    }
}

fn capability_allows_setup(report: &CapabilityReport) -> bool {
    !matches!(
        report.level,
        SupportLevel::Unsupported | SupportLevel::MissingDependency
    )
}

fn format_capability_description(report: &CapabilityReport) -> String {
    let mut description = format!("{} via {}", report.level.label(), report.backend);
    if let Some(reason) = report.reason.as_deref() {
        description.push_str(" — ");
        description.push_str(reason);
    }
    description
}

fn fonts_description(installed_nerd_fonts: usize, font_platform: &CapabilityReport) -> String {
    let availability = if installed_nerd_fonts > 0 {
        format!(
            "{} supported Nerd Font(s) already installed",
            installed_nerd_fonts
        )
    } else {
        format!(
            "no supported Nerd Font detected yet — setup can install one from {} choices",
            FontCatalog::all_fonts().len()
        )
    };

    format!(
        "{} — {}",
        format_capability_description(font_platform),
        availability
    )
}

fn format_blocking_section(check: &PreflightCheck) -> String {
    match check.name.as_str() {
        "OS" => [
            "OS".to_string(),
            format!("  What happened: {}", check.description),
            "  Completed: Preflight ran and no config was changed.".to_string(),
            "  Not completed: Slate only supports macOS and Linux in the current v0.1 baseline."
                .to_string(),
            "  Next: Run Slate on a supported OS target, then rerun `slate setup`.".to_string(),
        ]
        .join("\n"),
        "Arch" => [
            "Arch".to_string(),
            format!("  What happened: {}", check.description),
            "  Completed: Preflight finished safely without touching your files.".to_string(),
            "  Not completed: this release only targets x86_64 and aarch64 builds.".to_string(),
            "  Next: Use a supported Slate build for this machine, then rerun `slate setup`."
                .to_string(),
        ]
        .join("\n"),
        "Shell" => [
            "Shell".to_string(),
            format!("  What happened: {}", check.description),
            "  Completed: Slate checked the machine and did not modify shell files.".to_string(),
            "  Not completed: shared shell integration only targets zsh, bash, and fish today."
                .to_string(),
            "  Next: Switch to zsh, bash, or fish, then rerun `slate setup`.".to_string(),
        ]
        .join("\n"),
        "Package Manager" => [
            "Package Manager".to_string(),
            format!("  What happened: {}", check.description),
            "  Completed: Preflight confirmed the rest of the setup state safely.".to_string(),
            "  Not completed: this run still needs a supported package install path for missing tools or fonts."
                .to_string(),
            "  Next: Install Homebrew on macOS, or use apt on the supported Linux baseline, then rerun `slate setup`."
                .to_string(),
        ]
        .join("\n"),
        "Write Permissions" => [
            "Write Permissions".to_string(),
            "  What happened: Slate could not write under ~/.config for this user.".to_string(),
            "  Completed: Preflight finished and nothing was partially written.".to_string(),
            "  Not completed: managed config files, shell integration, and snapshots cannot be created."
                .to_string(),
            "  Next: Fix ownership or permissions for ~/.config, then rerun `slate setup`."
                .to_string(),
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
                name: "Package Manager".to_string(),
                description: "unsupported via unsupported".to_string(),
                passed: false,
                blocking: true,
            }],
        };

        let message = result.format_blocking_guidance();
        assert!(message.contains("Setup paused"));
        assert!(message.contains("Package Manager"));
        assert!(message.contains("Homebrew"));
        assert!(message.contains("apt"));
    }

    #[test]
    fn test_capability_preflight_check_formats_shared_snapshot_data() {
        let check = capability_preflight_check(
            "Package Manager",
            &CapabilityReport::best_effort("apt", "validated Linux baseline is still landing"),
            false,
        );

        assert_eq!(check.name, "Package Manager");
        assert!(check.description.contains("best effort via apt"));
        assert!(check
            .description
            .contains("validated Linux baseline is still landing"));
    }

    #[test]
    fn test_config_only_reconfigure_skips_download_expectation() {
        // Empty installed map + zero nerd fonts would normally trigger DownloadsLikely
        // in QuickSetup or GuidedSetup. ConfigOnlyReconfigure must ignore that and stay local.
        let installed = std::collections::HashMap::new();
        let expectation =
            infer_network_expectation(&installed, 0, PreflightScenario::ConfigOnlyReconfigure);
        assert_eq!(expectation, NetworkExpectation::LocalConfigOnly);

        let quick_same_inputs =
            infer_network_expectation(&installed, 0, PreflightScenario::QuickSetup);
        assert_eq!(quick_same_inputs, NetworkExpectation::DownloadsLikely);
    }

    #[test]
    fn test_config_only_reconfigure_does_not_block_package_manager() {
        // On ConfigOnlyReconfigure we expect LocalConfigOnly, which downgrades PM from blocking.
        let local_expectation = NetworkExpectation::LocalConfigOnly;
        assert!(!downloads_require_package_manager(local_expectation));

        let download_expectation = NetworkExpectation::DownloadsLikely;
        assert!(downloads_require_package_manager(download_expectation));
    }

    #[test]
    fn test_fonts_description_includes_platform_backend() {
        let description = fonts_description(
            0,
            &CapabilityReport::missing_dependency(
                "fontconfig",
                "Install fontconfig (`fc-cache`) so Slate can refresh Linux font caches automatically.",
            ),
        );

        assert!(description.contains("missing dependency via fontconfig"));
        assert!(description.contains("Install fontconfig"));
        assert!(description.contains("no supported Nerd Font detected yet"));
    }

    // ────────────────────────────────────────────────────────────
    // Phase 16 Plan 05 — LS-03 BSD-`ls` capability preflight check
    //
    // The new check lives inside `run_checks_for_setup_with_env` gated by
    // `#[cfg(target_os = "macos")]`. It must:
    //   - emit a non-blocking `PreflightCheck { name: "GNU ls", ... }` on
    //     macOS when `gls` is absent AND the acknowledgement flag is absent,
    //   - write the flag after emission so subsequent runs are silent,
    //   - stay silent when the flag is already present,
    //   - stay silent when `gls` is on PATH (nothing to nudge about),
    //   - disappear entirely on non-macOS targets (compile-time gate).
    //
    // `is_gnu_ls_present()` reads the process PATH and is NOT injected via
    // `SlateEnv`, so host state is what it is. Following the Plan 16-03
    // convention (see `is_gnu_ls_present_when_gls_on_path` in detection.rs)
    // the host-conditional tests skip gracefully when the host state doesn't
    // match their precondition — CI machines without coreutils still pass.
    // ────────────────────────────────────────────────────────────

    #[cfg(target_os = "macos")]
    #[test]
    fn preflight_emits_ls_capability_message_when_gls_absent_on_macos() {
        // Precondition: host must NOT have gls. If the dev machine has
        // coreutils installed, this positive path is untestable here — the
        // Linux no-op test plus the skip-when-present test pin the other
        // branches. The delegation test in detection.rs already proves
        // is_gnu_ls_present reflects command_path("gls").is_some().
        if crate::detection::is_gnu_ls_present() {
            return;
        }

        let temp = tempfile::TempDir::new().unwrap();
        let env = SlateEnv::with_home(temp.path().to_path_buf());

        let result =
            run_checks_for_setup_with_env(&env, PreflightScenario::GuidedSetup).unwrap();

        let ls_check = result
            .checks
            .iter()
            .find(|c| c.name == "GNU ls")
            .expect("preflight must push a 'GNU ls' check when gls is absent and flag is unset");

        assert_eq!(
            ls_check.description,
            Language::ls_capability_message(),
            "description must be the brand-voiced Language::ls_capability_message()",
        );
        assert!(ls_check.passed, "GNU ls check is advisory — must be passed=true");
        assert!(
            !ls_check.blocking,
            "GNU ls check must be non-blocking (LS-03 is a one-time nudge, not a gate)",
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn ls_capability_message_writes_acknowledgement_flag() {
        if crate::detection::is_gnu_ls_present() {
            return;
        }

        let temp = tempfile::TempDir::new().unwrap();
        let env = SlateEnv::with_home(temp.path().to_path_buf());

        // Sanity: flag does not exist yet.
        let flag_path = env.config_dir().join("ls-capability-acknowledged");
        assert!(
            !flag_path.exists(),
            "precondition: acknowledgement flag must be absent before preflight"
        );

        let _ = run_checks_for_setup_with_env(&env, PreflightScenario::GuidedSetup).unwrap();

        assert!(
            flag_path.exists(),
            "after emitting the message, the acknowledgement flag must be written \
             so subsequent preflight runs skip the check",
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn preflight_skips_ls_capability_when_acknowledged() {
        // This test works regardless of host gls state — the ack gate dominates.
        let temp = tempfile::TempDir::new().unwrap();
        let env = SlateEnv::with_home(temp.path().to_path_buf());

        // Pre-create the flag, simulating a machine that already saw the nudge.
        let config = crate::config::ConfigManager::with_env(&env).unwrap();
        config.acknowledge_ls_capability().unwrap();
        assert!(
            env.config_dir().join("ls-capability-acknowledged").exists(),
            "precondition: flag must be pre-created"
        );

        let result =
            run_checks_for_setup_with_env(&env, PreflightScenario::GuidedSetup).unwrap();

        assert!(
            result.checks.iter().all(|c| c.name != "GNU ls"),
            "preflight must NOT emit 'GNU ls' check when acknowledgement flag is present \
             (LS-03 is a one-time nudge — suppressed forever for this machine)",
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn preflight_skips_ls_capability_when_gls_present() {
        // Positive path — only runs when the host has coreutils's gls. On bare
        // CI without coreutils we skip (mirrors is_gnu_ls_present_when_gls_on_path
        // in detection.rs). The skip is acceptable because the combination of
        // (preflight_emits_ls_capability_message_when_gls_absent_on_macos) +
        // (preflight_skips_ls_capability_when_acknowledged) already pins the
        // presence-check and ack-check gates; this test just confirms the gls
        // branch closes correctly when coreutils happens to be installed.
        if !crate::detection::is_gnu_ls_present() {
            return;
        }

        let temp = tempfile::TempDir::new().unwrap();
        let env = SlateEnv::with_home(temp.path().to_path_buf());

        let result =
            run_checks_for_setup_with_env(&env, PreflightScenario::GuidedSetup).unwrap();

        assert!(
            result.checks.iter().all(|c| c.name != "GNU ls"),
            "preflight must NOT emit 'GNU ls' check when gls is already on PATH"
        );
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn preflight_skips_ls_capability_on_linux() {
        // Compile-time gate: the whole block is eliminated on non-macOS targets,
        // so no "GNU ls" check can ever appear regardless of gls presence or flag state.
        let temp = tempfile::TempDir::new().unwrap();
        let env = SlateEnv::with_home(temp.path().to_path_buf());

        let result =
            run_checks_for_setup_with_env(&env, PreflightScenario::GuidedSetup).unwrap();

        assert!(
            result.checks.iter().all(|c| c.name != "GNU ls"),
            "LS-03 is macOS-only — the check block must be compile-eliminated on Linux"
        );
    }
}
