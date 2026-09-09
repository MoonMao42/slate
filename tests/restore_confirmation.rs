//! Real confirmation prompts and library plans use private HOME/SLATE_HOME only.
use slate_cli::config::{
    begin_restore_point_baseline_with_env, execute_prepared_restore, execute_restore_with_env,
    list_restore_points_with_env, prepare_restore_with_env, ConfigManager, ConfigWriteGuard,
};
use slate_cli::env::SlateEnv;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::{symlink, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

#[path = "support/tree.rs"]
mod tree_snapshot;

struct RestoreProcess {
    child: Child,
    terminal: File,
    output: Vec<u8>,
}

impl RestoreProcess {
    fn start(home: &Path, id: &str) -> Self {
        Self::start_args(home, if id.is_empty() { vec![] } else { vec![id] })
    }

    fn start_args(home: &Path, args: Vec<&str>) -> Self {
        let mut command_args = vec!["--quiet", "restore"];
        command_args.extend(args);
        Self::start_command(home, command_args)
    }

    fn start_command(home: &Path, args: Vec<&str>) -> Self {
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
        // Optional explicit binary for repeating this private PTY check after
        // developer installation. Never changes the child's HOME/profile.
        let binary = std::env::var_os("SLATE_RESTORE_TEST_BINARY")
            .unwrap_or_else(|| assert_cmd::cargo::cargo_bin!("slate").into());
        let mut command = Command::new(binary);
        command
            .env_clear()
            .env("LANG", "en_US.UTF-8")
            .env(
                "LC_ALL",
                if cfg!(target_os = "macos") {
                    "en_US.UTF-8"
                } else {
                    "C.UTF-8"
                },
            )
            .env("HOME", home)
            .env("SLATE_HOME", home)
            .env("PATH", home.join("bin"))
            .env("TERM", "xterm-256color")
            .env("NO_COLOR", "1")
            .stdin(Stdio::from(slave.try_clone().unwrap()))
            .stdout(Stdio::from(slave.try_clone().unwrap()))
            .stderr(Stdio::from(slave));
        command.args(args);
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
        let mut buffer = [0; 8192];
        while let Ok(count) = self.terminal.read(&mut buffer) {
            if count == 0 {
                break;
            }
            self.output.extend_from_slice(&buffer[..count]);
        }
    }

    fn resize(&self, rows: u16, columns: u16) {
        let size = libc::winsize {
            ws_row: rows,
            ws_col: columns,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        assert_eq!(
            unsafe { libc::ioctl(self.terminal.as_raw_fd(), libc::TIOCSWINSZ, &size) },
            0
        );
        assert_eq!(
            unsafe { libc::kill(self.child.id() as i32, libc::SIGWINCH) },
            0
        );
    }

    fn wait_for_prompt(&mut self) {
        let start = Instant::now();
        loop {
            self.drain();
            let text = String::from_utf8_lossy(&self.output);
            if text.contains("Continue?")
                || text.contains("This will modify your configuration files.")
                || text.contains("选择恢复点：")
            {
                return;
            }
            assert!(
                self.child.try_wait().unwrap().is_none(),
                "{}",
                String::from_utf8_lossy(&self.output)
            );
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "confirmation timeout: {}",
                String::from_utf8_lossy(&self.output)
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
                return status;
            }
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "restore did not finish: {}",
                String::from_utf8_lossy(&self.output)
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn wait_for_text_since(&mut self, offset: usize, needle: &str) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            self.drain();
            if String::from_utf8_lossy(&self.output[offset..]).contains(needle) {
                return;
            }
            assert!(self.child.try_wait().unwrap().is_none());
            assert!(
                Instant::now() < deadline,
                "missing {needle}: {}",
                String::from_utf8_lossy(&self.output)
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for RestoreProcess {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
            // A killed macOS PTY session leader may need its master closed.
            if let Ok(null) = File::open("/dev/null") {
                self.terminal = null;
            }
            let _ = self.child.wait();
        }
    }
}

#[test]
fn hub_restore_refreshes_saved_language_without_restarting_or_prompting() {
    use slate_cli::config::ui_language::{self, UiLanguage};
    for saved in [Some(UiLanguage::Chinese), None] {
        let home = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        ConfigManager::with_env(&env).unwrap();
        if let Some(language) = saved {
            ui_language::save(&env, language).unwrap();
        }
        fs::write(env.zshrc_path(), "# original\n").unwrap();
        slate_cli::config::snapshot_current_state_with_env(&env, "pre-config").unwrap();
        ui_language::save(&env, UiLanguage::English).unwrap();
        let mut hub = RestoreProcess::start_command(home.path(), vec!["--quiet"]);
        hub.wait_for_text_since(0, "What would you like to change?");
        hub.wait_for_text_since(0, "└");
        let offset = hub.output.len();
        hub.terminal
            .write_all(b"\x1b[B\x1b[B\x1b[B\x1b[B\x1b[B\x1b[B\x1b[B\r")
            .unwrap();
        hub.wait_for_text_since(offset, "Choose restore point:");
        hub.wait_for_text_since(offset, "└");
        let offset = hub.output.len();
        hub.terminal.write_all(b"\r").unwrap();
        hub.wait_for_text_since(offset, "● Cancel");
        hub.wait_for_text_since(offset, "└");
        let offset = hub.output.len();
        hub.terminal.write_all(b"y").unwrap();
        hub.wait_for_text_since(
            offset,
            if saved.is_some() {
                "● 恢复配置"
            } else {
                "● Restore Configuration"
            },
        );
        hub.wait_for_text_since(offset, "└");
        let returned = String::from_utf8_lossy(&hub.output[offset..]);
        assert!(!returned.contains("语言 / Language"));
        assert_eq!(ui_language::read(&env).unwrap(), saved);
        assert!(hub.answer(b"\x1b").success());
    }
}

#[test]
fn restore_english_menu_cancel_and_apply_preserve_review_and_undo() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    ConfigManager::with_env(&env).unwrap();
    slate_cli::config::ui_language::save(&env, slate_cli::config::ui_language::UiLanguage::English)
        .unwrap();
    fs::write(env.zshrc_path(), "# original\n").unwrap();
    slate_cli::config::snapshot_current_state_with_env(&env, "pre-config").unwrap();
    fs::write(env.zshrc_path(), "# personal\n").unwrap();
    let before = tree_snapshot::tree(home.path());
    for keys in [b"\r".as_slice(), b"\x1b"] {
        let mut browser = RestoreProcess::start(home.path(), "");
        browser.wait_for_text_since(0, "Choose restore point:");
        browser.wait_for_text_since(0, "└");
        let offset = browser.output.len();
        browser.terminal.write_all(b"\r").unwrap();
        browser.wait_for_text_since(offset, "● Cancel");
        browser.wait_for_text_since(offset, "└");
        let review = String::from_utf8_lossy(&browser.output[offset..]);
        for text in [
            "Restore Preview · Before Settings Change",
            "Files will change as listed above",
            "undo point",
            "not rolled back automatically",
            "Full file list:",
        ] {
            assert!(review.contains(text), "missing {text}: {review}");
        }
        assert!(!review.contains("恢复") && !review.contains("Esc 返回"));
        let offset = browser.output.len();
        browser.terminal.write_all(keys).unwrap();
        browser.wait_for_text_since(offset, "Choose restore point:");
        browser.wait_for_text_since(offset, "└");
        assert!(browser.answer(b"\x1b").success());
        assert_eq!(tree_snapshot::tree(home.path()), before);
    }
    let mut accepted = RestoreProcess::start(home.path(), "");
    accepted.wait_for_text_since(0, "└");
    accepted.terminal.write_all(b"\r").unwrap();
    accepted.wait_for_text_since(0, "● Cancel");
    assert!(accepted.answer(b"y").success());
    assert_eq!(fs::read(env.zshrc_path()).unwrap(), b"# original\n");
    let output = String::from_utf8_lossy(&accepted.output);
    assert!(output.contains("file records.") && output.contains("appearance was not verified"));
    let undo = list_restore_points_with_env(&env)
        .unwrap()
        .into_iter()
        .find(|point| point.is_undo_checkpoint())
        .unwrap();
    assert!(output.contains(&format!(
        "Review before undoing: slate restore {} --dry-run",
        undo.id
    )));
    let receipt = execute_restore_with_env(&env, &undo.id).unwrap();
    assert!(receipt.results.iter().all(|result| result.success));
    assert_eq!(fs::read(env.zshrc_path()).unwrap(), b"# personal\n");
}

#[test]
fn restore_menu_uses_readable_checkpoint_names_through_confirmation() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    ConfigManager::with_env(&env).unwrap();
    drop(ConfigWriteGuard::acquire(&env).unwrap());
    fs::write(env.zshrc_path(), "# original\n").unwrap();
    slate_cli::config::snapshot_current_state_with_env(&env, "pre-config").unwrap();
    fs::write(env.zshrc_path(), "# later personal settings\n").unwrap();
    let before = tree_snapshot::tree(home.path());
    let mut browser = RestoreProcess::start(home.path(), "");
    browser.wait_for_text_since(0, "UTC · 设置修改前");
    browser.terminal.write_all(b"\r").unwrap();
    browser.wait_for_text_since(0, "● 取消");
    let text = String::from_utf8_lossy(&browser.output);
    assert!(text.contains("恢复预览 · 设置修改前"));
    assert!(text.contains("恢复到「设置修改前」"));
    let offset = browser.output.len();
    browser.terminal.write_all(b"\x1b").unwrap();
    browser.wait_for_text_since(offset, "UTC · 设置修改前");
    assert!(browser.answer(b"\x1b").success());
    assert_eq!(tree_snapshot::tree(home.path()), before);

    let mut accepted = RestoreProcess::start(home.path(), "");
    accepted.wait_for_text_since(0, "UTC · 设置修改前");
    accepted.terminal.write_all(b"\r").unwrap();
    accepted.wait_for_text_since(0, "● 取消");
    assert!(accepted.answer(b"y").success());
    assert_eq!(fs::read(env.zshrc_path()).unwrap(), b"# original\n");
    let output = String::from_utf8_lossy(&accepted.output);
    assert!(output.contains("项文件记录") && output.contains("外观未验证"));
    assert!(!output.contains("Restore results for") && !output.contains("Back on track"));
    let undo = list_restore_points_with_env(&env)
        .unwrap()
        .into_iter()
        .find(|point| point.is_undo_checkpoint())
        .unwrap();
    assert!(output.contains(&format!(
        "撤销前先查看：slate restore {} --dry-run",
        undo.id
    )));
    let receipt = execute_restore_with_env(&env, &undo.id).unwrap();
    assert!(receipt.results.iter().all(|result| result.success));
    assert_eq!(
        fs::read(env.zshrc_path()).unwrap(),
        b"# later personal settings\n"
    );
}

#[test]
fn delete_confirmation_declines_safely_and_only_deletes_selected_snapshot() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    ConfigManager::with_env(&env).unwrap();
    drop(ConfigWriteGuard::acquire(&env).unwrap());
    fs::write(env.zshrc_path(), "# keep personal settings\n").unwrap();
    let baseline = begin_restore_point_baseline_with_env(&env).unwrap();
    let point = slate_cli::config::snapshot_current_state_with_env(&env, "nord").unwrap();
    let before = tree_snapshot::tree(home.path());
    for keys in [
        b"\r".as_slice(),
        b"\x1b",
        b"n",
        b"\x03",
        b"\x1b[B\x1b",
        b"\x1b[200~y\r\x1b[201~\x1b",
    ] {
        let mut child = RestoreProcess::start_args(home.path(), vec!["--delete", &point.id]);
        child.wait_for_text_since(0, "● No");
        assert_eq!(
            child.answer(keys).code(),
            Some(if keys == b"\x03" { 130 } else { 0 })
        );
        assert_eq!(tree_snapshot::tree(home.path()), before);
    }
    let mut hidden = RestoreProcess::start_args(home.path(), vec!["--delete", &point.id]);
    hidden.wait_for_text_since(0, "● No");
    let offset = hidden.output.len();
    hidden.resize(2, 80);
    hidden.wait_for_text_since(offset, "窗口太小");
    hidden.terminal.write_all(b"yY\r").unwrap();
    std::thread::sleep(Duration::from_millis(150));
    assert!(hidden.child.try_wait().unwrap().is_none());
    assert_eq!(tree_snapshot::tree(home.path()), before);
    let offset = hidden.output.len();
    hidden.resize(40, 160);
    hidden.wait_for_text_since(offset, "● No");
    assert!(hidden.answer(b"\x1b").success());
    assert_eq!(tree_snapshot::tree(home.path()), before);

    let mut child = RestoreProcess::start_args(home.path(), vec!["--delete", &point.id]);
    child.wait_for_text_since(0, "● No");
    assert!(child.answer(b"y").success());
    let remaining = list_restore_points_with_env(&env).unwrap();
    assert!(remaining.iter().any(|p| p.id == baseline.id));
    assert!(!remaining.iter().any(|p| p.id == point.id));
    assert_eq!(
        fs::read(env.zshrc_path()).unwrap(),
        b"# keep personal settings\n"
    );
    let before = tree_snapshot::tree(home.path());
    let mut protected = RestoreProcess::start_args(home.path(), vec!["--delete", &baseline.id]);
    assert!(protected.answer(b"").success());
    assert!(String::from_utf8_lossy(&protected.output).contains("Cannot delete baseline"));
    assert_eq!(tree_snapshot::tree(home.path()), before);
}

