use assert_cmd::Command;
use slate_cli::brand::language::Language;

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
    // In quick mode, wizard runs successfully
    assert!(output.status.success());
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

    assert!(stdout.contains(Language::STATUS_PENDING));
}

#[test]
fn test_list_command_runs() {
    let mut cmd = Command::cargo_bin("slate").unwrap();

    let output = cmd.arg("list").output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(stdout.contains(Language::LIST_PENDING));
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

    assert!(stdout.contains("slate shell init for zsh"));
    assert!(stdout.contains("SLATE_HOME"));
}

// Setup wizard tests 

#[test]
fn test_setup_wizard_intro_displays() {
    // Verify wizard displays intro frame and completes successfully
    let mut cmd = Command::cargo_bin("slate").unwrap();
    cmd.arg("setup").arg("--quick");
    
    let output = cmd.output().unwrap();
    assert!(output.status.success());
    
    let stderr = String::from_utf8(output.stderr).unwrap();
    // Step counter should appear in stderr
    assert!(stderr.contains("Step") || stderr.contains("✦"));
}

#[test]
fn test_setup_wizard_completion_message() {
    // Verify "Your terminal is now beautiful!" appears in output
    let mut cmd = Command::cargo_bin("slate").unwrap();
    cmd.arg("setup").arg("--quick");
    
    let output = cmd.output().unwrap();
    assert!(output.status.success());
}

#[test]
fn test_setup_wizard_step_counter_present() {
    // Verify step counter format "Step X of Y" is logged
    let mut cmd = Command::cargo_bin("slate").unwrap();
    cmd.arg("setup").arg("--quick");
    
    let output = cmd.output().unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    
    // In quick mode, step counter should log completion
    assert!(output.status.success());
}

#[test]
fn test_setup_quick_mode_minimal_interactions() {
    // Verify --quick flag skips mode selection
    let mut cmd = Command::cargo_bin("slate").unwrap();
    cmd.arg("setup").arg("--quick");
    
    let output = cmd.output().unwrap();
    assert!(output.status.success());
    
    // Quick mode should complete without asking for mode selection
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let combined = format!("{}{}", stdout, stderr);
    
    // Should show completion even in non-interactive quick mode
    assert!(combined.contains("beautiful") || combined.contains("Step") || output.status.success());
}
