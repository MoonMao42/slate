//! `SlateTheme : impl cliclack::Theme` — cliclack framing integration.
//!
//! Controls visible framing and text styles:
//! - Intro / outro framing carries the terminal theme accent;
//!   outro emits the completion glyph `└─ ★`.
//! - The error symbol uses theme red, never lavender.
//!
//! Startup ordering: call [`init`] exactly once from `main.rs::run()`
//! before any `cliclack::*` call fires.

use cliclack::{Theme, ThemeState};
use console::Style;

/// Slate's cliclack theme — theme magenta bar color, intro/outro
/// framing, severity-correct error/warn symbols.
pub struct SlateTheme;

impl Theme for SlateTheme {
    /// Vertical side-bar color. Active / Submit use the terminal palette’s magenta. Cancel / Error
    /// cascade to red to surface severity.
    fn bar_color(&self, state: &ThemeState) -> Style {
        match state {
            ThemeState::Active | ThemeState::Submit => Style::new().magenta(),
            ThemeState::Cancel | ThemeState::Error(_) => Style::new().red(),
        }
    }

    fn checkbox_style(&self, state: &ThemeState, _selected: bool, _active: bool) -> Style {
        self.input_style(state)
    }

    fn input_style(&self, state: &ThemeState) -> Style {
        if matches!(state, ThemeState::Cancel) {
            Style::new().strikethrough()
        } else {
            Style::new()
        }
    }

    fn placeholder_style(&self, state: &ThemeState) -> Style {
        if matches!(state, ThemeState::Cancel) {
            Style::new().hidden()
        } else {
            Style::new().bright().black()
        }
    }

    /// State-symbol color — Submit uses the terminal palette’s green;
    /// every other state inherits the bar color.
    fn state_symbol_color(&self, state: &ThemeState) -> Style {
        match state {
            ThemeState::Submit => Style::new().green(),
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

    /// Info bullet follows the terminal palette's magenta.
    fn info_symbol(&self) -> String {
        Style::new().magenta().apply_to("●").to_string()
    }

    /// Warning bullet — theme yellow (cliclack default, kept).
    fn warning_symbol(&self) -> String {
        Style::new().yellow().apply_to("▲").to_string()
    }

    /// Error bullet — theme red, never lavender.
    fn error_symbol(&self) -> String {
        Style::new().red().apply_to("■").to_string()
    }

    /// Intro framing — `✦ title` on a theme magenta bar.
    fn format_intro(&self, title: &str) -> String {
        let color = Style::new().magenta();
        format!(
            "{start}  {title}\n{bar}\n",
            start = color.apply_to("✦"),
            bar = color.apply_to("┃"),
        )
    }

    /// Outro framing — `└─ ★ message` on a theme magenta bar.
    fn format_outro(&self, message: &str) -> String {
        let color = Style::new().magenta();
        format!("{end}  {message}\n", end = color.apply_to("└─ ★"))
    }

    /// Cancel-path outro — red body, red framing.
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

    /// Intro framing uses the audited terminal accent and the `✦` glyph.
    #[test]
    fn intro_uses_terminal_accent() {
        console::set_colors_enabled(true);
        let theme = SlateTheme;
        let out = theme.format_intro("Welcome");
        assert!(
            out.contains("[35m"),
            "intro must use the terminal accent, got: {out:?}"
        );
        assert!(out.contains('✦'), "intro must include the ✦ glyph: {out:?}");
        insta::assert_snapshot!("slate_theme_format_intro", out);
    }

    /// `init()` is callable without panic.
    #[test]
    fn set_theme_is_callable() {
        init();
        init();
    }

    /// `error_symbol` and `format_outro_cancel` must never emit the lavender
    /// `38;5;183` tint.
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