#[test]
fn edits_while_the_confirmation_prompt_is_open_require_a_new_preview() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    ConfigManager::with_env(&env).unwrap();
    drop(ConfigWriteGuard::acquire(&env).unwrap());
    fs::write(env.zshrc_path(), "saved original\n").unwrap();
    let point = begin_restore_point_baseline_with_env(&env).unwrap();
    fs::write(env.zshrc_path(), "before preview\n").unwrap();
    let mut child = RestoreProcess::start(home.path(), &point.id);
    child.wait_for_prompt();
    fs::write(env.zshrc_path(), "PRIVATE_EDIT_WHILE_CONFIRMING\n").unwrap();
    let before = tree_snapshot::tree(home.path());
    let status = child.answer(b"y\r");
    assert_eq!(
        status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&child.output)
    );
    let message = String::from_utf8_lossy(&child.output);
    assert!(message.contains("changed since preview"), "{message}");
    assert!(!message.contains("PRIVATE_EDIT_WHILE_CONFIRMING"));
    assert_eq!(tree_snapshot::tree(home.path()), before);
}

#[test]
fn confirmation_defaults_to_cancel_and_unchanged_inputs_restore_and_undo() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    ConfigManager::with_env(&env).unwrap();
    drop(ConfigWriteGuard::acquire(&env).unwrap());
    fs::write(env.zshrc_path(), b"original\xff\n").unwrap();
    fs::set_permissions(env.zshrc_path(), fs::Permissions::from_mode(0o600)).unwrap();
    let point = begin_restore_point_baseline_with_env(&env).unwrap();
    fs::write(env.zshrc_path(), b"current\xfe\n").unwrap();
    fs::set_permissions(env.zshrc_path(), fs::Permissions::from_mode(0o640)).unwrap();

    let before = tree_snapshot::tree(home.path());
    for keys in [
        b"\r".as_slice(),
        b"\x1b",
        b"n",
        b"N",
        b"\x03",
        b"\x1b[B\x1b",                // Escape declines even when Yes was highlighted.
        b"\x1b[200~y\r\x1b[201~\x1b", // Pasted answers are not consent.
    ] {
        let mut canceled = RestoreProcess::start(home.path(), &point.id);
        canceled.wait_for_text_since(0, "● No");
        assert_eq!(
            canceled.answer(keys).code(),
            Some(if keys == b"\x03" { 130 } else { 0 })
        );
        assert_eq!(tree_snapshot::tree(home.path()), before);
    }

    let mut accepted = RestoreProcess::start(home.path(), &point.id);
    accepted.wait_for_prompt();
    assert!(
        accepted.answer(b"y\r").success(),
        "{}",
        String::from_utf8_lossy(&accepted.output)
    );
    assert_eq!(fs::read(env.zshrc_path()).unwrap(), b"original\xff\n");
    assert_eq!(
        fs::metadata(env.zshrc_path()).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let points = list_restore_points_with_env(&env).unwrap();
    assert_eq!(points.len(), 2);
    let undo = points
        .iter()
        .find(|point| point.is_undo_checkpoint())
        .unwrap();
    assert!(execute_restore_with_env(&env, &undo.id)
        .unwrap()
        .is_fully_successful());
    assert_eq!(fs::read(env.zshrc_path()).unwrap(), b"current\xfe\n");
    assert_eq!(
        fs::metadata(env.zshrc_path()).unwrap().permissions().mode() & 0o777,
        0o640
    );
}

