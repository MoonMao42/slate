//! Brand role API — the 13-method text-role surface.
//!
//! Visual contracts:
//! * **Pill-led role differentiation** — `command` / `code`
//!   render as pill chips, `path` / `shortcut` / `theme_name` do not.
//! * **Medium lavender density** — brand anchors use the
//!   contrast-adjusted lavender `#7287fd`, daily-chrome roles use the active theme's
//!   `brand_accent`, severity roles use theme red / yellow / green (never
//!   lavender).
//! * **Tree-style header/receipt narrative** — `heading`
//!   yields `◆ title`, `tree_branch` prepends `┃ ├─`, `tree_end` prepends
//!   `└─ ★`.
//!
//! Fallback tree per [`RenderMode`]:
//! * `Truecolor` → blend pill + `38;2;R;G;B` accent fg (brand-anchor
//!   or theme `brand_accent` depending on role).
//! * `Basic` → `› text ‹` + Bold; no background ANSI.
//! * `None` → plain text, zero `\x1b[` bytes.

use crate::adapter::palette_renderer::PaletteRenderer;
use crate::brand::palette::BRAND_LAVENDER_FIXED;
use crate::brand::render_context::{RenderContext, RenderMode};

/// Short-lived holder of the active `&RenderContext` — built once at the
/// top of each user-facing handler, then used to render every per-role
/// string in that handler.
pub struct Roles<'a> {
    ctx: &'a RenderContext<'a>,
}

impl<'a> Roles<'a> {
    pub fn new(ctx: &'a RenderContext<'a>) -> Self {
        Self { ctx }
    }

    // ── Everyday role surfaces — theme-accented (daily chrome) ────

    /// Command pill (e.g. `slate theme set`) — blend bg + active
    /// theme's `brand_accent` foreground.
    pub fn command(&self, text: &str) -> String {
        let accent = theme_accent(self.ctx);
        match self.ctx.mode {
            RenderMode::Truecolor => {
                let bg = self.ctx.cached_pill_bg.as_deref().unwrap_or("48;2;0;0;0");
                let fg = ansi_fg_from_hex(&self.pill_foreground(accent));
                format!("\x1b[{bg};{fg}m {text} \x1b[0m")
            }
            RenderMode::Basic => format!("\x1b[1m› {text} ‹\x1b[0m"),
            RenderMode::None => text.to_string(),
        }
    }

    /// Secondary text uses a readable muted color, never terminal-dependent dimming.
    pub fn path(&self, text: &str) -> String {
        match self.ctx.mode {
            RenderMode::Truecolor => {
                let p = &self.ctx.theme.palette;
                let fg =
                    ansi_fg_from_hex(&crate::wcag::readable_color(&p.bright_black, &p.background));
                format!("\x1b[{fg};3m{text}\x1b[0m")
            }
            RenderMode::Basic => format!("\x1b[3m{text}\x1b[0m"),
            RenderMode::None => text.to_string(),
        }
    }

    /// Keyboard-shortcut keycap pill (`⌘N`, `Enter`, …) — bordered
    /// `[ text ]` chip in default fg; no theme tint.
    pub fn shortcut(&self, text: &str) -> String {
        match self.ctx.mode {
            RenderMode::Truecolor | RenderMode::Basic => format!("\x1b[1m[ {text} ]\x1b[0m"),
            RenderMode::None => format!("[ {text} ]"),
        }
    }

    /// Inline code / filename pill — neutral surface container
    /// (`surface0`-ish) + default fg.
    pub fn code(&self, text: &str) -> String {
        match self.ctx.mode {
            RenderMode::Truecolor => {
                let bg = self.ctx.cached_pill_bg.as_deref().unwrap_or("48;2;0;0;0");
                let fg =
                    ansi_fg_from_hex(&self.pill_foreground(&self.ctx.theme.palette.foreground));
                format!("\x1b[{bg};{fg}m {text} \x1b[0m")
            }
            RenderMode::Basic => format!("\x1b[1m`{text}`\x1b[0m"),
            RenderMode::None => format!("`{text}`"),
        }
    }

