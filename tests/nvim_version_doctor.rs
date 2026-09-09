//! Explicit version diagnostics against disposable executables only.
use slate_cli::{
    config::{ConfigManager, ConfigWriteGuard},
    env::SlateEnv,
};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::Duration,
};

#[path = "support/tree.rs"]
mod snapshot;

fn command(home: &Path, log: &Path) -> assert_cmd::Command {
    let mut command = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    command
        .env_clear()
        .env("HOME", home)
        .env("SLATE_HOME", home)
        .env("PATH", home.join("bin"))
        .env("NO_COLOR", "1")
        .env("SLATE_NVIM_PROBE_LOG", log)
        .current_dir(home)
        .timeout(Duration::from_secs(7));
    command
}

fn executable(home: &Path, body: &str, fallback: bool) -> PathBuf {
    let path = home.join(if fallback {
        ".local/bin/nvim"
    } else {
        "bin/nvim"
    });
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, format!(
        "#!/bin/sh\n[ \"$#\" = 1 ] && [ \"$1\" = --version ] || exit 91\nprintf 'probe\\n' >> \"$SLATE_NVIM_PROBE_LOG\"\n{body}\n"
    )).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn json(command: &mut assert_cmd::Command) -> serde_json::Value {
    let output = command.assert().success().get_output().clone();
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(!text.contains("PRIVATE_"), "{text}");
    serde_json::from_str(&text).unwrap()
}

#[test]
fn nvim_version_doctor_requires_opt_in_and_preserves_default_file_only_report() {
    let home = tempfile::tempdir().unwrap();
    let logs = tempfile::tempdir().unwrap();
    let log = logs.path().join("calls");
    executable(home.path(), "exit 99", false);
    let before = snapshot::tree(home.path());
    let report = json(command(home.path(), &log).args(["doctor", "nvim", "--json"]));
    assert_eq!(report["schema_version"], 1);
    assert!(report.get("version_probe").is_none());
    assert!(report["scope"]
        .as_str()
        .unwrap()
        .contains("no editor is launched"));
    command(home.path(), &log)
        .args(["doctor", "nvim"])
        .assert()
        .success();
    assert!(!log.exists(), "file-only doctor started an executable");
    assert_eq!(snapshot::tree(home.path()), before);
}

#[test]
fn nvim_version_doctor_reports_native_results_without_slate_writes() {
    for (body, status, version, reason) in [
        ("printf 'NVIM v0.12.0-dev+fixture\\nBuild type: PRIVATE_BUILD\\n'", "supported", Some("0.12.0-dev+fixture"), "meets the minimum"),
        ("printf 'NVIM v0.8.0-dev\\nLuaJIT 2.1.0\\n'", "unsupported", Some("0.8.0-dev"), "below the minimum"),
        ("printf 'NVIM PRIVATE_VERSION\\nLuaJIT 2.1.0\\n'", "failed", None, "complete semantic version"),
        ("printf 'NVIM v0.12.0\\n'; printf 'PRIVATE_STDERR' >&2; exit 9", "failed", None, "non-zero exit status"),
        ("printf 'NVIM v0.12.0\\n'; exec /bin/sleep 20", "failed", None, "timed out after 2000 ms"),
        ("printf 'NVIM v0.12.0\\n'; i=0; while [ $i -lt 6000 ]; do printf 'PRIVATE_NOISE'; i=$((i+1)); done", "failed", None, "65536-byte combined output limit"),
    ] {
        let home = tempfile::tempdir().unwrap();
        let logs = tempfile::tempdir().unwrap();
        let log = logs.path().join("calls");
        let binary = executable(home.path(), body, false);
        let env = SlateEnv::with_home(home.path().into());
        let config = ConfigManager::with_env(&env).unwrap();
        config.set_editor_auto_activation_enabled(false).unwrap();
        fs::write(env.nvim_init_path(), "-- PRIVATE_CONFIG: keep manual hook\n").unwrap();
        // Diagnostics remain available while another Slate writer owns the lock.
        let _guard = ConfigWriteGuard::acquire(&env).unwrap();
        let before = snapshot::tree(home.path());
        let report = json(command(home.path(), &log).args(["doctor", "nvim", "--check-version", "--json"]));
        let probe = &report["version_probe"];
        assert_eq!(probe["status"], status);
        assert_eq!(probe["binary"], binary.to_str().unwrap());
        assert_eq!(probe["binary_path_is_lossy"], false);
        assert_eq!(probe["in_path"], true);
        assert_eq!(probe["version"], serde_json::json!(version));
        assert_eq!(probe["minimum_version"], "0.8.0");
        assert_eq!(probe["timeout_ms"], 2000);
        assert_eq!(probe["max_output_bytes"], 65536);
        assert!(probe["message"].as_str().unwrap().contains(reason), "{probe}");
        let checks = report["checks"].as_array().unwrap();
        assert_eq!(checks.iter().filter(|check| check["code"] == "version_probe").count(), 1);
        assert!(checks.iter().any(|check| check["code"] == "auto_activation" && check["status"] == "info"));
        assert!(report["scope"].as_str().unwrap().contains("not sandboxed"));
        assert_eq!(fs::read_to_string(&log).unwrap(), "probe\n");
        assert_eq!(snapshot::tree(home.path()), before);
    }
}

#[test]
fn nvim_version_doctor_explains_fallback_binary_in_text_and_json() {
    let home = tempfile::Builder::new()
        .prefix("nvim 编辑器 ")
        .tempdir()
        .unwrap();
    let logs = tempfile::tempdir().unwrap();
    let log = logs.path().join("calls");
    let binary = executable(home.path(), "printf 'NVIM v0.8.0+fixture\\n'", true);
    // An unusable entry on PATH must not hide the executable fallback.
    let blocked = home.path().join("bin/nvim");
    fs::create_dir_all(blocked.parent().unwrap()).unwrap();
    fs::write(&blocked, "PRIVATE_BLOCKED").unwrap();
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o644)).unwrap();
    let before = snapshot::tree(home.path());
    let report =
        json(command(home.path(), &log).args(["doctor", "nvim", "--check-version", "--json"]));
    assert_eq!(report["version_probe"]["binary"], binary.to_str().unwrap());
    assert_eq!(report["version_probe"]["in_path"], false);
    let output = command(home.path(), &log)
        .args(["doctor", "nvim", "--check-version"])
        .assert()
        .success()
        .get_output()
        .clone();
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains(binary.to_str().unwrap()));
    assert!(text.contains("outside PATH"));
    assert!(text.contains("0.8.0+fixture"));
    assert!(!text.contains('\u{1b}'));
    assert_eq!(fs::read_to_string(log).unwrap(), "probe\nprobe\n");
    assert_eq!(snapshot::tree(home.path()), before);
}

#[test]
fn nvim_version_doctor_rejects_other_targets_before_probing_or_writing() {
    let home = tempfile::tempdir().unwrap();
    let logs = tempfile::tempdir().unwrap();
    let log = logs.path().join("calls");
    executable(home.path(), "exit 99", false);
    let before = snapshot::tree(home.path());
    for target in slate_cli::cli::doctor::TARGETS
        .into_iter()
        .filter(|target| *target != "nvim")
    {
        command(home.path(), &log)
            .args(["doctor", target, "--check-version", "--json"])
            .assert()
            .failure()
            .stderr(predicates::str::contains("only supported"));
    }
    command(home.path(), &log)
        .args(["doctor", "--check-version"])
        .assert()
        .failure();
    assert!(!log.exists());
    assert_eq!(snapshot::tree(home.path()), before);
}