#[test]
fn restore_confirmation_rejects_answers_when_choices_are_hidden() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    drop(ConfigWriteGuard::acquire(&env).unwrap());
    fs::write(env.zshrc_path(), "original\n").unwrap();
    let point = begin_restore_point_baseline_with_env(&env).unwrap();
    fs::write(env.zshrc_path(), "current personal\n").unwrap();
    let before = tree_snapshot::tree(home.path());
    let mut confirmation = RestoreProcess::start(home.path(), &point.id);
    confirmation.wait_for_text_since(0, "● No");
    // Preselect Yes while visible, then hide both choices. Enter must not
    // execute a previously highlighted destructive action in a tiny window.
    confirmation.terminal.write_all(b"\x1b[B").unwrap();
    confirmation.wait_for_text_since(0, "● Yes");
    let offset = confirmation.output.len();
    confirmation.resize(2, 80);
    confirmation.wait_for_text_since(offset, "窗口太小");
    confirmation.terminal.write_all(b"yY\r").unwrap();
    std::thread::sleep(Duration::from_millis(150));
    confirmation.drain();
    assert!(confirmation.child.try_wait().unwrap().is_none());
    assert_eq!(tree_snapshot::tree(home.path()), before);
    let offset = confirmation.output.len();
    confirmation.resize(40, 160);
    confirmation.wait_for_text_since(offset, "● Yes");
    assert_eq!(tree_snapshot::tree(home.path()), before);
    assert!(confirmation.answer(b"\x1b").success());
    assert_eq!(tree_snapshot::tree(home.path()), before);
}

