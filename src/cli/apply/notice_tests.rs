use super::*;

#[test]
#[ignore = "private subprocess fixture for compact result logging"]
fn compact_report_log_child() {
    let mode = std::env::var("SLATE_COMPACT_LOG_FIXTURE").unwrap();
    let mut report = ThemeApplyReport {
        results: vec![
            ToolApplyResult {
                tool_name: "btop".into(),
                status: ToolApplyStatus::Applied,
                requires_new_shell: false,
            },
            ToolApplyResult {
                tool_name: "starship".into(),
                status: ToolApplyStatus::Failed(crate::error::SlateError::Internal(
                    "adapter failure fixture".into(),
                )),
                requires_new_shell: false,
            },
            ToolApplyResult {
                tool_name: "delta".into(),
                status: ToolApplyStatus::Skipped(SkipReason::MissingIntegrationConfig),
                requires_new_shell: false,
            },
        ],
        commit_failure: Some(ThemeCommitFailure {
            stage: ThemeCommitStage::CurrentTheme,
            error: crate::error::SlateError::Internal("commit failure fixture".into()),
        }),
        reload_warnings: vec![ReloadWarning {
            informational: false,
            tool_name: "tmux".into(),
            message: "reload failure fixture".into(),
        }],
        restore_point_id: Some("recovery-fixture".into()),
    };
    if mode == "no-applied" {
        report.results.remove(0);
    }
    log_apply_report_with_summary(&report, mode != "fallback");
}

#[test]
fn compact_report_logging_keeps_all_failures_and_recovery_fallback() {
    for mode in ["summary", "fallback", "no-applied"] {
        let root = tempfile::tempdir().unwrap();
        let output = assert_cmd::Command::new(std::env::current_exe().unwrap())
            .env_clear()
            .env("HOME", root.path())
            .env("PATH", root.path().join("empty"))
            .env("SLATE_COMPACT_LOG_FIXTURE", mode)
            .args([
                "--ignored",
                "--exact",
                "--nocapture",
                "cli::apply::notice_tests::compact_report_log_child",
            ])
            .timeout(std::time::Duration::from_secs(5))
            .assert()
            .success()
            .get_output()
            .clone();
        let text = String::from_utf8(output.stderr).unwrap();
        for expected in [
            "adapter failure fixture",
            "delta: missing integration config",
            "warning: tmux: reload failure fixture",
            "commit failure fixture",
        ] {
            assert!(text.contains(expected), "{mode}: {text}");
        }
        assert_eq!(text.contains("✓ btop"), mode == "fallback", "{text}");
        assert_eq!(
            text.contains("Restore point: recovery-fixture"),
            mode != "summary",
            "{text}"
        );
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    }
}

#[test]
#[ignore = "private subprocess fixture for tmux notice classification"]
fn tmux_notice_child() {
    assert!(std::env::var_os("SLATE_TMUX_NOTICE_FIXTURE").is_some());
    let env = SlateEnv::from_process().unwrap();
    assert!(!env.session().is_isolated());
    let mut report = ThemeApplyReport {
        results: vec![ToolApplyResult {
            tool_name: "tmux".into(),
            status: ToolApplyStatus::Applied,
            requires_new_shell: false,
        }],
        commit_failure: None,
        reload_warnings: Vec::new(),
        restore_point_id: None,
    };
    reload_theme_targets(&env, &ToolRegistry::default(), &mut report);
    assert_eq!(report.reload_warnings.len(), 1);
    log_apply_warnings(&report);
}

#[test]
fn tmux_notice_distinguishes_inactive_default_from_reload_failure() {
    use std::{fs, os::unix::fs::PermissionsExt, time::Duration};
    for (reason, expected) in [
        (
            "No such file or directory",
            "info: tmux: Colors saved; no default tmux server",
        ),
        ("Permission denied", "warning: tmux: Failed to reload tmux"),
    ] {
        let root = tempfile::tempdir().unwrap();
        let binary = root.path().join(".local/bin/tmux");
        fs::create_dir_all(binary.parent().unwrap()).unwrap();
        fs::write(&binary, format!("#!/bin/sh\nprintf 'error connecting to /tmp/private-test/default ({reason})\\n' >&2\nexit 1\n")).unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
        let output = assert_cmd::Command::new(std::env::current_exe().unwrap())
            .env_clear()
            .env("HOME", root.path())
            .env("PATH", root.path().join("empty"))
            .env("SLATE_TMUX_NOTICE_FIXTURE", "1")
            .args([
                "--ignored",
                "--exact",
                "--nocapture",
                "cli::apply::notice_tests::tmux_notice_child",
            ])
            .timeout(Duration::from_secs(5))
            .assert()
            .success()
            .get_output()
            .clone();
        let text = String::from_utf8(output.stderr).unwrap();
        assert!(text.contains(expected), "{text}");
        assert!(
            !text.contains("/tmp/private-test"),
            "native output must remain omitted"
        );
        assert!(!root.path().join(".config").exists());
    }
}
