//! Invalid retry input must stop before profile, lock, sound or installer IO.
use std::time::Duration;

#[path = "support/tree.rs"]
mod tree_snapshot;

#[test]
fn setup_retry_input_errors_are_read_only_and_work_without_home() {
    let td = tempfile::tempdir().unwrap();
    let before = tree_snapshot::tree(td.path());
    for isolated in [false, true] {
        for (target, message) in [
            ("unknown-tool", "Unknown tool"),
            ("tmux", "not installable"),
            ("ghostty", "not installable"),
            ("\x1b[31munknown\n", "Unknown tool"),
        ] {
            let mut command = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
            command
                .env_clear()
                .env("NO_COLOR", "1")
                .env("PATH", td.path().join("bin"));
            if isolated {
                command.env("HOME", td.path()).env("SLATE_HOME", td.path());
            }
            let output = command
                .args(["setup", "--only", target])
                .timeout(Duration::from_secs(4))
                .assert()
                .code(1)
                .get_output()
                .clone();
            let error = String::from_utf8(output.stderr).unwrap();
            assert!(error.contains(message), "{error}");
            assert!(!error.contains('\x1b'));
            assert_eq!(error.lines().count(), 1);
            assert!(output.stdout.is_empty());
            assert_eq!(tree_snapshot::tree(td.path()), before);
        }
    }
}
