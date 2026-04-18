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
use crate::adapter::{ApplyOutcome, ApplyStrategy, ToolAdapter};
use crate::cli::picker::preview_panel::SemanticColor;
use crate::design::nvim_highlights::{lualine_theme, HighlightSpec, Style, HIGHLIGHT_GROUPS};
use crate::env::SlateEnv;
use crate::error::Result;
use crate::theme::{Palette, ThemeRegistry, ThemeVariant};
use atomic_write_file::AtomicWriteFile;
use std::fmt::Write as FmtWrite;
use std::io::Write as IoWrite;
use std::path::{Path, PathBuf};

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

// ── Plan 17-03 Task 3: Lua loader template ─────────────────────────────
//
// The three `LOADER_TEMPLATE_*` constants below, sandwiched around the
// spliced per-variant PALETTES entries produced by `render_colorscheme`
// and an (empty in this plan) `LUALINE_THEMES` block, form the complete
// `~/.config/nvim/lua/slate/init.lua` module Plan 05 ships through
// `NvimAdapter::apply_setup`.
//
// Six load-bearing details from 17-RESEARCH.md §Pitfalls are inlined
// inside these strings — every unit test in this file's `mod tests`
// block guards one of them:
//
//   1. `local uv = vim.uv or vim.loop` (Pitfall 1 — nvim 0.8/0.9 compat)
//   2. 100 ms debounce via `uv.new_timer()` (Pitfall 2 — APFS multi-fire)
//   3. Watcher re-arm inside callback (Pitfall 6 — driver-specific close)
//   4. `VimLeavePre` cleanup autocmd (no orphan libuv handles)
//   5. `package.loaded['lualine']` guard (Pitfall 5 — never force-require)
//   6. `doautocmd ColorScheme slate-<variant>` (downstream plugin hook)
//
// The strings are intentionally verbatim copies of 17-RESEARCH §Pattern 2
// lines 329-446. Do not paraphrase: Plan 07's integration tests parse
// these bytes directly via `nvim --headless -c 'luafile %'`, so any
// syntactic drift breaks the syntax gate.

/// Head of the loader: module prelude, uv shim, open PALETTES table.
const LOADER_TEMPLATE_HEAD: &str = "\
-- slate-managed: do not edit. Regenerate via `slate setup`.
local M = {}
local uv = vim.uv or vim.loop  -- nvim 0.8 compat (Pitfall 1)

