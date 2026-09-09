use super::*;
use std::{
    io::{Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::process::CommandExt,
    },
    process::{Child, Command, Stdio},
    time::Instant,
};

struct Native {
    child: Child,
    terminal: fs::File,
}
impl Drop for Native {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
#[ignore = "requires SLATE_YAZI_BINARY; native program runs only in disposable PTYs"]
fn yazi_native_loads_flavor_and_renders_dark_and_light_colors() {
    let binary = std::env::var_os("SLATE_YAZI_BINARY").expect("explicit Yazi executable");
    for id in ["nord", "catppuccin-latte"] {
        let home = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        let themes = ThemeRegistry::new().unwrap();
        let theme = themes.get(id).unwrap();
        YaziAdapter.apply_theme_with_env(theme, &env).unwrap();
        let config_before = YaziAdapter::paths(&env).map(|path| fs::read(path).unwrap());
        let cwd = home.path().join("demo-project");
        seed(
            &cwd.join("sample.rs"),
            "fn main() { println!(\"sample\"); }\n",
        );
        // The only external preview helper available to this fixture is file.
        fs::create_dir_all(env.user_local_bin()).unwrap();
        std::os::unix::fs::symlink("/usr/bin/file", env.user_local_bin().join("file")).unwrap();
        let (mut master, mut slave) = (-1, -1);
        let mut size = libc::winsize {
            ws_row: 32,
            ws_col: 120,
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
        let terminal = unsafe { fs::File::from_raw_fd(master) };
        let slave = unsafe { fs::File::from_raw_fd(slave) };
        let flags = unsafe { libc::fcntl(terminal.as_raw_fd(), libc::F_GETFL) };
        assert_eq!(
            unsafe {
                libc::fcntl(
                    terminal.as_raw_fd(),
                    libc::F_SETFL,
                    flags | libc::O_NONBLOCK,
                )
            },
            0
        );
        let mut cmd = Command::new(&binary);
        cmd.env_clear()
            .env("HOME", home.path())
            .env("YAZI_CONFIG_HOME", env.yazi_config_home())
            .env("XDG_CONFIG_HOME", env.xdg_config_home())
            .env("XDG_CACHE_HOME", home.path().join(".cache"))
            .env("XDG_STATE_HOME", home.path().join("state"))
            .env("TMPDIR", home.path())
            .env("PATH", env.user_local_bin())
            .env("TERM", "xterm-256color")
            .env("COLORTERM", "truecolor")
            .env("LANG", "en_US.UTF-8")
            .current_dir(&cwd)
            .stdin(Stdio::from(slave.try_clone().unwrap()))
            .stdout(Stdio::from(slave.try_clone().unwrap()))
            .stderr(Stdio::from(slave));
        unsafe {
            cmd.pre_exec(|| {
                if libc::setsid() == -1 || libc::ioctl(0, libc::TIOCSCTTY as _, 0) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut native = Native {
            child: cmd.spawn().unwrap(),
            terminal,
        };
        let rgb: Vec<_> = [1, 3, 5]
            .into_iter()
            .map(|i| u8::from_str_radix(&theme.palette.cyan[i..i + 2], 16).unwrap())
            .collect();
        let expected = format!("38;2;{};{};{}", rgb[0], rgb[1], rgb[2]);
        let code_rgb: Vec<_> = [1, 3, 5]
            .into_iter()
            .map(|i| u8::from_str_radix(&theme.palette.blue[i..i + 2], 16).unwrap())
            .collect();
        let code_color = format!("38;2;{};{};{}mmain", code_rgb[0], code_rgb[1], code_rgb[2]);
        let start = Instant::now();
        let mut output = Vec::new();
        let mut answered_background = false;
        let mut answered_device = false;
        loop {
            let mut chunk = [0; 16384];
            while let Ok(n) = native.terminal.read(&mut chunk) {
                if n == 0 {
                    break;
                }
                output.extend_from_slice(&chunk[..n]);
                assert!(
                    output.len() < 1024 * 1024,
                    "unexpected unbounded native output"
                );
            }
            let text = String::from_utf8_lossy(&output);
            // A real terminal answers OSC 11. Yazi applies its flavor after
            // determining appearance; an unanswered bare PTY stays at presets.
            if !answered_background && text.contains("\x1b]11;?") {
                let bg = &theme.palette.background;
                let reply = format!(
                    "\x1b]11;rgb:{0}{0}/{1}{1}/{2}{2}\x1b\\",
                    &bg[1..3],
                    &bg[3..5],
                    &bg[5..7]
                );
                native.terminal.write_all(reply.as_bytes()).unwrap();
                answered_background = true;
            }
            if !answered_device && text.contains("\x1b[0c") {
                // Primary device attributes terminates the capability probe;
                // Yazi waits for that probe again during graceful shutdown.
                native.terminal.write_all(b"\x1b[?62;c").unwrap();
                answered_device = true;
            }
            if text.contains(&expected) && text.contains("sample.rs") && text.contains(&code_color)
            {
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(8),
                "Yazi did not render {id}: {text:?}"
            );
            assert!(
                native.child.try_wait().unwrap().is_none(),
                "Yazi exited early: {text:?}"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        native.terminal.write_all(b"q").unwrap();
        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            if let Some(status) = native.child.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Yazi failed to quit: {:?}",
                String::from_utf8_lossy(&output)
            );
            let mut chunk = [0; 16384];
            while let Ok(n) = native.terminal.read(&mut chunk) {
                if n == 0 {
                    break;
                }
                output.extend_from_slice(&chunk[..n]);
                assert!(output.len() < 1024 * 1024);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(
            YaziAdapter::paths(&env).map(|path| fs::read(path).unwrap()),
            config_before
        );
    }
}
