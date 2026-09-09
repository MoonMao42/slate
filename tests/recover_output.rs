//! Private recovery records and held fixture locks, never a host picker/watcher.
use assert_cmd::assert::OutputAssertExt;
use slate_cli::env::SlateEnv;
use std::{
    fs,
    os::{
        fd::{AsRawFd, OwnedFd},
        unix::{
            fs::PermissionsExt,
            net::{UnixDatagram, UnixStream},
        },
    },
    path::Path,
    process::{Command, Stdio},
    time::Duration,
};

#[path = "support/redirected_output.rs"]
mod redirected_output;
#[path = "support/tree.rs"]
mod snapshot;

fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}

fn fixture(home: &Path, state: &str) -> (SlateEnv, Option<fs::File>) {
    let env = SlateEnv::with_home(home.to_owned());
    if state == "absent" {
        return (env, None);
    }
    let target = env.managed_file("managed/ghostty/theme.conf");
    let lock_path = env.slate_cache_dir().join("preview-session.lock");
    let record_path = env.slate_cache_dir().join("preview-session.json");
    write(&lock_path, b"");
    if state != "active-empty" {
        let expected: &[u8] = b"PRIVATE_PREVIEW_CONTENT";
        write(
            &target,
            if state == "conflicted" {
                b"PRIVATE_EXTERNAL_EDIT"
            } else {
                expected
            },
        );
        let mode = fs::metadata(&target).unwrap().permissions().mode();
        let record = serde_json::json!({
            "version": 1, "session_id": "recover-output-fixture", "pid": std::process::id(),
            "home": env.home(), "config_dir": env.config_dir(), "cache_dir": env.slate_cache_dir(),
            "writing": state == "interrupted", "missing_dirs": [],
            "files": [{"path": target, "destination": fs::canonicalize(&target).unwrap(),
                "original": {"Present": {"bytes": b"PRIVATE_ORIGINAL_CONTENT".to_vec(), "mode": mode}}}],
            "expected": [{"Present": {"bytes": expected.to_vec(), "mode": mode}}],
        });
        write(
            &record_path,
            if state == "unreadable" {
                b"PRIVATE_CORRUPT_RECORD".to_vec()
            } else {
                serde_json::to_vec(&record).unwrap()
            },
        );
    }
    let lock = if state.starts_with("active") {
        let lock = fs::File::open(lock_path).unwrap();
        assert_eq!(
            unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
            0
        );
        Some(lock)
    } else {
        None
    };
    (env, lock)
}

fn process(home: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    command
        .env_clear()
        .env("HOME", home)
        .env("SLATE_HOME", home)
        .env("PATH", "")
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .args(args);
    command
}

fn closed_stdout() -> Stdio {
    let (reader, writer) = UnixStream::pair().unwrap();
    drop(reader);
    Stdio::from(OwnedFd::from(writer))
}

fn no_private(output: &std::process::Output) {
    for bytes in [&output.stdout, &output.stderr] {
        let text = String::from_utf8_lossy(bytes);
        assert!(
            !text.contains("PRIVATE_") && !text.contains("panicked"),
            "{text}"
        );
    }
}

#[test]
fn recover_output_readonly_keeps_conflicts_and_active_status_after_real_pipe_closure() {
    for state in [
        "absent",
        "pending",
        "interrupted",
        "conflicted",
        "active",
        "active-empty",
        "unreadable",
    ] {
        let td = tempfile::tempdir().unwrap();
        let (_env, _lock) = fixture(td.path(), state);
        let before = snapshot::tree(td.path());
        let code = i32::from(matches!(
            state,
            "conflicted" | "active" | "active-empty" | "unreadable"
        ));
        for json in [false, true] {
            let mut args = vec!["recover", "--dry-run"];
            if json {
                args.push("--json");
            }
            let output = assert_cmd::Command::from_std(process(td.path(), &args))
                .timeout(Duration::from_secs(5))
                .assert()
                .code(code)
                .get_output()
                .clone();
            no_private(&output);
            if json && state != "unreadable" {
                let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
                assert_eq!(report["active"], state.starts_with("active"));
                assert_eq!(
                    report["available"],
                    !matches!(state, "absent" | "active-empty")
                );
            }
            let mut command = process(td.path(), &args);
            command.stdout(closed_stdout());
            let output = redirected_output::run(&mut command)
                .assert()
                .code(code)
                .get_output()
                .clone();
            no_private(&output);
            if code == 0 {
                assert!(output.stderr.is_empty());
            } else {
                assert!(!output.stderr.is_empty());
            }
            let mut command = process(td.path(), &args);
            command.stdout(Stdio::from(OwnedFd::from(UnixDatagram::unbound().unwrap())));
            no_private(
                redirected_output::run(&mut command)
                    .assert()
                    .code(1)
                    .get_output(),
            );
        }
        assert_eq!(snapshot::tree(td.path()), before, "{state}");
    }
}

