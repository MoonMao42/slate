//! Responsive fold composer for picker live-preview.
//! Pure fns: palette + rows + roles → String. No I/O; no state mutation.
//! Every `◆ Heading` label flows through [`crate::brand::roles::Roles::heading`]
//! so the brand-lavender (`#7287fd`) byte contract is honored.
//! Block bodies come from [`super::blocks`] — this module never emits its
//! own syntax-highlighting spans, only stacks what `blocks::*` produces.
//! ## Fold tiers 
//! | rows | tier | blocks |
//! |----------|-----------|----------------------------------------------|
//! | 0..=31 | Minimum | Palette, Prompt, Code, Files |
//! | 32..=39 | Medium | + Git, Diff |
//! | 40..=∞ | Large | + Lazygit, Nvim |
//! Boundaries may tune ±1-2 rows after UAT (RESEARCH §A6); the tests lock
//! 31→Minimum, 32→Medium, 39→Medium, 40→Large.
//! ## Prompt injection
//! [`compose_full`] takes an optional `prompt_line_override` so 
//! `starship_fork` can inject the real forked prompt. When `None`, the
//! composer falls back to
//! [`crate::cli::picker::preview_panel::self_draw_prompt_from_sample_tokens`]
//! (self-draw fallback). This keeps the composer fork-agnostic and
//! pure-testable.
//! ## Diff / Lazygit / Nvim placeholders
//! These tiers extend the preview beyond what demo.rs ever rendered, so we
//! ship lean placeholder bodies that reuse `palette.{green,red,blue,magenta}`
//! fields directly. They're stylistically consistent with the real blocks
//! (palette-tinted, 2-3 lines each) but are explicitly placeholders — if
//! UAT shows them too sparse, richer analogs can land in a follow-up.

use crate::brand::roles::Roles;
use crate::theme::Palette;

use super::blocks;

/// Responsive fold tier derived from terminal row count.
/// Thresholds were re-calibrated after UAT (2026-04-20) against the measured
/// heights of each compose tier (including the ◆ Heading labels and blank
/// separators) plus ~3 rows of picker chrome (logo + "preview · Tab to
/// return" breadcrumb + blank). Minimum content = ~29 rows, Medium = ~42,
/// Large = ~50. Original thresholds (32/40) were "number of blocks" not
/// "row budget" and clipped the top of the preview in any real terminal.
#[allow(dead_code)] // Wired by render::render mode dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FoldTier {
    /// < 45 rows — stack 4 blocks: Palette / Prompt / Code / Files.
    Minimum,
    /// 45..=53 rows — stack 6 blocks (+ Git, Diff).
    Medium,
    /// ≥ 54 rows — stack 8 blocks (+ Lazygit, Nvim).
    Large,
}

/// Decide the fold tier for `rows` terminal rows per.
/// Boundaries reflect actual compose output heights + picker chrome, not
/// raw block counts — see `FoldTier` docstring.
#[allow(dead_code)] // Wired by render::render mode dispatch.
pub(crate) fn decide_fold_tier(rows: u16) -> FoldTier {
    match rows {
        0..=44 => FoldTier::Minimum,
        45..=53 => FoldTier::Medium,
        _ => FoldTier::Large,
    }
}

/// Compose the 3-line mini-preview for list-dominant mode.
/// Layout:
/// 1. Palette swatch row — `blocks::render_palette_swatch(palette, false)`
/// (8 bg cells, already ends with `\n`).
/// 2. Self-drawn starship-esque prompt — never forks in mini mode.
/// 3. Blank separator — caller layers its own "↑↓ theme · Tab fullscreen"
/// help text below, so the composer just leaves a clean spacer line.
/// `_roles` is accepted for signature symmetry with [`compose_full`]; the
/// mini strip has no `◆ Heading` labels so the roles handle is unused
/// today. Keeping it in the signature means call-sites don't
/// need to branch on preview mode when materializing Roles.
/// Output always ends with `\n` so the caller can append further lines
/// cleanly.
#[allow(dead_code)] // Wired by render::render mode dispatch.
pub(crate) fn compose_mini(palette: &Palette, _roles: Option<&Roles<'_>>) -> String {
    let mut out = String::with_capacity(512);
    out.push_str(&blocks::render_palette_swatch(palette, false));
    out.push_str(&crate::cli::picker::preview_panel::self_draw_prompt_from_sample_tokens(palette));
    out.push('\n');
    // Blank separator line — keeps mini strip at 3 content lines + trailing
    // whitespace so render.rs can place its help text below without overlap.
    out.push('\n');
    out
}

