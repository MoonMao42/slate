//! Task 5 — insta snapshot harness for slate-tuned tmThemes.
//! Locks the rendered XML for every registered theme. Snapshots are
//! byte-stable because `render_tmtheme` derives the `<key>uuid</key>`
//! value via UUIDv5 over a fixed namespace + theme_id (see
//! `src/adapter/bat/tmtheme.rs::SLATE_NAMESPACE_UUID` — must never
//! change). Any palette tweak in `themes/themes.toml`, scope-mapping
//! drift in `tmtheme.rs`, or template structural change surfaces as a
//! visible snapshot diff at CI.
//! The non-empty registry sweep at the bottom is a self-extending
//! invariant: any future theme added to `themes/themes.toml` is
//! automatically required to render a non-empty `.tmTheme`.

use rstest::rstest;
use slate_cli::adapter::bat::tmtheme::render_tmtheme;
use slate_cli::theme::ThemeRegistry;

#[rstest]
#[case("catppuccin-frappe")]
#[case("catppuccin-latte")]
#[case("catppuccin-macchiato")]
#[case("catppuccin-mocha")]
#[case("dracula")]
#[case("everforest-dark")]
#[case("everforest-light")]
#[case("gruvbox-dark")]
#[case("gruvbox-light")]
#[case("kanagawa-dragon")]
#[case("kanagawa-lotus")]
#[case("kanagawa-wave")]
#[case("nord")]
#[case("rose-pine-dawn")]
#[case("rose-pine-main")]
#[case("rose-pine-moon")]
#[case("tokyo-night-dark")]
#[case("tokyo-night-light")]
#[case("solarized-dark")]
#[case("solarized-light")]
fn render_tmtheme_snapshot(#[case] theme_id: &str) {
    let registry = ThemeRegistry::new().expect("registry loads from embedded themes.toml");
    let theme = registry
        .get(theme_id)
        .unwrap_or_else(|| panic!("theme {theme_id} must exist in registry"));
    let xml = render_tmtheme(&theme.palette, theme_id);

    insta::with_settings!({snapshot_suffix => theme_id}, {
        insta::assert_snapshot!("bat_tmtheme", xml);
    });
}

/// Self-extending invariant: every theme in the registry must render a
/// non-empty `.tmTheme` containing the canonical plist scaffolding. New
/// themes added to `themes/themes.toml` automatically get this check
/// without touching this file.
#[test]
fn every_registered_theme_renders_non_empty_tmtheme() {
    let registry = ThemeRegistry::new().expect("registry loads");
    let mut count = 0usize;
    for theme in registry.all() {
        let xml = render_tmtheme(&theme.palette, &theme.id);
        assert!(!xml.is_empty(), "theme {} rendered empty XML", theme.id);
        assert!(
            xml.contains("<plist"),
            "theme {} missing <plist> root element",
            theme.id
        );
        assert!(
            xml.contains("<key>name</key>"),
            "theme {} missing <key>name</key>",
            theme.id
        );
        assert!(
            xml.contains("<key>uuid</key>"),
            "theme {} missing <key>uuid</key>",
            theme.id
        );
        assert!(
            !xml.contains("{{"),
            "theme {} has unsubstituted {{token}} in output",
            theme.id
        );
        count += 1;
    }
    assert!(
        count >= 20,
        "expected at least 20 registered themes, swept {count}"
    );
}
