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
        .contains("Project configuration"));
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
    let path = home.path().join(".config/opencode/tui.jsonc");
    for (input, code, status) in [
        (
            "// PRIVATE_CONTENT\n{\"theme\":\"system\",\"n\":1.230e2,}",
            "system_theme",
            "ok",
        ),
        ("{\"theme\":\"sys\\u0074em\"}", "system_theme", "ok"),
        ("{\"theme\":\"PRIVATE_CONTENT\"}", "custom_theme", "warning"),
        (
            "{/* \"theme\":\"system\" PRIVATE_CONTENT */}",
            "unset_theme",
            "warning",
        ),
        ("{\"theme\":42}", "invalid_config", "error"),
        (
            "{\"theme\":\"system\",\"theme\":\"PRIVATE_CONTENT\"}",
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
                .any(|c| c["code"] == "system_theme"));
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
fn opencode_doctor_shares_adapter_paths_but_does_not_claim_runtime_precedence() {
    for case in [
        "json", "jsonc", "both", "xdg", "override", "relative", "isolated",
    ] {
        let home = tempfile::tempdir().unwrap();
        let root = if case == "xdg" {
            home.path().join("custom-xdg")
        } else {
            home.path().join(".config")
        };
        let json_path = root.join("opencode/tui.json");
        let jsonc_path = root.join("opencode/tui.jsonc");
        let custom = home.path().join("custom/tui.jsonc");
        let mut cmd = command(home.path(), case == "isolated");
        if case == "xdg" {
            cmd.env("XDG_CONFIG_HOME", &root);
        }
        let expected = match case {
            "jsonc" => {
                write(&jsonc_path, "{\"theme\":\"system\"}");
                &jsonc_path
            }
            "override" | "relative" => {
                write(&custom, "{\"theme\":\"system\"}");
                write(&json_path, "{\"theme\":\"PRIVATE_CONTENT\"}");
                if case == "relative" {
                    cmd.env("OPENCODE_TUI_CONFIG", "custom/tui.jsonc");
                } else {
                    cmd.env("OPENCODE_TUI_CONFIG", &custom);
                }
                &custom
            }
            _ => {
                write(&json_path, "{\"theme\":\"system\"}");
                if case == "both" {
                    write(&jsonc_path, "{\"theme\":\"PRIVATE_CONTENT\"}");
                }
                if case == "isolated" {
                    write(&custom, "{\"theme\":\"PRIVATE_CONTENT\"}");
                    cmd.env("OPENCODE_TUI_CONFIG", &custom);
                }
                &json_path
            }
        };
        // A project config is not read or merged by Slate's doctor.
        write(
            &home.path().join("tui.json"),
            "{\"theme\":\"PRIVATE_CONTENT\"}",
        );
        let before = tree_snapshot::tree(home.path());
        let report = json(&mut cmd);
        let selected = check(&report, "selected_config")["path"].as_str().unwrap();
        let expected_path = if case == "relative" {
            fs::canonicalize(home.path())
                .unwrap()
                .join("custom/tui.jsonc")
        } else {
            expected.to_owned()
        };
        assert_eq!(home.path().join(selected), expected_path);
        assert_eq!(check(&report, "system_theme")["status"], "ok");
        if matches!(case, "both" | "override" | "relative") {
            assert_eq!(check(&report, "alternate_config")["status"], "warning");
        }
        if case == "isolated" {
            check(&report, "isolated_profile");
            assert!(!report.to_string().contains(custom.to_str().unwrap()));
        }
        if case == "relative" {
            assert!(Path::new(selected).is_absolute());
            assert_eq!(check(&report, "relative_config")["status"], "info");
        }
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
        let path = home.path().join(".config/opencode/tui.json");
        // A valid fallback must not disguise an obstructed preferred entry.
        if case != "missing" {
            write(&path.with_extension("jsonc"), "{\"theme\":\"system\"}");
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
                    write(&target, "{\"theme\":\"system\",\"x\":\"PRIVATE_CONTENT\"}");
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
            .any(|c| c["code"] == "system_theme"));
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
        let path = home.join(".config/opencode/tui.json");
        write(&path, "{\"theme\":\"system\"}");
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
        &env.xdg_config_home().join("opencode/tui.json"),
        "{\"theme\":\"system\"}",
    );
    let executable = home.path().join("bin/opencode");
    write(
        &executable,
        "#!/bin/sh\n: > \"$HOME/tool-was-launched\"\nprintf 'UNEXPECTED TOOL EXECUTION' >&2\nexit 92\n",
    );
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    let before = tree_snapshot::tree(home.path());
    check(&json(&mut command(home.path(), true)), "system_theme");
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
