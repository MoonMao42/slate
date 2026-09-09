use super::*;

fn resize(picker: &mut PickerProcess, cols: u16, rows: u16) {
    let mut size = libc::winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
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
}

fn last_frame(picker: &mut PickerProcess) -> String {
    picker.drain();
    let output = String::from_utf8_lossy(&picker.output);
    let start = output.rfind("\x1b[2J").expect("picker redraw");
    console::strip_ansi_codes(&output[start..]).into_owned()
}

fn send(picker: &mut PickerProcess, keys: &[u8], expected: &str) -> String {
    picker.output.clear();
    picker.terminal.write_all(keys).unwrap();
    picker.wait_for_output(expected);
    // Wait for the sticky footer too, not only an early matching body token.
    picker.wait_for_output("Esc 取消");
    last_frame(picker)
}

#[test]
fn real_picker_paging_resize_and_cancel_keep_configuration_unchanged() {
    let (home, env) = queued_picker_fixture();
    let bin = home.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let starship = bin.join("starship");
    fs::write(
        &starship,
        "#!/bin/sh\nprintf x >> \"$HOME/starship.calls\"\n/bin/cat \"$HOME/prompt-output\"\n",
    )
    .unwrap();
    fs::set_permissions(starship, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(
        home.path().join("prompt-output"),
        (0..100)
            .map(|i| format!("prompt-{i:03}\n"))
            .collect::<String>(),
    )
    .unwrap();
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
            fs::read(path).unwrap(),
        )
    };
    let mut picker = PickerProcess::start_with_path(home.path(), &["theme"], Some(&bin));
    picker.wait_for_output("s 保存配对");
    picker.output.clear();
    resize(&mut picker, 40, 12);
    picker.wait_for_output("s 保存配对");
    let initial = send(&mut picker, b"\t", "PgUp/PgDn");
    assert!(initial.contains("◆ 调色板"));
    let before = (stamp(&journal), stamp(&managed));
    let down = send(&mut picker, b"\x1b[6~", "PgUp/PgDn");
    assert_ne!(down, initial);
    assert!(down.contains("prompt-"));
    assert_eq!(send(&mut picker, b"\x1b[5~", "PgUp/PgDn"), initial);
    assert!(send(&mut picker, b"\x1b[F", "PgUp/PgDn").contains("◆ Nvim"));
    assert_eq!(send(&mut picker, b"\x1b[H", "PgUp/PgDn"), initial);
    let end = send(&mut picker, b"\x1b[F", "PgUp/PgDn");
    assert!(end.contains("◆ Nvim"));
    assert!(end.split("\r\n").count() <= 12);
    assert!(end
        .lines()
        .all(|line| console::measure_text_width(line) < 40));
    assert_eq!(fs::read(home.path().join("starship.calls")).unwrap(), b"x");
    assert_eq!((stamp(&journal), stamp(&managed)), before);

    picker.output.clear();
    resize(&mut picker, 80, 24);
    picker.wait_for_output("PgUp/PgDn");
    picker.wait_for_output("Esc 取消");
    assert!(last_frame(&mut picker).contains("◆ Nvim"));
    let up = send(&mut picker, b"\x1b[5~", "PgUp/PgDn");
    assert!(
        !up.contains("◆ Nvim"),
        "PageUp after resize must leave the bottom"
    );
    assert_eq!(
        fs::read(home.path().join("starship.calls")).unwrap(),
        b"xx",
        "only resize refreshes the cached prompt"
    );
    assert_eq!((stamp(&journal), stamp(&managed)), before);
    assert!(!env.managed_file("auto.toml").exists());
    picker.finish(b"\x1b");
    assert!(!journal.exists());
    assert!(!managed.exists());
    assert_eq!(
        fs::read(env.xdg_config_home().join("ghostty/config.ghostty")).unwrap(),
        b"# private terminal fixture\n"
    );
    assert_eq!(
        fs::read_to_string(env.managed_file("current")).unwrap(),
        "catppuccin-mocha"
    );
}
