//! Phase 17 integration harness. Real assertions land in Plan 07
//! (`tests/nvim_integration.rs` fill-in). Stubs here keep the
//! test-discovery surface stable.
//!
//! All stubs `#[ignore]` by default to keep `cargo test` green before
//! implementation lands. CI runs with `--features has-nvim`.

#[test]
fn integration_harness_compiles() {
    // Sanity check: this file is linked and discoverable. The empty body
    // is intentional — clippy's assertions-on-constants rule rejects
    // `assert!(true)`, so the test passes by virtue of compiling and
    // running without panic.
}

#[test]
#[ignore = "Plan 07 — headless source of every slate-<variant>.lua"]
#[cfg(feature = "has-nvim")]
fn nvim_headless_source_all_variants() {}

#[test]
#[ignore = "Plan 07 — atomic state-file write fires exactly one fs_event (notify)"]
fn state_file_atomic_write_single_event() {}

#[test]
#[ignore = "Plan 07 — file-watcher debounces multi-fire on macOS APFS"]
#[cfg(feature = "has-nvim")]
fn watcher_debounces_multi_fire() {}

#[test]
#[ignore = "Plan 07 — lualine refresh autocmd fires on colorscheme swap"]
#[cfg(feature = "has-nvim")]
fn lualine_refresh_fires() {}

#[test]
#[ignore = "Plan 07 — init.lua with slate marker block (Lua comment) is valid Lua (Pitfall 4 regression)"]
#[cfg(feature = "has-nvim")]
fn marker_block_lua_comment_regression() {}

#[test]
#[ignore = "Plan 07 — loader Lua parses (luafile gate) — covers all 18 spliced PALETTES sub-tables"]
#[cfg(feature = "has-nvim")]
fn loader_lua_parses_via_luafile() {}
