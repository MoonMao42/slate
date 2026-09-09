//! Real filesystem cleanup failures under private HOME/SLATE_HOME, not host state.
use slate_cli::env::SlateEnv;
use std::fs::{self, File};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn private_write(path: &Path, bytes: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}

fn fixture(home: &Path) -> (SlateEnv, PathBuf, PathBuf) {
    let env = SlateEnv::with_home(home.to_owned());
    let target = env.managed_file("managed/ghostty/theme.conf");
    let record = env.slate_cache_dir().join("preview-session.json");
    private_write(&target, b"PRIVATE_PREVIEW");
    // A preview may have a different mode than the saved private original.
    fs::set_permissions(&target, fs::Permissions::from_mode(0o644)).unwrap();
    let current_mode = fs::metadata(&target).unwrap().permissions().mode();
    private_write(&env.slate_cache_dir().join("preview-session.lock"), b"");
    let value = serde_json::json!({
        "version": 1, "session_id": "cleanup-fixture", "pid": std::process::id(),
        "home": env.home(), "config_dir": env.config_dir(), "cache_dir": env.slate_cache_dir(),
        "writing": false, "missing_dirs": [],
        "files": [{"path": target, "destination": fs::canonicalize(&target).unwrap(),
            "original": {"Present": {"bytes": b"PRIVATE_ORIGINAL".to_vec(), "mode": (current_mode & !0o777) | 0o600}}}],
        "expected": [{"Present": {"bytes": b"PRIVATE_PREVIEW".to_vec(), "mode": current_mode}}],
    });
    private_write(&record, serde_json::to_vec(&value).unwrap());
    (env, target, record)
}

struct DirectoryMode(PathBuf);

impl DirectoryMode {
    fn set(path: &Path, mode: u32) -> Self {
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
        Self(path.to_owned())
    }
}

impl Drop for DirectoryMode {
    fn drop(&mut self) {
        // Let TempDir clean up even when a test assertion fails.
        let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o700));
    }
}

fn command(home: &Path) -> assert_cmd::Command {
    let mut command = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    command
        .env_clear()
        .env("HOME", home)
        .env("SLATE_HOME", home)
        .env("PATH", "")
        .env("NO_COLOR", "1")
        .write_stdin("")
        .timeout(Duration::from_secs(5))
        .args(["--quiet", "recover"]);
    command
}

#[test]
fn recover_cleanup_errors_distinguish_retained_record_from_unsynced_removal() {
    if unsafe { libc::geteuid() } == 0 {
        // Root bypasses the real Unix permission failures this test requires.
        eprintln!("skipped: permission-denial recovery checks require a non-root user");
        return;
    }
    for discard in [false, true] {
        for directory_mode in [0o500, 0o300] {
            let home = tempfile::tempdir().unwrap();
            let (env, target, record) = fixture(home.path());
            let saved = fs::read(&record).unwrap();
            let guard = DirectoryMode::set(env.slate_cache_dir(), directory_mode);
            if directory_mode == 0o300 {
                // Existing children remain readable, but directory-open/fsync cannot start.
                assert_eq!(
                    File::open(env.slate_cache_dir()).unwrap_err().kind(),
                    std::io::ErrorKind::PermissionDenied
                );
            }
            let mut cli = command(home.path());
            if discard {
                cli.arg("--discard");
            }
            let output = cli.arg("--yes").assert().code(1).get_output().clone();
            drop(guard);
            let error = String::from_utf8_lossy(&output.stderr);
            assert!(!error.contains("PRIVATE_"), "{error}");
            if discard {
                assert!(
                    error.contains("Current config files were not changed"),
                    "{error}"
                );
            } else {
                assert!(error.contains("Preview files were restored"), "{error}");
                assert!(error.contains("not rolled back"), "{error}");
            }
            if directory_mode == 0o500 {
                assert!(error.contains("record could not be removed"), "{error}");
                assert_eq!(fs::read(&record).unwrap(), saved);
            } else {
                assert!(
                    error.contains("record was removed") && error.contains("durability"),
                    "{error}"
                );
                assert!(!record.exists());
            }
            assert_eq!(
                fs::read(&target).unwrap(),
                if discard {
                    b"PRIVATE_PREVIEW".as_slice()
                } else {
                    b"PRIVATE_ORIGINAL".as_slice()
                }
            );
            assert_eq!(
                fs::metadata(&target).unwrap().permissions().mode() & 0o777,
                if discard { 0o644 } else { 0o600 }
            );
            // Once access is repaired, the same action can finish idempotently.
            let mut retry = command(home.path());
            if discard {
                retry.arg("--discard");
            }
            retry.arg("--yes").assert().success();
            assert!(!record.exists());
            assert_eq!(
                fs::read(&target).unwrap(),
                if discard {
                    b"PRIVATE_PREVIEW".as_slice()
                } else {
                    b"PRIVATE_ORIGINAL".as_slice()
                }
            );
        }
    }
}

