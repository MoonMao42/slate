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
//! Plan 02 deliverables landed in this file:
//!   • `render_colorscheme(palette, variant_id) -> String` — emits ONE
//!     variant's highlight-group sub-table as `-- comment\n{ ... }`
//!     (splice target, not a standalone Lua module).
//!   • `render_shim(variant_id) -> String` — emits the 2-line shim that
//!     lives at `~/.config/nvim/colors/slate-<id>.lua`.
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

use crate::adapter::palette_renderer::PaletteRenderer;
use crate::cli::picker::preview_panel::SemanticColor;
use crate::design::nvim_highlights::{HighlightSpec, Style, HIGHLIGHT_GROUPS};
use crate::env::SlateEnv;
use crate::error::Result;
use crate::theme::Palette;
use atomic_write_file::AtomicWriteFile;
use std::fmt::Write as FmtWrite;
use std::io::Write as IoWrite;
use std::path::PathBuf;

/// Render ONE variant's highlight-group table as a Lua sub-table literal.
///
/// Shape (matches 17-RESEARCH.md §Pattern 2 loader `PALETTES` entry) —
/// leading comment + bare table literal, designed to be spliced into
/// `PALETTES['<variant-id>'] = <output>` by render_loader (Plan 03):
///
/// ```text
/// -- slate-managed palette for catppuccin-mocha
/// {
///   Normal     = { fg = '#cdd6f4', bg = '#1e1e2e' },
///   Comment    = { fg = '#6c7086', italic = true },
///   FloatBorder = { link = 'FloatBorder' },
///   ...
/// }
/// ```
///
/// NOT a standalone Lua file. Bare `{ ... }` at file-statement level is a
/// Lua parse error without `return` or assignment; do not attempt to
/// `luafile` this directly. Plan 07 validates each variant's sub-table
/// via the loader parse path (`loader_lua_parses_via_luafile`), which
/// parses all 18 spliced sub-tables together inside the loader's
/// `PALETTES` block.
pub fn render_colorscheme(palette: &Palette, variant_id: &str) -> String {
    // Body: `{\n  <entries>\n}` — the bare table literal.
    let mut body = String::with_capacity(16 * 1024);
    body.push_str("{\n");
    for (name, spec) in HIGHLIGHT_GROUPS {
        // Stable: iteration order is the slice declaration order, which
        // `nvim_highlights` documents as intentional.
        let _ = write_lua_entry(&mut body, name, spec, palette);
    }
    body.push('}');

    // Stamp a leading comment with the variant id so the spliced loader
    // is self-documenting. Plan 03 preserves this comment when splicing.
    let mut out = String::with_capacity(body.len() + 64);
    let _ = writeln!(out, "-- slate-managed palette for {}", variant_id);
    out.push_str(&body);
    out
}

/// Render the 2-line (really 3-line with the leading comment) shim written
/// to `~/.config/nvim/colors/slate-<id>.lua`.
///
/// Shape (17-CONTEXT.md D-02 + 17-RESEARCH.md §Example 1):
///
/// ```text
/// -- slate-managed: do not edit. Regenerate via `slate setup`.
/// vim.g.colors_name = 'slate-<variant-id>'
/// require('slate').load('<variant-id>')
/// ```
pub fn render_shim(variant_id: &str) -> String {
    format!(
        "-- slate-managed: do not edit. Regenerate via `slate setup`.\n\
         vim.g.colors_name = 'slate-{id}'\n\
         require('slate').load('{id}')\n",
        id = variant_id,
    )
}

