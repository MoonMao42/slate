use super::*;

#[test]
fn ghostty_literal_font_reference_does_not_accept_embedded_or_decorated_paths() {
    let managed = Path::new("/slate-owned/ghostty/font.conf");
    for original in [
        "config-file = /outside/settings # /slate-owned/ghostty/font.conf\n",
        "config-file = '/slate-owned/ghostty/font.conf'\n",
        "config-file = /slate-owned/ghostty/font.conf extra\n",
        "config-file \"/slate-owned/ghostty/font.conf\"\n",
        "config-file = /outside/\"/slate-owned/ghostty/font.conf\"\n",
    ] {
        let expected = format!("{original}config-file = \"{}\"\n", managed.display());
        let updated = GhosttyAdapter::font_include_content(original.as_bytes(), managed);
        assert_eq!(updated, expected.as_bytes(), "{original}");
        assert_eq!(
            GhosttyAdapter::font_include_content(&updated, managed),
            updated
        );
    }
}

#[test]
fn ghostty_literal_font_reference_after_reset_is_reconnected_once() {
    let managed = Path::new("/slate-owned/ghostty/font.conf");
    for original in [
        "config-file = /slate-owned/ghostty/font.conf\nconfig-file =\n",
        "include = /slate-owned/ghostty/font.conf\nconfig-file = \"\"\n",
    ] {
        let expected = format!("{original}config-file = \"{}\"\n", managed.display());
        let updated = GhosttyAdapter::font_include_content(original.as_bytes(), managed);
        assert_eq!(updated, expected.as_bytes());
        assert_eq!(
            GhosttyAdapter::font_include_content(&updated, managed),
            updated
        );
    }
}

#[test]
fn ghostty_literal_cleanup_keeps_external_values_and_binary_bytes() {
    let original = b"\xef\xbb\xbfconfig-file = /slate-owned/ghostty/theme.conf\r\nconfig-file = /outside # /slate-owned/ghostty/font.conf\nconfig-file = '/slate-owned/ghostty/font.conf'\nconfig-file = /slate-owned/ghostty/../elsewhere.conf\nconfig-file = /slate-owned/ghostty-old/theme.conf\nconfig-file = ?\"/slate-owned/ghostty/opacity.conf\"\ninclude = /slate-owned/ghostty/font.conf\nuser = \xff\n";
    let expected = b"\xef\xbb\xbfconfig-file = /outside # /slate-owned/ghostty/font.conf\nconfig-file = '/slate-owned/ghostty/font.conf'\nconfig-file = /slate-owned/ghostty/../elsewhere.conf\nconfig-file = /slate-owned/ghostty-old/theme.conf\nuser = \xff\n";
    let cleaned =
        GhosttyAdapter::strip_managed_references_from_bytes(original, b"/slate-owned/ghostty");
    assert_eq!(cleaned, expected);
    assert_eq!(
        GhosttyAdapter::strip_managed_references_from_bytes(&cleaned, b"/slate-owned/ghostty"),
        cleaned
    );
}

#[test]
fn ghostty_literal_optional_bom_and_later_valid_reference_remain_noops() {
    let managed = Path::new("/slate-owned/ghostty/font.conf");
    for original in [
        "\u{feff}config-file = ?/slate-owned/ghostty/font.conf\r\n",
        "config-file = \"\"/slate-owned/ghostty/font.conf\"\"\n",
        "include = /slate-owned/ghostty/font.conf\nconfig-file = /slate-owned/ghostty/font.conf\n",
        "config-file =\nconfig-file = /slate-owned/ghostty/font.conf\n",
    ] {
        assert_eq!(
            GhosttyAdapter::font_include_content(original.as_bytes(), managed),
            original.as_bytes()
        );
    }
    let legacy = b"\xef\xbb\xbfinclude = /slate-owned/ghostty/font.conf\n";
    assert_eq!(
        GhosttyAdapter::font_include_content(legacy, managed),
        b"\xef\xbb\xbfconfig-file = \"/slate-owned/ghostty/font.conf\"\n"
    );
}