#[test]
fn restore_picker_escape_and_explicit_back_leave_checkpoint_and_files_unchanged() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    ConfigManager::with_env(&env).unwrap();
    drop(ConfigWriteGuard::acquire(&env).unwrap());
    fs::write(env.zshrc_path(), "# PRIVATE_ORIGINAL\n").unwrap();
    begin_restore_point_baseline_with_env(&env).unwrap();
    fs::write(env.zshrc_path(), "# PRIVATE_CURRENT\n").unwrap();
    let before = tree_snapshot::tree(home.path());
    for keys in [b"\x1b".as_slice(), b"\x1b[F\r".as_slice()] {
        let mut picker = RestoreProcess::start(home.path(), "");
        picker.wait_for_prompt();
        assert!(picker.answer(keys).success());
        let output = String::from_utf8_lossy(&picker.output);
        assert!(!output.contains("Operation cancelled"));
        assert!(!output.contains("This will modify your configuration files"));
        assert_eq!(tree_snapshot::tree(home.path()), before);
    }
}

#[test]
fn restore_browser_blocked_plan_returns_without_execution_or_writer_reservation() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    ConfigManager::with_env(&env).unwrap();
    drop(ConfigWriteGuard::acquire(&env).unwrap());
    fs::write(env.zshrc_path(), "original").unwrap();
    let point = begin_restore_point_baseline_with_env(&env).unwrap();
    fs::remove_file(env.zshrc_path()).unwrap();
    fs::create_dir(env.zshrc_path()).unwrap();
    let before = tree_snapshot::tree(home.path());
    for key in [b"\r".as_slice(), b"\x1b"] {
        let mut picker = RestoreProcess::start(home.path(), "");
        picker.wait_for_text_since(0, "└");
        picker.terminal.write_all(b"\r").unwrap();
        picker.wait_for_text_since(0, "● 返回恢复点列表");
        let text = String::from_utf8_lossy(&picker.output);
        assert!(text.contains("1 个受阻"));
        assert!(text.contains(env.zshrc_path().to_str().unwrap()));
        assert!(!text.contains("● No"));
        assert!(!text.contains("Undo this restore:"));
        let writer = ConfigWriteGuard::acquire(&env).unwrap();
        assert_eq!(tree_snapshot::tree(home.path()), before);
        let offset = picker.output.len();
        picker.terminal.write_all(key).unwrap();
        picker.wait_for_text_since(offset, "选择恢复点：");
        assert!(picker.answer(b"\x1b").success());
        drop(writer);
        assert_eq!(tree_snapshot::tree(home.path()), before);
    }
    // Explicit-ID automation retains a nonzero exit, never a navigation page.
    let mut direct = RestoreProcess::start(home.path(), &point.id);
    assert_eq!(direct.answer(b"").code(), Some(1));
    assert!(String::from_utf8_lossy(&direct.output).contains("restore target(s) are blocked"));
    assert_eq!(tree_snapshot::tree(home.path()), before);
}