-- Per-variant highlight tables. Populated by slate setup from HIGHLIGHT_GROUPS.
local PALETTES = {
";

/// Separator between PALETTES and LUALINE_THEMES tables.
const LOADER_TEMPLATE_MID: &str = "\
}

-- Per-variant lualine theme tables. Plan 04 populates; empty stub now.
local LUALINE_THEMES = {
";

/// Tail of the loader: close LUALINE_THEMES, define M.load / M.setup,
/// wire the fs_event watcher with 100 ms debounce, register VimLeavePre
/// cleanup, bootstrap via `M.setup()`, and return M.
const LOADER_TEMPLATE_TAIL: &str = r#"}

function M.load(variant)
  local pal = PALETTES[variant]
  if not pal then return end
  vim.cmd('hi clear')
  if vim.fn.exists('syntax_on') == 1 then vim.cmd('syntax reset') end
  vim.g.colors_name = 'slate-' .. variant

  for name, spec in pairs(pal) do
    vim.api.nvim_set_hl(0, name, spec)
  end

  -- Lualine refresh guard (Pitfall 5)
  if package.loaded['lualine'] and LUALINE_THEMES[variant] then
    local ok, lualine = pcall(require, 'lualine')
    if ok then
      local cfg = lualine.get_config()
      cfg.options.theme = LUALINE_THEMES[variant]
      lualine.setup(cfg)
      lualine.refresh({ force = true })
    end
  end

  vim.cmd('doautocmd ColorScheme ' .. vim.g.colors_name)
end

local STATE_PATH = vim.fn.expand('~/.cache/slate/current_theme.lua')

local function read_state()
  local ok, mod = pcall(dofile, STATE_PATH)
  if ok and type(mod) == 'string' then return mod end
  return nil
end

local watcher
local debounce_timer

local function schedule_reload()
  if debounce_timer then debounce_timer:stop() end
  debounce_timer = uv.new_timer()
  debounce_timer:start(100, 0, vim.schedule_wrap(function()  -- Pitfall 2: 100ms debounce
    local variant = read_state()
    if variant then M.load(variant) end
    debounce_timer:close()
    debounce_timer = nil
  end))
end

function M.setup(opts)
  opts = opts or {}
  local variant = read_state()
  if variant then M.load(variant) end

  watcher = uv.new_fs_event()
  watcher:start(STATE_PATH, {}, vim.schedule_wrap(function(err, _fname, _events)
    if err then return end
    schedule_reload()
    -- Pitfall 6: re-arm watcher (some FS drivers close on first fire)
    watcher:stop()
    local ok = pcall(function()
      watcher:start(STATE_PATH, {}, vim.schedule_wrap(schedule_reload))
    end)
    if not ok then watcher = nil end
  end))

  -- VimLeavePre cleanup -- prevents orphan libuv handles
  vim.api.nvim_create_autocmd('VimLeavePre', {
    callback = function()
      if debounce_timer then pcall(function() debounce_timer:close() end) end
      if watcher then pcall(function() watcher:close() end) end
    end,
  })
end

M.setup()

return M
"#;

/// Render the complete `~/.config/nvim/lua/slate/init.lua` loader module.
///
/// Structure:
///   1. LOADER_TEMPLATE_HEAD — module prelude + `local PALETTES = {`
///   2. one `['<id>'] = <sub-table>,\n` line per built-in variant, the
///      sub-table produced by [`render_colorscheme`] with its leading
///      `-- slate-managed palette for …` comment stripped (the spliced
///      form must be a bare `{ ... }` expression, not a prefixed one).
///   3. LOADER_TEMPLATE_MID — close PALETTES + open LUALINE_THEMES.
///   4. (empty in Plan 03 — Plan 04 splices per-variant lualine themes)
///   5. LOADER_TEMPLATE_TAIL — close LUALINE_THEMES + M.load / M.setup
///      / watcher / debounce / cleanup / `return M`.
///
/// Deterministic: iteration order is `ThemeRegistry::all()` order, which
/// is the TOML declaration order (stable). Two calls yield byte-identical
/// strings.
pub fn render_loader() -> String {
    let registry =
        ThemeRegistry::new().expect("ThemeRegistry must initialise — validated at phase-load time");

    let mut out = String::with_capacity(32 * 1024);
    out.push_str(LOADER_TEMPLATE_HEAD);

    for variant in registry.all() {
        // `render_colorscheme` produces:
        //   -- slate-managed palette for <id>\n{ ... }
        // We need the bare table `{ ... }` keyed by the variant id inside
        // the PALETTES block; strip the leading comment line by slicing
        // after the first newline.
        let sub = render_colorscheme(&variant.palette, &variant.id);
        let body = sub.split_once('\n').map(|x| x.1).unwrap_or(&sub);

        out.push_str("  ['");
        out.push_str(&variant.id);
        out.push_str("'] = ");
        out.push_str(body);
        out.push_str(",\n");
    }

    out.push_str(LOADER_TEMPLATE_MID);
    // Plan 04: splice one lualine theme table per variant into the
    // LUALINE_THEMES block. `lualine_theme` returns a Lua table literal
    // starting with `{` and ending with `}` (no trailing newline), so we
    // wrap each entry as `  ['<id>'] = <table>,\n` exactly like the
    // PALETTES splice above.
    for variant in registry.all() {
        out.push_str("  ['");
        out.push_str(&variant.id);
        out.push_str("'] = ");
        out.push_str(&lualine_theme(&variant.palette));
        out.push_str(",\n");
    }
    out.push_str(LOADER_TEMPLATE_TAIL);
    out
}

// ── Plan 17-05 Task 2: NvimAdapter — ToolAdapter impl + setup ──────────

/// Neovim colorscheme adapter.
///
/// Two entry points:
///
/// - [`NvimAdapter::setup`] (slow path) — writes the full install (one
///   `slate-<id>.lua` shim per built-in variant + the loader module + the
///   initial state file). Called from the `slate setup` wizard; idempotent.
///
/// - [`ToolAdapter::apply_theme`] (fast path) — writes only the state file
///   at `~/.cache/slate/current_theme.lua`. The loader's `vim.uv.fs_event`
///   watcher (rendered by [`render_loader`]) picks up the change and
///   hot-reloads the colorscheme in every running nvim instance.
///
/// `apply_theme` (trait impl) obtains a [`SlateEnv`] via
/// `SlateEnv::from_process()` and delegates to
/// [`NvimAdapter::apply_theme_with_env`]. The helper exists so unit tests
/// can inject a tempdir-backed `SlateEnv::with_home(...)` without mutating
/// process env vars anywhere in the test suite.
pub struct NvimAdapter;

impl NvimAdapter {
    /// Full install: writes 18 `slate-<id>.lua` shims + the loader
    /// (`lua/slate/init.lua`) + the initial state file.
    ///
    /// Called from the `slate setup` wizard (Plan 06). Idempotent —
    /// re-running with the same env+theme produces byte-identical files
    /// via `AtomicWriteFile`.
    pub fn setup(env: &SlateEnv, initial_theme: &ThemeVariant) -> Result<()> {
        let nvim_home = env.home().join(".config/nvim");
        let colors_dir = nvim_home.join("colors");
        let lua_slate_dir = nvim_home.join("lua").join("slate");

        std::fs::create_dir_all(&colors_dir)?;
        std::fs::create_dir_all(&lua_slate_dir)?;

        // 1. Write one `slate-<id>.lua` shim per built-in variant.
        //    Iterating the registry keeps future variants automatic.
        let registry = ThemeRegistry::new()?;
        for variant in registry.all() {
            let shim_path = colors_dir.join(format!("slate-{}.lua", variant.id));
            let shim_content = render_shim(&variant.id);
            write_atomic(&shim_path, &shim_content)?;
        }

        // 2. Write the loader file. `render_loader` splices every variant's
        //    palette + lualine theme so the generated Lua is self-contained.
        let loader_path = lua_slate_dir.join("init.lua");
        let loader_content = render_loader();
        write_atomic(&loader_path, &loader_content)?;

        // 3. Seed the state file so a nvim instance that starts after
        //    `slate setup` picks up the initial theme immediately.
        write_state_file(env, &initial_theme.id)?;

        Ok(())
    }

    /// Fast-path apply: writes only the state file.
    ///
    /// Crate-private so unit tests can inject a tempdir-backed `SlateEnv`
    /// without mutating process env vars. The trait's `apply_theme` method
    /// delegates here using `SlateEnv::from_process()`.
    ///
    /// Running nvim instances pick up the state-file change via the
    /// `vim.uv.fs_event` watcher rendered by [`render_loader`]. The 18
    /// shims + loader are in place from [`NvimAdapter::setup`] — the fast
    /// path never re-emits them.
    pub(crate) fn apply_theme_with_env(
        &self,
        theme: &ThemeVariant,
        env: &SlateEnv,
    ) -> Result<ApplyOutcome> {
        write_state_file(env, &theme.id)?;
        Ok(ApplyOutcome::Applied {
            requires_new_shell: false,
        })
    }
}

/// Atomic write helper: fsync + rename semantics guarantee exactly one
/// `fs_event` fire on the Lua watcher (same load-bearing property as
/// [`write_state_file`]).
fn write_atomic(path: &Path, content: &str) -> Result<()> {
    let mut file = AtomicWriteFile::open(path)?;
    file.write_all(content.as_bytes())?;
    file.commit()?;
    Ok(())
}

impl ToolAdapter for NvimAdapter {
    fn tool_name(&self) -> &'static str {
        "nvim"
    }

    fn is_installed(&self) -> Result<bool> {
        // Exclude-only pattern (Phase 7 Decision 11, Phase 17 D-01):
        // - binary missing → Ok(false) (not an error).
        // - version < 0.8.0 → Ok(false) (not an error).
        // - version parse failure → Ok(false) (conservative: we don't
        //   write files for an nvim we can't verify).
        let presence = crate::detection::detect_tool_presence("nvim");
        if !presence.installed {
            return Ok(false);
        }
        let ver = match crate::platform::version_check::detect_version("nvim") {
            Ok(v) => v,
            Err(_) => return Ok(false),
        };
        Ok(crate::platform::version_check::VersionPolicy::check_version("nvim", &ver).is_ok())
    }

    fn integration_config_path(&self) -> Result<PathBuf> {
        let env = SlateEnv::from_process()?;
        Ok(env.home().join(".config/nvim/init.lua"))
    }

    fn managed_config_path(&self) -> PathBuf {
        // Per Phase 17 D-03: slate writes DIRECTLY to ~/.config/nvim/
        // (nvim's runtimepath), NOT ~/.config/slate/managed/nvim/. The
        // three-tier contract still holds — we just place the managed
        // tier where nvim expects it.
        SlateEnv::from_process()
            .map(|env| env.home().join(".config/nvim"))
            .unwrap_or_else(|_| PathBuf::from(".config/nvim"))
    }

    fn apply_strategy(&self) -> ApplyStrategy {
        ApplyStrategy::WriteAndInclude
    }

    fn apply_theme(&self, theme: &ThemeVariant) -> Result<ApplyOutcome> {
        // FAST PATH: state-file-only. The 18 shims + loader are written
        // by `NvimAdapter::setup` during the wizard; running nvim instances
        // hot-reload via the file watcher when the state file changes.
        let env = SlateEnv::from_process()?;
        self.apply_theme_with_env(theme, &env)
    }
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

    // ── Plan 17-04 Task 3: lualine theme splice ────────────────────────

    #[test]
    fn render_loader_populates_lualine_themes_for_all_variants() {
        let out = render_loader();
        let registry = ThemeRegistry::new().expect("registry init");
        // Locate the LUALINE_THEMES block between its declaration and the
        // start of the TAIL (the `function M.load` line). Every variant id
        // must appear as a `['<id>']` key inside that window.
        let lualine_block_start = out
            .find("local LUALINE_THEMES = {")
            .expect("LUALINE_THEMES block must exist");
        let rel_tail = out[lualine_block_start..]
            .find("\nfunction M.load")
            .expect("TAIL must follow LUALINE_THEMES");
        let lualine_block = &out[lualine_block_start..lualine_block_start + rel_tail];
        for v in registry.all() {
            let key = format!("['{}']", v.id);
            assert!(
                lualine_block.contains(&key),
                "LUALINE_THEMES missing entry for variant id {} (key {:?})",
                v.id,
                key
            );
        }
    }

    #[test]
    fn render_loader_lualine_entries_are_bold_capable() {
        let out = render_loader();
        // Each variant contributes 6 `gui = 'bold'` markers (one per mode).
        // With 18 variants that gives 108 bolds; the Plan-03 loader itself
        // contains no bold markers outside the LUALINE_THEMES block.
        let bold_count = out.matches("gui = 'bold'").count();
        assert!(
            bold_count >= 60,
            "expected >= 60 bold markers across spliced lualine themes, got {}",
            bold_count
        );
    }

    #[test]
    fn render_loader_size_adjusted_for_lualine() {
        // Rule 3 deviation: the plan's 256 KB upper bound was drafted on an
        // out-of-date assumption that Plan-03's loader was ~15 KB. Plan 03's
        // own test already asserts `<= 512 KB`, and Plan 03's summary records
        // a baseline of 230 KB. With Plan 04 adding 136 plugin entries (~100 KB
        // spread across 18 variants) plus 18 spliced lualine tables (~36 KB),
        // the realistic total lands around 370-400 KB. We keep Plan 03's
        // 512 KB upper bound for consistency; the lower bound moves to 8 KB
        // per the plan's stated intent of shifting the floor up for lualine.
        let out = render_loader();
        assert!(
            out.len() >= 8_000,
            "loader with lualine data must be >= 8KB, got {} bytes",
            out.len()
        );
        assert!(
            out.len() <= 512 * 1024,
            "loader too large: {} bytes",
            out.len()
        );
    }

    // ── Plan 17-05 Task 2: NvimAdapter ─────────────────────────────────

    #[test]
    fn nvim_adapter_tool_name() {
        assert_eq!(NvimAdapter.tool_name(), "nvim");
    }

    #[test]
    fn nvim_adapter_apply_strategy_is_write_and_include() {
        assert!(matches!(
            NvimAdapter.apply_strategy(),
            ApplyStrategy::WriteAndInclude
        ));
    }

    #[test]
    fn nvim_adapter_apply_theme_with_env_writes_state_file_only() {
        use crate::env::SlateEnv;
        use tempfile::TempDir;
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_path_buf());
        let registry = ThemeRegistry::new().unwrap();
        let theme = registry.get("catppuccin-mocha").unwrap().clone();

        // (a) Fast-path returns Applied { requires_new_shell: false }.
        let outcome = NvimAdapter
            .apply_theme_with_env(&theme, &env)
            .expect("apply_theme_with_env must succeed");
        assert!(
            matches!(
                outcome,
                ApplyOutcome::Applied {
                    requires_new_shell: false
                }
            ),
            "fast path must return Applied with requires_new_shell=false, got {:?}",
            outcome
        );

        // (b) State file exists with the expected variant string.
        let state = td.path().join(".cache/slate/current_theme.lua");
        assert!(state.is_file(), "state file must exist at {:?}", state);
        let content = std::fs::read_to_string(&state).unwrap();
        assert!(
            content.contains("catppuccin-mocha"),
            "state file must contain variant id, got {:?}",
            content
        );

        // (c) NO other slate files touched — the fast path is state-only.
        let colors_dir = td.path().join(".config/nvim/colors");
        assert!(
            !colors_dir.exists(),
            "fast path must NOT create colors/ dir; found {:?}",
            colors_dir
        );
        let lua_slate_dir = td.path().join(".config/nvim/lua/slate");
        assert!(
            !lua_slate_dir.exists(),
            "fast path must NOT create lua/slate/ dir; found {:?}",
            lua_slate_dir
        );
    }

    #[test]
    fn nvim_adapter_setup_writes_full_install() {
        use crate::env::SlateEnv;
        use tempfile::TempDir;
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_path_buf());
        let registry = ThemeRegistry::new().unwrap();
        let theme = registry.get("catppuccin-mocha").unwrap().clone();

        NvimAdapter::setup(&env, &theme).expect("setup ok");

        let colors_dir = td.path().join(".config/nvim/colors");
        assert!(colors_dir.is_dir(), "colors dir must exist");

        // Count slate-<id>.lua files — should equal registry.all().count().
        let shim_count = std::fs::read_dir(&colors_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("slate-"))
            .count();
        assert_eq!(
            shim_count,
            registry.all().len(),
            "one shim per built-in variant"
        );

        let loader = td.path().join(".config/nvim/lua/slate/init.lua");
        assert!(loader.is_file(), "loader must exist");

        let state = td.path().join(".cache/slate/current_theme.lua");
        assert!(state.is_file(), "initial state file must exist");
    }

    #[test]
    fn nvim_adapter_setup_is_idempotent() {
        use crate::env::SlateEnv;
        use tempfile::TempDir;
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_path_buf());
        let registry = ThemeRegistry::new().unwrap();
        let theme = registry.get("catppuccin-mocha").unwrap().clone();

        NvimAdapter::setup(&env, &theme).unwrap();
        let loader1 =
            std::fs::read_to_string(td.path().join(".config/nvim/lua/slate/init.lua")).unwrap();
        let shim1 = std::fs::read_to_string(
            td.path()
                .join(".config/nvim/colors/slate-catppuccin-mocha.lua"),
        )
        .unwrap();

        NvimAdapter::setup(&env, &theme).unwrap();
        let loader2 =
            std::fs::read_to_string(td.path().join(".config/nvim/lua/slate/init.lua")).unwrap();
        let shim2 = std::fs::read_to_string(
            td.path()
                .join(".config/nvim/colors/slate-catppuccin-mocha.lua"),
        )
        .unwrap();

        assert_eq!(loader1, loader2, "loader setup must be idempotent");
        assert_eq!(shim1, shim2, "shim setup must be idempotent");
    }

    #[test]
    fn nvim_adapter_managed_path_points_at_nvim_home() {
        let path = NvimAdapter.managed_config_path();
        let s = path.display().to_string();
        assert!(
            s.ends_with(".config/nvim") || s == ".config/nvim",
            "managed path should be ~/.config/nvim, got {}",
            s
        );
    }
}
