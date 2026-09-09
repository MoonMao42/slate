use super::*;
use assert_cmd::assert::OutputAssertExt;
use std::{
    ffi::OsString,
    os::{
        fd::OwnedFd,
        unix::{
            ffi::OsStringExt,
            net::{UnixDatagram, UnixStream},
        },
    },
    process::{Command, Stdio},
};

#[path = "../support/redirected_output.rs"]
mod redirected_output;

fn redirected(home: &Path, target: &str, json: bool, stdout: Stdio) -> std::process::Output {
    let mut process = Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    process
        .env_clear()
        .env("HOME", home)
        .env("SLATE_HOME", home)
        .env("PATH", "")
        .env("NO_COLOR", "1")
        .args(["doctor", target])
        .stdout(stdout)
        .stderr(Stdio::piped());
    if json {
        process.arg("--json");
    }
    redirected_output::run(&mut process)
}

#[test]
fn auto_theme_doctor_output_treats_closed_consumers_normally_but_preserves_other_write_errors() {
    let td = tempfile::tempdir().unwrap();
    let home = td.path().join("absent-profile");
    let before = tree(td.path());
    for target in ["auto-theme", "ghostty", "fish"] {
        for json in [false, true] {
            let (consumer, producer) = UnixStream::pair().unwrap();
            drop(consumer);
            redirected(&home, target, json, Stdio::from(OwnedFd::from(producer)))
                .assert()
                .success()
                .stderr("");
            // An unconnected datagram has no destination, giving a real write
            // error distinct from BrokenPipe or EBADF (which Rust stdio may ignore).
            let output = redirected(
                &home,
                target,
                json,
                Stdio::from(OwnedFd::from(UnixDatagram::unbound().unwrap())),
            )
            .assert()
            .code(1)
            .get_output()
            .clone();
            let error = String::from_utf8_lossy(&output.stderr);
            assert!(!error.is_empty());
            assert!(!error.contains("panicked") && !error.contains("PRIVATE_FIXTURE_CONTENT"));
        }
    }
    assert_eq!(tree(td.path()), before);
}

#[test]
fn auto_theme_doctor_output_escapes_control_paths_in_text_and_keeps_exact_utf8_in_json() {
    let td = tempfile::tempdir().unwrap();
    let home = td.path().join("profile\n\r\t\x1b[31m\u{202e}\u{2066}中文");
    fs::create_dir(&home).unwrap();
    let env = SlateEnv::with_home(home.clone());
    let before = tree(td.path());
    let output = command(&home)
        .args(["doctor", "auto-theme"])
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(output.stderr.is_empty());
    let text = String::from_utf8(output.stdout).unwrap();
    for c in ['\r', '\t', '\x1b', '\u{202e}', '\u{2066}'] {
        assert!(!text.contains(c));
    }
    assert!(!text.contains("profile\n"));
    assert!(text.contains("profile\\n\\r\\t\\u{1b}[31m\\u{202e}\\u{2066}中文"));
    let report = inspect(&home);
    let launcher = &report["installation"]["launcher"];
    assert_eq!(
        launcher["path"],
        env.managed_file("managed/bin/slate-dark-mode-notify")
            .to_str()
            .unwrap()
    );
    assert_eq!(launcher["path_is_lossy"], false);
    let runtime = RuntimeInspection::inspect(&env);
    assert_eq!(
        report["runtime"]["directory"],
        runtime.directory.unwrap().to_str().unwrap()
    );
    assert_eq!(report["runtime"]["directory_is_lossy"], false);
    assert_eq!(report["runtime"]["log_path_is_lossy"], false);
    assert_eq!(tree(td.path()), before);
}

#[test]
fn auto_theme_doctor_output_reports_lossy_paths_without_losing_json_or_inventing_runtime_paths() {
    for blocked in [false, true] {
        let td = tempfile::tempdir().unwrap();
        let parent = if blocked {
            let file = td.path().join("not-a-directory");
            fs::write(&file, "PRIVATE_FIXTURE_CONTENT").unwrap();
            file
        } else {
            td.path().to_owned()
        };
        // APFS rejects creating these names; inspect a supplied path without
        // creating it. Runtime paths may be unavailable on such filesystems.
        let home = parent.join(OsString::from_vec(b"profile-\xff".to_vec()));
        let before = tree(td.path());
        let output = command(&home)
            .args(["doctor", "auto-theme", "--json"])
            .assert()
            .success()
            .get_output()
            .clone();
        assert!(output.stderr.is_empty());
        assert!(!String::from_utf8_lossy(&output.stdout).contains("PRIVATE_FIXTURE_CONTENT"));
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let runtime = RuntimeInspection::inspect(&SlateEnv::with_home(home.clone()));
        assert_eq!(report["schema_version"], 1);
        let launcher = &report["installation"]["launcher"];
        assert_eq!(launcher["path_is_lossy"], true);
        assert!(launcher["path"]
            .as_str()
            .unwrap()
            .contains("profile-\u{fffd}"));
        if let Some(helper) = report["installation"]["helper"].as_object() {
            assert_eq!(helper["path_is_lossy"], true);
        }
        for (path, lossy) in [
            ("directory", "directory_is_lossy"),
            ("log_path", "log_path_is_lossy"),
        ] {
            if runtime.directory.is_none() {
                assert!(report["runtime"][path].is_null());
                assert!(report["runtime"][lossy].is_null());
            } else {
                assert!(report["runtime"][path]
                    .as_str()
                    .unwrap()
                    .contains("profile-\u{fffd}"));
                assert_eq!(report["runtime"][lossy], true);
            }
        }
        let output = command(&home)
            .args(["doctor", "auto-theme"])
            .assert()
            .success()
            .get_output()
            .clone();
        assert!(output.stderr.is_empty());
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains(" (lossy display; not an exact path)"));
        assert!(!text.contains("PRIVATE_FIXTURE_CONTENT"));
        assert_eq!(tree(td.path()), before);
    }
}
