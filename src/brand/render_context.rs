//! Rendering context for the brand text-role system (Phase 18 Wave 0).
//!
//! Decisions honored:
//! - **D-05** graceful degradation — `RenderMode::None` when the terminal
//!   does not support color, `RenderMode::Basic` when truecolor probe fails
//!   but a TTY is present, `RenderMode::Truecolor` otherwise.
//! - **D-06** `MockTheme` injection — tests build a [`RenderContext`]
//!   directly from the `mock_theme()` fixture rather than mutating
//!   `std::env`, so snapshot ANSI bytes are deterministic across CI and
//!   contributor workstations.
//! - **Pitfall 3 / feedback_no_tech_debt** — env-probe logic is split into
//!   the pure [`classify_env`] helper plus the cached
//!   [`detect_render_mode`] wrapper. Tests drive `classify_env` with
//!   explicit arguments; production reads the cached `OnceLock`.

#[cfg(test)]
use crate::adapter::palette_renderer::PaletteRenderer;
use crate::brand::palette;
use crate::error::Result;
use crate::theme::{ThemeRegistry, ThemeVariant, DEFAULT_THEME_ID};
use std::sync::OnceLock;

/// Rendering capability of the current terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
    /// Full 24-bit ANSI (`#RRGGBB` → `38;2;R;G;B`). Pills render the D-04
    /// blend; brand anchors use `BRAND_LAVENDER_FIXED`.
    Truecolor,
    /// 256-color or lower. Pills fall back to `› text ‹` + Dim+Bold
    /// lavender foreground, no background color.
    Basic,
    /// No color — either `NO_COLOR` set, or stdout is not a TTY, or
    /// `TERM=dumb`. Plain text, zero ANSI bytes.
    None,
}

/// Per-call rendering context — the active theme + the cached RenderMode +
/// (optionally) a pre-computed pill background to avoid recomputing the
/// D-04 blend on every pill render.
///
/// Built once at the top of each user-facing handler and threaded as
/// `&RenderContext` into [`crate::brand::Roles`] methods (never cloned —
/// see `Anti-Patterns` in 18-RESEARCH).
pub struct RenderContext<'a> {
    pub theme: &'a ThemeVariant,
    pub mode: RenderMode,
    /// Cached ANSI `48;2;R;G;B` substring (no `\x1b[` wrapper, no `m`) for
    /// the active theme's pill background. Computed at ctx-build time when
    /// `mode == Truecolor`; `None` for `Basic` / `None` where there's no
    /// background to precompute.
    pub cached_pill_bg: Option<String>,
}

impl<'a> RenderContext<'a> {
    /// Build a context for the given theme. Reads RenderMode via the cached
    /// env probe and precomputes the pill-background blend when truecolor
    /// is available.
    pub fn new(theme: &'a ThemeVariant) -> Self {
        let mode = detect_render_mode();
        let cached_pill_bg = match mode {
            RenderMode::Truecolor => pill_background_ansi(theme).ok(),
            RenderMode::Basic | RenderMode::None => None,
        };
        Self {
            theme,
            mode,
            cached_pill_bg,
        }
    }

    /// Build a context against the bundled default theme. Callers that
    /// don't already hold a `&ThemeVariant` can use this to bootstrap;
    /// subsequent calls within the same process reuse a cached clone so
    /// the returned context is safe to thread as `'static`.
    ///
    /// Returns an error only if the embedded theme registry fails to load
    /// (which would also break every other slate command — correctness
    /// guard, not a routine failure).
    pub fn from_active_theme() -> Result<RenderContext<'static>> {
        Ok(RenderContext::new(default_theme_ref()?))
    }
}

