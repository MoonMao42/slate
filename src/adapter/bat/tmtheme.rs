//! Render slate-tuned `.tmTheme` XML for bat from a slate Palette.
//! Pure-function module — no I/O, no side effects. Takes a `&Palette`
//! plus a `theme_id` (kebab-case ASCII validated at registry load) and
//! returns the rendered XML `String`. The XML is byte-stable across
//! rebuilds: UUIDs are derived deterministically (UUIDv5 over a fixed
//! namespace + theme_id), no timestamps, no per-call randomness.
//! Used by `BatAdapter::apply_tmtheme_files` (sibling `mod.rs`) to write
//! `<bat-config-dir>/themes/slate-<id>.tmTheme` for every registered
//! theme, then `bat cache --build` is invoked.

use std::borrow::Cow;
use uuid::Uuid;

use crate::theme::Palette;

const TMTHEME_TEMPLATE: &str = include_str!("../../../resources/bat/slate.tmTheme.template");

/// Fixed slate namespace UUID for deterministic UUIDv5 derivation.
/// WHY: bat caches compiled tmTheme assets keyed by the `<key>uuid</key>`
/// value declared inside each theme. Changing this namespace would
/// silently invalidate every existing user's cache and force a manual
/// `bat cache --build` to recover. Once committed, this constant MUST
/// NEVER CHANGE — it is a stable project-wide identifier in the same
/// way crate names or DB schema versions are.
/// Generated: 2026-04-28 (one-time random v4 UUID, then frozen).
/// Source: `python3 -c 'import uuid; print(uuid.uuid4())'`
/// Value: 462eba88-cb18-4276-97f1-af8e19236d66
const SLATE_NAMESPACE_UUID: Uuid = Uuid::from_bytes([
    0x46, 0x2e, 0xba, 0x88, 0xcb, 0x18, 0x42, 0x76, 0x97, 0xf1, 0xaf, 0x8e, 0x19, 0x23, 0x6d, 0x66,
]);

/// Derive a deterministic UUIDv5 for a given theme_id under the slate
/// namespace. Same input always produces the same UUID — required for
/// stable insta snapshots and stable bat cache keys across rebuilds.
pub(super) fn theme_uuid(theme_id: &str) -> Uuid {
    Uuid::new_v5(&SLATE_NAMESPACE_UUID, theme_id.as_bytes())
}

/// XML-escape the 5 standard plist entities (& < > " ').
/// slate's palette hex strings (`#RRGGBB`) and theme ids (kebab-case
/// ASCII) are XML-safe today, so most calls return the input borrowed
/// unchanged. The helper exists as defense-in-depth (RESEARCH §P2): if a
/// future palette field carries an `&` or `<`, the rendered tmTheme must
/// stay a valid plist or `bat cache --build` rejects it as malformed.
pub(super) fn xml_escape(s: &str) -> Cow<'_, str> {
    if !s.chars().any(|c| matches!(c, '&' | '<' | '>' | '"' | '\'')) {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len() + 16);
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            other => out.push(other),
        }
    }
    Cow::Owned(out)
}

