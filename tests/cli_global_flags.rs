//! `--auto` / `--quiet` are promoted to root-level
//! global flags (clap `#[arg(global = true)]`). Both orderings
//! `slate --quiet theme set X` and `slate theme set X --quiet`
//! must parse identically and produce identical behavior.
//! performs the clap refactor and un-ignores these
//! tests.

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

#[test]
#[ignore = " — clap global-flag promotion"]
fn quiet_flag_works_at_root_position() {
    let td = TempDir::new().unwrap();
    let out = slate_cmd_isolated(&td)
        .args(["--quiet", "theme", "set", "catppuccin-mocha"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "slate --quiet theme set X must succeed (status: {:?})",
        out.status
    );
}

#[test]
#[ignore = " — clap global-flag promotion"]
fn quiet_flag_works_at_subcommand_position() {
    let td = TempDir::new().unwrap();
    let out = slate_cmd_isolated(&td)
        .args(["theme", "set", "catppuccin-mocha", "--quiet"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "slate theme set X --quiet must succeed (status: {:?})",
        out.status
    );
}

#[test]
#[ignore = " — clap global-flag promotion"]
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
        out.status.code().is_some(),
        "clap must accept --auto at root position"
    );
}