#[test]
fn restore_browser_decline_returns_to_selected_point_without_holding_writer() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    ConfigManager::with_env(&env).unwrap();
    drop(ConfigWriteGuard::acquire(&env).unwrap());
    for _ in 0..3 {
        slate_cli::config::snapshot_current_state_with_env(&env, "nord").unwrap();
    }
    let before = tree_snapshot::tree(home.path());
    let mut picker = RestoreProcess::start(home.path(), "");
    picker.wait_for_text_since(0, "└");
    picker.terminal.write_all(b"\x1b[B").unwrap();
    for keys in [b"\r".as_slice(), b"\x1b"] {
        let offset = picker.output.len();
        picker.terminal.write_all(b"\r").unwrap();
        picker.wait_for_text_since(offset, "● 取消");
        let review = String::from_utf8_lossy(&picker.output[offset..]);
        assert!(review.contains("恢复预览 · nord"));
        assert!(review.contains("恢复到「nord」？将按上面的清单修改文件。"));
        assert!(review.contains("完整文件清单：slate restore"));
        assert!(review.contains("部分失败不会自动回滚"));
        assert!(!review.contains("◆ Restore preview:"));
        let offset = picker.output.len();
        picker.terminal.write_all(keys).unwrap();
        picker.wait_for_text_since(offset, "选择恢复点：");
        picker.wait_for_text_since(offset, "● 2.");
        let writer = ConfigWriteGuard::acquire(&env).unwrap();
        assert_eq!(tree_snapshot::tree(home.path()), before);
        drop(writer);
    }
    // A returned browser must not monopolize the writer or mutate snapshots.
    let writer = ConfigWriteGuard::acquire(&env).unwrap();
    assert_eq!(tree_snapshot::tree(home.path()), before);
    assert!(picker.answer(b"\x1b").success());
    drop(writer);
    assert_eq!(tree_snapshot::tree(home.path()), before);
}

#[test]
fn restore_browser_does_not_hold_writer_and_selection_rechecks_it() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    ConfigManager::with_env(&env).unwrap();
    drop(ConfigWriteGuard::acquire(&env).unwrap());
    fs::write(env.zshrc_path(), "# original\n").unwrap();
    begin_restore_point_baseline_with_env(&env).unwrap();
    fs::write(env.zshrc_path(), "# personal current\n").unwrap();
    let before = tree_snapshot::tree(home.path());
    let mut first = RestoreProcess::start(home.path(), "");
    first.wait_for_prompt();
    let _writer = ConfigWriteGuard::acquire(&env).expect("browsing must not hold a writer");
    let mut second = RestoreProcess::start(home.path(), "");
    second.wait_for_prompt();
    assert!(second.answer(b"\x1b").success());
    assert_eq!(first.answer(b"\r").code(), Some(1));
    assert!(String::from_utf8_lossy(&first.output)
        .contains(&slate_cli::error::SlateError::ConfigurationBusy.to_string()));
    assert_eq!(tree_snapshot::tree(home.path()), before);
}

#[test]
fn restore_browser_disambiguates_repeated_themes_and_bounds_visible_rows() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    ConfigManager::with_env(&env).unwrap();
    for _ in 0..12 {
        slate_cli::config::snapshot_current_state_with_env(&env, "nord").unwrap();
    }
    let before = tree_snapshot::tree(home.path());
    let mut picker = RestoreProcess::start(home.path(), "");
    picker.wait_for_prompt();
    // Wait for the completed menu, not only the heading's first write.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        picker.drain();
        if String::from_utf8_lossy(&picker.output).contains("└") {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = String::from_utf8_lossy(&picker.output);
    let frame = output.rsplit_once("选择恢复点：").unwrap().1;
    let rows: Vec<_> = frame
        .lines()
        .filter(|line| line.contains("● ") || line.contains("○ "))
        .collect();
    assert_eq!(rows.len(), 8, "{frame}");
    assert!(frame.contains("1/13"), "{frame}");
    for (index, row) in rows.iter().enumerate() {
        assert!(row.contains(&format!("{}. ", index + 1)), "{row}");
        assert!(row.contains(" UTC · nord"), "{row}");
    }
    assert!(picker.answer(b"\x1b[F\r").success());
    assert_eq!(tree_snapshot::tree(home.path()), before);
}

