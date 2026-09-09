use assert_cmd::assert::OutputAssertExt;
use slate_cli::{
    config::{begin_restore_point_baseline_with_env, ConfigManager, ConfigWriteGuard},
    env::SlateEnv,
};
use std::{
    fs,
    os::{
        fd::OwnedFd,
        unix::net::{UnixDatagram, UnixStream},
    },
    path::Path,
    process::{Command, Stdio},
    time::Duration,
};

#[path = "../support/redirected_output.rs"]
mod redirected_output;
#[path = "../support/tree.rs"]
mod snapshot;

fn process(home: &Path, id: &str, json: bool) -> Command {
    let mut command = Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    command
        .env_clear()
        .env("HOME", home)
        .env("SLATE_HOME", home)
        .env("PATH", "")
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .args(["restore", id, "--dry-run"]);
    if json {
        command.arg("--json");
    }
    command
}

fn fixture(home: &Path) -> (SlateEnv, String) {
    let env = SlateEnv::with_home(home.to_owned());
    fs::write(env.zshrc_path(), "PRIVATE_FIXTURE_CONTENT original\n").unwrap();
    let point = begin_restore_point_baseline_with_env(&env).unwrap();
    fs::write(env.zshrc_path(), "PRIVATE_FIXTURE_CONTENT current\n").unwrap();
    (env, point.id)
}

#[test]
fn restore_preview_output_preserves_blocked_status_after_real_pipe_closure_and_propagates_other_io_errors(
) {
    for blocked in [false, true] {
        let td = tempfile::tempdir().unwrap();
        let (env, id) = fixture(td.path());
        let _guard = ConfigWriteGuard::acquire(&env).unwrap();
        fs::write(
            env.slate_cache_dir().join("preview-session.json"),
            "PRIVATE_FIXTURE_CONTENT",
        )
        .unwrap();
        fs::create_dir_all(env.config_dir()).unwrap();
        fs::write(
            env.managed_file("config.toml"),
            b"PRIVATE_FIXTURE_CONTENT\xff",
        )
        .unwrap();
        if blocked {
            fs::remove_file(env.zshrc_path()).unwrap();
            fs::create_dir(env.zshrc_path()).unwrap();
        }
        let before = snapshot::tree(td.path());
        for json in [false, true] {
            let output = assert_cmd::Command::from_std(process(td.path(), &id, json))
                .timeout(Duration::from_secs(5))
                .assert()
                .code(i32::from(blocked))
                .get_output()
                .clone();
            assert!(!String::from_utf8_lossy(&output.stdout).contains("PRIVATE_FIXTURE_CONTENT"));
            if json {
                let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
                assert_eq!(report["restore_point_id"], id);
            }
            let (consumer, producer) = UnixStream::pair().unwrap();
            drop(consumer);
            let mut command = process(td.path(), &id, json);
            command.stdout(Stdio::from(OwnedFd::from(producer)));
            let output = redirected_output::run(&mut command)
                .assert()
                .code(i32::from(blocked))
                .get_output()
                .clone();
            let error = String::from_utf8_lossy(&output.stderr);
            if blocked {
                assert!(error.contains("blocked"));
            } else {
                assert!(error.is_empty());
            }
            assert!(!error.contains("panicked") && !error.contains("PRIVATE_FIXTURE_CONTENT"));
            let mut command = process(td.path(), &id, json);
            command.stdout(Stdio::from(OwnedFd::from(UnixDatagram::unbound().unwrap())));
            let output = redirected_output::run(&mut command)
                .assert()
                .code(1)
                .get_output()
                .clone();
            let error = String::from_utf8_lossy(&output.stderr);
            assert!(!error.is_empty() && !error.contains("panicked"));
        }
        assert_eq!(snapshot::tree(td.path()), before);
    }
}

#[test]
fn restore_preview_output_escapes_directional_paths_but_json_keeps_exact_target_paths() {
    let td = tempfile::tempdir().unwrap();
    // Record validation already rejects newline/ESC paths. Directional format
    // characters are valid filenames and must still be escaped for display.
    let home = td.path().join("profile\u{202e}\u{2066}中文");
    fs::create_dir(&home).unwrap();
    let (env, id) = fixture(&home);
    let before = snapshot::tree(td.path());
    let output = assert_cmd::Command::from_std(process(&home, &id, false))
        .timeout(Duration::from_secs(5))
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(output.stderr.is_empty());
    let text = String::from_utf8(output.stdout).unwrap();
    for c in ['\x1b', '\t', '\u{202e}', '\u{2066}'] {
        assert!(!text.contains(c), "unescaped {c:?}: {text:?}");
    }
    assert!(text.contains("profile\\u{202e}\\u{2066}中文"));
    assert!(text.contains("Preview only; no files or restore points were changed."));
    let output = assert_cmd::Command::from_std(process(&home, &id, true))
        .timeout(Duration::from_secs(5))
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(json["changes"].as_array().unwrap().iter().any(|change| {
        change["original_path"] == env.zshrc_path().to_str().unwrap()
            && change["action"] == "replace"
    }));
    assert_eq!(snapshot::tree(td.path()), before);
}

#[test]
fn restore_preview_output_unwritable_confirmation_plan_stops_before_actual_restore() {
    let td = tempfile::tempdir().unwrap();
    let (env, id) = fixture(td.path());
    ConfigManager::with_env(&env)
        .unwrap()
        .set_sound_enabled(false)
        .unwrap();
    drop(ConfigWriteGuard::acquire(&env).unwrap());
    let before = snapshot::tree(td.path());
    for broken_pipe in [false, true] {
        let stdout = if broken_pipe {
            let (consumer, producer) = UnixStream::pair().unwrap();
            drop(consumer);
            Stdio::from(OwnedFd::from(producer))
        } else {
            Stdio::from(OwnedFd::from(UnixDatagram::unbound().unwrap()))
        };
        let mut command = Command::new(assert_cmd::cargo::cargo_bin!("slate"));
        command
            .env_clear()
            .env("HOME", td.path())
            .env("SLATE_HOME", td.path())
            .env("PATH", "")
            .env("NO_COLOR", "1")
            .stdin(Stdio::null())
            .args(["restore", &id])
            .stdout(stdout);
        let output = redirected_output::run(&mut command)
            .assert()
            .code(1)
            .get_output()
            .clone();
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(!error.is_empty() && !error.contains("panicked"));
        if broken_pipe {
            assert!(error.to_ascii_lowercase().contains("broken pipe"));
        }
        assert!(!error.contains("PRIVATE_FIXTURE_CONTENT"));
        assert_eq!(snapshot::tree(td.path()), before);
    }
}
