//! Neovim commit ordering with a private version script, never a real editor.
use slate_cli::{
    adapter::ToolApplyStatus,
    cli::theme_apply::{SnapshotPolicy, ThemeApplyCoordinator, ThemeCommitStage},
    config::ConfigManager,
    detection::{detect_tool_presence_with_env, ToolEvidence},
    env::SlateEnv,
    theme::ThemeRegistry,
};
use std::{
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::Path,
    time::Duration,
};

fn write(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

#[test]
#[ignore = "run by a private-profile parent with a deadline"]
fn nvim_commit_private_child() {
    let env = SlateEnv::from_process().unwrap();
    let case = std::env::var("SLATE_NVIM_COMMIT_CASE").unwrap();
    let _config = ConfigManager::with_env(&env).unwrap();
    let current = env.managed_file("current");
    let state = env.slate_cache_dir().join("current_theme.lua");
    let untouched = env.home().join("untouched");
    write(&untouched, "untouched target\n");
    write(&current, "nord\n");
    write(&state, "return 'nord'\n");
    let mut selected = vec!["nvim".into()];
    match case.as_str() {
        "shell" => write(
            &env.managed_file("config.toml"),
            "[tools]\nstarship = 'PRIVATE_BAD_FLAG'\n",
        ),
        "adapter" => {
            write(
                &env.xdg_config_home().join("alacritty/alacritty.toml"),
                "[broken TOML\n",
            );
            selected.push("alacritty".into());
        }
        "tracking" => {
            fs::remove_file(&current).unwrap();
            symlink(&untouched, &current).unwrap();
        }
        "notification" => {
            fs::remove_file(&state).unwrap();
            symlink(&untouched, &state).unwrap();
        }
        "success" | "probe" | "old" => {}
        _ => panic!("unknown fixture case"),
    }
    assert_eq!(
        detect_tool_presence_with_env("nvim", &env).evidence,
        Some(ToolEvidence::Executable(env.home().join("bin/nvim")))
    );
    let registry = ThemeRegistry::new().unwrap();
    let theme = registry.get("catppuccin-mocha").unwrap();
    // Skip is an existing caller-owned-checkpoint mode. For these two fixtures
    // it allows exercising late write failures instead of snapshot preflight.
    let policy = if matches!(case.as_str(), "tracking" | "notification") {
        SnapshotPolicy::Skip
    } else {
        SnapshotPolicy::Create
    };
    let outcome =
        ThemeApplyCoordinator::with_snapshot_policy(&env, policy).apply_to_tools(theme, &selected);
    if case == "shell" {
        let error = outcome.unwrap_err().to_string();
        assert!(error.contains("shared shell configuration"));
        assert!(!error.contains("PRIVATE_BAD_FLAG"));
        assert!(!env.home().join("version-calls").exists());
        assert_eq!(fs::read_to_string(&current).unwrap(), "nord\n");
        assert_eq!(fs::read_to_string(&state).unwrap(), "return 'nord'\n");
        assert!(!env.nvim_config_dir().exists());
        return;
    }
    let report = outcome.unwrap();
    assert_eq!(
        fs::read(env.home().join("version-calls")).unwrap(),
        b"probe\n"
    );
    let nvim = report
        .results
        .iter()
        .find(|result| result.tool_name == "nvim")
        .unwrap();
    if case == "success" {
        report.ensure_no_failures().unwrap();
        assert_eq!(report.applied_count(), 1);
        assert!(matches!(nvim.status, ToolApplyStatus::Applied));
        assert_eq!(fs::read_to_string(&current).unwrap(), theme.id);
        assert_eq!(
            fs::read_to_string(&state).unwrap(),
            "return \"catppuccin-mocha\"\n"
        );
    } else if case == "probe" || case == "old" {
        if case == "probe" {
            let error = report.ensure_no_failures().unwrap_err().to_string();
            assert!(!error.contains("PRIVATE_HEADER"));
            assert!(matches!(nvim.status, ToolApplyStatus::Failed(_)));
            assert_eq!(report.failed_count(), 1);
        } else {
            report.ensure_no_failures().unwrap();
            assert!(matches!(
                nvim.status,
                ToolApplyStatus::Skipped(slate_cli::adapter::SkipReason::NotInstalled)
            ));
        }
        assert!(report.commit_failure.is_none());
        assert_eq!(report.applied_count(), 0);
        assert_eq!(fs::read_to_string(&current).unwrap(), "nord\n");
        assert_eq!(fs::read_to_string(&state).unwrap(), "return 'nord'\n");
    } else if case == "notification" {
        let error = report.ensure_no_failures().unwrap_err().to_string();
        assert!(error.contains("already saved"), "{error}");
        assert_eq!(report.failed_count(), 1);
        assert!(matches!(nvim.status, ToolApplyStatus::Failed(_)));
        assert_eq!(fs::read_to_string(&current).unwrap(), theme.id);
        assert_eq!(fs::read_link(&state).unwrap(), untouched);
    } else {
        assert!(report.ensure_no_failures().is_err());
        assert!(
            matches!(&nvim.status, ToolApplyStatus::Skipped(reason) if reason.to_string().contains("theme was not committed"))
        );
        assert_eq!(fs::read_to_string(&state).unwrap(), "return 'nord'\n");
        assert_eq!(report.applied_count(), 0);
        if case == "tracking" {
            assert_eq!(fs::read_link(&current).unwrap(), untouched);
            assert_eq!(
                report.commit_failure.as_ref().unwrap().stage,
                ThemeCommitStage::CurrentTheme
            );
        } else {
            assert_eq!(fs::read_to_string(&current).unwrap(), "nord\n");
        }
        if case == "shell" {
            assert_eq!(
                report.commit_failure.as_ref().unwrap().stage,
                ThemeCommitStage::ShellIntegration
            );
        }
    }
    assert_eq!(
        fs::read_to_string(&untouched).unwrap(),
        "untouched target\n"
    );
    assert!(!env.nvim_config_dir().exists());
}

#[test]
fn nvim_commit_preserves_old_notification_until_the_theme_is_saved() {
    for case in [
        "shell",
        "adapter",
        "tracking",
        "success",
        "notification",
        "probe",
        "old",
    ] {
        let td = tempfile::tempdir().unwrap();
        let executable = td.path().join("bin/nvim");
        let version = match case {
            "probe" => "NVIM PRIVATE_HEADER",
            "old" => "NVIM v0.7.0",
            _ => "NVIM v0.8.0",
        };
        write(&executable, &format!("#!/bin/sh\n[ \"$1\" = --version ] || exit 91\nprintf 'probe\\n' >> \"$HOME/version-calls\"\nprintf '{version}\\n'\n"));
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
        assert_cmd::Command::new(std::env::current_exe().unwrap())
            .env_clear()
            .env("HOME", td.path())
            .env("SLATE_HOME", td.path())
            .env("PATH", executable.parent().unwrap())
            .env("SLATE_NVIM_COMMIT_CASE", case)
            .current_dir(td.path())
            .args([
                "--exact",
                "nvim_commit_private_child",
                "--ignored",
                "--nocapture",
            ])
            .timeout(Duration::from_secs(7))
            .assert()
            .success();
    }
}