#[test]
fn empty_restore_browser_does_not_initialize_profile_or_sound_storage() {
    let home = tempfile::tempdir().unwrap();
    let output = Command::new(assert_cmd::cargo::cargo_bin!("slate"))
        .env_clear()
        .env("HOME", home.path())
        .env("SLATE_HOME", home.path())
        .env("PATH", "")
        .args(["restore"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output.stderr);
    assert_eq!(fs::read_dir(home.path()).unwrap().count(), 0);
}

#[test]
fn cli_undo_and_redo_restore_checkpoint_files_without_regenerating_theme() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let config = ConfigManager::with_env(&env).unwrap();
    config.set_current_theme("nord").unwrap();
    config.set_auto_theme_enabled(false).unwrap();
    drop(ConfigWriteGuard::acquire(&env).unwrap());
    let shell = env.managed_file("managed/shell/env.zsh");
    fs::create_dir_all(shell.parent().unwrap()).unwrap();
    fs::write(&shell, b"# PRIVATE_ORIGINAL_SHELL\xff\n").unwrap();
    fs::set_permissions(&shell, fs::Permissions::from_mode(0o600)).unwrap();
    fs::write(env.zshrc_path(), "# original zshrc\n").unwrap();
    let point = begin_restore_point_baseline_with_env(&env).unwrap();
    let original = profile_files(&env);
    fs::write(&shell, b"# PRIVATE_CURRENT_SHELL\xfe\n").unwrap();
    fs::set_permissions(&shell, fs::Permissions::from_mode(0o640)).unwrap();
    fs::write(env.zshrc_path(), "# current zshrc\n").unwrap();
    fs::write(env.bashrc_path(), "# newly created bashrc\n").unwrap();
    let current = profile_files(&env);

    let mut restore = RestoreProcess::start(home.path(), &point.id);
    restore.wait_for_prompt();
    assert!(restore.answer(b"y\r").success());
    assert_eq!(profile_files(&env), original);
    let points = list_restore_points_with_env(&env).unwrap();
    assert_eq!(points.len(), 2);
    let undo = points
        .iter()
        .find(|point| point.is_undo_checkpoint())
        .unwrap();

    let before_preview = tree_snapshot::tree(home.path());
    let output = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"))
        .env_clear()
        .env("HOME", home.path())
        .env("SLATE_HOME", home.path())
        .env("PATH", home.path().join("bin"))
        .env("NO_COLOR", "1")
        .args(["restore", &undo.id, "--dry-run", "--json"])
        .timeout(Duration::from_secs(5))
        .assert()
        .success()
        .get_output()
        .clone();
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(plan["may_regenerate_theme_files"], false);
    assert_eq!(tree_snapshot::tree(home.path()), before_preview);

    let mut canceled = RestoreProcess::start(home.path(), &undo.id);
    canceled.wait_for_prompt();
    assert!(canceled.answer(b"\r").success());
    assert_eq!(tree_snapshot::tree(home.path()), before_preview);

    let mut restore = RestoreProcess::start(home.path(), &undo.id);
    restore.wait_for_prompt();
    assert!(restore.answer(b"y\r").success());
    let output = String::from_utf8_lossy(&restore.output);
    assert!(
        profile_files(&env) == current,
        "Undo must restore checkpoint bytes, modes and absence without theme regeneration: {output}"
    );
    assert!(!output.contains("Re-applying theme:"), "{output}");
    assert!(output.contains("File-only restore:"), "{output}");
    assert!(!output.contains("PRIVATE_"));
    assert!(
        !slate_cli::config::preview_restore_with_env(&env, &undo.id)
            .unwrap()
            .may_regenerate_theme_files
    );

    let next_points = list_restore_points_with_env(&env).unwrap();
    assert_eq!(next_points.len(), 3);
    let redo = next_points
        .iter()
        .find(|point| points.iter().all(|previous| previous.id != point.id))
        .unwrap();
    assert!(redo.is_undo_checkpoint());
    let mut restore = RestoreProcess::start(home.path(), &redo.id);
    restore.wait_for_prompt();
    assert!(restore.answer(b"y\r").success());
    assert_eq!(profile_files(&env), original);
    assert_eq!(list_restore_points_with_env(&env).unwrap().len(), 4);
}

fn profile_files(env: &SlateEnv) -> std::collections::BTreeMap<std::path::PathBuf, (u32, Vec<u8>)> {
    tree_snapshot::tree(env.home())
        .into_iter()
        .filter(|(path, _)| {
            !fs::symlink_metadata(path).unwrap().is_dir()
                && !path.starts_with(env.slate_cache_dir().join("backups"))
                && path != &env.slate_cache_dir().join("preview-session.lock")
        })
        .collect()
}

#[test]
fn restore_reapplication_checks_shared_failure_and_exposes_undo() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let config = ConfigManager::with_env(&env).unwrap();
    config.set_current_theme("nord").unwrap();
    fs::write(
        env.managed_file("config.toml"),
        "[tools]\nstarship = 'PRIVATE_INVALID_BOOLEAN'\n",
    )
    .unwrap();
    let point = slate_cli::config::snapshot_current_state_with_env(&env, "nord").unwrap();
    config.set_current_theme("catppuccin-mocha").unwrap();
    fs::write(env.managed_file("config.toml"), "# valid current config\n").unwrap();

    let mut restore = RestoreProcess::start(home.path(), &point.id);
    restore.wait_for_prompt();
    let status = restore.answer(b"y\r");
    let message = String::from_utf8_lossy(&restore.output);
    assert_eq!(status.code(), Some(1), "{message}");
    assert!(
        message.contains("theme reapplication failed")
            && message.contains("shared shell configuration"),
        "{message}"
    );
    assert!(!message.contains("PRIVATE_INVALID_BOOLEAN"));
    assert!(!message.contains("Back on track"));
    let points = list_restore_points_with_env(&env).unwrap();
    let undo = points.iter().find(|p| p.is_undo_checkpoint()).unwrap();
    assert!(
        message.contains(&format!("slate restore {}", undo.id)),
        "{message}"
    );
    assert_eq!(config.get_current_theme().unwrap().as_deref(), Some("nord"));
    assert!(execute_restore_with_env(&env, &undo.id)
        .unwrap()
        .is_fully_successful());
    assert_eq!(
        config.get_current_theme().unwrap().as_deref(),
        Some("catppuccin-mocha")
    );
    assert_eq!(
        fs::read_to_string(env.managed_file("config.toml")).unwrap(),
        "# valid current config\n"
    );
}

