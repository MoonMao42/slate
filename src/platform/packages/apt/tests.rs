//! All executables are explicit paths under owned temporary directories. No
//! actual apt, sudo, repository, package database or privilege change is used.
use super::*;
use std::{fs, os::unix::fs::PermissionsExt, time::Instant};

struct Fixture {
    temp: tempfile::TempDir,
    apt: PathBuf,
    sudo: PathBuf,
}

fn executable(path: &Path, body: &str) {
    fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

impl Fixture {
    fn new(body: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let apt = temp.path().join("private apt-get");
        let sudo = temp.path().join("private sudo");
        executable(&apt, &format!(
            "fixture_dir=${{0%/*}}\nprintf '%s\\n' \"$@\" > \"$fixture_dir/apt-args\"\nprintf '%s\\n' \"$DEBIAN_FRONTEND\" \"$LC_ALL\" > \"$fixture_dir/apt-env\"\nif read -r unexpected; then exit 91; fi\n{body}"
        ));
        executable(&sudo,
            "fixture_dir=${0%/*}\nprintf '%s\\n' \"$@\" > \"$fixture_dir/sudo-args\"\n[ \"$1\" = -n ] && [ \"$2\" = DEBIAN_FRONTEND=noninteractive ] && [ \"$3\" = LC_ALL=C ] && [ \"$4\" = -- ] || exit 92\nexport \"$2\" \"$3\"\nshift 4\nexec \"$@\""
        );
        Self { temp, apt, sudo }
    }
    fn root(&self, tool_id: &str) -> Result<()> {
        install_resolving(
            tool_id,
            true,
            |command| {
                assert_eq!(command, "apt-get", "root must not look up or launch sudo");
                Some(self.apt.clone())
            },
            LIMITS,
        )
    }
    fn text(&self, name: &str) -> String {
        fs::read_to_string(self.temp.path().join(name)).unwrap()
    }
}

#[test]
fn apt_install_root_uses_exact_mappings_without_sudo_or_interactive_stdin() {
    for (tool, package) in [
        ("bat", "bat"),
        ("delta", "git-delta"),
        ("eza", "eza"),
        ("lazygit", "lazygit"),
        ("fastfetch", "fastfetch"),
        ("zsh-syntax-highlighting", "zsh-syntax-highlighting"),
    ] {
        let f = Fixture::new("exit 0");
        f.root(tool).unwrap();
        assert_eq!(
            f.text("apt-args"),
            format!("install\n-y\n--no-remove\n-o\nDPkg::Use-Pty=0\n--\n{package}\n")
        );
        assert_eq!(f.text("apt-env"), "noninteractive\nC\n");
        assert!(!f.temp.path().join("sudo-args").exists());
    }
    assert_eq!(LIMITS.timeout, Duration::from_secs(1800));
    assert_eq!(LIMITS.max_output, 2 * 1024 * 1024);
}

#[test]
fn apt_install_nonroot_preserves_sudo_policy_and_explicit_environment_arguments() {
    let f = Fixture::new("exit 0");
    let mut lookups = Vec::new();
    install_resolving(
        "delta",
        false,
        |command| {
            lookups.push(command.to_owned());
            match command {
                "apt-get" => Some(f.apt.clone()),
                "sudo" => Some(f.sudo.clone()),
                _ => panic!("unexpected helper"),
            }
        },
        LIMITS,
    )
    .unwrap();
    assert_eq!(lookups, ["apt-get", "sudo"]);
    assert_eq!(f.text("sudo-args"), format!("-n\nDEBIAN_FRONTEND=noninteractive\nLC_ALL=C\n--\n{}\ninstall\n-y\n--no-remove\n-o\nDPkg::Use-Pty=0\n--\ngit-delta\n", f.apt.display()));
    assert_eq!(f.text("apt-env"), "noninteractive\nC\n");
}

#[test]
fn apt_install_preflight_never_executes_for_unknown_mapping_or_missing_helpers() {
    let f = Fixture::new("exit 0");
    for tool in ["starship", "unknown\u{1b}"] {
        let error = install_resolving(
            tool,
            false,
            |_| panic!("mapping validation comes first"),
            LIMITS,
        )
        .unwrap_err();
        assert!(!error.to_string().contains('\u{1b}'));
    }
    for missing in ["apt-get", "sudo"] {
        let error = install_resolving(
            "bat",
            false,
            |command| {
                if command == missing {
                    None
                } else {
                    Some(f.apt.clone())
                }
            },
            LIMITS,
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains(&format!("{missing} was not found")));
    }
    assert!(!f.temp.path().join("apt-args").exists());
    assert!(!f.temp.path().join("sudo-args").exists());
}

#[test]
fn apt_install_known_failure_reasons_omit_native_output_and_never_report_success() {
    for (stderr, reason) in [
        ("permission denied", "permission or sudo policy denied"),
        ("could not get lock", "package database is busy"),
        ("unable to locate package", "package is unavailable"),
        ("has no installation candidate", "package is unavailable"),
        (
            "temporary failure resolving host",
            "package download failed",
        ),
        ("dpkg error", "inspect apt/dpkg"),
    ] {
        let f = Fixture::new(&format!(
            "printf 'PRIVATE_NATIVE_SECRET {stderr}\\033[2J' >&2; exit 100"
        ));
        let error = f.root("bat").unwrap_err();
        let message = error.to_string();
        assert!(
            message.contains("exit code 100") && message.contains(reason),
            "{message}"
        );
        assert!(message.contains("Partial package changes may remain"));
        assert!(!message.contains("PRIVATE_NATIVE_SECRET") && !message.contains('\u{1b}'));
        assert!(!matches!(error, SlateError::AptInstallUncertain(_)));
    }
}

#[test]
fn apt_install_sudo_denial_does_not_launch_apt_or_retry_privileges() {
    for (stderr, reason) in [
        ("a password is required", "sudo needs authentication"),
        ("not allowed to set environment", "sudo policy denied"),
    ] {
        let f = Fixture::new("exit 0");
        executable(
            &f.sudo,
            &format!("printf 'PRIVATE_NATIVE_SECRET {stderr}' >&2; exit 1"),
        );
        let error = run(&f.apt, Some(&f.sudo), "bat", LIMITS).unwrap_err();
        assert!(error.to_string().contains(reason));
        assert!(!error.to_string().contains("PRIVATE_NATIVE_SECRET"));
        assert!(!f.temp.path().join("apt-args").exists());
    }
}

#[test]
fn apt_install_interruption_is_typed_uncertain_and_keeps_partial_private_state() {
    for (body, reason) in [
        ("exec /bin/sleep 8", "wait limit"),
        ("/bin/sleep 8 &\nexit 0", "wait limit"),
        ("exec /usr/bin/yes PRIVATE_NATIVE_SECRET", "output limit"),
        ("kill -TERM \"$$\"", "signal"),
    ] {
        let f = Fixture::new(&format!(
            "printf partial > \"$fixture_dir/partial-package\"\n{body}"
        ));
        let started = Instant::now();
        let error = run(
            &f.apt,
            Some(&f.sudo),
            "bat",
            Limits {
                timeout: Duration::from_secs(1),
                max_output: 1024,
            },
        )
        .unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(matches!(error, SlateError::AptInstallUncertain(_)));
        assert!(!crate::cli::setup_executor::installation_fallback_allowed(
            &error
        ));
        assert!(error.to_string().contains(reason));
        assert!(error.to_string().contains("may still be running"));
        assert!(error.to_string().contains("do not delete lock files"));
        assert!(!error.to_string().contains("PRIVATE_NATIVE_SECRET"));
        assert_eq!(f.text("partial-package"), "partial");
    }
}

#[test]
fn apt_install_unstartable_executable_is_not_a_confirmed_failed_exit() {
    let f = Fixture::new("exit 0");
    for apt in [f.temp.path().join("missing apt"), f.apt.clone()] {
        fs::set_permissions(&f.apt, fs::Permissions::from_mode(0o644)).unwrap();
        let error = run(&apt, None, "bat", LIMITS).unwrap_err();
        assert!(matches!(error, SlateError::AptInstallUncertain(_)));
        assert!(!f.temp.path().join("apt-args").exists());
    }
}
