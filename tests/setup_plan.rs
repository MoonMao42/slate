//! Reject invalid setup requests before configuration mutation. No valid tool or
//! font installation is requested, even when exercising the unfixed executor.
use slate_cli::{cli::setup_executor::execute_setup_with_env, env::SlateEnv};
use std::time::Duration;

#[path = "support/tree.rs"]
mod tree;

#[test]
#[ignore = "private subprocess helper with an explicit deadline"]
fn setup_plan_rejection_child() {
    let env = SlateEnv::from_process().unwrap();
    assert!(env.session().is_isolated());
    let case = std::env::var("SLATE_PLAN_CASE").unwrap();
    if case == "loader-fifo" {
        let path =
            std::ffi::CString::new(env.bash_integration_path().as_os_str().as_encoded_bytes())
                .unwrap();
        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    }
    if case == "loader-markers" {
        std::fs::write(
            env.bash_integration_path(),
            format!(
                "{}\nPRIVATE_UNCLOSED\n",
                slate_cli::adapter::marker_block::START
            ),
        )
        .unwrap();
    }
    if case == "editor-consent" {
        let path = env.nvim_auto_activation_path();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "invalid consent").unwrap();
    }
    if case.starts_with("state-") {
        let path = env.managed_file("current");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        match case.as_str() {
            "state-unknown" => std::fs::write(&path, "unknown\u{1b}").unwrap(),
            "state-directory" => std::fs::create_dir(&path).unwrap(),
            "state-binary" => std::fs::write(&path, [0xff]).unwrap(),
            "state-oversized" => std::fs::write(&path, vec![b'x'; 4097]).unwrap(),
            "state-link" => std::os::unix::fs::symlink("missing-fixture", &path).unwrap(),
            _ => unreachable!(),
        }
    }
    let before = tree::tree(env.home());
    let invalid = ["unknown-fixture-tool\u{1b}".into()];
    let installs = if case == "install" { &invalid[..] } else { &[] };
    let configure = if case == "configure" {
        &invalid[..]
    } else {
        &[]
    };
    let result = execute_setup_with_env(
        installs,
        configure,
        if case == "font" {
            Some("Invalid\nFont")
        } else {
            None
        },
        if case.starts_with("state-") {
            None
        } else {
            Some(if case == "theme" {
                "unknown-fixture-theme\u{1b}"
            } else {
                "nord"
            })
        },
        &env,
    );
    let error = result.expect_err("invalid plans must be rejected, not silently skipped");
    assert!(!error.to_string().contains('\u{1b}'));
    assert_eq!(tree::tree(env.home()), before, "{case}");
}

#[test]
fn setup_plan_rejects_invalid_requests_without_changes() {
    for case in [
        "install",
        "configure",
        "theme",
        "shell",
        "font",
        "loader-fifo",
        "loader-markers",
        "editor-consent",
        "state-unknown",
        "state-directory",
        "state-binary",
        "state-oversized",
        "state-link",
    ] {
        let td = tempfile::tempdir().unwrap();
        assert_cmd::Command::new(std::env::current_exe().unwrap())
            .env_clear()
            .env("HOME", td.path())
            .env("SLATE_HOME", td.path())
            .env("PATH", td.path().join("bin"))
            .env(
                "SHELL",
                if case == "shell" {
                    "/bin/unsupported"
                } else {
                    "/bin/bash"
                },
            )
            .env("SLATE_PLAN_CASE", case)
            .env("NO_COLOR", "1")
            .args([
                "--exact",
                "setup_plan_rejection_child",
                "--ignored",
                "--nocapture",
            ])
            .timeout(Duration::from_secs(6))
            .assert()
            .success();
    }
}