/// Process-wide cached default `ThemeVariant`, cloned from the embedded
/// registry on first call. Uses `Box::leak` to hand out `&'static` without
/// any `unsafe` lifetime trickery — the leaked allocation lives for the
/// lifetime of the process, which is exactly what callers need. One-shot
/// via `OnceLock`.
fn default_theme_ref() -> Result<&'static ThemeVariant> {
    static CACHED: OnceLock<&'static ThemeVariant> = OnceLock::new();
    if let Some(theme) = CACHED.get() {
        return Ok(*theme);
    }
    let registry = ThemeRegistry::new()?;
    let theme = registry
        .get(DEFAULT_THEME_ID)
        .ok_or_else(|| {
            crate::error::SlateError::InvalidThemeData(format!(
                "default theme '{DEFAULT_THEME_ID}' not found"
            ))
        })?
        .clone();
    let leaked: &'static ThemeVariant = Box::leak(Box::new(theme));
    let _ = CACHED.set(leaked);
    Ok(CACHED.get().copied().unwrap_or(leaked))
}

/// Compute the ANSI background substring for the active theme's pill —
/// used by `RenderContext::new` to cache the D-04 blend result.
fn pill_background_ansi(theme: &ThemeVariant) -> Result<String> {
    let (r, g, b) = palette::pill_background_rgb(
        &theme.palette.brand_accent,
        &theme.palette.background,
        theme.appearance,
    )?;
    Ok(format!("48;2;{r};{g};{b}"))
}

/// Pure helper: classify the render mode from already-extracted env
/// signals. Keeps `std::env` out of tests — callers pass the values, we
/// return the decision (honors user feedback_no_tech_debt rule).
pub(crate) fn classify_env(
    no_color: bool,
    is_tty: bool,
    colorterm: Option<&str>,
    term: Option<&str>,
) -> RenderMode {
    if no_color {
        return RenderMode::None;
    }
    if !is_tty {
        return RenderMode::None;
    }
    if matches!(term, Some("dumb")) {
        return RenderMode::None;
    }
    if matches!(colorterm, Some("truecolor") | Some("24bit")) {
        return RenderMode::Truecolor;
    }
    // Known truecolor-capable terminals even when COLORTERM is absent.
    match term {
        Some(t)
            if t.contains("kitty")
                || t.contains("alacritty")
                || t.contains("ghostty")
                || t.contains("xterm-256color")
                || t.contains("-truecolor") =>
        {
            RenderMode::Truecolor
        }
        _ => RenderMode::Basic,
    }
}

/// Cached env probe for the current process. First call extracts
/// `NO_COLOR` / `COLORTERM` / `TERM` + `std::io::stdout().is_terminal()`
/// and delegates to [`classify_env`]; subsequent calls hit the
/// `OnceLock`.
pub fn detect_render_mode() -> RenderMode {
    static CACHED: OnceLock<RenderMode> = OnceLock::new();
    *CACHED.get_or_init(|| {
        use std::io::IsTerminal;
        let no_color = std::env::var_os("NO_COLOR").is_some();
        let is_tty = std::io::stdout().is_terminal();
        let colorterm = std::env::var("COLORTERM").ok();
        let term = std::env::var("TERM").ok();
        classify_env(no_color, is_tty, colorterm.as_deref(), term.as_deref())
    })
}

// ────────────────────────────────────────────────────────────────────────
// Test helpers (D-06: MockTheme injection for snapshot stability)
// ────────────────────────────────────────────────────────────────────────

/// Fixed test palette so snapshot ANSI bytes are byte-stable across CI and
/// contributor machines (D-06). Background `#000000` + brand_accent
/// `#7287fd` match the snapshot fixtures under `src/brand/snapshots/`.
#[cfg(test)]
pub fn mock_theme() -> ThemeVariant {
    use std::collections::HashMap;
    ThemeVariant {
        id: "mock".to_string(),
        name: "Mock".to_string(),
        family: "Mock".to_string(),
        tool_refs: HashMap::new(),
        appearance: crate::theme::ThemeAppearance::Dark,
        auto_pair: None,
        palette: crate::theme::Palette {
            foreground: "#ffffff".to_string(),
            background: "#000000".to_string(),
            cursor: None,
            selection_bg: None,
            selection_fg: None,
            brand_accent: "#7287fd".to_string(),
            black: "#000000".to_string(),
            red: "#f38ba8".to_string(),
            green: "#a6d189".to_string(),
            yellow: "#e5c890".to_string(),
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
            bright_white: "#cdd6f4".to_string(),
            bg_dim: None,
            bg_darker: None,
            bg_darkest: None,
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
        },
    }
}