#[test]
fn recover_output_required_plan_failure_preserves_records_and_prevents_mutating_actions() {
    for state in ["pending", "conflicted", "unreadable"] {
        for action in ["recover", "discard", "export"] {
            let td = tempfile::tempdir().unwrap();
            let (_env, _lock) = fixture(td.path(), state);
            let export = td.path().join("originals");
            let args = match action {
                "recover" => vec!["recover", "--yes"],
                "discard" => vec!["recover", "--discard", "--yes"],
                _ => vec!["recover", "--export", export.to_str().unwrap()],
            };
            let before = snapshot::tree(td.path());
            for stdout in [
                closed_stdout(),
                Stdio::from(OwnedFd::from(UnixDatagram::unbound().unwrap())),
            ] {
                let mut command = process(td.path(), &args);
                command.stdout(stdout);
                no_private(
                    redirected_output::run(&mut command)
                        .assert()
                        .code(1)
                        .get_output(),
                );
                assert_eq!(snapshot::tree(td.path()), before, "{state}/{action}");
            }
        }
    }
}

#[test]
fn recover_output_text_escapes_paths_without_changing_json_or_successful_actions() {
    for action in ["inspect", "recover", "discard", "discard-corrupt", "export"] {
        let td = tempfile::tempdir().unwrap();
        let home = td.path().join("profile\n\x1b[31m\u{202e}中文");
        fs::create_dir(&home).unwrap();
        let (env, _lock) = fixture(
            &home,
            if action == "discard-corrupt" {
                "unreadable"
            } else {
                "pending"
            },
        );
        let target = env.managed_file("managed/ghostty/theme.conf");
        let record_path = env.slate_cache_dir().join("preview-session.json");
        let before = snapshot::tree(td.path());
        let export = home.join("originals");
        let args = match action {
            "inspect" => vec!["recover", "--dry-run"],
            "recover" => vec!["recover", "--yes"],
            "discard" | "discard-corrupt" => vec!["recover", "--discard", "--yes"],
            _ => vec!["recover", "--export", export.to_str().unwrap()],
        };
        let output = assert_cmd::Command::from_std(process(&home, &args))
            .timeout(Duration::from_secs(5))
            .assert()
            .success()
            .get_output()
            .clone();
        no_private(&output);
        for bytes in [&output.stdout, &output.stderr] {
            let text = String::from_utf8_lossy(bytes);
            assert!(!text.contains('\x1b') && !text.contains('\u{202e}'));
        }
        if action == "inspect" {
            let output =
                assert_cmd::Command::from_std(process(&home, &["recover", "--dry-run", "--json"]))
                    .timeout(Duration::from_secs(5))
                    .assert()
                    .success()
                    .get_output()
                    .clone();
            let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(report["record_path"], record_path.to_str().unwrap());
            assert_eq!(report["changes"][0]["path"], target.to_str().unwrap());
            assert_eq!(snapshot::tree(td.path()), before);
        } else if action == "recover" {
            assert_eq!(fs::read(&target).unwrap(), b"PRIVATE_ORIGINAL_CONTENT");
            assert!(!record_path.exists());
        } else if action.starts_with("discard") {
            assert_eq!(fs::read(&target).unwrap(), b"PRIVATE_PREVIEW_CONTENT");
            assert!(!record_path.exists());
        } else {
            assert_eq!(
                fs::read(export.join("00.original")).unwrap(),
                b"PRIVATE_ORIGINAL_CONTENT"
            );
            assert!(record_path.exists());
            assert_eq!(fs::read(&target).unwrap(), b"PRIVATE_PREVIEW_CONTENT");
        }
    }
}