/// Write one `<GroupName> = { ... }` line to `out`.
///
/// Returns `Ok(())` on success; `String::write_*` never actually fails in
/// practice, but the result is propagated for future-proofing.
fn write_lua_entry(
    out: &mut String,
    name: &str,
    spec: &HighlightSpec,
    palette: &Palette,
) -> std::fmt::Result {
    // Treesitter / LSP group names like `@lsp.type.parameter` are NOT
    // valid Lua identifiers (the leading `@` + dots break it), so they
    // must use the bracketed-string-key form `["@..."] = …`. Plain
    // identifier names use the dot-style `Name = …` form.
    if name.starts_with('@') {
        // `{:?}` on a `&str` produces a quoted, escape-safe Lua-compatible
        // double-quoted string literal.
        write!(out, "  [{:?}] = ", name)?;
    } else {
        write!(out, "  {} = ", name)?;
    }

    // Link-style: `{ link = 'Target' }` — fg/bg/style are ignored when
    // `link` is present, mirroring nvim's own behaviour for
    // `nvim_set_hl`'s `link` attribute.
    if let Some(target) = spec.link {
        writeln!(out, "{{ link = '{}' }},", target)?;
        return Ok(());
    }

    // Plain spec: emit fg / bg / style in canonical order. Use a local
    // "wrote anything yet?" latch so we can emit comma separators cleanly
    // without leaving a trailing comma inside the inner table.
    out.push_str("{ ");
    let mut wrote_any = false;
    if let Some(color) = spec.fg {
        let hex = resolve_with_fallback(palette, color);
        if wrote_any {
            out.push_str(", ");
        }
        write!(out, "fg = '{}'", hex)?;
        wrote_any = true;
    }
    if let Some(color) = spec.bg {
        let hex = resolve_with_fallback(palette, color);
        if wrote_any {
            out.push_str(", ");
        }
        write!(out, "bg = '{}'", hex)?;
        wrote_any = true;
    }
    match spec.style {
        Style::None => {}
        Style::Bold => {
            if wrote_any {
                out.push_str(", ");
            }
            out.push_str("bold = true");
            wrote_any = true;
        }
        Style::Italic => {
            if wrote_any {
                out.push_str(", ");
            }
            out.push_str("italic = true");
            wrote_any = true;
        }
        Style::Underline => {
            if wrote_any {
                out.push_str(", ");
            }
            out.push_str("underline = true");
            wrote_any = true;
        }
        Style::Undercurl => {
            if wrote_any {
                out.push_str(", ");
            }
            out.push_str("undercurl = true");
            wrote_any = true;
        }
        Style::Reverse => {
            if wrote_any {
                out.push_str(", ");
            }
            out.push_str("reverse = true");
            wrote_any = true;
        }
    }
    // Empty spec (all None + Style::None) is emitted as `{ }`, which
    // nvim accepts as a no-op for that group (useful for `@none` etc.).
    // Only the `Bold`/`Italic`/`Underline`/`Undercurl`/`Reverse` table
    // above leaves `wrote_any = false`; suppress the "empty inner" noise
    // but keep the braces for a uniform shape.
    let _ = wrote_any;
    out.push_str(" },\n");
    Ok(())
}

/// Resolve a `SemanticColor` to a `#RRGGBB` hex string, degrading to the
/// Lua sentinel `'NONE'` on parse failure — `nvim_set_hl` interprets
/// `NONE` as "unset" for that attribute, so a single malformed hex does
/// not break the whole colorscheme.
///
/// Plan 01's `resolve` tests prove every shipped palette yields a clean
/// hex for all referenced `SemanticColor` variants, so this branch only
/// fires on hand-constructed test palettes with intentionally-broken
/// fields (see `invalid_hex_degrades_to_none_not_panic`).
fn resolve_with_fallback(palette: &Palette, role: SemanticColor) -> String {
    let hex = palette.resolve(role);
    if PaletteRenderer::hex_to_rgb(&hex).is_ok() {
        hex
    } else {
        String::from("NONE")
    }
}

// ── Plan 17-03 Task 2: state-file plumbing ─────────────────────────────

/// Atomically write the active-variant state file observed by the Lua
/// watcher registered by `render_loader` (Plan 03 Task 3).
///
/// Path: `<env.slate_cache_dir()>/current_theme.lua`.
/// Content: `return "<variant-id>"\n` — a minimal Lua string literal so
/// `dofile(path)` / `pcall(dofile, path)` returns the variant id.
///
/// Atomicity: `AtomicWriteFile::commit()` performs `fsync → rename`,
/// which fires EXACTLY ONE `fs_event` on the Lua watcher side — this
/// is the load-bearing behaviour D-04 depends on. Never replace this
/// with `std::fs::write` or a manual `.tmp` + rename dance; they can
/// fire multiple events (Plan 07 Task 2 has an fs-event counter that
/// would catch the regression).
///
/// The parent directory is created if missing (first-run safety).
pub fn write_state_file(env: &SlateEnv, variant_id: &str) -> Result<()> {
    let path = state_file_path(env);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = format!("return {}\n", lua_string_literal(variant_id));
    let mut file = AtomicWriteFile::open(&path)?;
    file.write_all(content.as_bytes())?;
    file.commit()?;
    Ok(())
}

/// Compute the canonical state-file path for a given env.
///
/// `pub(crate)` so Plan 05's adapter and Plan 06's clean helper can
/// reach it without duplicating the join.
pub(crate) fn state_file_path(env: &SlateEnv) -> PathBuf {
    env.slate_cache_dir().join("current_theme.lua")
}