/// Compose the full-screen stacked preview for Tab mode.
/// `prompt_line_override` is injected by `starship_fork` when
/// forking succeeds; `None` falls back to the self-drawn prompt.
/// Returns a single `String` containing every block, each prefixed with
/// its `◆ Heading` label (brand-lavender via `Roles::heading`).
#[allow(dead_code)] // Wired by render::render mode dispatch.
pub(crate) fn compose_full(
    palette: &Palette,
    tier: FoldTier,
    roles: Option<&Roles<'_>>,
    prompt_line_override: Option<&str>,
) -> String {
    let mut out = String::with_capacity(8192);

    push_heading(&mut out, roles, "Palette");
    out.push_str(&blocks::render_palette_swatch(palette, true));

    push_heading(&mut out, roles, "Prompt");
    match prompt_line_override {
        Some(fork) => {
            // starship's `add_newline = true` config option (enabled in
            // slate's managed plain.toml) prepends a `\n` to its output so
            // each invocation visually separates from the previous command.
            // Inside the preview composer that leading newline shows up as
            // an unwanted blank line between `◆ Prompt` and the prompt
            // itself. Strip leading newlines so the prompt butts directly
            // under its heading.
            out.push_str(fork.trim_start_matches('\n'));
            if !fork.ends_with('\n') {
                out.push('\n');
            }
        }
        None => {
            out.push_str(
                &crate::cli::picker::preview_panel::self_draw_prompt_from_sample_tokens(palette),
            );
            out.push('\n');
        }
    }

    push_heading(&mut out, roles, "Code");
    out.push_str(&blocks::render_code_block(palette));
    out.push('\n');

    push_heading(&mut out, roles, "Files");
    out.push_str(&blocks::render_tree_block(palette));
    out.push('\n');

    if matches!(tier, FoldTier::Medium | FoldTier::Large) {
        push_heading(&mut out, roles, "Git");
        out.push_str(&blocks::render_git_log_block(palette));
        out.push('\n');
        push_heading(&mut out, roles, "Diff");
        out.push_str(&render_diff_placeholder(palette));
        out.push('\n');
    }
    if matches!(tier, FoldTier::Large) {
        push_heading(&mut out, roles, "Lazygit");
        out.push_str(&render_lazygit_placeholder(palette));
        out.push('\n');
        push_heading(&mut out, roles, "Nvim");
        out.push_str(&render_nvim_placeholder(palette));
        out.push('\n');
    }

    out
}

/// Push `◆ title` + newline to `out`, routing through `Roles::heading`
/// when `roles` is `Some` (brand-lavender in Truecolor mode) and falling
/// back to plain `◆ title` otherwise. Mirrors the `heading_text` helper
/// in `src/cli/list.rs:73-78`.
fn push_heading(out: &mut String, roles: Option<&Roles<'_>>, title: &str) {
    match roles {
        Some(r) => out.push_str(&r.heading(title)),
        None => {
            out.push_str("◆ ");
            out.push_str(title);
        }
    }
    out.push('\n');
}

// ── Placeholder block bodies for Medium / Large tiers ─────────────────────
// These helpers render 2-3 lines of palette-tinted text. They're NOT full
// renderers — real diff / lazygit / nvim block renderers can land in a
// follow-up if UAT shows the placeholders are too sparse (see
// CONTEXT §deferred). Real Palette field names per V-07 preflight:
// `palette.green`, `palette.red`, `palette.blue`, `palette.magenta`
// (semantic names), NOT `ansi_00..ansi_15`.

/// 2-line diff placeholder — a `+` added line (green) and a `-` removed
/// line (red). Mirrors `git diff --color` summary output.
// SWATCH-RENDERER: intentionally raw ANSI (renders palette colors, not role text)
fn render_diff_placeholder(palette: &Palette) -> String {
    let green = rgb_fg(&palette.green);
    let red = rgb_fg(&palette.red);
    format!(
        "{green}+ pub fn handle() -> Result<()> {{\x1b[0m\n\
         {red}- pub fn handle() {{\x1b[0m\n"
    )
}

/// 3-line lazygit-style unstaged-changes summary.
// SWATCH-RENDERER: intentionally raw ANSI (renders palette colors, not role text)
fn render_lazygit_placeholder(palette: &Palette) -> String {
    let accent = rgb_fg(&palette.blue);
    format!(
        "{accent}  Unstaged changes (2)\x1b[0m\n\
         {accent}    modified  src/cli/picker/preview/compose.rs\x1b[0m\n\
         {accent}    modified  src/cli/picker/render.rs\x1b[0m\n"
    )
}

