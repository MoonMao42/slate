use super::wizard_support::wording;
use crate::adapter::font::FontAdapter;
use crate::brand::language::Language;
use crate::cli::font_selection::FontCatalog;
use crate::cli::tool_selection::{detect_installed_tools_with_env, ToolCatalog};
use crate::env::SlateEnv;
use crate::error::Result;
use crate::platform::capabilities::{detect_capabilities, CapabilityReport, SupportLevel};
use crate::platform::shell::ShellBackend;

mod install;
mod network;

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
    FontDiscoveryIncomplete,
}

impl PreflightResult {
    /// Only successful, confirmed setup may remember that its advisory was seen.
    /// This best-effort cosmetic marker must not turn successful setup into failure.
    pub(super) fn acknowledge_after_setup(&self, env: &SlateEnv) {
        if self.checks.iter().any(|check| check.name == "GNU ls") {
            let config = crate::config::ConfigManager::from_env_paths(env);
            let _ = config.acknowledge_ls_capability();
        }
    }

    /// Check if all required checks passed
    pub fn is_ready(&self) -> bool {
        self.checks.iter().filter(|c| c.blocking).all(|c| c.passed)
    }

    /// Format results for display
    pub fn format_for_display(&self) -> String {
        let language = super::ui_language::output_language();
        self.format_in(language)
    }

    fn format_in(&self, language: crate::config::ui_language::UiLanguage) -> String {
        let chinese = language == crate::config::ui_language::UiLanguage::Chinese;
        let mut output = String::from(if chinese {
            "环境检查"
        } else {
            Language::PREFLIGHT_HEADER
        });
        output.push('\n');
        for check in &self.checks {
            let icon = if !check.blocking {
                "•"
            } else if check.passed {
                "✓"
            } else {
                "✗"
            };
            let label = if chinese {
                match check.name.as_str() {
                    "OS" => "操作系统",
                    "Arch" => "处理器架构",
                    "Shell" => "Shell",
                    "Package Manager" => "包管理器",
                    "Desktop Appearance" => "系统外观",
                    "Write Permissions" => "写入权限",
                    "Network" => "网络",
                    "Tools" => "工具",
                    "Fonts" => "字体",
                    "Terminal Features" => "终端能力",
                    "Terminal" => "终端",
                    _ => &check.name,
                }
            } else {
                &check.name
            };
            output.push_str(&format!(
                "{} {}: {}\n",
                icon,
                super::file_output::terminal_text(label),
                super::file_output::terminal_text(&check.description)
            ));
        }
        output
    }

    pub fn format_blocking_guidance(&self) -> String {
        let language = super::ui_language::output_language();
        self.blocking_guidance_in(language)
    }

