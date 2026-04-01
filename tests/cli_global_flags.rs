//! `--auto` / `--quiet` are promoted to root-level
//! global flags (clap `#[arg(global = true)]`). Both orderings
//! flag-before-subcommand and flag-after-subcommand — must parse
//! identically.
//! performs the clap refactor and un-ignores these
//! tests. We drive a read-only subcommand (`list`) so the assertion
//! isolates clap parsing from apply-side effects that would require a
//! fully-populated host config.

use assert_cmd::Command;
use tempfile::TempDir;

/// Isolated slate invocation — SLATE_HOME under a tempdir so tests
/// never touch the real ~/.config or ~/.cache. Mirrors the helper
/// shape in `tests/integration_tests.rs`.
fn slate_cmd_isolated(tempdir: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("slate").unwrap();
    cmd.env("SLATE_HOME", tempdir.path());
    let shell = std::env::var("SHELL").unwrap_or_else(|_| {
        if cfg!(target_os = "macos") {
            "/bin/zsh".to_string()
        } else {
            "/bin/bash".to_string()
        }
    });
    cmd.env("SHELL", shell);
    cmd
}

/// A clap parse error returns exit code 2. Anything else (0, 1, …)
/// means clap accepted the flag and handed control off to a handler
/// which is all this suite is meant to certify.
fn clap_accepted(status: std::process::ExitStatus) -> bool {
    status.code().is_some_and(|c| c != 2)
}

#[test]
fn quiet_flag_works_at_root_position() {
    let td = TempDir::new().unwrap();
    let out = slate_cmd_isolated(&td)
        .args(["--quiet", "list"])
        .output()
        .unwrap();
    assert!(
        clap_accepted(out.status),
        "slate --quiet list must parse (status: {:?}, stderr: {})",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn quiet_flag_works_at_subcommand_position() {
    let td = TempDir::new().unwrap();
    let out = slate_cmd_isolated(&td)
        .args(["list", "--quiet"])
        .output()
        .unwrap();
    assert!(
        clap_accepted(out.status),
        "slate list --quiet must parse (status: {:?}, stderr: {})",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn auto_flag_works_at_root_position() {
    let td = TempDir::new().unwrap();
    let out = slate_cmd_isolated(&td)
        .args(["--auto", "theme"])
        .output()
        .unwrap();
    // auto on theme without arg → try to apply auto-resolved theme.
    // Assert that clap parses the flag (exit code comes from handler).
    // Success or a well-formed failure both satisfy "clap accepted the flag".
    assert!(
        clap_accepted(out.status),
        "clap must accept --auto at root position (status: {:?})",
        out.status
    );
}