    /// Theme display name — active theme's `brand_accent` foreground,
    /// no container. Used inside phrases ("current theme: catppuccin-mocha").
    pub fn theme_name(&self, text: &str) -> String {
        let accent = theme_accent(self.ctx);
        match self.ctx.mode {
            RenderMode::Truecolor => {
                let fg = ansi_fg_from_hex(&crate::wcag::readable_color(
                    accent,
                    &self.ctx.theme.palette.background,
                ));
                format!("\x1b[{fg}m{text}\x1b[0m")
            }
            RenderMode::Basic => format!("\x1b[1m{text}\x1b[0m"),
            RenderMode::None => text.to_string(),
        }
    }

    // ── Brand-anchor surfaces — adaptive lavender ───────────────────

    /// Brand-anchor text — the `slate` wordmark, inline product name.
    /// Uses the brand hue, adjusted for the active background.
    pub fn brand(&self, text: &str) -> String {
        match self.ctx.mode {
            RenderMode::Truecolor => {
                let fg = ansi_fg_from_hex(&crate::wcag::readable_color(
                    BRAND_LAVENDER_FIXED,
                    &self.ctx.theme.palette.background,
                ));
                format!("\x1b[{fg}m{text}\x1b[0m")
            }
            RenderMode::Basic => format!("\x1b[1m{text}\x1b[0m"),
            RenderMode::None => text.to_string(),
        }
    }

    /// Static logo string — "✦ slate", brand-anchor lavender (or plain
    /// under `RenderMode::None`).
    /// Returns a `String` (not `&'static str`) because the ANSI bytes
    /// depend on the active [`RenderMode`].
    pub fn logo(&self) -> String {
        self.brand("✦ slate")
    }

    // ── Severity — theme-derived, never lavender ─────────────────

    /// Success message: `✓ message`, theme.green foreground.
    pub fn status_success(&self, text: &str) -> String {
        self.severity_line("✓", &self.ctx.theme.palette.green, text)
    }

    /// Warning message: `⚠ message`, theme.yellow foreground.
    pub fn status_warn(&self, text: &str) -> String {
        self.severity_line("⚠", &self.ctx.theme.palette.yellow, text)
    }

    /// Error message: `✗ message`, theme.red foreground. Invariant:
    /// output never contains the `BRAND_LAVENDER_FIXED` byte sequence.
    pub fn status_error(&self, text: &str) -> String {
        self.severity_line("✗", &self.ctx.theme.palette.red, text)
    }

    fn severity_line(&self, glyph: &str, hex: &str, text: &str) -> String {
        match self.ctx.mode {
            RenderMode::Truecolor => {
                let fg = ansi_fg_from_hex(&crate::wcag::readable_color(
                    hex,
                    &self.ctx.theme.palette.background,
                ));
                format!("\x1b[{fg}m{glyph} {text}\x1b[0m")
            }
            RenderMode::Basic => format!("\x1b[1m{glyph} {text}\x1b[0m"),
            RenderMode::None => format!("{glyph} {text}"),
        }
    }

    fn pill_foreground(&self, preferred: &str) -> String {
        let channels: Vec<u8> = self
            .ctx
            .cached_pill_bg
            .as_deref()
            .unwrap_or("48;2;0;0;0")
            .split(';')
            .skip(2)
            .filter_map(|v| v.parse().ok())
            .collect();
        let bg = match channels.as_slice() {
            [r, g, b] => format!("#{r:02x}{g:02x}{b:02x}"),
            _ => self.ctx.theme.palette.background.clone(),
        };
        crate::wcag::readable_color(preferred, &bg)
    }

    // ── Tree narrative primitives (used with println!) ──────────

    /// Section heading — `◆ title`, brand-anchor lavender.
    pub fn heading(&self, title: &str) -> String {
        match self.ctx.mode {
            RenderMode::Truecolor => {
                let fg = ansi_fg_from_hex(&crate::wcag::readable_color(
                    BRAND_LAVENDER_FIXED,
                    &self.ctx.theme.palette.background,
                ));
                format!("\x1b[{fg}m◆\x1b[0m {title}")
            }
            RenderMode::Basic => format!("◆ {title}"),
            RenderMode::None => format!("◆ {title}"),
        }
    }

