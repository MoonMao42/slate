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
#[ignore = "requires explicit SLATE_BTOP_BINARY; runs btop only inside owned PTYs"]
fn btop_native_loads_generated_dark_and_light_palettes() {
    let binary = std::env::var_os("SLATE_BTOP_BINARY").expect("explicit native btop binary");
    for id in ["nord", "catppuccin-latte"] {
        let home = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        let registry = ThemeRegistry::new().unwrap();
        let theme = registry.get(id).unwrap();
        BtopAdapter.apply_theme_with_env(theme, &env).unwrap();
        let (mut master, mut slave) = (-1, -1);
        let mut size = libc::winsize {
            ws_row: 40,
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
            .env("XDG_CONFIG_HOME", env.xdg_config_home())
            .env("XDG_STATE_HOME", home.path().join("state"))
            .env("TERM", "xterm-256color")
            .env("LANG", "en_US.UTF-8")
            .args(["--force-utf", "--no-tty", "--debug"])
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
            .map(|i| u8::from_str_radix(&theme.palette.foreground[i..i + 2], 16).unwrap())
            .collect();
        let expected = format!("\x1b[38;2;{};{};{}m", rgb[0], rgb[1], rgb[2]);
        let started = Instant::now();
        let mut output = Vec::new();
        loop {
            let mut chunk = [0; 16384];
            while let Ok(count) = native.terminal.read(&mut chunk) {
                if count == 0 {
                    break;
                }
                output.extend_from_slice(&chunk[..count]);
            }
            if String::from_utf8_lossy(&output).contains(&expected) {
                break;
            }
            assert!(
                started.elapsed() < Duration::from_secs(8),
                "native btop did not render the selected palette"
            );
            assert!(
                native.child.try_wait().unwrap().is_none(),
                "native btop exited before rendering"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        native.terminal.write_all(b"q").unwrap();
        let deadline = Instant::now() + Duration::from_secs(4);
        while native.child.try_wait().unwrap().is_none() {
            assert!(Instant::now() < deadline, "native btop failed to exit");
            let mut chunk = [0; 16384];
            while native.terminal.read(&mut chunk).is_ok_and(|n| n > 0) {}
            std::thread::sleep(Duration::from_millis(20));
        }
        let log = fs::read_to_string(home.path().join("state/btop.log")).unwrap();
        assert!(log.contains(&format!(
            "Loading theme file: {}",
            BtopAdapter::theme_path(&env).display()
        )));
        assert!(!log.contains("Invalid hex") && !log.contains("Invalid RGB"));
    }
}