#[test]
fn recover_cleanup_partial_restoration_keeps_the_record_and_protects_later_edits() {
    if unsafe { libc::geteuid() } == 0 {
        eprintln!("skipped: permission-denial recovery checks require a non-root user");
        return;
    }
    for external_edit in [false, true] {
        let temporary = tempfile::tempdir().unwrap();
        let home = temporary.path().join("profile\n\u{202e}中文");
        let (env, first, record) = fixture(&home);
        let second = env.managed_file("managed/kitty/theme.conf");
        private_write(&second, b"PRIVATE_SECOND_PREVIEW");
        let mode = fs::metadata(&second).unwrap().permissions().mode();
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&record).unwrap()).unwrap();
        value["files"].as_array_mut().unwrap().push(serde_json::json!({
            "path": second, "destination": fs::canonicalize(&second).unwrap(),
            "original": {"Present": {"bytes": b"PRIVATE_SECOND_ORIGINAL".to_vec(), "mode": mode}},
        }));
        value["expected"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "Present": {"bytes": b"PRIVATE_SECOND_PREVIEW".to_vec(), "mode": mode},
            }));
        private_write(&record, serde_json::to_vec(&value).unwrap());
        let saved = fs::read(&record).unwrap();
        let guard = DirectoryMode::set(second.parent().unwrap(), 0o500);
        let output = command(&home)
            .arg("--yes")
            .assert()
            .code(1)
            .get_output()
            .clone();
        drop(guard);
        // Failure happened during the second write, not during plan preparation.
        assert_eq!(fs::read(&first).unwrap(), b"PRIVATE_ORIGINAL");
        assert_eq!(fs::read(&second).unwrap(), b"PRIVATE_SECOND_PREVIEW");
        assert_eq!(fs::read(&record).unwrap(), saved);
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            error.contains("Some files may already have been restored"),
            "{error}"
        );
        assert!(
            error.contains("Recovery finalization was not attempted"),
            "{error}"
        );
        assert!(error.contains("slate recover --dry-run"), "{error}");
        let displayed = second
            .to_str()
            .unwrap()
            .replace('\n', "\\n")
            .replace('\u{202e}', "\\u{202e}");
        assert!(error.contains(&displayed), "{error}");
        assert!(!error.contains('\u{202e}') && !error.contains("profile\n"));
        assert!(!error.contains("PRIVATE_"));
        if external_edit {
            private_write(&first, b"PRIVATE_LATER_EDIT");
        }
        let output = command(&home)
            .args(["--dry-run", "--json"])
            .assert()
            .code(i32::from(external_edit))
            .get_output()
            .clone();
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            report["changes"][0]["action"],
            if external_edit {
                "blocked"
            } else {
                "unchanged"
            }
        );
        assert_eq!(report["changes"][1]["action"], "replace");
        command(&home)
            .arg("--yes")
            .assert()
            .code(i32::from(external_edit));
        if external_edit {
            assert_eq!(fs::read(&first).unwrap(), b"PRIVATE_LATER_EDIT");
            assert_eq!(fs::read(&second).unwrap(), b"PRIVATE_SECOND_PREVIEW");
            assert_eq!(fs::read(&record).unwrap(), saved);
        } else {
            assert_eq!(fs::read(&first).unwrap(), b"PRIVATE_ORIGINAL");
            assert_eq!(fs::read(&second).unwrap(), b"PRIVATE_SECOND_ORIGINAL");
            assert!(!record.exists());
        }
    }
}