#[test]
fn opacity_checkpoint_cli_restore_does_not_regenerate_theme() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let config = ConfigManager::with_env(&env).unwrap();
    config.set_current_theme("nord").unwrap();
    config
        .set_current_opacity_preset(slate_cli::opacity::OpacityPreset::Solid)
        .unwrap();
    drop(ConfigWriteGuard::acquire(&env).unwrap());
    let path = env.managed_file("managed/ghostty/opacity.conf");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"# private original\xff\r\n").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    let before = profile_files(&env);
    assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"))
        .env_clear()
        .env("HOME", home.path())
        .env("SLATE_HOME", home.path())
        .env("PATH", home.path().join("bin"))
        .env("NO_COLOR", "1")
        .args(["--quiet", "config", "set", "opacity", "frosted"])
        .timeout(Duration::from_secs(5))
        .assert()
        .success();
    let points = list_restore_points_with_env(&env).unwrap();
    assert_eq!(points.len(), 1);
    assert_eq!(points[0].theme_name, "pre-opacity");
    let mut restore = RestoreProcess::start(home.path(), &points[0].id);
    restore.wait_for_prompt();
    let status = restore.answer(b"y\r");
    let message = String::from_utf8_lossy(&restore.output);
    assert!(status.success(), "{message}");
    assert!(!message.contains("Re-applying theme:"), "{message}");
    assert!(message.contains("File-only restore:"), "{message}");
    assert_eq!(profile_files(&env), before);
}

#[test]
fn cli_baseline_restore_does_not_initialize_an_absent_slate_config_directory() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    drop(ConfigWriteGuard::acquire(&env).unwrap());
    fs::write(env.zshrc_path(), "# before Slate\n").unwrap();
    let point = begin_restore_point_baseline_with_env(&env).unwrap();
    fs::write(env.zshrc_path(), "# after Slate\n").unwrap();
    assert!(!env.config_dir().exists());
    let mut restore = RestoreProcess::start(home.path(), &point.id);
    restore.wait_for_prompt();
    assert!(restore.answer(b"y\r").success());
    assert_eq!(fs::read(env.zshrc_path()).unwrap(), b"# before Slate\n");
    assert!(
        !env.config_dir().exists(),
        "File restore must not initialize an unrelated config directory"
    );
    assert_eq!(list_restore_points_with_env(&env).unwrap().len(), 2);
}

#[test]
fn cli_named_theme_restore_still_discloses_and_performs_regeneration() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let config = ConfigManager::with_env(&env).unwrap();
    config.set_current_theme("nord").unwrap();
    config.set_auto_theme_enabled(false).unwrap();
    let shell = env.managed_file("managed/shell/env.zsh");
    fs::create_dir_all(shell.parent().unwrap()).unwrap();
    fs::write(&shell, "# exact saved shell\n").unwrap();
    let point = slate_cli::config::snapshot_current_state_with_env(&env, "nord").unwrap();
    assert!(
        slate_cli::config::preview_restore_with_env(&env, &point.id)
            .unwrap()
            .may_regenerate_theme_files
    );
    fs::write(&shell, "# changed shell\n").unwrap();
    let mut restore = RestoreProcess::start(home.path(), &point.id);
    restore.wait_for_prompt();
    assert!(restore.answer(b"y\r").success());
    let output = String::from_utf8_lossy(&restore.output);
    assert!(
        output.contains("additional changes are not included"),
        "{output}"
    );
    assert!(output.contains("Re-applying theme: Nord"), "{output}");
    assert!(!output.contains("File-only restore:"), "{output}");
    let actual = fs::read(&shell).unwrap();
    assert_ne!(actual, b"# exact saved shell\n");
    assert_ne!(actual, b"# changed shell\n");
    assert_eq!(list_restore_points_with_env(&env).unwrap().len(), 2);
}