    /// Mid-receipt tree branch — `┃ ├─ text`.
    pub fn tree_branch(&self, text: &str) -> String {
        match self.ctx.mode {
            RenderMode::Truecolor => {
                let fg = ansi_fg_from_hex(&crate::wcag::readable_color(
                    BRAND_LAVENDER_FIXED,
                    &self.ctx.theme.palette.background,
                ));
                format!("\x1b[{fg}m┃ ├─\x1b[0m {text}")
            }
            RenderMode::Basic | RenderMode::None => format!("┃ ├─ {text}"),
        }
    }

    /// Receipt terminator — `└─ ★ text`.
    pub fn tree_end(&self, text: &str) -> String {
        match self.ctx.mode {
            RenderMode::Truecolor => {
                let fg = ansi_fg_from_hex(&crate::wcag::readable_color(
                    BRAND_LAVENDER_FIXED,
                    &self.ctx.theme.palette.background,
                ));
                format!("\x1b[{fg}m└─ ★\x1b[0m {text}")
            }
            RenderMode::Basic | RenderMode::None => format!("└─ ★ {text}"),
        }
    }
}

/// Resolve the active theme's `brand_accent`, falling back to the fixed
/// brand lavender if the palette slot is somehow empty (which would only
/// happen in a malformed mock theme — production palettes are validated
/// at registry init).
fn theme_accent<'a>(ctx: &'a RenderContext<'a>) -> &'a str {
    if ctx.theme.palette.brand_accent.is_empty() {
        BRAND_LAVENDER_FIXED
    } else {
        &ctx.theme.palette.brand_accent
    }
}

