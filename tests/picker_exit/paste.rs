use super::*;

fn paste(picker: &mut PickerProcess, text: &str, expected: &str) -> String {
    picker.output.clear();
    let bytes = format!("\x1b[200~{text}\x1b[201~");
    let mut remaining = bytes.as_bytes();
    let deadline = Instant::now() + Duration::from_secs(5);
    // The owned PTY is nonblocking. A large clipboard can exceed one kernel
    // write; retain the exact unsent suffix instead of duplicating partial input.
    while !remaining.is_empty() {
        assert!(Instant::now() < deadline, "paste input delivery timed out");
        match picker.terminal.write(remaining) {
            Ok(0) => panic!("private PTY closed during paste"),
            Ok(count) => remaining = &remaining[count..],
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                picker.drain();
                assert!(picker.child.try_wait().unwrap().is_none());
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => panic!("paste input failed: {error}"),
        }
    }
    picker.wait_for_output(expected);
    picker.wait_for_output("Esc 取消");
    picker.drain();
    let output = String::from_utf8_lossy(&picker.output);
    let start = output.rfind("\x1b[2J").expect("paste feedback redraw");
    console::strip_ansi_codes(&output[start..]).into_owned()
}

#[test]
fn real_picker_paste_multiline_requires_explicit_exit_and_restores_input_mode() {
    for commit in [false, true] {
        let (home, env) = queued_picker_fixture();
        let mut picker = PickerProcess::start(home.path());
        picker.wait_for_output("s 保存配对");
        assert!(String::from_utf8_lossy(&picker.output).contains("\x1b[?2004h"));
        let frame = paste(&mut picker, "DAWN\tRosé\r\n", "已忽略粘贴");
        assert!(!frame.contains("Search /"));
        assert!(frame.contains("› Catppuccin Mocha"));
        assert!(
            picker.child.try_wait().unwrap().is_none(),
            "pasted newline must not confirm"
        );
        assert_eq!(
            fs::read_to_string(env.managed_file("current")).unwrap(),
            "catppuccin-mocha"
        );
        assert!(!env.managed_file("auto.toml").exists());
        picker.finish(if commit { b"\r" } else { b"\x1b" });
        let output = String::from_utf8_lossy(&picker.output);
        let disabled = output.find("\x1b[?2004l").expect("paste mode cleanup");
        assert!(disabled < output.find("\x1b[?1049l").unwrap());
        assert!(!env.slate_cache_dir().join("preview-session.json").exists());
        assert_eq!(
            fs::read_to_string(env.managed_file("current")).unwrap(),
            "catppuccin-mocha"
        );
        if !commit {
            assert!(!env.managed_file("managed/ghostty/theme.conf").exists());
            assert_eq!(
                fs::read(env.xdg_config_home().join("ghostty/config.ghostty")).unwrap(),
                b"# private terminal fixture\n"
            );
        }
    }
}

#[test]
fn real_picker_paste_shortcut_letters_and_arbitrary_payloads_do_not_rewrite_preview() {
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
            fs::read(path).unwrap(),
        )
    };
    let mut picker = PickerProcess::start(home.path());
    picker.wait_for_output("s 保存配对");
    let before = (stamp(&journal), stamp(&managed));
    for text in [
        "s\nq\rhjkl\t".to_owned(),
        format!("rose dawn{}", "x".repeat(65)),
        " ".repeat(4097),
        "rose dawn\x1b[31mPRIVATE_PASTE".to_owned(),
    ] {
        let frame = paste(&mut picker, &text, "已忽略粘贴");
        assert!(!frame.contains("PRIVATE_PASTE"));
        assert!(!frame.contains("Search /"));
        assert!(picker.child.try_wait().unwrap().is_none());
        assert_eq!((stamp(&journal), stamp(&managed)), before);
        assert!(!env.managed_file("auto.toml").exists());
    }
    picker.finish(b"q");
    assert!(String::from_utf8_lossy(&picker.output).contains("\x1b[?2004l"));
    assert!(!journal.exists());
    assert!(!managed.exists());
    assert_eq!(
        fs::read(env.xdg_config_home().join("ghostty/config.ghostty")).unwrap(),
        b"# private terminal fixture\n"
    );
}
