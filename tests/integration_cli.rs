use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn test_cli_help() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Apply a theme"));
}

#[test]
fn test_cli_version() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("0.1.0"));
}

#[test]
fn test_cli_set_help() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.args(&["set", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("THEME"));
}

#[test]
fn test_cli_set_valid_theme() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("set")
        .arg("catppuccin-mocha")
        .assert()
        .success()
        .stdout(predicate::str::contains("Theme Applied:"))
        .stdout(predicate::str::contains("tools updated"));
}

#[test]
fn test_cli_set_invalid_theme() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("set")
        .arg("nonexistent-theme")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Error:"))
        .stderr(predicate::str::contains("not recognized"));
}

#[test]
fn test_cli_set_verbose_mode() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("set")
        .arg("catppuccin-mocha")
        .arg("--verbose")
        .assert()
        .success()
        .stdout(predicate::str::contains("Scanning for tools"))
        .stdout(predicate::str::contains("Checking"));
}

#[test]
fn test_cli_set_dracula_theme() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("set")
        .arg("dracula")
        .assert()
        .success()
        .stdout(predicate::str::contains("Theme Applied:"))
        .stdout(predicate::str::contains("dracula"));
}

#[test]
fn test_cli_set_tokyo_night_dark() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("set")
        .arg("tokyo-night-dark")
        .assert()
        .success()
        .stdout(predicate::str::contains("Theme Applied:"))
        .stdout(predicate::str::contains("tokyo-night-dark"));
}

#[test]
fn test_cli_set_nord_theme() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("set")
        .arg("nord")
        .assert()
        .success()
        .stdout(predicate::str::contains("Theme Applied:"))
        .stdout(predicate::str::contains("nord"));
}

#[test]
fn test_cli_set_case_insensitive() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("set")
        .arg("Catppuccin-Mocha")
        .assert()
        .success()
        .stdout(predicate::str::contains("Theme Applied:"));
}

#[test]
fn test_cli_exit_code_success() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("set")
        .arg("catppuccin-mocha")
        .assert()
        .success();
}

#[test]
fn test_cli_exit_code_failure() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("set")
        .arg("invalid-theme")
        .assert()
        .failure();
}

#[test]
fn test_cli_output_format_has_emoji() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("set")
        .arg("catppuccin-mocha")
        .assert()
        .success()
        .stdout(predicate::str::contains("🎨"));
}

#[test]
fn test_cli_output_format_has_checkmark() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("set")
        .arg("catppuccin-mocha")
        .assert()
        .success()
        .stdout(predicate::str::contains("✓"));
}

#[test]
fn test_cli_no_arguments() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("missing positional argument")
            .or(predicate::str::contains("COMMAND")));
}

#[test]
fn test_cli_set_no_theme() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("set")
        .assert()
        .failure()
        .stderr(predicate::str::contains("missing")
            .or(predicate::str::contains("required")));
}

#[test]
fn test_cli_status_help() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.args(&["status", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Show current theme state"));
}

#[test]
fn test_cli_status_basic() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("status")
        .assert()
        .success();
}

#[test]
fn test_cli_status_verbose() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("status")
        .arg("--verbose")
        .assert()
        .success();
}

#[test]
fn test_cli_status_verbose_short() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("status")
        .arg("-v")
        .assert()
        .success();
}

#[test]
fn test_cli_status_output_has_tools_or_empty_message() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    let output = cmd.arg("status")
        .output()
        .unwrap();
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should contain either tool names (if any installed) or the no-tools message
    let has_tools = stdout.contains("Tool Status") 
        || stdout.contains("Ghostty")
        || stdout.contains("Starship")
        || stdout.contains("bat")
        || stdout.contains("Delta")
        || stdout.contains("Lazygit");
    let has_empty_msg = stdout.contains("No supported tools detected");
    
    assert!(has_tools || has_empty_msg, 
        "Status output should show tools or empty message, got: {}", stdout);
}

// List command tests

#[test]
fn test_cli_list_help() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.args(&["list", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("List available themes"));
}

#[test]
fn test_cli_list_plain_text_mode() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    // Simulate non-TTY by piping
    cmd.arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("Catppuccin"))
        .stdout(predicate::str::contains("Tokyo Night"))
        .stdout(predicate::str::contains("Dracula"))
        .stdout(predicate::str::contains("Nord"));
}

#[test]
fn test_cli_list_contains_catppuccin_variants() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("catppuccin-latte"))
        .stdout(predicate::str::contains("catppuccin-frappe"))
        .stdout(predicate::str::contains("catppuccin-macchiato"))
        .stdout(predicate::str::contains("catppuccin-mocha"));
}

#[test]
fn test_cli_list_contains_tokyo_night_variants() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("tokyo-night-light"))
        .stdout(predicate::str::contains("tokyo-night-dark"));
}

#[test]
fn test_cli_list_contains_descriptions() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("Dark theme"))
        .stdout(predicate::str::contains("Light theme"));
}

#[test]
fn test_cli_list_contains_dracula() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("dracula"));
}

#[test]
fn test_cli_list_contains_nord() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("nord"));
}