    fn blocking_guidance_in(&self, language: crate::config::ui_language::UiLanguage) -> String {
        let chinese = language == crate::config::ui_language::UiLanguage::Chinese;
        let sections = self
            .checks
            .iter()
            .filter(|check| check.blocking && !check.passed)
            .map(|check| {
                if chinese {
                    format_blocking_section_zh(check)
                } else {
                    let safe = PreflightCheck {
                        name: super::file_output::terminal_text(&check.name),
                        description: super::file_output::terminal_text(&check.description),
                        ..check.clone()
                    };
                    format_blocking_section(&safe)
                }
            })
            .collect::<Vec<_>>();

        if sections.is_empty() {
            return String::new();
        }

        if chinese {
            return format!(
                "设置已暂停，请先处理以下问题：\n\n{}\n\n处理后重新运行 `slate setup`。",
                sections.join("\n\n")
            );
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

/// Exact-tool retry does not configure shells/themes or select fonts. Do not
/// run unrelated font scans, DNS queries or configuration write probes here.
pub(crate) fn run_checks_for_retry(tool_id: &str) -> PreflightResult {
    use crate::platform::{capabilities, packages::InstallContext};
    PreflightResult {
        checks: vec![
            capability_preflight_check("OS", &capabilities::os_capability_report(), true),
            capability_preflight_check("Arch", &capabilities::arch_capability_report(), true),
            install::retry_check(tool_id, InstallContext::detect()),
        ],
    }
}

pub fn run_checks_for_setup_with_env(
    env: &SlateEnv,
    scenario: PreflightScenario,
) -> Result<PreflightResult> {
    let mut checks = Vec::new();
    let write_permissions = check_write_permissions_with_env(env);
    let installed = detect_installed_tools_with_env(env);
    let tool_count = installed.values().filter(|p| p.installed).count();
    let font_scan = FontAdapter::scan_fonts_with_env(env);
    let installed_nerd_fonts = font_scan.fonts.nerd_fonts.len();
    let font_count =
        (font_scan.is_complete() || installed_nerd_fonts > 0).then_some(installed_nerd_fonts);
    let shell = crate::platform::shell::detect_backend();
    let network_expectation = infer_network_expectation(&installed, font_count, scenario, shell);
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
        install::requires_package_manager(
            &installed,
            scenario,
            crate::platform::packages::InstallContext::detect(),
            shell,
        ),
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
        description: format!(
            "{} — {}{}{}",
            if write_permissions {
                wording("可写", "ready")
            } else {
                wording("受阻", "blocked")
            },
            wording("已尝试在 ", "temporary-file create/write/remove probe in "),
            env.xdg_config_home().display().to_string().escape_debug(),
            wording(
                " 创建、写入并删除临时文件；具体目标文件稍后检查",
                "; individual targets are checked later"
            ),
        ),
        passed: write_permissions,
        blocking: true,
    });

    // Advisory checks keep setup honest without blocking local-only runs.
    checks.push(PreflightCheck {
        name: "Network".to_string(),
        description: network_preflight_description(network_expectation, check_network_reachable),
        passed: true, // Network is optional — user may be offline
        blocking: false,
    });

    checks.push(PreflightCheck {
        name: "Tools".to_string(),
        description: format!(
            "{}{}{}{}{}",
            wording("已检测到 ", ""),
            tool_count,
            wording(" / ", " of "),
            ToolCatalog::all_tools().len(),
            wording(" 个受管工具", " managed tools already present")
        ),
        passed: true, // Optional — user can install from scratch
        blocking: false,
    });

    checks.push(PreflightCheck {
        name: "Fonts".to_string(),
        description: if font_scan.is_complete() {
            fonts_description(installed_nerd_fonts, &capabilities.font_platform)
        } else {
            font_scan.warning_in(super::ui_language::output_language())
        },
        passed: font_scan.is_complete(), // Advisory; executor won't infer absence from failures.
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

    // (LS-03 / D-B1): On macOS, if `gls` (GNU ls from coreutils) is
    // absent AND the user hasn't been nudged before, push a non-blocking
    // advisory explaining why the slate-managed LS_COLORS needs GNU `ls`.
    // Preflight is read-only. Successful setup remembers the advisory later;
    // cancellation must not create profile directories or acknowledgement flags.
    // Scenario gate (RESEARCH §Pattern 3 "scenario gate"): only emit during
    // first-touch flows (GuidedSetup / QuickSetup). RetryInstall and
    // ConfigOnlyReconfigure are re-runs against an existing slate install
    // where the user already saw the message on their first setup; the ack
    // flag usually already suppresses it, but the scenario gate is defensive
    // insurance against an edge case where someone's very first run is a
    // retry/reconfigure (e.g. scripted installs).
    #[cfg(target_os = "macos")]
    {
        let emits_for_scenario = matches!(
            scenario,
            PreflightScenario::GuidedSetup | PreflightScenario::QuickSetup
        );
        if emits_for_scenario {
            let config = crate::config::ConfigManager::from_env_paths(env);
            let acknowledged = config.is_ls_capability_acknowledged().unwrap_or(false);
            let gnu_ls_present = crate::detection::is_gnu_ls_present();

            if !gnu_ls_present && !acknowledged {
                checks.push(PreflightCheck {
                    name: "GNU ls".to_string(),
                    description: Language::ls_capability_message().to_string(),
                    passed: true, // advisory — never blocks setup
                    blocking: false,
                });
            }
        }
    }

    Ok(PreflightResult { checks })
}

/// Check if network is reachable (simple DNS check)
fn check_network_reachable() -> Option<bool> {
    network::check()
}

/// Check if we can write to config directory
fn check_write_permissions_with_env(env: &SlateEnv) -> bool {
    let config_dir = env.xdg_config_home();
    if std::fs::create_dir_all(config_dir).is_err() {
        return false;
    }
    use std::io::Write;
    // Exclusive random creation must never truncate an existing file or follow
    // a pre-planted link at the old fixed probe name. Drop cleans up write errors.
    let probe = || -> std::io::Result<()> {
        let mut file = tempfile::Builder::new()
            .prefix(".slate-preflight-")
            .tempfile_in(config_dir)?;
        file.write_all(b"test")?;
        file.flush()?;
        file.close()
    };
    probe().is_ok()
}

fn infer_network_expectation(
    installed: &std::collections::HashMap<String, crate::detection::ToolPresence>,
    installed_nerd_fonts: Option<usize>,
    scenario: PreflightScenario,
    shell: ShellBackend,
) -> NetworkExpectation {
    let missing_installable_tool = ToolCatalog::installable_tools().into_iter().any(|tool| {
        !installed
            .get(tool.id)
            .map(|presence| presence.installed)
            .unwrap_or(false)
    });

    let quick_mode_missing_core = crate::cli::tool_selection::quick_core_tools(shell)
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
            if quick_mode_missing_core || installed_nerd_fonts == Some(0) {
                NetworkExpectation::DownloadsLikely
            } else if installed_nerd_fonts.is_none() {
                NetworkExpectation::FontDiscoveryIncomplete
            } else {
                NetworkExpectation::LocalConfigOnly
            }
        }
        PreflightScenario::GuidedSetup => {
            if missing_installable_tool || installed_nerd_fonts == Some(0) {
                NetworkExpectation::DownloadsLikely
            } else if installed_nerd_fonts.is_none() {
                NetworkExpectation::FontDiscoveryIncomplete
            } else {
                NetworkExpectation::LocalConfigOnly
            }
        }
    }
}

fn network_preflight_description(
    expectation: NetworkExpectation,
    probe: impl FnOnce() -> Option<bool>,
) -> String {
    match expectation {
        NetworkExpectation::LocalConfigOnly => super::wizard_support::wording(
            "本次仅配置本地工具，未检查网络；下载访问未验证。",
            "No download looks necessary for this run; network check skipped. Download access is not verified.",
        ).into(),
        NetworkExpectation::FontDiscoveryIncomplete => network_description(false, expectation),
        NetworkExpectation::DownloadsLikely => match probe() {
            Some(reachable) => network_description(reachable, expectation),
            None => super::wizard_support::wording(
                "DNS 检查未完成，网络状态未知；安装程序仍需验证下载访问。",
                "DNS check did not finish; network status is unknown. Installers still need to verify download access.",
            ).into(),
        },
    }
}

fn network_description(reachable: bool, expectation: NetworkExpectation) -> String {
    match (reachable, expectation) {
        (_, NetworkExpectation::FontDiscoveryIncomplete) => {
            wording("字体扫描未完成，是否需要下载尚不确定；请先检查扫描结果，避免重复安装", "font discovery is incomplete — font download needs are unknown; check the font scan before installing another copy").to_string()
        }
        (true, NetworkExpectation::DownloadsLikely) => {
            wording("github.com 可解析；安装程序仍需验证能否下载", "github.com resolves — installers still need to verify download access").to_string()
        }
        (true, NetworkExpectation::LocalConfigOnly) => {
            wording("github.com 可解析；DNS 检查不能证明下载可用", "github.com resolves — download access is not verified by this DNS check").to_string()
        }
        (false, NetworkExpectation::DownloadsLikely) => {
            wording("github.com 未能解析；安装或字体下载可能失败，仍可配置已安装的工具", "github.com did not resolve — installs or font downloads may fail, but already-installed tools can still be configured").to_string()
        }
        (false, NetworkExpectation::LocalConfigOnly) => {
            wording("github.com 未能解析；本次预计无需下载，可继续配置本地工具", "github.com did not resolve — no download looks necessary for this run, so local config can still continue").to_string()
        }
    }
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
    if super::ui_language::output_language() == crate::config::ui_language::UiLanguage::Chinese {
        let availability = if installed_nerd_fonts > 0 {
            format!("找到 {installed_nerd_fonts} 个 Nerd Font 文件名候选；尚未验证实际渲染")
        } else {
            format!(
                "尚未检测到受支持的 Nerd Font；可从 {} 款字体中选择安装，也可保留现有设置",
                FontCatalog::all_fonts().len()
            )
        };
        return format!(
            "{} — {}",
            format_capability_description(font_platform),
            availability
        );
    }
    let availability = if installed_nerd_fonts > 0 {
        format!(
            "{} Nerd Font filename candidate(s) found; native rendering is not verified",
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

fn format_blocking_section_zh(check: &PreflightCheck) -> String {
    let (label, next) = match check.name.as_str() {
        "OS" => ("操作系统", "请在受支持的 macOS 或 Linux 系统上运行。"),
        "Arch" => ("处理器架构", "请使用适合本机的 x86_64 或 aarch64 构建。"),
        "Shell" => ("Shell", "请切换到 zsh、bash 或 fish 后重试。"),
        "Package Manager" => (
            "包管理器",
            "缺少工具的安装方式不可用。请检查 macOS 上的 Homebrew，或受支持 Linux 上的 apt。",
        ),
        "Write Permissions" => ("写入权限", "请检查下列配置目录的归属和写入权限。"),
        _ => (check.name.as_str(), "请先解决下列检查问题。"),
    };
    format!(
        "{}\n  {}\n  检测原因：{}",
        super::file_output::terminal_text(label),
        next,
        super::file_output::terminal_text(&check.description)
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
            "  Not completed: this run still needs a supported package install path for missing tools."
                .to_string(),
            "  Next: Install Homebrew on macOS, or use apt on the supported Linux baseline, then rerun `slate setup`."
                .to_string(),
        ]
        .join("\n"),
        "Write Permissions" => [
            "Write Permissions".to_string(),
            format!("  What happened: {}", super::file_output::terminal_text(&check.description)),
            "  Completed: Preflight finished and nothing was partially written.".to_string(),
            "  Not completed: managed config files, shell integration, and snapshots cannot be created."
                .to_string(),
            "  Next: Fix ownership or permissions for the configuration directory reported above, then rerun `slate setup`."
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
    fn local_setup_and_incomplete_font_scan_skip_dns_probes() {
        for expectation in [
            NetworkExpectation::LocalConfigOnly,
            NetworkExpectation::FontDiscoveryIncomplete,
        ] {
            let description =
                network_preflight_description(expectation, || panic!("unexpected DNS lookup"));
            assert!(
                !description.contains("did not resolve")
                    && !description.contains("github.com resolves")
            );
        }
        let calls = std::cell::Cell::new(0);
        let description =
            network_preflight_description(NetworkExpectation::DownloadsLikely, || {
                calls.set(calls.get() + 1);
                Some(false)
            });
        assert_eq!(calls.get(), 1);
        assert!(description.contains("downloads may fail"));
    }

    #[test]
    fn preflight_display_languages_keep_failures_and_escape_control_text() {
        use crate::config::ui_language::UiLanguage::{Chinese, English};
        let result = PreflightResult {
            checks: vec![PreflightCheck {
                name: "Write Permissions".into(),
                description: "blocked in /fixture/config\n\x1b[2J".into(),
                passed: false,
                blocking: true,
            }],
        };
        for language in [Chinese, English] {
            let text = result.format_in(language);
            assert!(text.contains("✗") && text.contains("blocked in /fixture/config"));
            assert!(!text.contains('\x1b') && !text.contains("config\n"));
        }
        assert!(result.format_in(Chinese).contains("写入权限"));
        assert!(result.format_in(English).contains("Write Permissions"));
        assert!(!result.is_ready());
        let guidance = result.format_blocking_guidance();
        assert!(guidance.contains("/fixture/config"));
        assert!(!guidance.contains("~/.config") && !guidance.contains('\x1b'));
        let zh = result.blocking_guidance_in(Chinese);
        assert!(zh.contains("设置已暂停") && zh.contains("/fixture/config"));
        assert!(zh.contains("归属和写入权限") && zh.contains("slate setup"));
        assert!(!zh.contains('\x1b') && !zh.contains("~/.config"));
        let advisory = PreflightResult {
            checks: vec![PreflightCheck {
                name: "Network".into(),
                description: "offline".into(),
                passed: false,
                blocking: false,
            }],
        };
        assert!(advisory.blocking_guidance_in(Chinese).is_empty());
        assert!(advisory.is_ready());
    }

    #[test]
    fn quick_shell_network_intent_ignores_unneeded_zsh_plugin_but_keeps_font_evidence() {
        let installed = std::collections::HashMap::from([(
            "starship".into(),
            crate::detection::ToolPresence {
                installed: true,
                in_path: false,
                evidence: None,
            },
        )]);
        for shell in [ShellBackend::Bash, ShellBackend::Fish, ShellBackend::Zsh] {
            for fonts in [Some(0), Some(1), None] {
                let expected = if shell == ShellBackend::Zsh || fonts == Some(0) {
                    NetworkExpectation::DownloadsLikely
                } else if fonts.is_none() {
                    NetworkExpectation::FontDiscoveryIncomplete
                } else {
                    NetworkExpectation::LocalConfigOnly
                };
                assert_eq!(
                    infer_network_expectation(
                        &installed,
                        fonts,
                        PreflightScenario::QuickSetup,
                        shell
                    ),
                    expected
                );
            }
        }
    }

    #[test]
    fn font_discovery_incomplete_preflight_does_not_invent_a_download_requirement() {
        let installed = ToolCatalog::installable_tools()
            .into_iter()
            .map(|tool| {
                (
                    tool.id.to_string(),
                    crate::detection::ToolPresence {
                        installed: true,
                        in_path: true,
                        evidence: None,
                    },
                )
            })
            .collect();
        for scenario in [
            PreflightScenario::QuickSetup,
            PreflightScenario::GuidedSetup,
        ] {
            assert_eq!(
                infer_network_expectation(&installed, None, scenario, ShellBackend::Zsh),
                NetworkExpectation::FontDiscoveryIncomplete
            );
            assert_eq!(
                infer_network_expectation(&installed, Some(0), scenario, ShellBackend::Zsh),
                NetworkExpectation::DownloadsLikely
            );
            assert_eq!(
                infer_network_expectation(&installed, Some(1), scenario, ShellBackend::Zsh),
                NetworkExpectation::LocalConfigOnly
            );
        }
        assert!(!install::requires_package_manager(
            &installed,
            PreflightScenario::QuickSetup,
            crate::platform::packages::InstallContext {
                package_manager: crate::platform::packages::PackageManagerBackend::Unsupported,
                supported_os: true,
            },
            ShellBackend::Zsh,
        ));
        assert!(
            network_description(false, NetworkExpectation::FontDiscoveryIncomplete)
                .contains("unknown")
        );
    }

    #[test]
    fn preflight_write_probe_preserves_existing_files_and_links() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        for linked in [false, true] {
            let td = tempfile::tempdir().unwrap();
            let custom = td.path().join("custom-xdg");
            std::fs::create_dir(&custom).unwrap();
            let env = SlateEnv::from_vars(|key| match key {
                "HOME" => Some(td.path().as_os_str().to_owned()),
                "XDG_CONFIG_HOME" => Some(custom.as_os_str().to_owned()),
                _ => None,
            })
            .unwrap();
            let old_probe = custom.join(".slate_preflight_test");
            let target = if linked {
                td.path().join("private-target")
            } else {
                old_probe.clone()
            };
            std::fs::write(&target, b"private bytes\xff\n").unwrap();
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o640)).unwrap();
            if linked {
                symlink(&target, &old_probe).unwrap();
            }
            for _ in 0..2 {
                assert!(check_write_permissions_with_env(&env));
                assert_eq!(std::fs::read(&target).unwrap(), b"private bytes\xff\n");
                assert_eq!(
                    std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
                    0o640
                );
                assert_eq!(std::fs::read_dir(&custom).unwrap().count(), 1);
                if linked {
                    assert_eq!(std::fs::read_link(&old_probe).unwrap(), target);
                }
            }
        }
    }

    #[test]
    fn preflight_write_probe_reports_obstructed_config_directory() {
        let td = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        std::fs::write(env.xdg_config_home(), b"user file").unwrap();
        assert!(!check_write_permissions_with_env(&env));
        assert_eq!(std::fs::read(env.xdg_config_home()).unwrap(), b"user file");
    }

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
        let resolved = network_description(true, NetworkExpectation::DownloadsLikely);
        assert!(resolved.contains("resolves") && resolved.contains("verify download access"));
        assert!(!resolved.contains("are available"));
        assert!(
            network_description(true, NetworkExpectation::LocalConfigOnly).contains("not verified")
        );
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
        let expectation = infer_network_expectation(
            &installed,
            Some(0),
            PreflightScenario::ConfigOnlyReconfigure,
            ShellBackend::Zsh,
        );
        assert_eq!(expectation, NetworkExpectation::LocalConfigOnly);

        let quick_same_inputs = infer_network_expectation(
            &installed,
            Some(0),
            PreflightScenario::QuickSetup,
            ShellBackend::Zsh,
        );
        assert_eq!(quick_same_inputs, NetworkExpectation::DownloadsLikely);
    }

    #[test]
    fn test_config_only_reconfigure_does_not_block_package_manager() {
        let context = crate::platform::packages::InstallContext {
            package_manager: crate::platform::packages::PackageManagerBackend::Unsupported,
            supported_os: true,
        };
        assert!(!install::requires_package_manager(
            &Default::default(),
            PreflightScenario::ConfigOnlyReconfigure,
            context,
            ShellBackend::Zsh,
        ));
        assert!(install::requires_package_manager(
            &Default::default(),
            PreflightScenario::QuickSetup,
            context,
            ShellBackend::Zsh,
        ));
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

    // LS-03 BSD-`ls` capability preflight check
    // The new check lives inside `run_checks_for_setup_with_env` gated by
    // `#[cfg(target_os = "macos")]`. It must:
    // - emit a non-blocking `PreflightCheck { name: "GNU ls", ... }` on
    // macOS when `gls` is absent AND the acknowledgement flag is absent,
    // - keep inspection read-only, acknowledging only after successful setup,
    // - stay silent when the flag is already present,
    // - stay silent when `gls` is on PATH (nothing to nudge about),
    // - disappear entirely on non-macOS targets (compile-time gate).
    // `is_gnu_ls_present()` reads the process PATH and is NOT injected via
    // `SlateEnv`, so host state is what it is. Following the
    // convention (see `is_gnu_ls_present_when_gls_on_path` in detection.rs)
    // the host-conditional tests skip gracefully when the host state doesn't
    // match their precondition — CI machines without coreutils still pass.

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

        let result = run_checks_for_setup_with_env(&env, PreflightScenario::GuidedSetup).unwrap();

        let ls_check =
            result.checks.iter().find(|c| c.name == "GNU ls").expect(
                "preflight must push a 'GNU ls' check when gls is absent and flag is unset",
            );

        assert_eq!(
            ls_check.description,
            Language::ls_capability_message(),
            "description must be the brand-voiced Language::ls_capability_message()",
        );
        assert!(
            ls_check.passed,
            "GNU ls check is advisory — must be passed=true"
        );
        assert!(
            !ls_check.blocking,
            "GNU ls check must be non-blocking (LS-03 is a one-time nudge, not a gate)",
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn ls_capability_message_defers_acknowledgement_until_successful_setup() {
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

        let report = run_checks_for_setup_with_env(&env, PreflightScenario::GuidedSetup).unwrap();
        assert!(!flag_path.exists());
        assert!(
            !env.config_dir().exists(),
            "preflight must not initialize the profile"
        );
        // Simulate the configuration directory created by a confirmed setup.
        std::fs::create_dir_all(env.config_dir()).unwrap();
        report.acknowledge_after_setup(&env);

        assert!(
            flag_path.exists(),
            "after successful setup, the acknowledgement flag must be written \
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

        let result = run_checks_for_setup_with_env(&env, PreflightScenario::GuidedSetup).unwrap();

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

        let result = run_checks_for_setup_with_env(&env, PreflightScenario::GuidedSetup).unwrap();

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

        let result = run_checks_for_setup_with_env(&env, PreflightScenario::GuidedSetup).unwrap();

        assert!(
            result.checks.iter().all(|c| c.name != "GNU ls"),
            "LS-03 is macOS-only — the check block must be compile-eliminated on Linux"
        );
    }
}
