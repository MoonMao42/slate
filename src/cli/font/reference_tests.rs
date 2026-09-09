use super::*;

#[test]
fn font_literal_receipt_uses_kitty_continuations_and_complete_values() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("kitty.conf");
    let managed = Path::new("/slate-owned/kitty/font.conf");
    for (content, expected) in [
        ("include /slate-owned/kitty/\n\\font.conf\n", true),
        ("include\u{2003}/slate-owned/kitty/font.conf\n", true),
        ("include /slate-owned/kitty/font.conf\n\\.user\n", false),
        ("include /outside # /slate-owned/kitty/font.conf\n", false),
        ("include '/slate-owned/kitty/font.conf'\n", false),
        ("include = /slate-owned/kitty/font.conf\n", false),
    ] {
        std::fs::write(&config, content).unwrap();
        assert_eq!(
            file_contains_managed_ref(&config, managed, TerminalRefSyntax::Kitty),
            expected,
            "{content}"
        );
    }
}

#[test]
fn font_literal_receipt_distinguishes_missing_from_uninspectable_entries() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(temp.path().to_owned());
    let ghostty = env.xdg_config_home().join("ghostty/config.ghostty");
    let alacritty = env.xdg_config_home().join("alacritty/alacritty.toml");
    std::fs::create_dir_all(ghostty.parent().unwrap()).unwrap();
    std::fs::create_dir_all(alacritty.parent().unwrap()).unwrap();
    symlink(temp.path().join("absent"), &ghostty).unwrap();
    std::fs::write(&alacritty, "PRIVATE_CONTENT = [").unwrap();
    let report = collect_font_apply_report(&env);
    assert!(report.applied.is_empty());
    assert!(report
        .skipped
        .contains(&("Ghostty", "entry config could not be inspected")));
    assert!(report
        .skipped
        .contains(&("Alacritty", "entry config could not be inspected")));
    assert!(report.skipped.contains(&("Kitty", "missing kitty.conf")));
    assert!(!format_font_apply_report(None, &report)
        .unwrap()
        .contains("PRIVATE_CONTENT"));
    let fifo = env.xdg_config_home().join("kitty/kitty.conf");
    std::fs::create_dir_all(fifo.parent().unwrap()).unwrap();
    let fifo = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
    assert!(collect_font_apply_report(&env)
        .skipped
        .contains(&("Kitty", "entry config could not be inspected")));
}

#[test]
fn font_literal_receipt_uses_ghostty_values_and_resets_not_legacy_hints() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("ghostty.conf");
    let managed = Path::new("/slate-owned/ghostty/font.conf");
    for (content, expected) in [
        ("config-file = /slate-owned/ghostty/font.conf\n", true),
        (
            "\u{feff}config-file = ?/slate-owned/ghostty/font.conf\n",
            true,
        ),
        ("include = /slate-owned/ghostty/font.conf\n", false),
        (
            "config-file = /slate-owned/ghostty/font.conf\nconfig-file =\n",
            false,
        ),
        (
            "config-file = /outside # /slate-owned/ghostty/font.conf\n",
            false,
        ),
        ("config-file = '/slate-owned/ghostty/font.conf'\n", false),
    ] {
        std::fs::write(&config, content).unwrap();
        assert_eq!(
            file_contains_managed_ref(&config, managed, TerminalRefSyntax::Ghostty),
            expected,
            "{content}"
        );
    }
}
