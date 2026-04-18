//! Neovim colorscheme adapter — emits Lua files under the user's
//! `~/.config/nvim/` runtimepath. See Phase 17 plans 01-08.
//!
//! Delivered in waves:
//!   W1  — src/design/nvim_highlights.rs (role→group table)
//!   W2  — render_colorscheme + render_shim (this file, Plan 02)
//!   W3  — render_loader + write_state_file (this file)
//!   W4  — plugin groups + lualine_theme (this file)
//!   W5  — NvimAdapter trait impl + registry wiring (this file)
//!
//! Plan 02 deliverables (RED phase — tests in, implementations stubbed):
//!   • `render_colorscheme(palette, variant_id) -> String`
//!   • `render_shim(variant_id) -> String`
//!
//! Output shape reminder: `render_colorscheme`'s output is a leading Lua
//! comment followed by a BARE table literal. It is designed to be spliced
//! into the loader's `PALETTES` table by Plan 03 (i.e.
//! `PALETTES['<variant-id>'] = <render_colorscheme output>`). It is NOT a
//! standalone Lua module — don't wrap with `return` / `local t =` — and
//! Plan 07's syntax gate validates each variant's sub-table through the
//! loader parse path, not via a direct luafile on render_colorscheme
//! output (bare `{ ... }` at file-statement level is a Lua parse error).

#![allow(dead_code)]

use crate::theme::Palette;

/// Render ONE variant's highlight-group table as a Lua sub-table literal.
/// Plan 02 GREEN-phase implementation follows the RED test contract.
pub fn render_colorscheme(_palette: &Palette, _variant_id: &str) -> String {
    unimplemented!("render_colorscheme — implemented in Plan 02 GREEN phase")
}

