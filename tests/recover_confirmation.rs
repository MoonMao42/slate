//! Real recovery confirmations, using only private profiles and owned children.
use slate_cli::env::SlateEnv;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::fd::FromRawFd;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

#[path = "support/tree.rs"]
mod snapshot;

fn fixture(home: &Path, corrupt: bool) -> (PathBuf, PathBuf) {
    let env = SlateEnv::with_home(home.to_owned());
    let target = env.managed_file("managed/ghostty/theme.conf");
    let record = env.slate_cache_dir().join("preview-session.json");
    private_write(&target, b"PRIVATE_PREVIEW");
    private_write(&env.slate_cache_dir().join("preview-session.lock"), b"");
    let mode = fs::metadata(&target).unwrap().permissions().mode();
    let value = serde_json::json!({
        "version": 1, "session_id": "confirmation-fixture", "pid": std::process::id(),
        "home": env.home(), "config_dir": env.config_dir(), "cache_dir": env.slate_cache_dir(),
        "writing": false, "missing_dirs": [],
        "files": [{"path": target, "destination": fs::canonicalize(&target).unwrap(),
            "original": {"Present": {"bytes": b"PRIVATE_ORIGINAL".to_vec(), "mode": mode}}}],
        "expected": [{"Present": {"bytes": b"PRIVATE_PREVIEW".to_vec(), "mode": mode}}],
    });
    private_write(
        &record,
        if corrupt {
            b"PRIVATE_CORRUPT".to_vec()
        } else {
            serde_json::to_vec(&value).unwrap()
        },
    );
    (target, record)
}

fn private_write(path: &Path, bytes: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}

fn command(home: &Path) -> Command {
    let mut command = Command::new(
        std::env::var_os("SLATE_RECOVER_TEST_BINARY")
            .unwrap_or_else(|| assert_cmd::cargo::cargo_bin!("slate").into()),
    );
    command
        .env_clear()
        .env("HOME", home)
        .env("SLATE_HOME", home)
        .env("PATH", "")
        .env("TERM", "xterm-256color")
        .env("NO_COLOR", "1")
        .args(["--quiet", "recover"]);
    command
}

struct RecoveryProcess {
    child: Child,
    terminal: File,
    output: Vec<u8>,
}