/// `#RRGGBB` → `38;2;R;G;B` with a graceful degrade to `0` on parse
/// failure (mirrors `src/adapter/ls_colors.rs::ansi_code`).
fn ansi_fg_from_hex(hex: &str) -> String {
    match PaletteRenderer::hex_to_rgb(hex) {
        Ok((r, g, b)) => PaletteRenderer::rgb_to_ansi_24bit(r, g, b),
        Err(_) => String::from("0"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brand::render_context::{mock_context, mock_context_with_mode, mock_theme};

    #[test]
    fn all_text_roles_are_readable_without_terminal_dimming() {
        use crate::wcag::contrast_hex;
        let sgr = regex::Regex::new(r"\x1b\[([0-9;]+)m").unwrap();
        for theme in crate::theme::ThemeRegistry::new().unwrap().all() {
            let ctx = mock_context(theme);
            let roles = Roles::new(&ctx);
            for output in [
                roles.command("command"),
                roles.code("code"),
                roles.path("path"),
                roles.theme_name("theme"),
                roles.logo(),
                roles.heading("heading"),
                roles.tree_branch("branch"),
                roles.tree_end("end"),
                roles.status_success("success"),
                roles.status_warn("warning"),
                roles.status_error("error"),
            ] {
                let codes: Vec<u8> = sgr.captures(&output).unwrap()[1]
                    .split(';')
                    .map(|v| v.parse().unwrap())
                    .collect();
                let mut fg = theme.palette.foreground.clone();
                let mut bg = theme.palette.background.clone();
                let mut i = 0;
                while i < codes.len() {
                    if matches!(codes[i], 38 | 48) && codes.get(i + 1) == Some(&2) {
                        let hex = format!(
                            "#{:02x}{:02x}{:02x}",
                            codes[i + 2],
                            codes[i + 3],
                            codes[i + 4]
                        );
                        if codes[i] == 38 {
                            fg = hex;
                        } else {
                            bg = hex;
                        }
                        i += 5;
                    } else {
                        assert_ne!(codes[i], 2, "terminal dim makes contrast unpredictable");
                        i += 1;
                    }
                }
                assert!(contrast_hex(&fg, &bg) >= 4.5, "{}: {fg} on {bg}", theme.id);
            }
        }
    }

    /// Command pill emits the blend bg + accent fg + reset.
    #[test]
    fn snapshot_command_role_truecolor() {
        let theme = mock_theme();
        let ctx = mock_context(&theme);
        let roles = Roles::new(&ctx);
        insta::assert_snapshot!("role_command_truecolor", roles.command("slate theme set"));
    }

    /// Basic mode emits `› text ‹` + Bold, no background ANSI.
    #[test]
    fn snapshot_command_role_basic_fallback() {
        let theme = mock_theme();
        let ctx = mock_context_with_mode(&theme, RenderMode::Basic);
        let roles = Roles::new(&ctx);
        let out = roles.command("slate theme set");
        assert!(
            !out.contains("48;2;"),
            "Basic mode must not emit background bytes, got: {out:?}"
        );
        insta::assert_snapshot!("role_command_basic_fallback", out);
    }

    #[test]
    fn code_role_truecolor_reuses_dynamic_pill_background() {
        let theme = mock_theme();
        let ctx = mock_context(&theme);
        let roles = Roles::new(&ctx);
        let out = roles.code("frosted");
        let bg = ctx
            .cached_pill_bg
            .as_deref()
            .expect("truecolor mock caches pill bg");
        assert!(
            out.contains(bg),
            "code pill should reuse the cached background, got: {out:?}"
        );
        assert!(
            !out.contains("48;5;236"),
            "code pill must not fall back to the old fixed 256-color background, got: {out:?}"
        );
    }

    /// None mode is zero-ANSI plain text.
    #[test]
    fn none_mode_emits_plain_text() {
        let theme = mock_theme();
        let ctx = mock_context_with_mode(&theme, RenderMode::None);
        let roles = Roles::new(&ctx);
        let out = roles.command("slate theme set");
        assert!(
            !out.contains('\x1b'),
            "None mode must contain zero ANSI bytes, got: {out:?}"
        );
        assert_eq!(out, "slate theme set");
    }

    /// `status_error` output must never contain the lavender brand-accent RGB
    /// triple `38;2;114;135;253`, across all three render modes.
    #[test]
    fn error_role_never_contains_brand_accent_bytes() {
        let theme = mock_theme();
        for mode in [RenderMode::Truecolor, RenderMode::Basic, RenderMode::None] {
            let ctx = mock_context_with_mode(&theme, mode);
            let roles = Roles::new(&ctx);
            let out = roles.status_error("catastrophe");
            assert!(
                !out.contains("38;2;114;135;253"),
                "error role must never emit brand-accent lavender bytes. mode={mode:?}, out={out:?}"
            );
        }
    }

    /// Brand-anchor surfaces (logo, heading, tree primitives) carry
    /// brand-lavender `#7287fd` bytes in truecolor mode.
    #[test]
    fn brand_anchors_do_use_lavender_bytes_in_truecolor() {
        let theme = mock_theme();
        let ctx = mock_context(&theme);
        let roles = Roles::new(&ctx);
        for s in [
            roles.logo(),
            roles.brand("slate"),
            roles.heading("Theme"),
            roles.tree_branch("item"),
            roles.tree_end("done"),
        ] {
            assert!(
                s.contains("38;2;114;135;253"),
                "brand-anchor surface must carry lavender bytes, got: {s:?}"
            );
        }
    }

    #[test]
    fn severity_uses_theme_colors_not_brand_accent() {
        // Mock theme: red=#f38ba8 → 243;139;168, green=#a6d189 → 166;209;137,
        // yellow=#e5c890 → 229;200;144. None should contain lavender.
        let theme = mock_theme();
        let ctx = mock_context(&theme);
        let roles = Roles::new(&ctx);
        assert!(roles.status_error("err").contains("38;2;243;139;168"));
        assert!(roles.status_warn("warn").contains("38;2;229;200;144"));
        assert!(roles.status_success("ok").contains("38;2;166;209;137"));
    }
}