/// Render the 2-line shim written to `~/.config/nvim/colors/slate-<id>.lua`.
/// Plan 02 GREEN-phase implementation follows the RED test contract.
pub fn render_shim(_variant_id: &str) -> String {
    unimplemented!("render_shim — implemented in Plan 02 GREEN phase")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design::nvim_highlights::HIGHLIGHT_GROUPS;
    use crate::theme::ThemeRegistry;

    // ── render_shim ────────────────────────────────────────────────────

    #[test]
    fn render_shim_matches_exact_shape() {
        let out = render_shim("catppuccin-mocha");
        assert_eq!(
            out,
            "-- slate-managed: do not edit. Regenerate via `slate setup`.\n\
             vim.g.colors_name = 'slate-catppuccin-mocha'\n\
             require('slate').load('catppuccin-mocha')\n"
        );
    }

    #[test]
    fn render_shim_contains_single_require_slate_load_call_for_each_id() {
        for variant_id in ["catppuccin-mocha", "tokyo-night-dark", "dracula"] {
            let out = render_shim(variant_id);
            let require_line = format!("require('slate').load('{}')", variant_id);
            assert_eq!(
                out.matches(&require_line).count(),
                1,
                "shim for {} must contain exactly one `{}` call; output: {}",
                variant_id,
                require_line,
                out
            );
            assert_eq!(
                out.matches("require('slate').load(").count(),
                1,
                "shim for {} must contain exactly one require('slate').load(...) call",
                variant_id
            );
        }
    }

    // ── render_colorscheme — determinism, line endings, shape ──────────

    #[test]
    fn render_colorscheme_is_deterministic() {
        let registry = ThemeRegistry::new().expect("registry init");
        let v = registry.get("catppuccin-mocha").expect("theme exists");
        let a = render_colorscheme(&v.palette, &v.id);
        let b = render_colorscheme(&v.palette, &v.id);
        assert_eq!(a, b, "render_colorscheme must be deterministic");
    }

    #[test]
    fn render_colorscheme_has_lf_line_endings_only() {
        let registry = ThemeRegistry::new().expect("registry init");
        let v = registry.get("catppuccin-mocha").expect("theme exists");
        let out = render_colorscheme(&v.palette, &v.id);
        assert!(!out.contains('\r'), "output must use LF only");
    }

    #[test]
    fn render_colorscheme_output_is_splice_target_shape() {
        // Guards the Plan 03 splice contract: output must be
        // `-- comment\n{ ... }` — leading comment plus bare table literal.
        // NOT `return { ... }` or `local t = { ... }` — doing so would
        // break Plan 03's `PALETTES['<id>'] = <output>` splice.
        let registry = ThemeRegistry::new().expect("registry init");
        let v = registry.get("catppuccin-mocha").expect("theme exists");
        let out = render_colorscheme(&v.palette, &v.id);

        assert!(
            out.starts_with("-- slate-managed palette for catppuccin-mocha\n"),
            "output must start with the variant comment stamp"
        );

        let rest = out.split_once('\n').map(|x| x.1).unwrap_or("");
        assert!(
            rest.starts_with('{'),
            "after comment, output must be a bare table literal starting with '{{', got: {:?}",
            &rest[..rest.len().min(40)]
        );

        assert!(
            out.trim_end().ends_with('}'),
            "output must end with '}}' — no wrapping allowed"
        );

        assert!(
            !out.contains("return {"),
            "output must NOT be wrapped with `return {{` — it's a splice target"
        );
        assert!(
            !out.contains("local t ="),
            "output must NOT be wrapped with `local t =` — it's a splice target"
        );
    }

    #[test]
    fn render_colorscheme_contains_variant_marker_comment() {
        let registry = ThemeRegistry::new().expect("registry init");
        let v = registry.get("catppuccin-mocha").expect("theme exists");
        let out = render_colorscheme(&v.palette, &v.id);
        assert!(
            out.starts_with("-- slate-managed palette for catppuccin-mocha\n"),
            "variant id must be stamped at top of output"
        );
    }

    // ── render_colorscheme — per-variant coverage + size bounds ────────

    #[test]
    fn render_colorscheme_smoke_all_variants_size_bounded() {
        let registry = ThemeRegistry::new().expect("registry init");
        for v in registry.all() {
            let out = render_colorscheme(&v.palette, &v.id);
            assert!(
                out.len() >= 5_000,
                "variant {}: output too small ({} bytes)",
                v.id,
                out.len()
            );
            assert!(
                out.len() <= 80_000,
                "variant {}: output too large ({} bytes)",
                v.id,
                out.len()
            );
        }
    }

    #[test]
    fn render_includes_treesitter_and_lsp_keys() {
        let registry = ThemeRegistry::new().expect("registry init");
        let v = registry.get("catppuccin-mocha").expect("theme exists");
        let out = render_colorscheme(&v.palette, &v.id);
        for required in &[
            "[\"@comment\"]",
            "[\"@function\"]",
            "[\"@lsp.type.parameter\"]",
            "DiagnosticError",
            "DiffAdd",
        ] {
            assert!(
                out.contains(required),
                "output missing {:?}:\n---\n{}\n---",
                required,
                &out[..out.len().min(500)]
            );
        }
    }

    #[test]
    fn render_colorscheme_emits_at_least_one_entry_per_highlight_group() {
        let registry = ThemeRegistry::new().expect("registry init");
        let v = registry.get("catppuccin-mocha").expect("theme exists");
        let out = render_colorscheme(&v.palette, &v.id);
        for (name, _spec) in HIGHLIGHT_GROUPS {
            if name.starts_with('@') {
                let needle = format!("[{:?}] = ", name);
                assert!(
                    out.contains(&needle),
                    "missing bracketed treesitter/lsp key `{}`",
                    needle
                );
            } else {
                let needle = format!("  {} = ", name);
                assert!(
                    out.contains(&needle),
                    "missing identifier key `{}`",
                    needle.trim_end()
                );
            }
        }
    }

    #[test]
    fn invalid_hex_degrades_to_none_not_panic() {
        // Palette fields are public; corrupt `background` so the bad hex
        // reaches at least one HighlightSpec (`Normal.bg = Background`).
        let mut v = ThemeRegistry::new()
            .expect("registry init")
            .get("catppuccin-mocha")
            .expect("theme exists")
            .clone();
        v.palette.background = String::from("#notahex");

        let out = render_colorscheme(&v.palette, &v.id);
        assert!(!out.is_empty(), "render_colorscheme returned empty");
        assert!(
            out.contains("bg = 'NONE'"),
            "expected `bg = 'NONE'` sentinel somewhere in output; \
             corruption of background did not degrade gracefully"
        );
    }

    // ── Snapshot gate for the canonical theme ──────────────────────────

    #[test]
    fn insta_snapshot_catppuccin_mocha() {
        let v = ThemeRegistry::new()
            .expect("registry init")
            .get("catppuccin-mocha")
            .expect("theme exists")
            .clone();
        let out = render_colorscheme(&v.palette, &v.id);
        insta::assert_snapshot!("nvim_render_colorscheme_catppuccin_mocha", out);
    }
}