impl RecoveryProcess {
    fn start(home: &Path, discard: bool) -> Self {
        let (mut master, mut slave) = (-1, -1);
        let mut size = libc::winsize {
            ws_row: 40,
            ws_col: 160,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        assert_eq!(
            unsafe {
                libc::openpty(
                    &mut master,
                    &mut slave,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::addr_of_mut!(size),
                )
            },
            0
        );
        let terminal = unsafe { File::from_raw_fd(master) };
        let slave = unsafe { File::from_raw_fd(slave) };
        let flags = unsafe { libc::fcntl(master, libc::F_GETFL) };
        assert!(flags >= 0);
        assert_eq!(
            unsafe { libc::fcntl(master, libc::F_SETFL, flags | libc::O_NONBLOCK) },
            0
        );
        let mut command = command(home);
        if discard {
            command.arg("--discard");
        }
        command
            .stdin(Stdio::from(slave.try_clone().unwrap()))
            .stdout(Stdio::from(slave.try_clone().unwrap()))
            .stderr(Stdio::from(slave));
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 || libc::ioctl(0, libc::TIOCSCTTY as _, 0) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        Self {
            child: command.spawn().unwrap(),
            terminal,
            output: Vec::new(),
        }
    }

    fn drain(&mut self) {
        let mut bytes = [0; 8192];
        while let Ok(count) = self.terminal.read(&mut bytes) {
            if count == 0 {
                break;
            }
            self.output.extend_from_slice(&bytes[..count]);
            assert!(self.output.len() < 65536, "unbounded confirmation output");
        }
    }

    fn wait_for_prompt(&mut self) {
        let start = Instant::now();
        loop {
            self.drain();
            let text = String::from_utf8_lossy(&self.output);
            if text.contains("Restore the preview files shown above")
                || text.contains("Discard the saved preview recovery record?")
                || text.contains("将上列文件恢复为预览前保存的内容？")
                || text.contains("放弃预览恢复？")
            {
                return;
            }
            assert!(self.child.try_wait().unwrap().is_none(), "{text}");
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "confirmation timeout: {text}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn answer(&mut self, keys: &[u8]) -> ExitStatus {
        self.terminal.write_all(keys).unwrap();
        let start = Instant::now();
        loop {
            self.drain();
            if let Some(status) = self.child.try_wait().unwrap() {
                self.drain();
                assert!(!String::from_utf8_lossy(&self.output).contains("PRIVATE_"));
                return status;
            }
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "recovery timeout: {}",
                String::from_utf8_lossy(&self.output)
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for RecoveryProcess {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
            if let Ok(null) = File::open("/dev/null") {
                self.terminal = null;
            }
            let _ = self.child.wait();
        }
    }
}

#[test]
fn recover_confirmation_rejects_replaced_and_same_session_edited_records() {
    for (discard, corrupt) in [(false, false), (true, false), (true, true)] {
        for replacement in [false, true] {
            let home = tempfile::tempdir().unwrap();
            let (_, record) = fixture(home.path(), corrupt);
            let mut child = RecoveryProcess::start(home.path(), discard);
            child.wait_for_prompt();
            if replacement {
                // Identical bytes in a new inode are still a different saved record.
                let next = record.with_extension("next");
                private_write(&next, fs::read(&record).unwrap());
                fs::rename(next, &record).unwrap();
            } else if corrupt {
                private_write(&record, b"PRIVATE_DIFFERENT_CORRUPT_RECORD");
            } else {
                let mut value: serde_json::Value =
                    serde_json::from_slice(&fs::read(&record).unwrap()).unwrap();
                value["files"][0]["original"]["Present"]["bytes"] =
                    serde_json::json!(b"PRIVATE_DIFFERENT_ORIGINAL".to_vec());
                private_write(&record, serde_json::to_vec(&value).unwrap());
            }
            let before = snapshot::tree(home.path());
            assert_eq!(
                child.answer(b"y\r").code(),
                Some(1),
                "{}",
                String::from_utf8_lossy(&child.output)
            );
            assert!(String::from_utf8_lossy(&child.output).contains("changed since inspection"));
            assert_eq!(snapshot::tree(home.path()), before);
        }
    }
}

#[test]
fn recover_confirmation_holds_the_lock_but_allows_readonly_inspection() {
    for (discard, corrupt) in [(false, false), (true, false), (true, true)] {
        let home = tempfile::tempdir().unwrap();
        fixture(home.path(), corrupt);
        let mut child = RecoveryProcess::start(home.path(), discard);
        child.wait_for_prompt();
        let before = snapshot::tree(home.path());
        let mut inspect = assert_cmd::Command::from_std(command(home.path()));
        let output = inspect
            .args(["--dry-run", "--json"])
            .timeout(Duration::from_secs(5))
            .assert()
            .code(1)
            .get_output()
            .clone();
        if !corrupt {
            let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(report["active"], true);
        }
        for args in [
            vec!["--yes"],
            vec!["--discard", "--yes"],
            vec!["--export", "unused"],
        ] {
            let mut other = command(home.path());
            // Export must not create anything, even if another operation is waiting.
            other.current_dir(home.path());
            let output = assert_cmd::Command::from_std(other)
                .args(args)
                .timeout(Duration::from_secs(5))
                .assert()
                .code(1)
                .get_output()
                .clone();
            let error = String::from_utf8_lossy(&output.stderr);
            assert!(error.contains("still running"), "{error}");
            assert!(!error.contains("PRIVATE_"));
        }
        assert_eq!(snapshot::tree(home.path()), before);
        assert!(child.answer(b"\r").success());
        assert_eq!(snapshot::tree(home.path()), before);
        // Cancellation drops the lock: the next explicit discard can now finish.
        assert_cmd::Command::from_std(command(home.path()))
            .args(["--discard", "--yes"])
            .timeout(Duration::from_secs(5))
            .assert()
            .success();
    }
}

#[test]
fn recover_confirmation_defaults_to_cancel_and_unchanged_inputs_succeed() {
    for language in ["zh-CN", "en"] {
        for (discard, corrupt) in [(false, false), (true, false), (true, true)] {
            let home = tempfile::tempdir().unwrap();
            let (target, record) = fixture(home.path(), corrupt);
            private_write(
                &home.path().join(".config/slate/config.toml"),
                format!("[preferences]\nlanguage = {language:?}\n"),
            );
            let before = snapshot::tree(home.path());
            let mut canceled = RecoveryProcess::start(home.path(), discard);
            canceled.wait_for_prompt();
            assert!(canceled.answer(b"\r").success());
            let text = String::from_utf8_lossy(&canceled.output);
            let (prompt, cancel) = match (language, discard) {
                ("zh-CN", true) => ("放弃预览恢复？", "取消"),
                ("zh-CN", false) => ("将上列文件恢复为预览前保存的内容？", "取消"),
                (_, true) => ("Discard the saved preview recovery record?", "Cancel"),
                (_, false) => ("Restore the preview files shown above", "Cancel"),
            };
            assert!(text.contains(prompt) && text.contains(cancel), "{text}");
            if !corrupt {
                assert!(
                    text.contains(if language == "zh-CN" {
                        "预览恢复:"
                    } else {
                        "Preview recovery:"
                    }),
                    "{text}"
                );
                assert!(
                    text.contains(if language == "zh-CN" {
                        "替换"
                    } else {
                        "replace"
                    }),
                    "{text}"
                );
                assert!(
                    text.contains(if language == "zh-CN" {
                        "1 个文件 · 0 处冲突"
                    } else {
                        "1 file(s) listed; 0 conflict(s)."
                    }),
                    "{text}"
                );
            }
            assert_eq!(snapshot::tree(home.path()), before);
            let mut accepted = RecoveryProcess::start(home.path(), discard);
            accepted.wait_for_prompt();
            assert!(
                accepted.answer(b"y\r").success(),
                "{}",
                String::from_utf8_lossy(&accepted.output)
            );
            assert!(!record.exists());
            let text = String::from_utf8_lossy(&accepted.output);
            let receipt = match (language, discard) {
                ("zh-CN", true) => "已删除恢复记录，当前配置文件未改动。",
                ("zh-CN", false) => "已恢复预览前的文件，并清除恢复记录。",
                (_, true) => "Recovery record deleted; current config files were not changed.",
                (_, false) => "Preview files restored; recovery record cleared.",
            };
            assert!(text.contains(receipt), "{text}");
            assert_eq!(
                fs::read(target).unwrap(),
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
fn recover_confirmation_preserves_target_edits_and_discard_keeps_them() {
    for discard in [false, true] {
        let home = tempfile::tempdir().unwrap();
        let (target, record) = fixture(home.path(), false);
        let mut child = RecoveryProcess::start(home.path(), discard);
        child.wait_for_prompt();
        private_write(&target, b"PRIVATE_EXTERNAL_EDIT");
        let before = snapshot::tree(home.path());
        assert_eq!(
            child.answer(b"y\r").success(),
            discard,
            "{}",
            String::from_utf8_lossy(&child.output)
        );
        assert_eq!(fs::read(target).unwrap(), b"PRIVATE_EXTERNAL_EDIT");
        assert_eq!(record.exists(), !discard);
        if !discard {
            assert_eq!(snapshot::tree(home.path()), before);
        }
    }
}

#[test]
fn recover_confirmation_unsafe_records_fail_promptly_without_discarding() {
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::symlink;
    for kind in ["fifo", "directory", "symlink", "public", "dangling-parent"] {
        let home = tempfile::tempdir().unwrap();
        let (_, record) = fixture(home.path(), false);
        let saved = record.with_extension("saved");
        fs::rename(&record, &saved).unwrap();
        match kind {
            "fifo" => {
                let path = std::ffi::CString::new(record.as_os_str().as_bytes()).unwrap();
                assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
            }
            "directory" => fs::create_dir(&record).unwrap(),
            "symlink" => symlink(&saved, &record).unwrap(),
            "public" => {
                fs::copy(&saved, &record).unwrap();
                fs::set_permissions(&record, fs::Permissions::from_mode(0o644)).unwrap();
            }
            "dangling-parent" => {
                let cache = record.parent().unwrap();
                fs::rename(cache, home.path().join("saved-cache")).unwrap();
                symlink(home.path().join("missing-cache"), cache).unwrap();
            }
            _ => unreachable!(),
        }
        let before = snapshot::tree(home.path());
        for args in [["--dry-run", "--json"], ["--discard", "--yes"]] {
            let output = assert_cmd::Command::from_std(command(home.path()))
                .args(args)
                .timeout(Duration::from_secs(5))
                .assert()
                .code(1)
                .get_output()
                .clone();
            assert!(!String::from_utf8_lossy(&output.stderr).contains("PRIVATE_"));
            assert_eq!(snapshot::tree(home.path()), before);
        }
    }
}
