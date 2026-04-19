//! Phase 17 Plan 07 integration gate for the Neovim editor adapter.
//!
//! Six end-to-end assertions land here; two additional contracts
//! (`clean_removes_all_nvim_files`, `config_editor_disable_preserves_colors`)
//! live alongside the code they exercise in `src/cli/clean.rs` and
//! `src/cli/config.rs` because the helpers they drive are crate-private —
//! moving the assertions next to the helpers keeps tests honest without
//! widening public surface (plan Task 5 option (b)).
//!
//! Tests that spawn `nvim --headless` are feature-gated behind `has-nvim`
//! so `cargo test` on a machine without nvim still compiles and runs the
//! non-spawning tests. When `has-nvim` IS set but the binary is not on
//! PATH (CI setup bug, local dev on a minimal box), each spawning test
//! checks `which nvim` at entry and skips cleanly with an `eprintln!`
//! marker — NEVER panics. This matches `feedback_no_tech_debt`: tests
//! never depend on global env-var mutation, and missing prerequisites are
//! a skip, not a red alarm.

#[test]
fn integration_harness_compiles() {
    // Sanity check: this file is linked and discoverable. The empty body
    // is intentional — clippy's assertions-on-constants rule rejects
    // `assert!(true)`, so the test passes by virtue of compiling and
    // running without panic.
}

// ─────────────────────────────────────────────────────────────────────
// Shared helpers (available regardless of has-nvim)
// ─────────────────────────────────────────────────────────────────────

/// Returns true when `nvim` is on PATH. Gating condition for every
/// `has-nvim`-featured test that spawns the binary: when false, the test
/// logs a skip marker and returns Ok early — it never panics because a
/// developer machine / CI without nvim is a valid environment per the
/// plan's "capability hint, not error" posture (D-01).
#[cfg(feature = "has-nvim")]
fn nvim_available() -> bool {
    std::process::Command::new("nvim")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Convenience wrapper: prints a skip marker and returns true when nvim
/// is absent. Callers: `if skip_if_no_nvim("test-name") { return; }`.
#[cfg(feature = "has-nvim")]
fn skip_if_no_nvim(test_name: &str) -> bool {
    if !nvim_available() {
        eprintln!("SKIP {test_name}: nvim not on PATH (has-nvim feature set but binary missing)");
        true
    } else {
        false
    }
}

// ─────────────────────────────────────────────────────────────────────
// Task 1 — nvim_headless_source_all_variants
// ─────────────────────────────────────────────────────────────────────

/// After `NvimAdapter::setup` writes the 18 shims + loader, every
/// variant's shim must source cleanly via `nvim --headless` and set
/// `vim.g.colors_name` to `slate-<id>`. This proves the shim + loader +
/// PALETTES chain end-to-end for all 18 variants. A malformed `{ ... }`
/// sub-table in any variant would surface here with the offending id
/// named in the failure message.
#[test]
#[cfg(feature = "has-nvim")]
fn nvim_headless_source_all_variants() {
    use slate_cli::adapter::NvimAdapter;
    use slate_cli::env::SlateEnv;
    use slate_cli::theme::ThemeRegistry;
    use std::process::Command;
    use tempfile::TempDir;

    if skip_if_no_nvim("nvim_headless_source_all_variants") {
        return;
    }

    let td = TempDir::new().expect("tempdir");
    let env = SlateEnv::with_home(td.path().to_path_buf());
    let registry = ThemeRegistry::new().expect("registry init");

    // Full setup once — populates colors/ + lua/slate/ + initial state file.
    let first = *registry.all().first().expect("registry has ≥1 variant");
    NvimAdapter::setup(&env, first).expect("setup");

    let rtp = td.path().join(".config/nvim");
    let mut failures: Vec<String> = Vec::new();

    for variant in registry.all() {
        let expected = format!("slate-{}", variant.id);
        let out = Command::new("nvim")
            .args([
                "--headless",
                "-u",
                "NONE",
                "--cmd",
                &format!("set runtimepath^={}", rtp.display()),
                "-c",
                &format!("colorscheme slate-{}", variant.id),
                "-c",
                "echo g:colors_name",
                "-c",
                "q",
            ])
            .env("HOME", td.path())
            .output()
            .expect("spawn nvim");

        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            failures.push(format!("{}: nvim exit failed: {}", variant.id, stderr));
            continue;
        }
        // `:echo` output on --headless lands on stderr.
        let stderr = String::from_utf8_lossy(&out.stderr);
        if !stderr.contains(&expected) {
            failures.push(format!(
                "{}: expected '{}' in output, got stderr={:?}",
                variant.id, expected, stderr
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "headless-source failures ({} variant(s)):\n{}",
        failures.len(),
        failures.join("\n---\n")
    );
}

// ─────────────────────────────────────────────────────────────────────
// Task 2 — state_file_atomic_write_single_event
// ─────────────────────────────────────────────────────────────────────
//
// Lands in Task 2. Stub retained here to keep the test-discovery surface
// stable for the commit boundary.
#[test]
#[ignore = "Plan 07 Task 2 — atomic state-file write fires exactly one fs_event (notify)"]
fn state_file_atomic_write_single_event() {}

// ─────────────────────────────────────────────────────────────────────
// Task 3 — watcher_debounces_multi_fire + lualine_refresh_fires
// ─────────────────────────────────────────────────────────────────────
#[test]
#[ignore = "Plan 07 Task 3 — file-watcher debounces multi-fire on macOS APFS"]
#[cfg(feature = "has-nvim")]
fn watcher_debounces_multi_fire() {}

#[test]
#[ignore = "Plan 07 Task 3 — lualine refresh autocmd fires on colorscheme swap"]
#[cfg(feature = "has-nvim")]
fn lualine_refresh_fires() {}

// ─────────────────────────────────────────────────────────────────────
// Task 4 — marker_block_lua_comment_regression + loader_lua_parses_via_luafile
// ─────────────────────────────────────────────────────────────────────
#[test]
#[ignore = "Plan 07 Task 4 — init.lua with slate marker block (Lua comment) is valid Lua (Pitfall 4 regression)"]
#[cfg(feature = "has-nvim")]
fn marker_block_lua_comment_regression() {}

#[test]
#[ignore = "Plan 07 Task 4 — loader Lua parses (luafile gate) — covers all 18 spliced PALETTES sub-tables"]
#[cfg(feature = "has-nvim")]
fn loader_lua_parses_via_luafile() {}
