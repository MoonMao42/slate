//! Private executable fixtures only. No real Homebrew, font installer, network
//! service, font cache or terminal process is invoked.
use super::{installation_fallback_allowed, tool_install::should_try_local_starship_fallback};
use crate::error::{Result, SlateError};
use crate::platform::packages::{homebrew::*, BrewKind};
use crate::platform::process_output::Limits;
use std::time::Duration;
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf, time::Instant};

struct Fixture {
    _temp: tempfile::TempDir,
    brew: PathBuf,
}
impl Fixture {
    fn new(body: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let brew = temp.path().join("private brew");
        fs::write(&brew, format!("#!/bin/sh\nfixture_dir=${{0%/*}}\nprintf '%s\\n' \"$@\" > \"$fixture_dir/args\"\n{body}\n")).unwrap();
        fs::set_permissions(&brew, fs::Permissions::from_mode(0o755)).unwrap();
        Self { _temp: temp, brew }
    }
    fn run(&self, limits: Limits) -> Result<()> {
        install(&self.brew, "font-hack-nerd-font", BrewKind::Cask, limits)
    }
}

#[test]
fn homebrew_tool_formula_and_cask_use_exact_arguments_and_separate_limits() {
    let f = Fixture::new("exit 0");
    for (kind, expected) in [
        (BrewKind::Formula, "install\nstarship\n"),
        (BrewKind::Cask, "install\n--cask\nstarship\n"),
    ] {
        install(&f.brew, "starship", kind, TOOL_LIMITS).unwrap();
        assert_eq!(
            fs::read_to_string(f.brew.with_file_name("args")).unwrap(),
            expected
        );
    }
    assert_eq!(TOOL_LIMITS.timeout, Duration::from_secs(1800));
    assert_eq!(TOOL_LIMITS.max_output, 2 * 1024 * 1024);
    assert_eq!(FONT_LIMITS.timeout, Duration::from_secs(600));
    assert_eq!(FONT_LIMITS.max_output, 512 * 1024);
}

#[test]
fn homebrew_tool_uncertainty_never_activates_permission_based_starship_fallback() {
    let f = Fixture::new("printf 'permission denied PRIVATE_NATIVE_SECRET' >&2; exit 7");
    let known_failure = install(&f.brew, "starship", BrewKind::Formula, TOOL_LIMITS).unwrap_err();
    assert!(should_try_local_starship_fallback(&known_failure));
    assert!(!known_failure.to_string().contains("PRIVATE_NATIVE_SECRET"));
    let uncertain =
        SlateError::HomebrewInstallUncertain("permission denied; Homebrew was not found".into());
    assert!(!installation_fallback_allowed(&uncertain));
    assert!(!should_try_local_starship_fallback(&uncertain));
}

#[test]
fn font_brew_success_preserves_exact_arguments_and_uses_closed_stdin() {
    let f = Fixture::new(
        "if read -r unexpected; then exit 91; fi\nprintf PRIVATE_NATIVE_SECRET\nexit 0",
    );
    f.run(FONT_LIMITS).unwrap();
    assert_eq!(
        fs::read(f.brew.with_file_name("args")).unwrap(),
        b"install\n--cask\nfont-hack-nerd-font\n"
    );
}

#[test]
fn font_brew_known_failures_remain_retryable_without_echoing_native_output() {
    for (stderr, expected) in [
        (
            "PRIVATE_NATIVE_SECRET permission denied",
            "permission denied",
        ),
        (
            "PRIVATE_NATIVE_SECRET could not resolve host",
            "network unreachable",
        ),
        ("PRIVATE_NATIVE_SECRET\\033[2J", "inspect Homebrew"),
    ] {
        let f = Fixture::new(&format!("printf '{stderr}' >&2; exit 7"));
        let error = f.run(FONT_LIMITS).unwrap_err();
        assert!(installation_fallback_allowed(&error));
        let message = error.to_string();
        assert!(
            message.contains("exit code 7") && message.contains(expected),
            "{message}"
        );
        assert!(message.contains("Command output omitted"));
        assert!(!message.contains("PRIVATE_NATIVE_SECRET") && !message.contains('\x1b'));
    }
}

#[test]
fn font_brew_interruption_stops_fallback_and_keeps_partial_private_state() {
    for (body, expected) in [
        ("exec /bin/sleep 8", "wait limit"),
        // A completed leader is not sufficient when a child still owns pipes.
        ("/bin/sleep 8 &\nexit 0", "wait limit"),
        ("exec /usr/bin/yes PRIVATE_NATIVE_SECRET", "output limit"),
        ("kill -TERM \"$$\"", "signal"),
    ] {
        let f = Fixture::new(&format!(
            "printf partial > \"$fixture_dir/partial-state\"\n{body}"
        ));
        let started = Instant::now();
        let error = f
            .run(Limits {
                timeout: Duration::from_secs(1),
                max_output: 1024,
            })
            .unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(!installation_fallback_allowed(&error));
        let message = error.to_string();
        assert!(
            message.contains(expected) && message.contains("may have changed"),
            "{message}"
        );
        assert!(!message.contains("PRIVATE_NATIVE_SECRET"));
        assert_eq!(
            fs::read(f.brew.with_file_name("partial-state")).unwrap(),
            b"partial"
        );
    }
}

#[test]
fn font_brew_unstartable_executable_has_an_uncertain_typed_result() {
    let f = Fixture::new("exit 0");
    for binary in [f.brew.with_file_name("missing"), f.brew.clone()] {
        fs::set_permissions(&f.brew, fs::Permissions::from_mode(0o644)).unwrap();
        let error =
            install(&binary, "font-hack-nerd-font", BrewKind::Cask, FONT_LIMITS).unwrap_err();
        assert!(!installation_fallback_allowed(&error));
        assert!(error.to_string().contains("could not start or capture"));
        assert!(!f.brew.with_file_name("args").exists());
    }
}