/// Escape a variant id for embedding inside a Lua double-quoted string
/// literal. Variant ids are kebab-case ASCII in practice (no trigger),
/// but defensive escaping keeps the contract safe for any future id
/// scheme that could reach this code path.
fn lua_string_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
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
            // Guard against substring collisions by counting the generic
            // pattern and asserting it equals 1 too.
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

        // After the comment line, the rest must begin with `{`.
        let rest = out.split_once('\n').map(|x| x.1).unwrap_or("");
        assert!(
            rest.starts_with('{'),
            "after comment, output must be a bare table literal starting with '{{', got: {:?}",
            &rest[..rest.len().min(40)]
        );

        // And end with `}` (no trailing `return`, no trailing `end`).
        assert!(
            out.trim_end().ends_with('}'),
            "output must end with '}}' — no wrapping allowed"
        );

        // Explicitly reject accidental wrapping patterns.
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
        // The output must reference every entry name from HIGHLIGHT_GROUPS
        // verbatim (either as `Name = { ... }` or `["@name"] = { ... }`).
        // This catches silent regressions where the renderer skips entries.
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
        // Construct a palette with a hand-broken hex. The `Background`
        // role resolves straight to `Palette::background`, which is a
        // required field, so corrupting that field guarantees the bad
        // hex flows through at least one HighlightSpec (`Normal` has
        // bg = Background).
        let mut v = ThemeRegistry::new()
            .expect("registry init")
            .get("catppuccin-mocha")
            .expect("theme exists")
            .clone();
        v.palette.background = String::from("#notahex");

        // The render call must not panic AND the `NONE` sentinel must
        // appear at least once (because `Normal.bg` and the other groups
        // that bind to Background all degrade to NONE).
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

    // ── write_state_file — Plan 17-03 Task 2 ──────────────────────────

    #[test]
    fn write_state_file_writes_exact_content() {
        use crate::env::SlateEnv;
        use tempfile::TempDir;
        let td = TempDir::new().expect("tempdir");
        let env = SlateEnv::with_home(td.path().to_path_buf());
        write_state_file(&env, "catppuccin-mocha").expect("write ok");
        let got = std::fs::read_to_string(state_file_path(&env)).expect("read");
        assert_eq!(got, "return \"catppuccin-mocha\"\n");
    }

    #[test]
    fn write_state_file_creates_parent_directory() {
        use crate::env::SlateEnv;
        use tempfile::TempDir;
        let td = TempDir::new().expect("tempdir");
        let env = SlateEnv::with_home(td.path().to_path_buf());
        let path = state_file_path(&env);
        // Precondition: cache dir must be absent before the call.
        assert!(
            !path.parent().expect("has parent").exists(),
            "precondition: cache dir must be absent before write_state_file"
        );
        write_state_file(&env, "tokyo-night-dark").expect("creates parent");
        assert!(path.exists(), "state file should exist after write");
    }

    #[test]
    fn write_state_file_is_overwrite_not_append() {
        use crate::env::SlateEnv;
        use tempfile::TempDir;
        let td = TempDir::new().expect("tempdir");
        let env = SlateEnv::with_home(td.path().to_path_buf());
        write_state_file(&env, "a").expect("write a");
        write_state_file(&env, "b").expect("write b");
        let got = std::fs::read_to_string(state_file_path(&env)).expect("read");
        assert_eq!(got, "return \"b\"\n");
    }

    #[test]
    fn lua_string_literal_escapes_metachars() {
        assert_eq!(lua_string_literal("simple"), "\"simple\"");
        assert_eq!(lua_string_literal("has\"quote"), "\"has\\\"quote\"");
        assert_eq!(lua_string_literal("has\\back"), "\"has\\\\back\"");
    }

    #[test]
    fn write_state_file_escapes_quote_metachar() {
        // Defensive: even though variant ids are kebab-case ASCII in
        // practice, the escaping contract must hold for any input that
        // could ever reach this call path (future id schemes).
        use crate::env::SlateEnv;
        use tempfile::TempDir;
        let td = TempDir::new().expect("tempdir");
        let env = SlateEnv::with_home(td.path().to_path_buf());
        write_state_file(&env, "has\"quote").expect("write escapes quote");
        let got = std::fs::read_to_string(state_file_path(&env)).expect("read");
        assert_eq!(got, "return \"has\\\"quote\"\n");
    }

    #[test]
    fn write_state_file_escapes_backslash_metachar() {
        use crate::env::SlateEnv;
        use tempfile::TempDir;
        let td = TempDir::new().expect("tempdir");
        let env = SlateEnv::with_home(td.path().to_path_buf());
        write_state_file(&env, "has\\back").expect("write escapes backslash");
        let got = std::fs::read_to_string(state_file_path(&env)).expect("read");
        assert_eq!(got, "return \"has\\\\back\"\n");
    }

    #[test]
    fn write_state_file_loop_yields_final_variant_content() {
        // Atomicity is a structural property of AtomicWriteFile::commit
        // (fsync+rename). We can't observe mid-write partial state in
        // pure Rust, so the practical proof is: N writes in a tight
        // loop produce a file whose final content matches the last
        // write exactly (no appends, no partial writes surviving).
        use crate::env::SlateEnv;
        use tempfile::TempDir;
        let td = TempDir::new().expect("tempdir");
        let env = SlateEnv::with_home(td.path().to_path_buf());
        for i in 0..25 {
            write_state_file(&env, &format!("variant-{i:02}")).expect("write");
        }
        let got = std::fs::read_to_string(state_file_path(&env)).expect("read");
        assert_eq!(got, "return \"variant-24\"\n");
    }

    // ── render_loader — Plan 17-03 Task 3 ──────────────────────────────

    #[test]
    fn render_loader_includes_uv_compat_shim() {
        // Pitfall 1 (17-RESEARCH §Pitfall 1): nvim 0.8–0.9 ship only
        // `vim.loop`; `vim.uv` alias arrives in 0.10. The compat shim
        // keeps the watcher working across supported versions.
        let out = render_loader();
        assert!(
            out.contains("local uv = vim.uv or vim.loop"),
            "Pitfall 1: missing uv compat shim"
        );
    }

    #[test]
    fn render_loader_includes_100ms_debounce() {
        // Pitfall 2: macOS APFS fires 2–3 fs_events on an atomic rename;
        // the 100 ms debounce collapses them so M.load runs once.
        let out = render_loader();
        assert!(
            out.contains("start(100, 0,"),
            "Pitfall 2: missing 100ms debounce timer start"
        );
    }

    #[test]
    fn render_loader_registers_vim_leave_pre_cleanup() {
        // Prevents orphan libuv handles leaking past nvim exit.
        let out = render_loader();
        assert!(out.contains("VimLeavePre"), "missing VimLeavePre autocmd");
        assert!(
            out.contains("watcher:close"),
            "missing watcher close inside cleanup"
        );
    }

    #[test]
    fn render_loader_guards_lualine_package_load() {
        // Pitfall 5: only refresh lualine when it's already loaded — we
        // must never force-require it.
        let out = render_loader();
        let single_quoted = out.contains("package.loaded['lualine']");
        let double_quoted = out.contains("package.loaded[\"lualine\"]");
        assert!(
            single_quoted || double_quoted,
            "Pitfall 5: lualine must be package.loaded-guarded"
        );
    }

    #[test]
    fn render_loader_fires_colorscheme_autocmd() {
        let out = render_loader();
        assert!(
            out.contains("doautocmd ColorScheme"),
            "missing doautocmd ColorScheme fire"
        );
    }

    #[test]
    fn render_loader_includes_palettes_for_all_builtin_variants() {
        let out = render_loader();
        let registry = ThemeRegistry::new().expect("registry init");
        for v in registry.all() {
            let key = format!("['{}']", v.id);
            assert!(
                out.contains(&key),
                "missing PALETTES entry for variant id {} (key {:?})",
                v.id,
                key
            );
        }
    }

    #[test]
    fn render_loader_declares_lualine_themes_table() {
        // Plan 03 ships an EMPTY `LUALINE_THEMES = {}`; Plan 04 fills it.
        let out = render_loader();
        assert!(
            out.contains("local LUALINE_THEMES = {"),
            "missing LUALINE_THEMES table declaration"
        );
    }

    #[test]
    fn render_loader_ends_with_return_m() {
        let out = render_loader();
        let tail = &out[out.len().saturating_sub(80)..];
        assert!(
            out.trim_end().ends_with("return M"),
            "loader must end with `return M`, got tail: {:?}",
            tail
        );
    }

    #[test]
    fn render_loader_is_deterministic() {
        let a = render_loader();
        let b = render_loader();
        assert_eq!(a, b, "render_loader must be deterministic");
    }

    #[test]
    fn render_loader_uses_lf_line_endings() {
        let out = render_loader();
        assert!(!out.contains('\r'), "loader must use LF only");
    }

    #[test]
    fn render_loader_size_is_bounded() {
        let out = render_loader();
        assert!(
            out.len() >= 2_500,
            "loader too small: {} bytes (expected >= 2500)",
            out.len()
        );
        // 18 variants × ~5-15 KB each + ~3 KB skeleton. Cap at 512 KB.
        assert!(
            out.len() <= 512 * 1024,
            "loader too large: {} bytes (expected <= 512KB)",
            out.len()
        );
    }

    #[test]
    fn render_loader_calls_nvim_set_hl() {
        // D-05: M.load applies groups via the Lua API, never via
        // `:highlight` command strings.
        let out = render_loader();
        assert!(
            out.contains("vim.api.nvim_set_hl"),
            "M.load must call nvim_set_hl per D-05"
        );
    }
}
