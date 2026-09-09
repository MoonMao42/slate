//! Captured custom paths across cwd/environment changes, clean and recovery.
//! Cwd mutations run in a deadline-bound child, never the parallel test runner.
use slate_cli::cli::theme_apply::ThemeApplyCoordinator;
use slate_cli::config::{
    begin_restore_point_baseline_with_env, execute_restore_with_env, get_restore_point_with_env,
    list_restore_points_with_env, OriginalFileState,
};
use slate_cli::{env::SlateEnv, theme::ThemeRegistry};
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[path = "support/tree.rs"]
mod tree_snapshot;

const ORIGINAL: &str =
    "// PRIVATE_CONTENT\n{\"$schema\":\"user\",\"theme\":\"custom\",\"mouse\":false}\n";

fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o640)).unwrap();
}

fn command(env: &SlateEnv) -> assert_cmd::Command {
    let mut cmd = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    cmd.env_clear()
        .env("HOME", env.home())
        .env("PATH", env.home().join("bin"))
        .env("SSH_CONNECTION", "private-fixture")
        .env("NO_COLOR", "1")
        .env("OPENCODE_TUI_CONFIG", env.opencode_tui_config().unwrap())
        .timeout(Duration::from_secs(5));
    cmd
}

#[test]
fn opencode_captured_path_probe() {
    let Some(expected) = std::env::var_os("SLATE_EXPECTED_TUI") else {
        return;
    };
    let expected = PathBuf::from(expected);
    let second = PathBuf::from(std::env::var_os("SLATE_NEXT_CWD").unwrap());
    let env = SlateEnv::from_process().unwrap();
    assert_eq!(env.opencode_tui_config(), Some(expected.as_path()));
    assert!(expected.is_absolute());
    let original = fs::read(&expected).ok();
    let unrelated = second.join("other.json");
    let other_before = fs::read(&unrelated).unwrap();
    // After capture, neither a different cwd nor a changed process variable
    // may reinterpret the path used by this environment (or its clone).
    std::env::set_current_dir(&second).unwrap();
    std::env::set_var("OPENCODE_TUI_CONFIG", "other.json");
    assert_eq!(env.clone().opencode_tui_config(), Some(expected.as_path()));
    let theme = ThemeRegistry::new().unwrap();
    let report = ThemeApplyCoordinator::new(&env)
        .apply_to_tools(theme.get("nord").unwrap(), &["opencode".into()])
        .unwrap();
    report.ensure_no_failures().unwrap();
    assert_eq!(report.applied_count(), 1);
    let baseline = report.restore_point_id.unwrap();
    let point = get_restore_point_with_env(&env, &baseline).unwrap();
    assert!(!point.is_baseline);
    let entry = point
        .entries
        .iter()
        .find(|entry| entry.original_path == expected)
        .unwrap();
    assert_eq!(entry.original_path, expected);
    match &original {
        Some(bytes) => {
            assert_eq!(entry.original_state, OriginalFileState::Present);
            assert_eq!(
                fs::read(entry.backup_path.as_ref().unwrap()).unwrap(),
                *bytes
            );
            assert_eq!(entry.unix_mode, Some(0o640));
        }
        None => assert_eq!(entry.original_state, OriginalFileState::Absent),
    }
    let applied = fs::read(&expected).unwrap();
    if original.is_some() {
        assert_eq!(
            applied,
            ORIGINAL.replace("\"custom\"", "\"system\"").as_bytes()
        );
    } else {
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&applied).unwrap()["theme"],
            "system"
        );
    }
    assert_eq!(fs::read(&unrelated).unwrap(), other_before);

    let before_doctor = tree_snapshot::tree(env.home());
    let output = command(&env)
        .args(["doctor", "opencode", "--json"])
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(!String::from_utf8_lossy(&output.stdout).contains("PRIVATE_CONTENT"));
    let doctor: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(doctor["checks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["code"] == "selected_config" && c["path"].as_str() == expected.to_str()));
    assert_eq!(tree_snapshot::tree(env.home()), before_doctor);
    assert_eq!(fs::read(&expected).unwrap(), applied);

    let output = command(&env)
        .args(["clean", "--dry-run", "--json"])
        .assert()
        .success()
        .get_output()
        .clone();
    let preview: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let selected: Vec<_> = preview["changes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["path"].as_str() == expected.to_str())
        .collect();
    assert_eq!(selected.len(), 1);
    assert_eq!(
        selected[0]["action"],
        if original.is_some() {
            "rewrite"
        } else {
            "remove"
        }
    );
    assert_eq!(tree_snapshot::tree(env.home()), before_doctor);
    assert_eq!(fs::read(&expected).unwrap(), applied);
    command(&env).args(["--quiet", "clean"]).assert().success();
    if original.is_some() {
        assert!(!fs::read_to_string(&expected).unwrap().contains("\"theme\""));
    } else {
        assert!(!expected.exists());
    }

    // A new environment now selects a completely different existing config.
    // File-only checkpoints must still restore their recorded absolute target.
    let restore_env = SlateEnv::from_process().unwrap();
    assert_eq!(restore_env.opencode_tui_config(), Some(unrelated.as_path()));
    let clean = list_restore_points_with_env(&env)
        .unwrap()
        .into_iter()
        .find(|p| p.theme_name == "pre-clean-nord")
        .unwrap();
    assert!(execute_restore_with_env(&restore_env, &clean.id)
        .unwrap()
        .is_fully_successful());
    assert_eq!(fs::read(&expected).unwrap(), applied);
    std::env::set_var("OPENCODE_TUI_CONFIG", "missing/../unresolved.json");
    let unresolved_env = SlateEnv::from_process().unwrap();
    assert!(unresolved_env.validate_opencode_tui_config().is_err());
    assert!(execute_restore_with_env(&unresolved_env, &baseline)
        .unwrap()
        .is_fully_successful());
    assert_eq!(fs::read(&expected).ok(), original);
    if original.is_some() {
        assert_eq!(
            fs::metadata(&expected).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }
    assert_eq!(fs::read(&unrelated).unwrap(), other_before);
    assert!(!env.home().join("tool-was-launched").exists());
}

#[test]
fn opencode_relative_and_alias_paths_survive_apply_clean_and_file_only_restore() {
    for case in [
        "absolute",
        "relative",
        "dot",
        "parent",
        "alias-parent",
        "missing",
        "same-default",
        "missing-alias",
    ] {
        let td = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(td.path()).unwrap();
        let home = root.join("profile");
        let first = root.join("work/first");
        let second = root.join("work/second");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();
        let (raw, expected) = match case {
            "absolute" => {
                let path = first.join("custom config/tui.jsonc");
                (path.clone(), path)
            }
            "relative" => (
                PathBuf::from("custom config/tui.jsonc"),
                first.join("custom config/tui.jsonc"),
            ),
            "dot" => (
                PathBuf::from("./custom config/tui.jsonc"),
                first.join("custom config/tui.jsonc"),
            ),
            "parent" => (
                PathBuf::from("../settings/tui.jsonc"),
                root.join("work/settings/tui.jsonc"),
            ),
            "alias-parent" => {
                fs::create_dir_all(root.join("real/leaf")).unwrap();
                symlink(root.join("real/leaf"), first.join("pointer")).unwrap();
                write(&first.join("tui.jsonc"), "decoy PRIVATE_CONTENT");
                (
                    PathBuf::from("pointer/../tui.jsonc"),
                    root.join("real/tui.jsonc"),
                )
            }
            "missing" => (
                PathBuf::from("new/tree/tui.jsonc"),
                first.join("new/tree/tui.jsonc"),
            ),
            "same-default" | "missing-alias" => {
                let config = home.join(".config/opencode");
                fs::create_dir_all(&config).unwrap();
                symlink(&config, first.join("config-alias")).unwrap();
                (
                    PathBuf::from("config-alias/tui.json"),
                    first.join("config-alias/tui.json"),
                )
            }
            _ => unreachable!(),
        };
        if !matches!(case, "missing" | "missing-alias") {
            write(&expected, ORIGINAL);
        }
        write(
            &second.join("other.json"),
            "// PRIVATE_CONTENT\n{\"theme\":\"unchanged\"}",
        );
        let binary = home.join("bin/opencode");
        write(
            &binary,
            "#!/bin/sh\n: > \"$HOME/tool-was-launched\"\nexit 92\n",
        );
        fs::set_permissions(binary, fs::Permissions::from_mode(0o755)).unwrap();
        assert_cmd::Command::new(std::env::current_exe().unwrap())
            .env_clear()
            .env("HOME", &home)
            .env("PATH", home.join("bin"))
            .env("SSH_CONNECTION", "private-fixture")
            .env("NO_COLOR", "1")
            .env("OPENCODE_TUI_CONFIG", raw)
            .env("SLATE_EXPECTED_TUI", &expected)
            .env("SLATE_NEXT_CWD", second)
            .current_dir(&first)
            .args(["--exact", "opencode_captured_path_probe", "--nocapture"])
            .timeout(Duration::from_secs(10))
            .assert()
            .success();
        if case == "alias-parent" {
            assert_eq!(
                fs::read_to_string(first.join("tui.jsonc")).unwrap(),
                "decoy PRIVATE_CONTENT"
            );
        }
    }
}

#[test]
fn opencode_unresolvable_directory_syntax_never_falls_back_or_modifies_files() {
    let td = tempfile::tempdir().unwrap();
    let home = td.path();
    write(&home.join("regular"), "PRIVATE_CONTENT");
    write(&home.join(".config/opencode/tui.json"), ORIGINAL);
    // Prepare the cooperative lock before snapshots below: failed imports may
    // acquire it, but must not create checkpoints or change profile settings.
    drop(
        slate_cli::config::ConfigWriteGuard::acquire(&SlateEnv::with_home(home.to_owned()))
            .unwrap(),
    );
    for raw in [
        "regular/",
        "regular/.",
        "regular/../tui.json",
        "missing/../tui.json",
    ] {
        let before = tree_snapshot::tree(home);
        let injected = SlateEnv::from_vars(|key| match key {
            "HOME" => Some(home.as_os_str().to_owned()),
            "OPENCODE_TUI_CONFIG" => Some(home.join(raw).into_os_string()),
            _ => None,
        })
        .unwrap();
        use slate_cli::adapter::{OpencodeAdapter, ToolAdapter};
        assert!(OpencodeAdapter
            .apply_theme_with_env(
                ThemeRegistry::new().unwrap().get("nord").unwrap(),
                &injected
            )
            .is_err());
        assert!(begin_restore_point_baseline_with_env(&injected).is_err());
        assert_eq!(tree_snapshot::tree(home), before);
        for isolated in [false, true] {
            let mut cmd = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
            cmd.env_clear()
                .env("HOME", home)
                .env("PATH", home.join("bin"))
                .env("OPENCODE_TUI_CONFIG", raw)
                .current_dir(home)
                .args(["doctor", "opencode", "--json"])
                .timeout(Duration::from_secs(5));
            if isolated {
                cmd.env("SLATE_HOME", home);
            }
            let output = cmd.output().unwrap();
            assert!(output.status.success());
            assert!(!String::from_utf8_lossy(&output.stderr).contains("PRIVATE_CONTENT"));
            let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(
                report["checks"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|c| c["code"] == "unresolved_config" && c["status"] == "error"),
                !isolated
            );
            if !isolated {
                assert!(String::from_utf8_lossy(&output.stdout)
                    .contains("no fallback path was selected"));
            }
            assert_eq!(tree_snapshot::tree(home), before);
        }
        // Other file-only diagnostics remain usable with this broken override.
        assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"))
            .env_clear()
            .env("HOME", home)
            .env("PATH", home.join("bin"))
            .env("OPENCODE_TUI_CONFIG", raw)
            .current_dir(home)
            .args(["doctor", "kitty", "--json"])
            .timeout(Duration::from_secs(5))
            .assert()
            .success();
        for preview in [true, false] {
            let mut cmd = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
            cmd.env_clear()
                .env("HOME", home)
                .env("PATH", home.join("bin"))
                .env("OPENCODE_TUI_CONFIG", raw)
                .current_dir(home)
                .args(["--quiet", "clean"])
                .timeout(Duration::from_secs(5));
            if preview {
                cmd.args(["--dry-run", "--json"]);
            }
            cmd.assert().failure();
            assert_eq!(tree_snapshot::tree(home), before);
        }
        assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"))
            .env_clear()
            .env("HOME", home)
            .env("PATH", home.join("bin"))
            .env("OPENCODE_TUI_CONFIG", raw)
            .current_dir(home)
            .args(["--quiet", "import", "slate://nord/none/none/none"])
            .timeout(Duration::from_secs(5))
            .assert()
            .failure();
        assert_eq!(tree_snapshot::tree(home), before);
    }
}
