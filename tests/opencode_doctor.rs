//! Real read-only CLI reports in temporary profiles; no OpenCode is launched.
use predicates::prelude::PredicateBooleanExt;
use slate_cli::{config::ConfigWriteGuard, env::SlateEnv};
use std::fs;
use std::os::fd::OwnedFd;
use std::os::unix::{
    ffi::OsStringExt,
    fs::{symlink, MetadataExt, PermissionsExt},
    net::UnixStream,
};
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

#[path = "support/redirected_output.rs"]
mod redirected_output;
#[path = "support/tree.rs"]
mod tree_snapshot;

fn command(home: &Path, isolated: bool) -> assert_cmd::Command {
    let mut command = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    command
        .env_clear()
        .env("HOME", home)
        .env("PATH", home.join("bin"))
        .env("NO_COLOR", "1")
        .current_dir(home)
        .timeout(Duration::from_secs(5));
    if isolated {
        command.env("SLATE_HOME", home);
    }
    command
}

fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o640)).unwrap();
}

fn json(command: &mut assert_cmd::Command) -> serde_json::Value {
    let output = command
        .args(["doctor", "opencode", "--json"])
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("PRIVATE_CONTENT"));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["target"], "opencode");
    assert!(report["scope"]
        .as_str()
        .unwrap()
        .contains("Inline overrides"));
    assert!(report["scope"]
        .as_str()
        .unwrap()
        .contains("no tool is launched"));
    report
}

fn check<'a>(report: &'a serde_json::Value, code: &str) -> &'a serde_json::Value {
    report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["code"] == code)
        .unwrap_or_else(|| panic!("missing {code}: {report}"))
}

#[test]
fn opencode_doctor_explains_theme_states_without_values_or_writes() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join(".config/opencode/cli.json");
    for (input, code, status) in [
        (
            "// PRIVATE_CONTENT\n{\"theme\":{\"name\":\"slate-sync\"},\"n\":1.230e2,}",
            "slate_theme",
            "ok",
        ),
        (
            "{\"theme\":{\"name\":\"sla\\u0074e-sync\"}}",
            "slate_theme",
            "ok",
        ),
        (
            "{\"theme\":{\"name\":\"PRIVATE_CONTENT\"}}",
            "custom_theme",
            "warning",
        ),
        (
            "{/* \"theme\":{\"name\":\"slate-sync\"} PRIVATE_CONTENT */}",
            "unset_theme",
            "warning",
        ),
        ("{\"theme\":42}", "invalid_config", "error"),
        (
            "{\"theme\":{\"name\":\"slate-sync\"},\"theme\":{\"name\":\"PRIVATE_CONTENT\"}}",
            "invalid_config",
            "error",
        ),
        ("{} /* PRIVATE_CONTENT", "invalid_config", "error"),
        ("{\"x\":\"PRIVATE_CONTENT\n\"}", "invalid_config", "error"),
        ("[]", "invalid_config", "error"),
    ] {
        write(&path, input);
        let before = tree_snapshot::tree(home.path());
        let meta = fs::metadata(&path).unwrap();
        let report = json(&mut command(home.path(), true));
        assert_eq!(check(&report, code)["status"], status);
        assert_eq!(
            check(&report, "selected_config")["path"],
            path.to_str().unwrap()
        );
        if status == "error" {
            assert!(!report["checks"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["code"] == "slate_theme"));
        }
        command(home.path(), true)
            .args(["doctor", "opencode"])
            .assert()
            .success()
            .stdout(predicates::str::contains("PRIVATE_CONTENT").not());
        assert_eq!(tree_snapshot::tree(home.path()), before);
        let after = fs::metadata(&path).unwrap();
        assert_eq!(
            (after.ino(), after.modified().unwrap()),
            (meta.ino(), meta.modified().unwrap())
        );
    }
}

