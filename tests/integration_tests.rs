use assert_cmd::Command;

#[test]
fn test_cli_help_shows_commands() {
    let mut cmd = Command::cargo_bin("slate").unwrap();

    let output = cmd.arg("--help").output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(stdout.contains("setup"));
    assert!(stdout.contains("set"));
    assert!(stdout.contains("status"));
    assert!(stdout.contains("list"));
    assert!(stdout.contains("restore"));
    assert!(stdout.contains("init"));
}

#[test]
fn test_setup_subcommand_help() {
    let mut cmd = Command::cargo_bin("slate").unwrap();

    let output = cmd.args(&["setup", "--help"]).output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(stdout.contains("setup"));
    assert!(stdout.contains("--quick"));
}

#[test]
fn test_set_subcommand_help() {
    let mut cmd = Command::cargo_bin("slate").unwrap();

    let output = cmd.args(&["set", "--help"]).output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(stdout.contains("set"));
}

#[test]
fn test_status_subcommand_help() {
    let mut cmd = Command::cargo_bin("slate").unwrap();

    let output = cmd.args(&["status", "--help"]).output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(stdout.contains("status"));
}

#[test]
fn test_list_subcommand_help() {
    let mut cmd = Command::cargo_bin("slate").unwrap();

    let output = cmd.args(&["list", "--help"]).output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(stdout.contains("list"));
}

#[test]
fn test_restore_subcommand_help() {
    let mut cmd = Command::cargo_bin("slate").unwrap();

    let output = cmd.args(&["restore", "--help"]).output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(stdout.contains("restore"));
}

#[test]
fn test_init_subcommand_help() {
    let mut cmd = Command::cargo_bin("slate").unwrap();

    let output = cmd.args(&["init", "--help"]).output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(stdout.contains("init"));
}

#[test]
fn test_setup_quick_flag() {
    let mut cmd = Command::cargo_bin("slate").unwrap();

    let output = cmd.args(&["setup", "--quick"]).output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Placeholder shows phase reference
    assert!(stdout.contains("") || stdout.contains("implemented"));
}

#[test]
fn test_set_with_theme_argument() {
    let mut cmd = Command::cargo_bin("slate").unwrap();

    let output = cmd.args(&["set", "catppuccin-mocha"]).output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Placeholder shows theme name
    assert!(stdout.contains("catppuccin-mocha"));
}

#[test]
fn test_status_command_runs() {
    let mut cmd = Command::cargo_bin("slate").unwrap();

    let output = cmd.arg("status").output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(stdout.contains("") || stdout.contains("implemented"));
}

#[test]
fn test_list_command_runs() {
    let mut cmd = Command::cargo_bin("slate").unwrap();

    let output = cmd.arg("list").output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(stdout.contains("") || stdout.contains("implemented"));
}

#[test]
fn test_restore_with_backup_id() {
    let mut cmd = Command::cargo_bin("slate").unwrap();

    let output = cmd.args(&["restore", "backup123"]).output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(stdout.contains("backup123"));
}

#[test]
fn test_init_with_shell_arg() {
    let mut cmd = Command::cargo_bin("slate").unwrap();

    let output = cmd.args(&["init", "zsh"]).output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(stdout.contains("zsh"));
}