/// Build a truecolor `RenderContext` against the mock theme for snapshot
/// tests. Bypasses the env probe entirely.
#[cfg(test)]
pub fn mock_context(theme: &ThemeVariant) -> RenderContext<'_> {
    let cached_pill_bg = palette::pill_background_rgb(
        &theme.palette.brand_accent,
        &theme.palette.background,
        theme.appearance,
    )
    .ok()
    .map(|(r, g, b)| PaletteRenderer::rgb_to_ansi_24bit(r, g, b).replace("38;", "48;"));
    RenderContext {
        theme,
        mode: RenderMode::Truecolor,
        cached_pill_bg,
    }
}

/// Build a `RenderContext` with an explicit mode (used by the Basic/None
/// fallback snapshot tests in `roles.rs`).
#[cfg(test)]
pub fn mock_context_with_mode(theme: &ThemeVariant, mode: RenderMode) -> RenderContext<'_> {
    let cached_pill_bg = match mode {
        RenderMode::Truecolor => palette::pill_background_rgb(
            &theme.palette.brand_accent,
            &theme.palette.background,
            theme.appearance,
        )
        .ok()
        .map(|(r, g, b)| PaletteRenderer::rgb_to_ansi_24bit(r, g, b).replace("38;", "48;")),
        RenderMode::Basic | RenderMode::None => None,
    };
    RenderContext {
        theme,
        mode,
        cached_pill_bg,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_env_respects_no_color() {
        assert_eq!(
            classify_env(true, true, Some("truecolor"), Some("ghostty")),
            RenderMode::None
        );
    }

    #[test]
    fn classify_env_non_tty_is_none() {
        assert_eq!(
            classify_env(false, false, Some("truecolor"), Some("ghostty")),
            RenderMode::None
        );
    }

    #[test]
    fn classify_env_dumb_term_is_none() {
        assert_eq!(
            classify_env(false, true, Some("truecolor"), Some("dumb")),
            RenderMode::None
        );
    }

    /// D-05 + 18-VALIDATION row `18-W0-classify-env`: `COLORTERM=truecolor`
    /// with a TTY must upgrade to `RenderMode::Truecolor`.
    #[test]
    fn classify_env_returns_expected_mode() {
        assert_eq!(
            classify_env(false, true, Some("truecolor"), Some("xterm-256color")),
            RenderMode::Truecolor
        );
        assert_eq!(
            classify_env(false, true, Some("24bit"), Some("xterm")),
            RenderMode::Truecolor
        );
    }

    #[test]
    fn classify_env_basic_when_no_truecolor_hint() {
        assert_eq!(
            classify_env(false, true, None, Some("xterm")),
            RenderMode::Basic
        );
    }

    #[test]
    fn classify_env_infers_truecolor_from_known_terms() {
        assert_eq!(
            classify_env(false, true, None, Some("ghostty")),
            RenderMode::Truecolor
        );
    }

    #[test]
    fn mock_theme_has_expected_fixture_values() {
        let theme = mock_theme();
        assert_eq!(theme.palette.brand_accent, "#7287fd");
        assert_eq!(theme.palette.background, "#000000");
        assert_eq!(theme.palette.red, "#f38ba8");
    }

    #[test]
    fn mock_context_carries_truecolor_mode() {
        let theme = mock_theme();
        let ctx = mock_context(&theme);
        assert_eq!(ctx.mode, RenderMode::Truecolor);
        assert_eq!(ctx.theme.palette.brand_accent, "#7287fd");
        assert_eq!(ctx.theme.palette.background, "#000000");
    }
}
