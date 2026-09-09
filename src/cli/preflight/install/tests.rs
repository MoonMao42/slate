use super::*;
use crate::{cli::preflight::PreflightResult, platform::packages::PackageManagerBackend};

fn no_manager() -> InstallContext {
    InstallContext {
        package_manager: PackageManagerBackend::Unsupported,
        supported_os: true,
    }
}

#[test]
fn quick_shell_preflight_requires_only_shell_relevant_package_routes() {
    let empty = HashMap::new();
    let mut installed = HashMap::new();
    installed.insert(
        "starship".into(),
        ToolPresence {
            installed: true,
            in_path: false,
            evidence: None,
        },
    );
    for shell in [ShellBackend::Bash, ShellBackend::Fish, ShellBackend::Zsh] {
        assert_eq!(
            requires_package_manager(&empty, PreflightScenario::QuickSetup, no_manager(), shell),
            shell == ShellBackend::Zsh
        );
        assert_eq!(
            requires_package_manager(
                &installed,
                PreflightScenario::QuickSetup,
                no_manager(),
                shell
            ),
            shell == ShellBackend::Zsh
        );
        let brew = InstallContext {
            package_manager: PackageManagerBackend::Homebrew,
            ..no_manager()
        };
        assert!(requires_package_manager(
            &empty,
            PreflightScenario::QuickSetup,
            brew,
            shell
        ));
        assert!(!requires_package_manager(
            &empty,
            PreflightScenario::GuidedSetup,
            no_manager(),
            shell
        ));
    }
    // Explicit --only still means the requested tool, independent of shell defaults.
    assert!(!retry_check("zsh-syntax-highlighting", no_manager()).passed);
}

#[test]
fn bootstrap_preflight_exact_retry_allows_local_starship_without_package_manager() {
    for backend in [
        PackageManagerBackend::Unsupported,
        PackageManagerBackend::Apt,
        PackageManagerBackend::Homebrew,
    ] {
        let context = InstallContext {
            package_manager: backend,
            supported_os: true,
        };
        let check = retry_check("starship", context);
        assert!(check.passed && check.blocking);
        assert!(PreflightResult {
            checks: vec![check]
        }
        .is_ready());
    }
    let check = retry_check("bat", no_manager());
    assert!(!check.passed && check.blocking);
    assert!(!PreflightResult {
        checks: vec![check]
    }
    .is_ready());
    let check = retry_check(
        "starship",
        InstallContext {
            supported_os: false,
            ..no_manager()
        },
    );
    assert!(!check.passed);
    assert!(!retry_check("unknown\u{1b}", no_manager())
        .description
        .contains('\u{1b}'));
}

#[test]
fn bootstrap_preflight_does_not_confuse_font_downloads_with_package_installs() {
    let mut installed = HashMap::new();
    assert!(!requires_package_manager(
        &installed,
        PreflightScenario::GuidedSetup,
        no_manager(),
        ShellBackend::Zsh,
    ));
    assert!(!requires_package_manager(
        &installed,
        PreflightScenario::ConfigOnlyReconfigure,
        no_manager(),
        ShellBackend::Zsh,
    ));
    assert!(requires_package_manager(
        &installed,
        PreflightScenario::QuickSetup,
        no_manager(),
        ShellBackend::Zsh,
    ));
    installed.insert(
        "zsh-syntax-highlighting".into(),
        ToolPresence {
            installed: true,
            in_path: false,
            evidence: None,
        },
    );
    // Only missing core tool is Starship: no package manager is required, even
    // when a font download is needed or font inventory is unknown.
    assert!(!requires_package_manager(
        &installed,
        PreflightScenario::QuickSetup,
        no_manager(),
        ShellBackend::Zsh,
    ));
    installed.insert(
        "starship".into(),
        ToolPresence {
            installed: true,
            in_path: true,
            evidence: None,
        },
    );
    assert!(!requires_package_manager(
        &installed,
        PreflightScenario::QuickSetup,
        no_manager(),
        ShellBackend::Zsh,
    ));
    // Do not silently relax the old generic RetryInstall API without an ID.
    assert!(requires_package_manager(
        &installed,
        PreflightScenario::RetryInstall,
        no_manager(),
        ShellBackend::Zsh,
    ));
}

#[test]
fn bootstrap_preflight_retry_does_not_run_unrelated_profile_probes() {
    // The actual production retry collector performs only platform/path lookup;
    // it has no profile argument and creates no config/font/network checks.
    let result = crate::cli::preflight::run_checks_for_retry("starship");
    assert_eq!(
        result
            .checks
            .iter()
            .map(|check| check.name.as_str())
            .collect::<Vec<_>>(),
        ["OS", "Arch", "Tool Installation"]
    );
    let description = &result.checks[2].description;
    assert!(
        description.contains("checked during installation")
            || description.contains("checks run during installation")
    );
}
