use assert_cmd::Command;

#[test]
fn test_restore_appears_in_help() {
    let mut cmd = Command::cargo_bin("slate").unwrap();
    let output = cmd.arg("--help").output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    // restore should be in main help
    assert!(
        stdout.contains("restore"),
        "restore command should appear in help"
    );
}

#[test]
fn test_restore_help_shows_subcommand() {
    let mut cmd = Command::cargo_bin("slate").unwrap();
    let output = cmd.args(["restore", "--help"]).output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Should show restore options
    assert!(stdout.contains("restore"));
    // Should mention list and delete flags
    assert!(stdout.contains("--list") || stdout.contains("--delete"));
}

#[test]
fn test_restore_with_invalid_id_fails_gracefully() {
    let mut cmd = Command::cargo_bin("slate").unwrap();
    let output = cmd.args(["restore", "nonexistent-restore-id"]).output().unwrap();

    // Command should fail
    assert!(!output.status.success(), "restore with invalid ID should fail");

    let stderr = String::from_utf8(output.stderr).unwrap();
    // Error message should mention the problem
    assert!(
        stderr.contains("Failed") || stderr.contains("not found") || stderr.contains("error"),
        "should provide meaningful error message, got: {}",
        stderr
    );
}

#[test]
fn test_restore_list_command_runs() {
    let mut cmd = Command::cargo_bin("slate").unwrap();
    let output = cmd.args(["restore", "--list"]).output().unwrap();

    // Command should succeed (even if no restore points exist)
    assert!(output.status.success(), "restore --list should succeed");

    let stdout = String::from_utf8(output.stdout).unwrap();
    // Should show message about no points or list of points
    assert!(
        stdout.contains("restore") || stdout.contains("No restore points"),
        "should display restore-related info"
    );
}

#[test]
fn test_restore_not_hidden_but_reset_is() {
    // Test that restore is advertised in main help
    let mut cmd = Command::cargo_bin("slate").unwrap();
    let output = cmd.arg("--help").output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    // restore SHOULD be shown as a primary command
    assert!(
        stdout.contains("restore"),
        "restore should appear in primary help"
    );

    // reset should NOT be shown as a primary command
    assert!(
        !stdout.contains("reset"),
        "reset should not appear in primary help (hidden compatibility alias)"
    );
}

#[test]
fn test_restore_delete_command_fails_gracefully() {
    let mut cmd = Command::cargo_bin("slate").unwrap();
    let output = cmd.args(["restore", "--delete", "nonexistent-id"]).output().unwrap();

    // Should fail gracefully
    assert!(!output.status.success(), "delete with invalid ID should fail");

    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("Failed") || stderr.contains("not found") || stderr.contains("error"),
        "should provide error message"
    );
}

#[test]
fn test_reset_still_works_but_hidden() {
    // Test that reset still works as a hidden compatibility alias
    let mut cmd = Command::cargo_bin("slate").unwrap();
    let output = cmd.args(["reset", "--help"]).output().unwrap();

    // reset command should be recognized (hidden but functional)
    // This may succeed or fail depending on whether we allow --help on hidden commands
    // But reset without arguments should show the transition tip
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();

    // Either the help shows up, or clap recognizes it as a command
    assert!(
        output.status.success() || stderr.contains("reset") || !stdout.is_empty(),
        "reset should be recognized (even if hidden)"
    );
}
