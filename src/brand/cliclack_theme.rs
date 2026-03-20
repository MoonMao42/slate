//! `SlateTheme : impl cliclack::Theme` — cliclack framing integration
//! for.
//! Decisions honored:
//! - **** — override the 9 methods that govern slate-visible framing.
//! CONTEXT named `prompt_style` / `error_style`; those do NOT exist
//! on cliclack 0.5.4's `Theme` trait (see RESEARCH Pitfall 1 — the
//! authoritative list is encoded here).
//! - **** — intro / outro framing carries the fixed brand lavender
//! `#7287fd`; outro emits the sketch-003 completion glyph `└─ ★`.
//! - **D-01a** — the error symbol uses theme.red, NEVER lavender.
//! Startup ordering: call [`init`] exactly once from `main.rs::run()`
//! **before** any `cliclack::*` call fires. Second and later calls are a
//! silent override (cliclack 0.5.4 uses `Lazy<RwLock<Box<dyn Theme>>>`);
//! has no runtime reconfiguration path, so a single call is
//! sufficient.

use cliclack::{Theme, ThemeState};
use console::Style;

/// Slate's cliclack theme — lavender bar color, sketch-003 intro/outro
/// framing, severity-correct error/warn symbols.
pub struct SlateTheme;

impl Theme for SlateTheme {
    /// Vertical side-bar color. Active / Submit use brand lavender (256-
    /// color approximation because `console::Style` doesn't guarantee
    /// truecolor on every terminal cliclack renders into). Cancel / Error
    /// cascade to red to surface severity.
    fn bar_color(&self, state: &ThemeState) -> Style {
        match state {
            ThemeState::Active | ThemeState::Submit => Style::new().color256(183),
            ThemeState::Cancel | ThemeState::Error(_) => Style::new().red(),
        }
    }

    /// State-symbol color — Submit uses theme-green-adjacent `color256(114)`;
    /// every other state inherits the bar color.
    fn state_symbol_color(&self, state: &ThemeState) -> Style {
        match state {
            ThemeState::Submit => Style::new().color256(114),
            _ => self.bar_color(state),
        }
    }

    /// State glyph — preserve cliclack's default glyphs but tint via
    /// [`state_symbol_color`]. The actual glyphs (`◆`, `■`, `▲`, `◇`)
    /// come from cliclack internals; we just recolor them.
    fn state_symbol(&self, state: &ThemeState) -> String {
        let color = self.state_symbol_color(state);
        let glyph = match state {
            ThemeState::Active => "◆",
            ThemeState::Cancel => "■",
            ThemeState::Submit => "◇",
            ThemeState::Error(_) => "▲",
        };
        color.apply_to(glyph).to_string()
    }

    /// Info bullet — lavender `●` (brand anchor).
    fn info_symbol(&self) -> String {
        Style::new().color256(183).apply_to("●").to_string()
    }

    /// Warning bullet — theme yellow (cliclack default, kept).
    fn warning_symbol(&self) -> String {
        Style::new().yellow().apply_to("▲").to_string()
    }

    /// Error bullet — theme red. D-01a invariant: NEVER lavender here.
    fn error_symbol(&self) -> String {
        Style::new().red().apply_to("■").to_string()
    }

    /// Intro framing — `✦ title` on a lavender bar (sketch 003 winner).
    fn format_intro(&self, title: &str) -> String {
        let color = Style::new().color256(183);
        format!(
            "{start}  {title}\n{bar}\n",
            start = color.apply_to("✦"),
            bar = color.apply_to("┃"),
        )
    }

    /// Outro framing — `└─ ★ message` on a lavender bar (sketch 003
    /// completion glyph).
    fn format_outro(&self, message: &str) -> String {
        let color = Style::new().color256(183);
        format!("{end}  {message}\n", end = color.apply_to("└─ ★"))
    }

    /// Cancel-path outro — red body, red framing. D-01a invariant: the
    /// message NEVER carries lavender bytes.
    fn format_outro_cancel(&self, message: &str) -> String {
        let red = Style::new().red();
        format!(
            "{end}  {msg}\n",
            end = red.apply_to("└─"),
            msg = red.apply_to(message),
        )
    }
}

/// One-shot theme registration — call from `main.rs::run()` before any
/// `cliclack::*` invocation. Safe to call multiple times in tests (the
/// second call silently overrides), but in production should fire exactly
/// once.
pub fn init() {
    cliclack::set_theme(SlateTheme);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Row `18-W0-cliclack-theme` — intro framing carries the brand-anchor
    /// lavender ansi-256 index (183) AND the `✦` glyph. Byte-locked via
    /// `insta` so sketch-003 drift is flagged at CI.
    /// `console::Style` suppresses ANSI output when stdout is not a TTY
    /// (CI runners, `cargo test` without a pty). We force colors ON for
    /// this test so the byte-locked assertion exercises the real
    /// production code path; production always runs attached to a TTY
    /// because it's only reached via `cliclack::intro(...)` which
    /// already guards on that.
    #[test]
    fn intro_uses_lavender_bar() {
        console::set_colors_enabled(true);
        let theme = SlateTheme;
        let out = theme.format_intro("Welcome");
        // 183 is the ansi-256 "light lavender" index closest to #7287fd;
        // its SGR byte is `38;5;183`.
        assert!(
            out.contains("38;5;183"),
            "intro must carry lavender color bytes, got: {out:?}"
        );
        assert!(out.contains('✦'), "intro must include the ✦ glyph: {out:?}");
        insta::assert_snapshot!("slate_theme_format_intro", out);
    }

    /// Row `18-W0-cliclack-init` — `init()` is callable without panic.
    /// Called once per test; cliclack's global `RwLock<Box<dyn Theme>>`
    /// absorbs the second call silently.
    #[test]
    fn set_theme_is_callable() {
        init();
        // Second call must not panic either (documented anti-pattern but
        // the runtime tolerates it).
        init();
    }

    /// D-01a guard at the cliclack-framing layer: `error_symbol` and
    /// `format_outro_cancel` must NEVER emit the lavender `38;5;183`
    /// tint. Forces `console::set_colors_enabled(true)` so this test
    /// exercises the full styling path (same reason as
    /// `intro_uses_lavender_bar`).
    #[test]
    fn error_symbol_is_not_lavender() {
        console::set_colors_enabled(true);
        let theme = SlateTheme;
        assert!(!theme.error_symbol().contains("38;5;183"));
        assert!(!theme.format_outro_cancel("boom").contains("38;5;183"));
    }

    /// Smoke: every overridden method is callable without panic across
    /// all four cliclack `ThemeState` variants.
    #[test]
    fn all_theme_methods_run_across_states() {
        let theme = SlateTheme;
        for state in [
            ThemeState::Active,
            ThemeState::Cancel,
            ThemeState::Submit,
            ThemeState::Error("bad".to_string()),
        ] {
            let _ = theme.bar_color(&state);
            let _ = theme.state_symbol_color(&state);
            let _ = theme.state_symbol(&state);
        }
        let _ = theme.info_symbol();
        let _ = theme.warning_symbol();
        let _ = theme.error_symbol();
        let _ = theme.format_intro("title");
        let _ = theme.format_outro("done");
        let _ = theme.format_outro_cancel("cancelled");
    }
}
