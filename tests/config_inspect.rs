//! Real CLI reads and rejected writes in disposable profiles only.
use serde_json::{json, Value};
use slate_cli::{
    config::{ConfigManager, ConfigWriteGuard},
    env::SlateEnv,
};
use std::{
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};

#[path = "support/redirected_output.rs"]
mod redirected_output;
#[path = "support/tree.rs"]
mod snapshot;

fn command(home: &Path) -> assert_cmd::Command {
    let mut command = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    command
        .env_clear()
        .env("HOME", home)
        .env("SLATE_HOME", home)
        .env("PATH", home.join("bin"))
        .timeout(Duration::from_secs(5));
    command
}

fn read(home: &Path, args: &[&str]) -> Value {
    let output = command(home)
        .args(args)
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(!String::from_utf8_lossy(&output.stdout).contains("PRIVATE_CONTENT"));
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn entry<'a>(report: &'a Value, key: &str) -> &'a Value {
    report["settings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["key"] == key)
        .unwrap()
}

#[test]
fn config_inspect_defaults_and_single_reads_create_nothing() {
    let td = tempfile::tempdir().unwrap();
    let home = td.path().join("absent home");
    let report = read(&home, &["config", "list", "--json"]);
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["settings"].as_array().unwrap().len(), 5);
    for (key, expected) in [
        ("opacity", Value::Null),
        ("auto-theme", json!(false)),
        ("fastfetch", json!(false)),
        ("sound", json!(true)),
        ("editor", json!(true)),
    ] {
        let setting = entry(&report, key);
        assert_eq!(setting["value"], expected);
        assert_eq!(
            setting["status"],
            if key == "opacity" { "unset" } else { "ok" }
        );
        let single = read(&home, &["config", "get", key, "--json"]);
        assert_eq!(single["settings"], json!([setting]));
        assert_eq!(single["scope"], report["scope"]);
    }
    command(&home)
        .args(["config", "list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("opacity: unset\n"))
        .stdout(predicates::str::contains("sound: on\n"))
        .stdout(predicates::str::contains(
            "not proof that running tools applied them",
        ));
    assert!(!home.exists());
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

#[test]
fn config_inspect_retains_partial_values_and_stays_available_under_writer_lock() {
    let td = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(td.path().into());
    let _guard = ConfigWriteGuard::acquire(&env).unwrap();
    write(
        &env.managed_file("config.toml"),
        "[auto_theme]\nenabled=true\n[preferences]\nsound='PRIVATE_CONTENT'\n",
    );
    write(&env.managed_file("current-opacity"), "PRIVATE_CONTENT");
    write(&env.managed_file("autorun-fastfetch"), "");
    write(&env.nvim_auto_activation_path(), "disabled\n");
    write(
        &env.slate_cache_dir().join("preview-session.json"),
        "PRIVATE_CONTENT",
    );
    for name in ["nvim", "pgrep", "starship", "osascript", "fastfetch"] {
        let stub = env.home().join("bin").join(name);
        write(
            &stub,
            "#!/bin/sh\nprintf launch > \"$HOME/unexpected-launch\"\nexit 93\n",
        );
        fs::set_permissions(stub, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let before = snapshot::tree(td.path());
    let report = read(td.path(), &["config", "list", "--json"]);
    for key in ["opacity", "sound"] {
        assert_eq!(entry(&report, key)["status"], "error");
        assert!(entry(&report, key)["value"].is_null());
        assert!(entry(&report, key)["issue"].is_string());
    }
    for (key, value) in [("auto-theme", true), ("fastfetch", true), ("editor", false)] {
        assert_eq!(entry(&report, key)["value"], value);
    }
    let output = command(td.path())
        .args(["config", "list"])
        .assert()
        .success()
        .get_output()
        .clone();
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(!text.contains("PRIVATE_CONTENT"));
    assert!(text.contains("sound: error") && text.contains("auto-theme: enable"));
    let single = read(td.path(), &["config", "get", "editor", "--json"]);
    assert_eq!(single["settings"], json!([entry(&report, "editor")]));
    command(td.path())
        .args(["config", "set", "sound", "invalid"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("Invalid sound value/action"));
    assert_eq!(snapshot::tree(td.path()), before);
}

fn path(env: &SlateEnv, key: &str) -> PathBuf {
    match key {
        "sound" => env.managed_file("config.toml"),
        "opacity" => env.managed_file("current-opacity"),
        "fastfetch" => env.managed_file("autorun-fastfetch"),
        "editor" => env.nvim_auto_activation_path(),
        _ => unreachable!(),
    }
}

#[test]
fn config_inspect_rejects_unsafe_sources_and_fastfetch_never_confuses_them_with_flags() {
    for key in ["sound", "opacity", "fastfetch", "editor"] {
        for kind in ["fifo", "directory", "oversized", "symlink"] {
            let td = tempfile::tempdir().unwrap();
            let env = SlateEnv::with_home(td.path().into());
            let config = ConfigManager::with_env(&env).unwrap();
            let path = path(&env, key);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            match kind {
                "fifo" => {
                    let name = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
                    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
                }
                "directory" => fs::create_dir(&path).unwrap(),
                "oversized" => fs::File::create(&path)
                    .unwrap()
                    .set_len(if key == "sound" { 256 * 1024 + 1 } else { 4097 })
                    .unwrap(),
                "symlink" => symlink(td.path().join("absent"), &path).unwrap(),
                _ => unreachable!(),
            }
            let before = snapshot::tree(td.path());
            let report = read(td.path(), &["config", "get", key, "--json"]);
            assert_eq!(entry(&report, key)["status"], "error", "{key}/{kind}");
            assert!(entry(&report, key)["value"].is_null());
            if key == "fastfetch" {
                assert!(config.has_fastfetch_autorun().is_err());
            }
            assert_eq!(snapshot::tree(td.path()), before);
        }
    }
    let td = tempfile::tempdir().unwrap();
    let home = td.path().join("home");
    let external = td.path().join("external");
    fs::create_dir(&home).unwrap();
    fs::create_dir(&external).unwrap();
    symlink(&external, home.join(".config")).unwrap();
    write(
        &external.join("slate/config.toml"),
        "[preferences]\nsound=false\n",
    );
    let before = snapshot::tree(td.path());
    let report = read(&home, &["config", "get", "sound", "--json"]);
    assert_eq!(entry(&report, "sound")["status"], "error");
    assert_eq!(snapshot::tree(td.path()), before);
}

#[test]
fn config_inspect_invalid_requests_are_bounded_escaped_and_profile_independent() {
    for args in [
        vec!["config", "get", "\x1b[31munknown\n"],
        vec!["config", "set", "sound", "\x1b[31mnope\n"],
        vec!["config", "set", "unknown", "on"],
    ] {
        let output = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"))
            .env_clear()
            .args(args)
            .timeout(Duration::from_secs(5))
            .assert()
            .failure()
            .get_output()
            .clone();
        let text = String::from_utf8(output.stderr).unwrap();
        assert!(!text.contains('\x1b'));
        assert!(!text.contains("HOME not set"));
        assert!(text.contains("Unknown config key") || text.contains("Invalid sound"));
    }
    let long = "x".repeat(4000);
    let error = slate_cli::cli::config::validate_set("editor", &long)
        .unwrap_err()
        .to_string();
    assert!(error.len() < 300 && error.contains("truncated"));
    for setting in &slate_cli::cli::config::SETTINGS {
        for value in setting.set_values {
            slate_cli::cli::config::validate_set(setting.key, value).unwrap();
        }
    }
}

#[test]
fn config_inspect_closed_stdout_is_normal_and_control_paths_are_escaped() {
    use assert_cmd::assert::OutputAssertExt;
    use std::{
        os::{fd::OwnedFd, unix::net::UnixStream},
        process::Stdio,
    };
    let td = tempfile::tempdir().unwrap();
    let home = td.path().join("control\n\x1b[31m");
    fs::create_dir(&home).unwrap();
    let before = snapshot::tree(td.path());
    let output = command(&home)
        .args(["config", "get", "sound"])
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(!output.stdout.contains(&0x1b));
    assert!(String::from_utf8(output.stdout)
        .unwrap()
        .contains("\\n\\u{1b}[31m"));
    let (consumer, producer) = UnixStream::pair().unwrap();
    drop(consumer);
    let mut process = std::process::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    process
        .env_clear()
        .env("HOME", &home)
        .env("SLATE_HOME", &home)
        .env("PATH", "")
        .args(["config", "list", "--json"])
        .stdout(Stdio::from(OwnedFd::from(producer)))
        .stderr(Stdio::piped());
    redirected_output::run(&mut process)
        .assert()
        .success()
        .stderr("");
    assert_eq!(snapshot::tree(td.path()), before);
    use std::os::unix::ffi::OsStringExt;
    let non_utf8 = td
        .path()
        .join(std::ffi::OsString::from_vec(b"profile-\xff".to_vec()));
    let report = read(&non_utf8, &["config", "get", "sound", "--json"]);
    assert_eq!(entry(&report, "sound")["path_is_lossy"], true);
    assert_eq!(snapshot::tree(td.path()), before);
}

#[test]
fn config_inspect_set_then_get_reports_the_new_value_and_preserves_other_preferences() {
    let td = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(td.path().into());
    write(
        &env.managed_file("config.toml"),
        "# PRIVATE_CONTENT\n[auto_theme]\nenabled=false\n[preferences]\nsound=true\ncustom=42\n",
    );
    for (action, expected) in [("off", false), ("on", true)] {
        command(td.path())
            .args(["--quiet", "config", "set", "sound", action])
            .assert()
            .success();
        let before = snapshot::tree(td.path());
        let report = read(td.path(), &["config", "get", "sound", "--json"]);
        assert_eq!(entry(&report, "sound")["value"], expected);
        assert_eq!(entry(&report, "sound")["status"], "ok");
        assert_eq!(snapshot::tree(td.path()), before);
        let content = fs::read_to_string(env.managed_file("config.toml")).unwrap();
        assert!(
            content.contains("# PRIVATE_CONTENT")
                && content.contains("custom=42")
                && content.contains("enabled=false")
        );
    }
}