/// Render a slate-tuned tmTheme for the given palette + theme id.
/// Output is byte-stable across rebuilds. `theme_id` is used both as the
/// `<key>name</key>` value (prefixed `slate-`) and as the seed for the
/// deterministic UUIDv5. Insta snapshots in
/// `tests/bat_tmtheme_snapshots.rs` lock the output for all 20 themes.
pub fn render_tmtheme(palette: &Palette, theme_id: &str) -> String {
    // Substitution map: token name -> palette-derived hex. Every value
    // flows through xml_escape as defense-in-depth.
    // Mapping reference: PLAN 23-03 Task 2 + RESEARCH §Q13.
    let name_value = format!("slate-{theme_id}");
    let uuid_value = theme_uuid(theme_id).hyphenated().to_string();

    // Global settings — bg/fg/caret + UI chrome.
    let background = &palette.background;
    let foreground = &palette.foreground;
    let caret = palette.cursor.as_deref().unwrap_or(&palette.foreground);
    let selection_bg = palette
        .selection_bg
        .as_deref()
        .unwrap_or(&palette.bright_black);
    let line_highlight = palette
        .bg_dim
        .as_deref()
        .or(palette.surface0.as_deref())
        .unwrap_or(&palette.background);
    let invisibles = &palette.bright_black;
    let gutter = palette
        .bg_darker
        .as_deref()
        .or(palette.surface0.as_deref())
        .unwrap_or(&palette.background);
    let gutter_foreground = &palette.bright_black;

    // Per-scope colors — direct ANSI-slot mapping per RESEARCH §Q13.
    let comment = &palette.bright_black;
    let string_color = &palette.green;
    let keyword = &palette.magenta;
    let constant = &palette.yellow;
    let function = &palette.blue;
    let type_color = &palette.cyan;
    let variable = &palette.foreground;
    let operator = &palette.foreground;
    let tag = &palette.magenta;
    let attribute = &palette.yellow;
    let markup_inserted = &palette.green;
    let markup_deleted = &palette.red;
    let markup_changed = &palette.yellow;
    let markup_heading = &palette.blue;
    let markup_bold = &palette.magenta;
    let markup_italic = &palette.magenta;
    let regexp = &palette.yellow;
    let escape = &palette.cyan;
    let invalid_fg = &palette.background;
    let invalid_bg = &palette.red;
    let diff_meta = &palette.blue;

    // Render: 22 token replacements via str::replace. Avoids pulling a
    // templating crate; every value is xml-escaped at substitution.
    let mut out = TMTHEME_TEMPLATE.to_string();
    let pairs: [(&str, &str); 31] = [
        ("{{name}}", &name_value),
        ("{{uuid}}", &uuid_value),
        ("{{background}}", background),
        ("{{foreground}}", foreground),
        ("{{caret}}", caret),
        ("{{selection_bg}}", selection_bg),
        ("{{line_highlight}}", line_highlight),
        ("{{invisibles}}", invisibles),
        ("{{gutter}}", gutter),
        ("{{gutter_foreground}}", gutter_foreground),
        ("{{comment}}", comment),
        ("{{string}}", string_color),
        ("{{keyword}}", keyword),
        ("{{constant}}", constant),
        ("{{function}}", function),
        ("{{type}}", type_color),
        ("{{variable}}", variable),
        ("{{operator}}", operator),
        ("{{tag}}", tag),
        ("{{attribute}}", attribute),
        ("{{markup_inserted}}", markup_inserted),
        ("{{markup_deleted}}", markup_deleted),
        ("{{markup_changed}}", markup_changed),
        ("{{markup_heading}}", markup_heading),
        ("{{markup_bold}}", markup_bold),
        ("{{markup_italic}}", markup_italic),
        ("{{regexp}}", regexp),
        ("{{escape}}", escape),
        ("{{invalid_fg}}", invalid_fg),
        ("{{invalid_bg}}", invalid_bg),
        ("{{diff_meta}}", diff_meta),
    ];
    for (token, value) in pairs.iter() {
        let escaped = xml_escape(value);
        out = out.replace(token, &escaped);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_uuid_is_deterministic() {
        let a = theme_uuid("catppuccin-mocha");
        let b = theme_uuid("catppuccin-mocha");
        assert_eq!(a, b, "UUIDv5 must be byte-stable for the same input");
    }

    #[test]
    fn theme_uuid_differs_across_theme_ids() {
        let a = theme_uuid("catppuccin-mocha");
        let b = theme_uuid("solarized-light");
        assert_ne!(
            a, b,
            "different theme ids must derive different UUIDs (no namespace clash)"
        );
    }

    #[test]
    fn xml_escape_handles_all_five_entities() {
        assert_eq!(xml_escape("safe").as_ref(), "safe");
        assert_eq!(xml_escape("&").as_ref(), "&amp;");
        assert_eq!(xml_escape("<").as_ref(), "&lt;");
        assert_eq!(xml_escape(">").as_ref(), "&gt;");
        assert_eq!(xml_escape("\"").as_ref(), "&quot;");
        assert_eq!(xml_escape("'").as_ref(), "&apos;");
        assert_eq!(
            xml_escape("a&b<c>d\"e'f").as_ref(),
            "a&amp;b&lt;c&gt;d&quot;e&apos;f"
        );
    }

    #[test]
    fn xml_escape_borrows_when_no_entities_present() {
        // Defense-in-depth: hex strings should never copy.
        let hex = "#fdf6e3";
        let escaped = xml_escape(hex);
        assert!(matches!(escaped, Cow::Borrowed(_)));
    }

    fn sample_palette() -> Palette {
        use std::collections::HashMap;
        Palette {
            foreground: "#cdd6f4".to_string(),
            background: "#1e1e2e".to_string(),
            cursor: Some("#f5e0dc".to_string()),
            selection_bg: Some("#585b70".to_string()),
            selection_fg: None,
            brand_accent: "#cba6f7".to_string(),
            black: "#45475a".to_string(),
            red: "#f38ba8".to_string(),
            green: "#a6e3a1".to_string(),
            yellow: "#f9e2af".to_string(),
            blue: "#89b4fa".to_string(),
            magenta: "#f5c2e7".to_string(),
            cyan: "#94e2d5".to_string(),
            white: "#bac2de".to_string(),
            bright_black: "#585b70".to_string(),
            bright_red: "#f38ba8".to_string(),
            bright_green: "#a6e3a1".to_string(),
            bright_yellow: "#f9e2af".to_string(),
            bright_blue: "#89b4fa".to_string(),
            bright_magenta: "#f5c2e7".to_string(),
            bright_cyan: "#94e2d5".to_string(),
            bright_white: "#a6adc8".to_string(),
            bg_dim: Some("#181825".to_string()),
            bg_darker: Some("#11111b".to_string()),
            bg_darkest: Some("#11111b".to_string()),
            rosewater: None,
            flamingo: None,
            pink: None,
            mauve: None,
            lavender: None,
            text: None,
            subtext1: None,
            subtext0: None,
            overlay2: None,
            overlay1: None,
            overlay0: None,
            surface2: None,
            surface1: None,
            surface0: None,
            extras: HashMap::new(),
        }
    }

    #[test]
    fn render_tmtheme_substitutes_all_tokens() {
        let xml = render_tmtheme(&sample_palette(), "catppuccin-mocha");
        assert!(!xml.contains("{{"), "no placeholder tokens may remain in output");
        assert!(xml.contains("<string>slate-catppuccin-mocha</string>"));
        assert!(xml.contains("<plist version=\"1.0\">"));
        assert!(xml.contains("<key>uuid</key>"));
    }

    #[test]
    fn render_tmtheme_is_byte_stable() {
        let palette = sample_palette();
        let a = render_tmtheme(&palette, "catppuccin-mocha");
        let b = render_tmtheme(&palette, "catppuccin-mocha");
        assert_eq!(a, b, "render must be deterministic across calls");
    }
}