#[test]
fn opencode_doctor_uses_global_cli_settings_in_default_xdg_and_isolated_profiles() {
    for case in ["default", "xdg", "isolated"] {
        let home = tempfile::tempdir().unwrap();
        let external = home.path().join("custom-xdg");
        let root = if case == "xdg" {
            external.clone()
        } else {
            home.path().join(".config")
        };
        let path = root.join("opencode/cli.json");
        write(&path, r#"{"theme":{"name":"slate-sync","mode":"light"}}"#);
        let mut cmd = command(home.path(), case == "isolated");
        if case != "default" {
            cmd.env("XDG_CONFIG_HOME", &external);
        }
        let before = tree_snapshot::tree(home.path());
        let report = json(&mut cmd);
        assert_eq!(
            check(&report, "selected_config")["path"],
            path.to_str().unwrap()
        );
        assert_eq!(check(&report, "slate_theme")["status"], "ok");
        assert_eq!(tree_snapshot::tree(home.path()), before);
    }
}

#[test]
fn opencode_doctor_reports_unsafe_inputs_without_blocking_or_reading_fallbacks() {
    for case in [
        "utf8",
        "fifo",
        "link",
        "dangling",
        "directory",
        "large",
        "deep",
        "missing",
    ] {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join(".config/opencode/cli.json");
        // A valid fallback must not disguise an obstructed preferred entry.
        if case != "missing" {
            write(
                &path.with_extension("jsonc"),
                "{\"theme\":{\"name\":\"slate-sync\"}}",
            );
        }
        match case {
            "utf8" => write(&path, b"// PRIVATE_CONTENT\xff"),
            "fifo" => {
                let name = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
                assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
            }
            "link" | "dangling" => {
                let target = home.path().join("outside");
                if case == "link" {
                    write(
                        &target,
                        "{\"theme\":{\"name\":\"slate-sync\"},\"x\":\"PRIVATE_CONTENT\"}",
                    );
                }
                symlink(target, &path).unwrap();
            }
            "directory" => fs::create_dir(&path).unwrap(),
            "large" => fs::File::create(&path)
                .unwrap()
                .set_len(8 * 1024 * 1024 + 1)
                .unwrap(),
            "deep" => write(
                &path,
                format!("{{\"x\":{}0{}}}", "[".repeat(2000), "]".repeat(2000)),
            ),
            "missing" => {}
            _ => unreachable!(),
        }
        let before = tree_snapshot::tree(home.path());
        let report = json(&mut command(home.path(), true));
        let code = match case {
            "utf8" => "invalid_encoding",
            "deep" => "invalid_config",
            "missing" => "missing_config",
            _ => "unsafe_config",
        };
        check(&report, code);
        assert!(!report["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["code"] == "slate_theme"));
        assert_eq!(tree_snapshot::tree(home.path()), before);
    }
}

#[test]
fn integration_doctor_paths_are_terminal_safe_and_json_discloses_lossy_names() {
    // APFS rejects invalid UTF-8 names. Their serialization is tested in lib
    // tests on every host; Linux additionally exercises a real such filename.
    let names: &[&[u8]] = &[
        "terminal\u{1b}[31m\n\u{202e}".as_bytes(),
        #[cfg(target_os = "linux")]
        b"non-utf8-\xff",
    ];
    for raw in names {
        let td = tempfile::tempdir().unwrap();
        let home = td.path().join(std::ffi::OsString::from_vec(raw.to_vec()));
        fs::create_dir(&home).unwrap();
        let path = home.join(".config/opencode/cli.json");
        write(&path, "{\"theme\":{\"name\":\"slate-sync\"}}");
        let before = tree_snapshot::tree(td.path());
        let report = json(&mut command(&home, true));
        assert_eq!(
            check(&report, "selected_config")["path_is_lossy"],
            home.to_str().is_none()
        );
        for target in ["opencode", "kitty", "alacritty", "nvim", "zsh", "opacity"] {
            let out = command(&home, true)
                .args(["doctor", target])
                .assert()
                .success()
                .get_output()
                .stdout
                .clone();
            let text = String::from_utf8(out).unwrap();
            assert!(!text.contains('\u{1b}') && !text.contains('\u{202e}'));
            if home.to_str().is_some() {
                assert!(text.contains("\\n\\u{202e}"));
            } else {
                assert!(text.contains("lossy display; not an exact path"));
            }
        }
        assert_eq!(tree_snapshot::tree(td.path()), before);
    }
}

#[test]
fn integration_doctors_work_while_locked_or_pending_and_handle_closed_stdout() {
    use assert_cmd::assert::OutputAssertExt;
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let _guard = ConfigWriteGuard::acquire(&env).unwrap();
    write(
        &env.slate_cache_dir().join("preview-session.json"),
        "PRIVATE_CONTENT",
    );
    write(&env.managed_file("config.toml"), b"PRIVATE_CONTENT\xff");
    write(
        &env.xdg_config_home().join("opencode/cli.json"),
        "{\"theme\":{\"name\":\"slate-sync\"}}",
    );
    let executable = home.path().join("bin/opencode");
    write(
        &executable,
        "#!/bin/sh\n: > \"$HOME/tool-was-launched\"\nprintf 'UNEXPECTED TOOL EXECUTION' >&2\nexit 92\n",
    );
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    let before = tree_snapshot::tree(home.path());
    check(&json(&mut command(home.path(), true)), "slate_theme");
    for target in [
        "opencode",
        "kitty",
        "alacritty",
        "nvim",
        "zsh",
        "opacity",
        "btop",
        "starship",
    ] {
        for json in [false, true] {
            let (consumer, producer) = UnixStream::pair().unwrap();
            drop(consumer);
            let mut cmd = std::process::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
            cmd.env_clear()
                .env("HOME", home.path())
                .env("SLATE_HOME", home.path())
                .env("PATH", home.path().join("bin"))
                .args(["doctor", target])
                .stdout(Stdio::from(OwnedFd::from(producer)))
                .stderr(Stdio::piped());
            if json {
                cmd.arg("--json");
            }
            redirected_output::run(&mut cmd)
                .assert()
                .success()
                .stderr("");
        }
    }
    assert_eq!(tree_snapshot::tree(home.path()), before);
}
