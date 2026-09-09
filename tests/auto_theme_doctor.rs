//! Read-only CLI contracts. Fixtures contain simulated control records; real
//! lifecycle ownership is separately checked by the watcher subprocess tests.
use slate_cli::env::SlateEnv;
use slate_cli::platform::dark_mode_notify::RuntimeInspection;
use std::collections::BTreeMap;
use std::fs;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{symlink, DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[path = "auto_theme_doctor/output.rs"]
mod output;
#[path = "auto_theme_doctor/preference.rs"]
mod preference;
#[path = "auto_theme_doctor/resolution.rs"]
mod resolution;

fn command(home: &Path) -> assert_cmd::Command {
    let mut command = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    command
        .env_clear()
        .env("HOME", home)
        .env("SLATE_HOME", home)
        .env("PATH", home.join("bin"))
        .env("NO_COLOR", "1")
        .timeout(Duration::from_secs(4));
    command
}

fn write(path: &Path, content: impl AsRef<[u8]>, mode: u32) {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path.parent().unwrap())
        .unwrap();
    fs::write(path, content).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

fn tree(root: &Path) -> BTreeMap<PathBuf, (u32, Vec<u8>)> {
    fn visit(root: &Path, path: &Path, out: &mut BTreeMap<PathBuf, (u32, Vec<u8>)>) {
        let meta = fs::symlink_metadata(path).unwrap();
        let bytes = if meta.is_file() {
            fs::read(path).unwrap()
        } else if meta.file_type().is_symlink() {
            fs::read_link(path)
                .unwrap()
                .as_os_str()
                .as_encoded_bytes()
                .to_vec()
        } else {
            Vec::new()
        };
        out.insert(
            path.strip_prefix(root).unwrap().to_owned(),
            (meta.permissions().mode(), bytes),
        );
        if meta.is_dir() {
            for entry in fs::read_dir(path).unwrap() {
                visit(root, &entry.unwrap().path(), out);
            }
        }
    }
    let mut entries = BTreeMap::new();
    visit(root, root, &mut entries);
    entries
}

fn inspect(home: &Path) -> serde_json::Value {
    let before = tree(home);
    let output = command(home)
        .args(["doctor", "auto-theme", "--json"])
        .assert()
        .success()
        .get_output()
        .clone();
    assert_eq!(tree(home), before, "doctor changed the fixture tree");
    assert!(output.stderr.is_empty(), "{:?}", output.stderr);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("PRIVATE_FIXTURE_CONTENT"));
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn auto_theme_doctor_reports_fresh_current_legacy_and_changed_launchers_without_writes() {
    let td = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(td.path().to_owned());
    let fresh = inspect(td.path());
    assert_eq!(fresh["target"], "auto-theme");
    assert_eq!(fresh["auto_theme_enabled"], false);
    assert_eq!(fresh["runtime"]["state"], "absent");
    assert_eq!(fresh["installation"]["launcher"]["state"], "missing");
    assert_eq!(fs::read_dir(td.path()).unwrap().count(), 0);
    // Scoped CLI setup writes only this fixture and cannot launch a desktop
    // watcher under SLATE_HOME. Empty PATH is an additional safeguard.
    command(td.path())
        .args(["config", "set", "auto-theme", "enable"])
        .assert()
        .success();
    let current = inspect(td.path());
    assert_eq!(current["installation"]["launcher"]["state"], "current");
    assert_eq!(current["runtime"]["state"], "absent");
    let launcher = env.config_dir().join("managed/bin/slate-dark-mode-notify");
    let original = fs::read(&launcher).unwrap();
    fs::write(
        &launcher,
        [original, b"# PRIVATE_FIXTURE_CONTENT\n".to_vec()].concat(),
    )
    .unwrap();
    assert_eq!(
        inspect(td.path())["installation"]["launcher"]["state"],
        "outdated"
    );
    write(&launcher, b"\xcf\xfa\xed\xfePRIVATE_FIXTURE_CONTENT", 0o755);
    assert_eq!(
        inspect(td.path())["installation"]["launcher"]["state"],
        "legacy"
    );
    write(
        &launcher,
        "#!/bin/sh\n# PRIVATE_FIXTURE_CONTENT\nexit 0\n",
        0o755,
    );
    assert_eq!(
        inspect(td.path())["installation"]["launcher"]["state"],
        "unrecognized"
    );
    let before = tree(td.path());
    let output = command(td.path())
        .args(["doctor", "auto-theme"])
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(!String::from_utf8_lossy(&output.stdout).contains("PRIVATE_FIXTURE_CONTENT"));
    assert_eq!(tree(td.path()), before);
}

#[test]
fn auto_theme_doctor_distinguishes_runtime_states_and_never_reads_logs_or_exposes_tokens() {
    let td = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(td.path().to_owned());
    let runtime = RuntimeInspection::inspect(&env).directory.unwrap();
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&runtime)
        .unwrap();
    let token =
        uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, b"private test instance").to_string();
    let instance = serde_json::json!({ "version": 1, "profile": runtime.file_name().unwrap().to_str().unwrap(), "token": token });
    write(
        &runtime.join("instance.json"),
        serde_json::to_vec(&instance).unwrap(),
        0o600,
    );
    assert_eq!(inspect(td.path())["runtime"]["state"], "stale");
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(runtime.join("instance.lock"))
        .unwrap();
    assert_eq!(
        unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
        0
    );
    assert_eq!(inspect(td.path())["runtime"]["state"], "starting");
    write(&runtime.join("ready"), token.as_bytes(), 0o600);
    assert_eq!(inspect(td.path())["runtime"]["state"], "ready");
    // A ready lifetime-lock fixture must not hide an unusable choice. No real
    // watcher is started; readiness here is only simulated control-file evidence.
    write(
        &env.managed_file("auto.toml"),
        "dark_theme='PRIVATE_FIXTURE_CONTENT'\nlight_theme='catppuccin-latte'\n",
        0o600,
    );
    let report = inspect(td.path());
    assert_eq!(report["runtime"]["state"], "ready");
    assert_eq!(report["resolution"]["dark"]["status"], "error");
    assert_eq!(
        report["resolution"]["light"]["theme_id"],
        "catppuccin-latte"
    );
    assert!(report["issues"].as_array().unwrap().iter().any(|issue| {
        issue
            .as_str()
            .unwrap()
            .contains("Conditional automatic selection")
    }));
    // A FIFO log would block if doctor tried to open it; the path is reported
    // but no log bytes are read, even when the runtime has failed.
    let log =
        std::ffi::CString::new(runtime.join("watcher.log").as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(log.as_ptr(), 0o600) }, 0);
    write(&runtime.join("stop"), token.as_bytes(), 0o600);
    let report = inspect(td.path());
    assert_eq!(report["runtime"]["state"], "stopping");
    assert!(!serde_json::to_string(&report).unwrap().contains(&token));
    drop(lock);
    for outcome in ["failed", "stopped"] {
        write(
            &runtime.join("exit.json"),
            serde_json::to_vec(&serde_json::json!({ "instance": instance, "outcome": outcome }))
                .unwrap(),
            0o600,
        );
        assert_eq!(inspect(td.path())["runtime"]["state"], outcome);
    }
    write(
        &runtime.join("instance.json"),
        b"PRIVATE_FIXTURE_CONTENT not json",
        0o600,
    );
    assert_eq!(inspect(td.path())["runtime"]["state"], "unreadable");
}

#[test]
fn auto_theme_doctor_handles_links_fifos_and_invalid_preferences_without_blocking() {
    let td = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(td.path().to_owned());
    let config = env.managed_file("config.toml");
    write(
        &config,
        "[auto_theme]\nenabled = 'PRIVATE_FIXTURE_CONTENT'\n",
        0o600,
    );
    assert!(inspect(td.path())["auto_theme_enabled"].is_null());
    fs::remove_file(&config).unwrap();
    let fifo = std::ffi::CString::new(config.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
    assert!(inspect(td.path())["auto_theme_enabled"].is_null());
    let runtime = RuntimeInspection::inspect(&env).directory.unwrap();
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&runtime)
        .unwrap();
    let fifo = std::ffi::CString::new(runtime.join("instance.json").as_os_str().as_encoded_bytes())
        .unwrap();
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
    assert_eq!(inspect(td.path())["runtime"]["state"], "unreadable");
    let launcher = env.config_dir().join("managed/bin/slate-dark-mode-notify");
    fs::create_dir_all(launcher.parent().unwrap()).unwrap();
    symlink(&config, launcher).unwrap();
    assert_eq!(
        inspect(td.path())["installation"]["launcher"]["state"],
        "unsafe"
    );
}
