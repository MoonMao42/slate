//! Drive real picker navigation through a private PTY and isolated HOME.

use slate_cli::config::list_restore_points_with_env;
use slate_cli::env::SlateEnv;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};
use tempfile::TempDir;

#[path = "picker_exit/hub_entry.rs"]
mod hub_entry;
#[path = "picker_exit/paste.rs"]
mod paste;
#[path = "picker_exit/preview_scroll.rs"]
mod preview_scroll;
#[path = "support/tree.rs"]
mod recovery_tree;

struct PickerProcess {
    child: Child,
    terminal: File,
    output: Vec<u8>,
}

impl PickerProcess {
    fn start(home: &std::path::Path) -> Self {
        Self::start_with_args(home, &["theme"])
    }

    fn start_with_args(home: &std::path::Path, args: &[&str]) -> Self {
        Self::start_with_path(home, args, None)
    }

    fn start_with_path(
        home: &std::path::Path,
        args: &[&str],
        path: Option<&std::path::Path>,
    ) -> Self {
        let (mut master, mut slave) = (-1, -1);
        let mut size = libc::winsize {
            ws_row: 40,
            ws_col: 120,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // The returned descriptors are owned only by this fixture.
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
        let slave_file = unsafe { File::from_raw_fd(slave) };
        let flags = unsafe { libc::fcntl(master, libc::F_GETFL) };
        assert!(flags >= 0);
        assert_eq!(
            unsafe { libc::fcntl(master, libc::F_SETFL, flags | libc::O_NONBLOCK) },
            0
        );
        let binary = std::env::var_os("SLATE_PICKER_TEST_BINARY")
            .unwrap_or_else(|| env!("CARGO_BIN_EXE_slate").into());
        let mut command = Command::new(binary);
        command
            .args(args)
            .env("SLATE_HOME", home)
            // Legacy workflow cases select a language explicitly; first-run
            // language consent has its own private PTY driver.
            .env("SLATE_LANGUAGE", "zh-CN")
            .env("TERM", "xterm-256color")
            .env_remove("TERM_PROGRAM")
            .env_remove("SSH_CONNECTION")
            .env_remove("SSH_TTY")
            .env_remove("SSH_CLIENT")
            .stdin(Stdio::from(slave_file.try_clone().unwrap()))
            .stdout(Stdio::from(slave_file.try_clone().unwrap()))
            .stderr(Stdio::from(slave_file));
        if args.first() == Some(&"font") {
            // Font fixtures use only their private executable tripwires and
            // disable sound in preferences; reloads remain profile-isolated.
            command.env("HOME", home).env("PATH", home.join("bin"));
        }
        if let Some(path) = path {
            command.env("HOME", home).env("PATH", path);
        }
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 || libc::ioctl(0, libc::TIOCSCTTY as _, 0) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = command.spawn().unwrap();
        Self {
            child,
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
        }
    }

    fn wait_for(&mut self, ready: impl Fn() -> bool) {
        let started = Instant::now();
        loop {
            self.drain();
            if ready() {
                return;
            }
            assert!(
                self.child.try_wait().unwrap().is_none(),
                "picker exited early: {}",
                String::from_utf8_lossy(&self.output)
            );
            assert!(
                started.elapsed() < Duration::from_secs(15),
                "picker timeout: {}",
                String::from_utf8_lossy(&self.output)
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn finish(&mut self, key: &[u8]) {
        self.finish_with_code(key, 0);
    }

    fn finish_with_code(&mut self, key: &[u8], expected_code: i32) {
        self.finish_matching(key, |status| status.code() == Some(expected_code));
    }

    fn finish_interrupted(&mut self, key: &[u8]) {
        // Ctrl+C can be read by cliclack or delivered by the PTY as SIGINT
        // between key reads. Shells report either form as an interruption.
        self.finish_matching(key, |status| {
            status.code() == Some(130) || status.signal() == Some(libc::SIGINT)
        });
    }

    fn finish_matching(&mut self, key: &[u8], expected: impl Fn(ExitStatus) -> bool) {
        self.terminal.write_all(key).unwrap();
        let started = Instant::now();
        loop {
            self.drain();
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(
                    expected(status),
                    "unexpected exit {status}: {}",
                    String::from_utf8_lossy(&self.output)
                );
                return;
            }
            assert!(
                started.elapsed() < Duration::from_secs(15),
                "picker failed to exit"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn wait_for_output(&mut self, text: &str) {
        self.wait_for_output_since(text, 0);
    }

    fn wait_for_output_since(&mut self, text: &str, offset: usize) {
        let started = Instant::now();
        loop {
            self.drain();
            if String::from_utf8_lossy(&self.output[offset..]).contains(text) {
                return;
            }
            assert!(
                self.child.try_wait().unwrap().is_none(),
                "child exited before prompt: {}",
                String::from_utf8_lossy(&self.output)
            );
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "prompt timeout: {text}; {}",
                String::from_utf8_lossy(&self.output)
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn kill_for_recovery(&mut self) {
        assert!(self.child.try_wait().unwrap().is_none());
        self.child.kill().unwrap();
        // macOS may keep a killed PTY session leader in exit until the master
        // closes. Release only this fixture's PTY before polling the child.
        self.terminal = File::open("/dev/null").unwrap();
        let started = Instant::now();
        loop {
            if self.child.try_wait().unwrap().is_some() {
                return;
            }
            assert!(
                started.elapsed() < Duration::from_secs(3),
                "killed picker did not exit"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for PickerProcess {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
            if let Ok(null) = File::open("/dev/null") {
                self.terminal = null;
            }
            for _ in 0..100 {
                if !matches!(self.child.try_wait(), Ok(None)) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
}

fn recover_command(home: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(
        std::env::var_os("SLATE_PICKER_TEST_BINARY")
            .unwrap_or_else(|| env!("CARGO_BIN_EXE_slate").into()),
    )
    .env("SLATE_HOME", home)
    .args(args)
    .output()
    .unwrap()
}

#[test]
fn font_list_picker_preserves_literal_recommendation_suffix_on_real_selection() {
    let home = TempDir::new().unwrap();
    let env = SlateEnv::with_home(home.path().into());
    let family = "AAASlateListNerdFont (recommended)";
    fs::create_dir_all(env.config_dir()).unwrap();
    fs::write(
        env.managed_file("config.toml"),
        "[preferences]\nsound = false\n",
    )
    .unwrap();
    let root = slate_cli::platform::fonts::user_font_dir(&env);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join(format!("{family}.ttf")),
        b"\0\x01\0\0private-fixture-not-a-real-font",
    )
    .unwrap();
    let bin = home.path().join("bin");
    fs::create_dir(&bin).unwrap();
    for tool in ["fc-cache", "brew", "curl", "ghostty", "kitten", "osascript"] {
        let path = bin.join(tool);
        fs::write(
            &path,
            "#!/bin/sh\n: > \"$HOME/native-command-ran\"\nexit 92\n",
        )
        .unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let output = Command::new(env!("CARGO_BIN_EXE_slate"))
        .env_clear()
        .env("HOME", home.path())
        .env("SLATE_HOME", home.path())
        .env("PATH", &bin)
        .args(["font", "--list", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let index = report["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|candidate| candidate["kind"] == "nerd")
        .position(|candidate| candidate["family"] == family)
        .unwrap();
    let mut keys = b"\x1b[H".to_vec();
    keys.extend(b"\x1b[B".repeat(index));
    keys.push(b'\r');
    for args in [
        &["font"][..],
        &["font", "--quiet"][..],
        &["font", "--auto"][..],
    ] {
        let mut picker = PickerProcess::start_with_args(home.path(), args);
        picker.wait_for_output("选择字体：");
        picker.terminal.write_all(&keys).unwrap();
        picker.wait_for_output("● 暂不更换");
        picker.wait_for_output("确认使用");
        picker.finish(b"\x1b[B\x1b[B\r");
        picker.drain();
        let output = String::from_utf8_lossy(&picker.output);
        assert_eq!(
            output.contains("Updated font to"),
            !args.contains(&"--quiet"),
            "{output}"
        );
        assert_eq!(
            output.contains("your new palette lives there"),
            args.len() == 1,
            "{output}"
        );
    }
    assert_eq!(
        fs::read_to_string(env.managed_file("current-font")).unwrap(),
        family
    );
    assert_eq!(
        fs::read_to_string(env.managed_file("managed/ghostty/font.conf")).unwrap(),
        format!("font-family = \"{family}\"\n")
    );
    assert!(!home.path().join("native-command-ran").exists());
    assert_eq!(fs::read_dir(root).unwrap().count(), 1);
}

#[test]
fn recovery_hub_requires_confirmation_before_restoring_a_killed_preview() {
    let td = TempDir::new().unwrap();
    let env = SlateEnv::with_home(td.path().to_owned());
    let entry = env.xdg_config_home().join("ghostty/config.ghostty");
    let original = b"# user settings before preview\nbackground = #123456\n";
    fs::create_dir_all(entry.parent().unwrap()).unwrap();
    fs::write(&entry, original).unwrap();
    fs::create_dir_all(env.config_dir()).unwrap();
    fs::write(env.managed_file("current-font"), "Fixture Mono").unwrap();
    fs::write(
        env.managed_file("config.toml"),
        "[sound]\nenabled = false\n",
    )
    .unwrap();
    let journal = env.slate_cache_dir().join("preview-session.json");
    let mut picker = PickerProcess::start(td.path());
    picker.wait_for(|| {
        fs::read_to_string(&entry).is_ok_and(|text| text.contains("blur.conf"))
            && fs::read(&journal)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
                .is_some_and(|record| record["writing"] == false)
    });
    picker.kill_for_recovery();
    let previewed = fs::read(&entry).unwrap();
    let record = fs::read(&journal).unwrap();

    // The dedicated menu cannot take ordinary theme/font/setup actions.
    for keys in [b"\x1b[B\x1b[B\x1b[B\r".as_slice(), b"\x1b", b"\x03"] {
        let mut hub = PickerProcess::start_with_args(td.path(), &[]);
        hub.wait_for_output("上次预览尚未结束，请先检查恢复方案");
        hub.wait_for_output("● 查看文件差异");
        // Hints are only shown on the selected row; inspect each action
        // without confirming it, then return to the original selection.
        for hint in [
            "可能覆盖当前文件",
            "仅删除预览恢复副本，不恢复旧文件",
            "保留当前文件与恢复副本",
        ] {
            hub.terminal.write_all(b"\x1b[B").unwrap();
            hub.wait_for_output(hint);
            assert_eq!(fs::read(&entry).unwrap(), previewed);
            assert_eq!(fs::read(&journal).unwrap(), record);
        }
        let offset = hub.output.len();
        hub.terminal.write_all(b"\x1b[H").unwrap();
        hub.wait_for_output_since("● 查看文件差异", offset);
        assert!(!String::from_utf8_lossy(&hub.output).contains("想调整什么？"));
        hub.finish_with_code(keys, if keys == b"\x03" { 130 } else { 0 });
        assert_eq!(fs::read(&entry).unwrap(), previewed);
        assert_eq!(fs::read(&journal).unwrap(), record);
    }

    // Review is read-only and returns to a freshly inspected recovery menu.
    for back in [b"\r".as_slice(), b"\x1b"] {
        let mut hub = PickerProcess::start_with_args(td.path(), &[]);
        hub.wait_for_output("上次预览尚未结束，请先检查恢复方案");
        hub.terminal.write_all(b"\r").unwrap();
        hub.wait_for_output("● 返回恢复菜单");
        assert_eq!(fs::read(&entry).unwrap(), previewed);
        assert_eq!(fs::read(&journal).unwrap(), record);
        // A changed journal must not leave an obsolete Restore action enabled.
        fs::write(&journal, b"unreadable after review").unwrap();
        let offset = hub.output.len();
        hub.terminal.write_all(back).unwrap();
        hub.wait_for_output_since("退出", offset);
        assert!(!String::from_utf8_lossy(&hub.output[offset..]).contains("恢复预览前的文件"));
        hub.finish(b"\x1b");
        assert_eq!(fs::read(&journal).unwrap(), b"unreadable after review");
        assert_eq!(fs::read(&entry).unwrap(), previewed);
        fs::write(&journal, &record).unwrap();
    }

    // Conflicts block restoration, not read-only comparison and returning.
    fs::write(&entry, b"# later personal edit\nbackground = #654321\n").unwrap();
    let conflicted = recovery_tree::tree(td.path());
    for back in [b"\r".as_slice(), b"\x1b"] {
        let mut hub = PickerProcess::start_with_args(td.path(), &[]);
        hub.wait_for_output("● 查看文件差异");
        assert!(!String::from_utf8_lossy(&hub.output).contains("恢复预览前的文件"));
        hub.terminal.write_all(b"\r").unwrap();
        hub.wait_for_output("● 返回恢复菜单");
        assert_eq!(recovery_tree::tree(td.path()), conflicted);
        let offset = hub.output.len();
        hub.terminal.write_all(back).unwrap();
        hub.wait_for_output_since("● 查看文件差异", offset);
        hub.finish(b"\x1b");
        assert_eq!(recovery_tree::tree(td.path()), conflicted);
    }
    let dry_run = recover_command(td.path(), &["recover", "--dry-run"]);
    assert_eq!(dry_run.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&dry_run.stdout).contains("conflict(s)"));
    assert_eq!(recovery_tree::tree(td.path()), conflicted);
    fs::write(&entry, &previewed).unwrap();

    // Neither restoring nor discarding may treat Escape, pasted answers, or
    // a highlighted Yes as consent. Exercise both real CLI entry points.
    let before = recovery_tree::tree(td.path());
    for args in [&["recover"][..], &["recover", "--discard"][..]] {
        for keys in [
            b"\r".as_slice(),
            b"\x1b",
            b"n",
            b"N",
            b"\x03",
            b"\x1b[B\x1b",
            b"\x1b[200~y\r\x1b[201~\x1b",
        ] {
            let mut command = PickerProcess::start_with_args(td.path(), args);
            command.wait_for_output("● 取消");
            command.finish_with_code(keys, if keys == b"\x03" { 130 } else { 0 });
            assert_eq!(recovery_tree::tree(td.path()), before);
        }
    }

    for confirm in [false, true] {
        let mut hub = PickerProcess::start_with_args(td.path(), &[]);
        hub.wait_for_output("上次预览尚未结束，请先检查恢复方案");
        hub.terminal.write_all(b"\x1b[B\r").unwrap();
        hub.wait_for_output("● 取消");
        assert_eq!(
            fs::read(&entry).unwrap(),
            previewed,
            "menu selection restored without confirmation"
        );
        hub.finish(if confirm { b"y\r" } else { b"\r" });
        if confirm {
            assert_eq!(fs::read(&entry).unwrap(), original);
            assert!(!journal.exists());
        } else {
            assert_eq!(fs::read(&entry).unwrap(), previewed);
            assert_eq!(fs::read(&journal).unwrap(), record);
        }
    }
}

#[test]
fn recovery_inspection_is_read_only_on_a_fresh_home() {
    let td = TempDir::new().unwrap();
    let output = recover_command(td.path(), &["recover", "--dry-run", "--json"]);
    assert!(output.status.success());
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(plan["available"], false);
    assert_eq!(fs::read_dir(td.path()).unwrap().count(), 0);
    for args in [
        &["recover", "--json"][..],
        &["recover", "--dry-run", "--yes"][..],
        &["recover", "--discard", "--export", "unused"][..],
    ] {
        assert_eq!(recover_command(td.path(), args).status.code(), Some(2));
        assert_eq!(fs::read_dir(td.path()).unwrap().count(), 0);
    }
    let env = SlateEnv::with_home(td.path().to_owned());
    fs::create_dir_all(env.slate_cache_dir()).unwrap();
    for (name, bytes) in [
        ("preview-session.lock", &b""[..]),
        ("preview-session.json", &b"{broken record"[..]),
    ] {
        let path = env.slate_cache_dir().join(name);
        fs::write(&path, bytes).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let output = recover_command(td.path(), &["recover", "--discard", "--yes"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!env.slate_cache_dir().join("preview-session.json").exists());
    assert!(!env.config_dir().exists());
}

#[test]
fn killed_picker_can_be_recovered_without_touching_live_sessions_or_later_edits() {
    for external_edit in [false, true] {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        let entry = env.xdg_config_home().join("ghostty/config.ghostty");
        let original = b"# private fixture comment\nbackground = #123456\n";
        fs::create_dir_all(entry.parent().unwrap()).unwrap();
        fs::write(&entry, original).unwrap();
        fs::create_dir_all(env.config_dir()).unwrap();
        fs::write(env.managed_file("current-font"), "Fixture Mono").unwrap();
        fs::write(
            env.managed_file("config.toml"),
            "[sound]\nenabled = false\n",
        )
        .unwrap();
        let journal = env.slate_cache_dir().join("preview-session.json");
        let mut picker = PickerProcess::start(td.path());
        picker.wait_for(|| {
            fs::read_to_string(&entry).is_ok_and(|text| text.contains("blur.conf"))
                && fs::read(&journal)
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
                    .is_some_and(|record| record["writing"] == false)
        });
        assert_eq!(
            fs::metadata(&journal).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let previewed = fs::read(&entry).unwrap();
        for args in [
            &["config", "set", "opacity", "clear"][..],
            &["set", "nord"][..],
        ] {
            let blocked = recover_command(td.path(), args);
            assert!(!blocked.status.success());
            assert!(String::from_utf8_lossy(&blocked.stderr).contains("still running"));
            assert_eq!(fs::read(&entry).unwrap(), previewed);
        }
        let blocked = recover_command(td.path(), &["recover", "--yes"]);
        assert!(!blocked.status.success());
        assert!(String::from_utf8_lossy(&blocked.stderr).contains("still running"));
        let competing = recover_command(td.path(), &["theme"]);
        assert!(!competing.status.success());
        assert!(String::from_utf8_lossy(&competing.stderr).contains("still running"));
        assert_eq!(fs::read(&entry).unwrap(), previewed);
        assert!(picker.child.try_wait().unwrap().is_none());

        // SIGKILL only this test's Child handle; no Drop or panic cleanup can run.
        picker.kill_for_recovery();
        if external_edit {
            fs::write(&entry, b"# user's edit after interruption\n").unwrap();
        }
        let before_entry = fs::read(&entry).unwrap();
        let before_record = fs::read(&journal).unwrap();
        let output = recover_command(td.path(), &["recover", "--dry-run", "--json"]);
        let plan: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(plan["active"], false);
        assert_eq!(plan["available"], true);
        assert_eq!(output.status.success(), !external_edit);
        assert!(plan.get("files").is_none() && plan.get("expected").is_none());
        assert!(!String::from_utf8_lossy(&output.stdout).contains("private fixture comment"));
        assert_eq!(fs::read(&entry).unwrap(), before_entry);
        assert_eq!(fs::read(&journal).unwrap(), before_record);
        let unfinished = recover_command(td.path(), &["theme"]);
        assert!(!unfinished.status.success());
        assert!(String::from_utf8_lossy(&unfinished.stderr).contains("slate recover"));

        let recovered = recover_command(td.path(), &["recover", "--yes"]);
        assert_eq!(
            recovered.status.success(),
            !external_edit,
            "{}",
            String::from_utf8_lossy(&recovered.stderr)
        );
        if external_edit {
            assert_eq!(fs::read(&entry).unwrap(), before_entry);
            assert_eq!(fs::read(&journal).unwrap(), before_record);
            let exported = td.path().join("original-preview-files");
            let output = recover_command(
                td.path(),
                &["recover", "--export", exported.to_str().unwrap()],
            );
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let manifest: serde_json::Value =
                serde_json::from_slice(&fs::read(exported.join("manifest.json")).unwrap()).unwrap();
            let file = manifest
                .as_array()
                .unwrap()
                .iter()
                .find(|file| file["path"] == entry.to_str().unwrap())
                .unwrap();
            assert_eq!(
                fs::read(exported.join(file["original_file"].as_str().unwrap())).unwrap(),
                original
            );
            assert_eq!(
                fs::metadata(&exported).unwrap().permissions().mode() & 0o777,
                0o700
            );
            assert!(journal.exists());
            let discarded = recover_command(td.path(), &["recover", "--discard", "--yes"]);
            assert!(discarded.status.success());
            assert_eq!(fs::read(&entry).unwrap(), before_entry);
        } else {
            assert_eq!(fs::read(&entry).unwrap(), original);
            assert!(!env.config_dir().join("managed/ghostty/theme.conf").exists());
        }
        assert!(!journal.exists());
    }
}

fn queued_picker_fixture() -> (TempDir, SlateEnv) {
    let home = TempDir::new().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let entry = env.xdg_config_home().join("ghostty/config.ghostty");
    fs::create_dir_all(entry.parent().unwrap()).unwrap();
    fs::write(entry, "# private terminal fixture\n").unwrap();
    fs::create_dir_all(env.config_dir()).unwrap();
    fs::write(env.managed_file("current"), "catppuccin-mocha").unwrap();
    fs::write(env.managed_file("current-font"), "Fixture Mono").unwrap();
    fs::write(
        env.managed_file("config.toml"),
        "[sound]\nenabled = false\n",
    )
    .unwrap();
    (home, env)
}

#[test]
fn real_picker_small_window_resize_keeps_help_and_does_not_reapply_preview() {
    let (home, env) = queued_picker_fixture();
    let journal = env.slate_cache_dir().join("preview-session.json");
    let managed = env.managed_file("managed/ghostty/theme.conf");
    let stamp = |path: &std::path::Path| {
        let meta = fs::metadata(path).unwrap();
        (
            meta.dev(),
            meta.ino(),
            meta.len(),
            meta.mtime(),
            meta.mtime_nsec(),
        )
    };
    let mut picker = PickerProcess::start(home.path());
    picker.wait_for_output("s 保存配对");
    let before = (stamp(&journal), stamp(&managed));
    for (cols, rows) in [(40, 12), (24, 8), (80, 24), (120, 40)] {
        picker.output.clear();
        let mut size = libc::winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // Only the PTY owned by this fixture receives a resize event.
        assert_eq!(
            unsafe {
                libc::ioctl(
                    picker.terminal.as_raw_fd(),
                    libc::TIOCSWINSZ,
                    std::ptr::addr_of_mut!(size),
                )
            },
            0
        );
        picker.wait_for_output(if cols < 40 {
            "Tab 预览"
        } else {
            "s 保存配对"
        });
        picker.drain();
        let output = String::from_utf8_lossy(&picker.output);
        let last_clear = output
            .rfind("\x1b[2J")
            .expect("resize should draw a new frame");
        let frame = console::strip_ansi_codes(&output[last_clear..]);
        assert!(frame.contains("Esc 取消"));
        assert!(frame.contains('›'));
        assert!(frame.split("\r\n").count() <= rows as usize, "{frame:?}");
        for line in frame.split("\r\n") {
            assert!(
                console::measure_text_width(line) < cols as usize,
                "{line:?}"
            );
        }
        assert_eq!((stamp(&journal), stamp(&managed)), before);
    }
    picker.finish(b"q");
    assert!(!managed.exists());
    assert!(!journal.exists());
    assert_eq!(
        fs::read(env.xdg_config_home().join("ghostty/config.ghostty")).unwrap(),
        b"# private terminal fixture\n"
    );
}

#[test]
fn real_picker_full_preview_filters_terminal_controls_and_contains_styles() {
    let (home, env) = queued_picker_fixture();
    let bin = home.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let starship = bin.join("starship");
    fs::write(&starship, "#!/bin/sh\n/bin/cat \"$HOME/prompt-output\"\n").unwrap();
    fs::set_permissions(starship, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(home.path().join("prompt-output"), "\x1b[38;5;183mRENDER_SAFE_TOKEN🦀\x1b]52;c;PRIVATE_CLIPBOARD\x07\x1b[999;1H\x1b[?1049l\x1bPPRIVATE_DCS\x1b\\").unwrap();
    let mut picker = PickerProcess::start_with_path(home.path(), &["theme"], Some(&bin));
    picker.wait_for_output("s 保存配对");
    picker.output.clear();
    picker.terminal.write_all(b"\t").unwrap();
    picker.wait_for_output("RENDER_SAFE_TOKEN");
    let output = String::from_utf8_lossy(&picker.output);
    for forbidden in ["PRIVATE_", "\x1b]52", "\x1b[999;1H", "\x1b[?1049l"] {
        assert!(
            !output.contains(forbidden),
            "external terminal action reached picker output"
        );
    }
    assert!(output.contains("\x1b[38;5;183mRENDER_SAFE_TOKEN🦀\x1b[0m"));
    picker.finish(b"q");
    assert!(!env.slate_cache_dir().join("preview-session.json").exists());
    assert_eq!(
        fs::read(env.xdg_config_home().join("ghostty/config.ghostty")).unwrap(),
        b"# private terminal fixture\n"
    );
}

#[test]
fn real_picker_full_preview_timeout_falls_back_and_cancel_stays_usable() {
    let (home, env) = queued_picker_fixture();
    let bin = home.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let starship = bin.join("starship");
    fs::write(&starship, "#!/bin/sh\nprintf x >> \"$HOME/starship.calls\"\nprintf PRIVATE_PARTIAL\nprintf PRIVATE_STDERR >&2\nexec /bin/sleep 10\n").unwrap();
    fs::set_permissions(starship, fs::Permissions::from_mode(0o755)).unwrap();
    let mut picker = PickerProcess::start_with_path(home.path(), &["theme"], Some(&bin));
    picker.wait_for_output("s 保存配对");
    let started = Instant::now();
    picker.terminal.write_all(b"\t").unwrap();
    picker.wait_for_output("预览 · Tab 返回");
    assert!(started.elapsed() < Duration::from_secs(4));
    assert!(!String::from_utf8_lossy(&picker.output).contains("PRIVATE_"));
    assert_eq!(fs::read(home.path().join("starship.calls")).unwrap(), b"x");
    picker.output.clear();
    picker.terminal.write_all(b"\t").unwrap();
    picker.wait_for_output("s 保存配对");
    picker.output.clear();
    picker.terminal.write_all(b"\t").unwrap();
    picker.wait_for_output("预览 · Tab 返回");
    assert_eq!(fs::read(home.path().join("starship.calls")).unwrap(), b"x");
    picker.finish(b"q");
    assert_eq!(
        fs::read(env.xdg_config_home().join("ghostty/config.ghostty")).unwrap(),
        b"# private terminal fixture\n"
    );
    assert!(!env.managed_file("managed/ghostty/theme.conf").exists());
    assert!(!env.slate_cache_dir().join("preview-session.json").exists());
}

#[test]
fn real_picker_queued_navigation_commits_the_requested_row() {
    let (home, env) = queued_picker_fixture();
    let mut expected = slate_cli::cli::picker::PickerState::new(
        "catppuccin-mocha",
        slate_cli::opacity::OpacityPreset::Solid,
    )
    .unwrap();
    expected.move_down();
    expected.move_down();
    let mut picker = PickerProcess::start(home.path());
    picker.wait_for_output("s 保存配对");
    picker.finish(b"jj\r");
    assert_eq!(
        fs::read_to_string(env.managed_file("current")).unwrap(),
        expected.get_current_theme_id(),
        "queued navigation was lost before Enter"
    );
    assert!(!env.slate_cache_dir().join("preview-session.json").exists());
}

#[test]
fn real_picker_queued_save_shows_feedback_without_rewriting_preview() {
    let (home, env) = queued_picker_fixture();
    let journal = env.slate_cache_dir().join("preview-session.json");
    let managed = env.managed_file("managed/ghostty/theme.conf");
    let stamp = |path: &std::path::Path| {
        let meta = fs::metadata(path).unwrap();
        (
            meta.dev(),
            meta.ino(),
            meta.len(),
            meta.mtime(),
            meta.mtime_nsec(),
        )
    };
    let mut picker = PickerProcess::start(home.path());
    picker.wait_for_output("s 保存配对");
    let before = (stamp(&journal), stamp(&managed));
    picker.terminal.write_all(b"s").unwrap();
    picker.wait_for_output("深色配对已保存：");
    assert_eq!((stamp(&journal), stamp(&managed)), before);
    picker.finish(b"q");
    let auto = slate_cli::config::ConfigManager::with_env(&env)
        .unwrap()
        .read_auto_config()
        .unwrap()
        .unwrap();
    assert_eq!(auto.dark_theme.as_deref(), Some("catppuccin-mocha"));
    assert!(!managed.exists());
    assert!(!journal.exists());
}

#[test]
fn real_picker_queued_first_exit_wins_over_later_input() {
    for (keys, commit) in [(b"qjs\r".as_slice(), false), (b"\rjsq".as_slice(), true)] {
        let (home, env) = queued_picker_fixture();
        let mut picker = PickerProcess::start(home.path());
        picker.wait_for_output("s 保存配对");
        picker.finish(keys);
        assert_eq!(
            fs::read_to_string(env.managed_file("current")).unwrap(),
            "catppuccin-mocha"
        );
        assert!(
            !env.managed_file("auto.toml").exists(),
            "keys after exit were executed"
        );
        assert_eq!(
            env.managed_file("managed/ghostty/theme.conf").exists(),
            commit
        );
        assert!(!env.slate_cache_dir().join("preview-session.json").exists());
    }
}

#[test]
fn real_picker_cancel_restores_files_and_commit_snapshots_pre_preview_bytes() {
    for commit in [false, true] {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        let entry = env.xdg_config_home().join("ghostty/config.ghostty");
        let original = b"# original user comment\nbackground = #123456\nfont-size = 17\n";
        fs::create_dir_all(entry.parent().unwrap()).unwrap();
        fs::write(&entry, original).unwrap();
        fs::set_permissions(&entry, fs::Permissions::from_mode(0o600)).unwrap();
        fs::create_dir_all(env.config_dir()).unwrap();
        fs::write(env.managed_file("current"), "catppuccin-mocha").unwrap();
        fs::write(env.managed_file("current-font"), "Fixture Mono").unwrap();
        fs::write(
            env.managed_file("config.toml"),
            "[sound]\nenabled = false\n",
        )
        .unwrap();
        let managed = env.config_dir().join("managed/ghostty/theme.conf");
        let mut picker = PickerProcess::start(td.path());
        picker.wait_for(|| fs::read_to_string(&entry).is_ok_and(|text| text.contains("blur.conf")));
        let first = fs::read(&managed).unwrap();
        picker.terminal.write_all(b"\x1b[B").unwrap();
        picker.wait_for(|| fs::read(&managed).is_ok_and(|bytes| bytes != first));
        picker.finish(if commit { b"\r" } else { b"q" });
        let points = list_restore_points_with_env(&env).unwrap();
        if commit {
            assert!(String::from_utf8_lossy(&picker.output).contains("正在应用…"));
            assert!(
                !picker.output.windows(4).any(|bytes| bytes == b"\x1b[7m"),
                "confirmation must not flash reverse video"
            );
            assert_eq!(points.len(), 1);
            let backup = points[0]
                .entries
                .iter()
                .find(|file| file.original_path == entry)
                .unwrap();
            assert_eq!(
                fs::read(backup.backup_path.as_ref().unwrap()).unwrap(),
                original
            );
            assert_ne!(
                fs::read_to_string(env.managed_file("current")).unwrap(),
                "catppuccin-mocha"
            );
            assert!(managed.exists());
        } else {
            assert!(points.is_empty());
            assert_eq!(fs::read(&entry).unwrap(), original);
            assert_eq!(
                fs::metadata(&entry).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert_eq!(
                fs::read_to_string(env.managed_file("current")).unwrap(),
                "catppuccin-mocha"
            );
            assert!(!managed.exists());
        }
    }
}

#[test]
fn real_picker_window_style_upgrade_keeps_cancel_and_commit_recoverable() {
    for commit in [false, true] {
        let (home, env) = queued_picker_fixture();
        let entry = env.xdg_config_home().join("ghostty/config.ghostty");
        let original_entry = b"# user window preference\nmacos-titlebar-style = tabs\n";
        fs::write(&entry, original_entry).unwrap();
        fs::set_permissions(&entry, fs::Permissions::from_mode(0o600)).unwrap();
        let managed = env.config_dir().join("managed/ghostty/theme.conf");
        fs::create_dir_all(managed.parent().unwrap()).unwrap();
        let original_theme =
            b"window-theme = dark\nmacos-titlebar-style = transparent\nbackground = #123456\n";
        fs::write(&managed, original_theme).unwrap();
        fs::set_permissions(&managed, fs::Permissions::from_mode(0o640)).unwrap();

        let mut picker = PickerProcess::start(home.path());
        picker.wait_for(|| {
            fs::read_to_string(&managed).is_ok_and(|text| {
                text.contains("palette = ") && !text.contains("macos-titlebar-style")
            })
        });
        let first = fs::read(&managed).unwrap();
        picker.terminal.write_all(b"\x1b[B").unwrap();
        picker.wait_for(|| fs::read(&managed).is_ok_and(|bytes| bytes != first));
        picker.finish(if commit { b"\r" } else { b"q" });

        let points = list_restore_points_with_env(&env).unwrap();
        if commit {
            assert_eq!(points.len(), 1);
            for (path, original) in [
                (&entry, original_entry.as_slice()),
                (&managed, original_theme.as_slice()),
            ] {
                let backup = points[0]
                    .entries
                    .iter()
                    .find(|file| &file.original_path == path)
                    .unwrap();
                assert_eq!(
                    fs::read(backup.backup_path.as_ref().unwrap()).unwrap(),
                    original
                );
            }
            let entry_text = fs::read_to_string(&entry).unwrap();
            assert_eq!(entry_text.matches("macos-titlebar-style = tabs").count(), 1);
            assert!(!fs::read_to_string(&managed)
                .unwrap()
                .contains("macos-titlebar-style"));
        } else {
            assert!(points.is_empty());
            assert_eq!(fs::read(&entry).unwrap(), original_entry);
            assert_eq!(fs::read(&managed).unwrap(), original_theme);
            assert_eq!(
                fs::metadata(&entry).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert_eq!(
                fs::metadata(&managed).unwrap().permissions().mode() & 0o777,
                0o640
            );
        }
    }
}