/// Single-line nvim-style snippet: keyword + string literal.
// SWATCH-RENDERER: intentionally raw ANSI (renders palette colors, not role text)
fn render_nvim_placeholder(palette: &Palette) -> String {
    let keyword = rgb_fg(&palette.magenta);
    let string = rgb_fg(&palette.green);
    format!("  {keyword}fn\x1b[0m main() {{ println!({string}\"hello from nvim\"\x1b[0m); }}\n")
}

/// Build a 24-bit foreground ANSI prefix from a `#RRGGBB` palette hex.
/// Falls back to gray (128,128,128) so malformed palettes never crash
/// the composer.
// SWATCH-RENDERER: intentionally raw ANSI (renders palette colors, not role text)
fn rgb_fg(hex: &str) -> String {
    use crate::adapter::palette_renderer::PaletteRenderer;
    let (r, g, b) = PaletteRenderer::hex_to_rgb(hex).unwrap_or((128, 128, 128));
    format!("\x1b[38;2;{};{};{}m", r, g, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Strip ANSI escape sequences (CSI … final byte) so visible-content
    /// assertions aren't brittle against the SGR codes blocks::* emits.
    /// Iterates chars (NOT bytes) so multi-byte UTF-8 glyphs like `◆` and
    /// `❯` survive round-trip intact.
    fn strip_ansi(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\x1b' && chars.peek() == Some(&'[') {
                // Consume the '[' and then the CSI parameter+intermediate
                // bytes until we hit a final byte in 0x40..=0x7e.
                chars.next();
                while let Some(&nc) = chars.peek() {
                    chars.next();
                    let code = nc as u32;
                    if (0x40..=0x7e).contains(&code) {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    /// Deterministic mock palette (no cross-module dependency on
    /// `brand::render_context::mock_theme` for the purely-palette tests).
    fn mock_palette() -> Palette {
        crate::brand::render_context::mock_theme().palette
    }

    // ── Task 19-04-01 tests ─────────────────────────────────────────────

    /// Responsive fold thresholds — re-calibrated 2026-04-20 to match actual
    /// compose heights + picker chrome. Boundary assertions:
    /// 44→Minimum, 45→Medium, 53→Medium, 54→Large.
    #[test]
    fn fold_thresholds_45_54() {
        assert!(matches!(decide_fold_tier(0), FoldTier::Minimum));
        assert!(matches!(decide_fold_tier(32), FoldTier::Minimum));
        assert!(matches!(decide_fold_tier(44), FoldTier::Minimum));
        assert!(matches!(decide_fold_tier(45), FoldTier::Medium));
        assert!(matches!(decide_fold_tier(46), FoldTier::Medium));
        assert!(matches!(decide_fold_tier(53), FoldTier::Medium));
        assert!(matches!(decide_fold_tier(54), FoldTier::Large));
        assert!(matches!(decide_fold_tier(80), FoldTier::Large));
    }

    /// mini-preview contract — exactly 3 `\n` characters: swatch row, prompt row, separator row.
    /// `compose_mini` must stay compact because render.rs layers its help line below.
    #[test]
    fn mini_is_exactly_three_lines() {
        let palette = mock_palette();
        let out = compose_mini(&palette, None);
        let newlines = out.matches('\n').count();
        assert_eq!(
            newlines, 3,
            "compose_mini must emit exactly 3 newlines (swatch + prompt + separator), got {newlines}: {out:?}"
        );
    }

    /// Mini strip must contain both swatch-bg bytes and prompt-fg bytes
    /// plus a visible prompt sigil, so the list-dominant footer is
    /// actually informative.
    #[test]
    fn mini_contains_swatch_and_prompt() {
        let palette = mock_palette();
        let out = compose_mini(&palette, None);
        assert!(
            out.contains("\x1b[48;2;"),
            "mini output must contain at least one 24-bit bg swatch escape, got: {out:?}"
        );
        assert!(
            out.contains("\x1b[38;2;"),
            "mini output must contain at least one 24-bit fg prompt escape, got: {out:?}"
        );
        let visible = strip_ansi(&out);
        assert!(
            visible.contains('❯'),
            "mini output visible text must include the ❯ prompt sigil, got: {visible:?}"
        );
    }

    /// `self_draw_prompt_from_sample_tokens` returns a prompt-like line
    /// with 24-bit fg escapes and at least one SAMPLE_TOKENS character.
    #[test]
    fn self_draw_prompt_uses_sample_tokens() {
        let palette = mock_palette();
        let out = crate::cli::picker::preview_panel::self_draw_prompt_from_sample_tokens(&palette);
        assert!(
            out.contains("\x1b[38;2;"),
            "self-draw prompt must carry 24-bit fg bytes, got: {out:?}"
        );
        let visible = strip_ansi(&out);
        // Must contain some recognizable token from SAMPLE_TOKENS prompt prefix.
        let has_dir = visible.contains("~/code/slate");
        let has_branch = visible.contains("main");
        let has_sigil = visible.contains('❯');
        assert!(
            has_dir || has_branch || has_sigil,
            "self-draw prompt must contain at least one SAMPLE_TOKENS identifier, got visible: {visible:?}"
        );
        // Caller owns trailing newline — we must NOT add one.
        assert!(
            !out.ends_with('\n'),
            "self_draw_prompt_from_sample_tokens must not end with newline (caller policy), got: {out:?}"
        );
    }

    // ── Task 19-04-02 tests ─────────────────────────────────────────────

    /// Minimum tier — exactly 4 `◆ ` heading labels.
    #[test]
    fn compose_full_minimum_has_four_heading_labels() {
        let palette = mock_palette();
        let out = compose_full(&palette, FoldTier::Minimum, None, None);
        let count = out.matches("◆ ").count();
        assert_eq!(
            count, 4,
            "Minimum tier must emit exactly 4 ◆ labels (Palette/Prompt/Code/Files), got {count}: {out:?}"
        );
        // Verify label identities survive.
        let visible = strip_ansi(&out);
        for label in ["◆ Palette", "◆ Prompt", "◆ Code", "◆ Files"] {
            assert!(
                visible.contains(label),
                "Minimum tier must include label {label:?}, got visible: {visible:?}"
            );
        }
    }

    /// Medium tier — exactly 6 `◆ ` heading labels.
    #[test]
    fn compose_full_medium_has_six_heading_labels() {
        let palette = mock_palette();
        let out = compose_full(&palette, FoldTier::Medium, None, None);
        let count = out.matches("◆ ").count();
        assert_eq!(
            count, 6,
            "Medium tier must emit exactly 6 ◆ labels (+ Git/Diff), got {count}: {out:?}"
        );
        let visible = strip_ansi(&out);
        for label in ["◆ Git", "◆ Diff"] {
            assert!(
                visible.contains(label),
                "Medium tier must include label {label:?}, got visible: {visible:?}"
            );
        }
    }

    /// Large tier — exactly 8 `◆ ` heading labels.
    #[test]
    fn compose_full_large_has_eight_heading_labels() {
        let palette = mock_palette();
        let out = compose_full(&palette, FoldTier::Large, None, None);
        let count = out.matches("◆ ").count();
        assert_eq!(
            count, 8,
            "Large tier must emit exactly 8 ◆ labels (+ Lazygit/Nvim), got {count}: {out:?}"
        );
        let visible = strip_ansi(&out);
        for label in ["◆ Lazygit", "◆ Nvim"] {
            assert!(
                visible.contains(label),
                "Large tier must include label {label:?}, got visible: {visible:?}"
            );
        }
    }

    /// invariant — when `roles` is `Some`, every `◆`
    /// carries the brand-lavender byte triple `38;2;114;135;253` (from
    /// `BRAND_LAVENDER_FIXED = #7287fd`). Uses the test-only
    /// `brand::render_context::mock_{theme,context}` helpers so the test
    /// is byte-stable across machines.
    #[test]
    fn heading_uses_roles_lavender_when_roles_some() {
        let theme = crate::brand::render_context::mock_theme();
        let ctx = crate::brand::render_context::mock_context(&theme);
        let roles = Roles::new(&ctx);
        let out = compose_full(&theme.palette, FoldTier::Minimum, Some(&roles), None);
        assert!(
            out.contains("38;2;114;135;253"),
            "Roles::heading must emit brand-lavender bytes (#7287fd = 114;135;253), got: {out:?}"
        );
        // And the literal diamond still appears (bytes wrap the glyph).
        assert!(
            out.contains("◆"),
            "◆ glyph must appear alongside the lavender fg wrapper, got: {out:?}"
        );
    }

    /// contract — `prompt_line_override = Some(fork)` replaces
    /// the self-drawn prompt verbatim. The override string must appear
    /// and the self-draw signature must be absent.
    #[test]
    fn prompt_override_replaces_self_draw() {
        let palette = mock_palette();
        let marker = "(phase-19-test-override-prompt)";
        let out = compose_full(&palette, FoldTier::Minimum, None, Some(marker));
        assert!(
            out.contains(marker),
            "override string must appear verbatim in compose_full output, got: {out:?}"
        );
        // Self-draw signature — the `❯` prompt sigil — must NOT appear
        // in the Prompt block when an override is injected.
        let visible = strip_ansi(&out);
        assert!(
            !visible.contains('❯'),
            "self-draw sigil ❯ must NOT appear when prompt_line_override is Some(_), got visible: {visible:?}"
        );
    }
}