#[test]
fn prepared_restore_rejects_changed_inputs_before_creating_an_undo_point() {
    for kind in [
        "target-bytes",
        "target-mode",
        "target-inode",
        "target-removed",
        "absent-created",
        "backup-bytes",
        "backup-inode",
        "manifest-target",
        "manifest-mode",
        "alias-redirect",
        "record-directory",
    ] {
        let home = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        ConfigManager::with_env(&env).unwrap();
        drop(ConfigWriteGuard::acquire(&env).unwrap());
        fs::write(env.zshrc_path(), "PRIVATE_ORIGINAL\n").unwrap();
        fs::set_permissions(env.zshrc_path(), fs::Permissions::from_mode(0o600)).unwrap();
        let point = begin_restore_point_baseline_with_env(&env).unwrap();
        fs::write(env.zshrc_path(), "PRIVATE_CURRENT\n").unwrap();
        let record = env.slate_cache_dir().join("backups").join(&point.id);
        let manifest = record.join("manifest.toml");
        let mut doc: toml::Value = fs::read_to_string(&manifest).unwrap().parse().unwrap();
        let index = doc["entries"]
            .as_array()
            .unwrap()
            .iter()
            .position(|entry| entry["tool_key"].as_str() == Some("zshrc"))
            .unwrap();
        let backup =
            std::path::PathBuf::from(doc["entries"][index]["backup_path"].as_str().unwrap());
        if kind == "alias-redirect" {
            fs::create_dir(home.path().join("first")).unwrap();
            fs::create_dir(home.path().join("second")).unwrap();
            symlink(home.path().join("first"), home.path().join("alias")).unwrap();
            doc["entries"][index]["original_path"] = home
                .path()
                .join("alias/config")
                .display()
                .to_string()
                .into();
            fs::write(&manifest, toml::to_string(&doc).unwrap()).unwrap();
        }
        let before_prepare = tree_snapshot::tree(home.path());
        let prepared = prepare_restore_with_env(&env, &point.id).unwrap();
        assert_eq!(prepared.plan().blocked_count(), 0, "{kind}");
        assert!(!format!("{prepared:?}").contains("PRIVATE_"));
        assert!(!serde_json::to_string(prepared.plan())
            .unwrap()
            .contains("PRIVATE_"));
        assert_eq!(tree_snapshot::tree(home.path()), before_prepare, "{kind}");

        match kind {
            "target-bytes" => fs::write(env.zshrc_path(), "PRIVATE_EDIT\n").unwrap(),
            "target-mode" => {
                fs::set_permissions(env.zshrc_path(), fs::Permissions::from_mode(0o640)).unwrap();
            }
            "target-inode" | "backup-inode" => {
                let target = if kind == "target-inode" {
                    env.zshrc_path()
                } else {
                    backup.clone()
                };
                let replacement = home.path().join("replacement");
                fs::copy(&target, &replacement).unwrap();
                fs::rename(replacement, target).unwrap();
            }
            "target-removed" => fs::remove_file(env.zshrc_path()).unwrap(),
            "absent-created" => fs::write(home.path().join(".bashrc"), "PRIVATE_NEW\n").unwrap(),
            "backup-bytes" => fs::write(&backup, "PRIVATE_CHANGED_BACKUP\n").unwrap(),
            "manifest-target" | "manifest-mode" => {
                if kind == "manifest-target" {
                    doc["entries"][index]["original_path"] =
                        home.path().join("redirected").display().to_string().into();
                } else {
                    doc["entries"][index]["unix_mode"] = 0o640.into();
                }
                fs::write(&manifest, toml::to_string(&doc).unwrap()).unwrap();
            }
            "alias-redirect" => {
                fs::remove_file(home.path().join("alias")).unwrap();
                symlink(home.path().join("second"), home.path().join("alias")).unwrap();
            }
            "record-directory" => {
                // Keep the manifest/backup file inodes, replacing only the point directory.
                let aside = home.path().join("record-aside");
                fs::rename(&record, &aside).unwrap();
                fs::create_dir(&record).unwrap();
                for entry in fs::read_dir(aside).unwrap() {
                    let entry = entry.unwrap();
                    fs::hard_link(entry.path(), record.join(entry.file_name())).unwrap();
                }
            }
            _ => unreachable!(),
        }
        let before_execute = tree_snapshot::tree(home.path());
        let error = execute_prepared_restore(prepared).unwrap_err().to_string();
        assert!(error.contains("changed since preview"), "{kind}: {error}");
        assert!(!error.contains("PRIVATE_"), "{kind}: {error}");
        assert_eq!(tree_snapshot::tree(home.path()), before_execute, "{kind}");
    }
}

#[test]
fn discarded_plans_are_read_only_and_manifest_comments_do_not_invalidate_confirmation() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    fs::write(env.zshrc_path(), "original\n").unwrap();
    let point = begin_restore_point_baseline_with_env(&env).unwrap();
    drop(ConfigWriteGuard::acquire(&env).unwrap());
    fs::write(env.zshrc_path(), "current\n").unwrap();
    let before = tree_snapshot::tree(home.path());
    drop(prepare_restore_with_env(&env, &point.id).unwrap());
    assert_eq!(tree_snapshot::tree(home.path()), before);

    let prepared = prepare_restore_with_env(&env, &point.id).unwrap();
    let manifest = env
        .slate_cache_dir()
        .join("backups")
        .join(&point.id)
        .join("manifest.toml");
    let content = fs::read_to_string(&manifest).unwrap();
    fs::write(
        &manifest,
        format!("# A comment changes no restore input.\n{content}\n"),
    )
    .unwrap();
    assert!(execute_prepared_restore(prepared)
        .unwrap()
        .is_fully_successful());
    assert_eq!(fs::read(env.zshrc_path()).unwrap(), b"original\n");
    assert_eq!(list_restore_points_with_env(&env).unwrap().len(), 2);
}

#[test]
fn preparation_is_read_only_and_execution_still_respects_another_writer() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    fs::write(env.zshrc_path(), "original\n").unwrap();
    let point = begin_restore_point_baseline_with_env(&env).unwrap();
    drop(ConfigWriteGuard::acquire(&env).unwrap());
    let before = tree_snapshot::tree(home.path());
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let writer_env = env.clone();
    let writer = std::thread::spawn(move || {
        let _guard = ConfigWriteGuard::acquire(&writer_env).unwrap();
        ready_tx.send(()).unwrap();
        let _ = release_rx.recv_timeout(Duration::from_secs(5));
    });
    ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let prepared = prepare_restore_with_env(&env, &point.id);
    let outcome = prepared.and_then(execute_prepared_restore);
    release_tx.send(()).unwrap();
    writer.join().unwrap();
    assert!(matches!(
        outcome,
        Err(slate_cli::error::SlateError::ConfigurationBusy)
    ));
    assert_eq!(tree_snapshot::tree(home.path()), before);
}
