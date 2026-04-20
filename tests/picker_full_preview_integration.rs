//! Phase 19 · Plan 19-08 · Wave-6 integration gate — picker full-preview
//! Tab / Enter / Esc end-to-end paths with a tempdir-backed `SlateEnv`.
//!
//! These tests are the integration-scope companions to the unit tests in
//! `src/cli/picker/state.rs` and the preview-fork tests in
//! `src/cli/picker/preview/starship_fork.rs`. They drive the real
//! `silent_preview_apply` adapter pipeline (now env-injected per Plan 19-09)
//! so every managed/* write lands under the tempdir — zero developer-home
//! pollution, zero `std::env::set_var` global mutation.
//!
//! Rules enforced here per user MEMORY `feedback_no_tech_debt` + CONTEXT
//! §Anti-patterns:
//! - NO `std::env::set_var`, NO `PathGuard`, NO `PATH_LOCK`
//! - NO subprocess `Command::new` against `slate` — call library functions
//!   directly with an injected `SlateEnv`
//! - HARD assertions on managed/* state — no silent-return escape hatches

use slate_cli::cli::picker::PickerState;
use slate_cli::cli::set::silent_preview_apply;
use slate_cli::env::SlateEnv;
use slate_cli::opacity::OpacityPreset;
use std::fs;
use tempfile::TempDir;

/// Build a fresh tempdir-backed `SlateEnv` and return both so the TempDir
/// stays alive for the test duration (Drop cleans up on normal exit).
fn fresh_env() -> (TempDir, SlateEnv) {
    let tmp = TempDir::new().expect("tempdir");
    let env = SlateEnv::with_home(tmp.path().to_path_buf());
    (tmp, env)
}

/// Resolve the managed Ghostty theme config path that
/// `silent_preview_apply` writes to when the Ghostty adapter reacts to a
/// preview call.
///
/// Centralised so if Phase 20 changes the path layout, only this helper
/// updates. Mirrors the adapter's own
/// `config_manager.write_managed_file("ghostty", "theme.conf", ...)`
/// landing at `~/.config/slate/managed/ghostty/theme.conf`.
fn managed_ghostty_theme_path(env: &SlateEnv) -> std::path::PathBuf {
    env.managed_subdir("managed")
        .join("ghostty")
        .join("theme.conf")
}

/// The Ghostty adapter's `apply_theme_with_env` returns `Skipped` when the
/// user has no integration config file (`~/.config/ghostty/config`) on
/// disk — that file is the opt-in signal that the user wants Slate to
/// manage their Ghostty. Our tempdir `SlateEnv` starts empty, so we must
/// materialise an integration config stub before the adapter will write
/// its managed/* palette.
///
/// We write a minimal (but non-empty) config so the adapter picks it up.
/// This is test-setup scaffolding, not a behavioural contract — the real
/// integration test is that `silent_preview_apply` then populates
/// `managed/ghostty/theme.conf`.
fn seed_ghostty_integration(env: &SlateEnv) {
    let integration = env.xdg_config_home().join("ghostty").join("config");
    fs::create_dir_all(integration.parent().unwrap()).unwrap();
    if !integration.exists() {
        fs::write(&integration, "# slate-integration-test placeholder\n").unwrap();
    }
}

/// VALIDATION row 3 — D-10 layer 1 (two-layer ephemeral): navigating in
/// the picker MUST NOT persist anything to `~/.config/slate/current`.
/// Commit happens only on Enter; Up/Down/arrow keys are navigation-only.
///
/// We simulate navigation by calling `PickerState::move_down`/`move_up`
/// without ever invoking `silent_commit_apply`, and verify the pre-seeded
/// `current` file is untouched (content + mtime both stable).
#[test]
fn picker_nav_does_not_persist_current_file() {
    let (_tmp, env) = fresh_env();

    // Pre-seed current file with "catppuccin-mocha".
    let current_path = env.managed_file("current");
    fs::create_dir_all(current_path.parent().unwrap()).unwrap();
    fs::write(&current_path, "catppuccin-mocha\n").unwrap();
    let before_mtime = fs::metadata(&current_path).unwrap().modified().unwrap();
    let before_content = fs::read_to_string(&current_path).unwrap();

    // Build state + navigate (simulates picker Up/Down keystrokes).
    let mut state = PickerState::new("catppuccin-mocha", OpacityPreset::Solid)
        .expect("picker state must build");
    state.move_down();
    state.move_down();
    state.move_up();
    // Key move: we do NOT call silent_commit_apply anywhere — navigation
    // is ephemeral (D-10 layer 1).

    // Current file content unchanged.
    let after_content = fs::read_to_string(&current_path).unwrap();
    assert_eq!(
        after_content.trim(),
        "catppuccin-mocha",
        "D-10 layer 1: current file content must not change during navigation"
    );
    assert_eq!(
        before_content, after_content,
        "D-10 layer 1: current file byte-for-byte unchanged"
    );

    // mtime unchanged (stricter: no rewrite even with identical content).
    let after_mtime = fs::metadata(&current_path).unwrap().modified().unwrap();
    assert_eq!(
        before_mtime, after_mtime,
        "D-10 layer 1: current file mtime must not change during navigation"
    );
}

/// VALIDATION row 5 — D-11 layer 1 (triple-guard rollback, active Esc):
/// when the user navigates mid-picker (managed/ghostty-config drifts),
/// then presses Esc (the picker event loop calls
/// `silent_preview_apply(env, original_theme, original_opacity)`), the
/// managed file MUST match the pre-drift baseline byte-for-byte.
///
/// V-08 fix: the previous `if !exists { return; }` silent-skip escape
/// hatch is replaced with a hard `assert!` — if the tempdir SlateEnv
/// wiring is broken (e.g. adapter not env-aware), the test MUST fail
/// loudly rather than pretend-pass.
#[test]
fn picker_esc_rolls_back_managed_ghostty() {
    let (_tmp, env) = fresh_env();

    // Ghostty adapter only writes managed/* when it sees the user has
    // opted in via an integration config. Seed the tempdir accordingly —
    // without this, apply_theme_with_env returns Skipped and nothing
    // lands under managed/. This is setup, not a contract.
    seed_ghostty_integration(&env);

    // Initial state: catppuccin-mocha at Solid — mimics picker launch
    // snapshot from `PickerState::new(current_theme, current_opacity)`.
    silent_preview_apply(&env, "catppuccin-mocha", OpacityPreset::Solid)
        .expect("initial silent_preview_apply must succeed in tempdir env");

    let managed_ghostty = managed_ghostty_theme_path(&env);
    // V-08 fix: HARD assertion — if silent_preview_apply did not produce
    // the expected managed file in the tempdir, the env-injection wiring
    // is broken (Plan 19-09 regression) and we must surface it here, not
    // silently pass.
    assert!(
        managed_ghostty.exists(),
        "test setup invariant: silent_preview_apply must have created \
         managed/ghostty/theme.conf at {managed_ghostty:?}; \
         tempdir SlateEnv wiring is broken"
    );
    let baseline = fs::read_to_string(&managed_ghostty).unwrap();
    assert!(
        !baseline.is_empty(),
        "baseline Ghostty config must be non-empty"
    );

    // Simulate mid-navigation drift to catppuccin-frappe + Frosted —
    // picker Down arrow + Right arrow in the real event loop.
    silent_preview_apply(&env, "catppuccin-frappe", OpacityPreset::Frosted)
        .expect("drift silent_preview_apply must succeed");
    let drifted = fs::read_to_string(&managed_ghostty).unwrap();
    assert_ne!(
        baseline, drifted,
        "silent_preview_apply must have mutated managed/ghostty-config mid-navigation"
    );

    // Simulate Esc rollback — the picker event loop Cancel branch
    // replays the snapshot by calling silent_preview_apply with the
    // original theme + opacity.
    silent_preview_apply(&env, "catppuccin-mocha", OpacityPreset::Solid)
        .expect("rollback silent_preview_apply must succeed");
    let rolled_back = fs::read_to_string(&managed_ghostty).unwrap();
    assert_eq!(
        baseline, rolled_back,
        "D-11 layer 1 (active Esc rollback): managed/ghostty-config must \
         match baseline after rollback"
    );
}

/// VALIDATION row 1 (Tab mode) — Tab toggles `preview_mode_full` in-memory
/// only. The `current` file, which encodes the committed theme, must not
/// change when the user enters/exits full preview.
///
/// Complements the Esc-rollback test: together they lock the D-10
/// two-layer ephemeral invariant from two angles (navigation AND Tab).
#[test]
fn picker_tab_enters_full_mode_without_persisting() {
    let (_tmp, env) = fresh_env();

    let current_path = env.managed_file("current");
    fs::create_dir_all(current_path.parent().unwrap()).unwrap();
    fs::write(&current_path, "catppuccin-mocha\n").unwrap();
    let before = fs::read_to_string(&current_path).unwrap();

    let mut state = PickerState::new("catppuccin-mocha", OpacityPreset::Solid)
        .expect("picker state must build");

    // Toggle Tab on, navigate, toggle off.
    state.preview_mode_full = true;
    state.move_down();
    state.preview_mode_full = false;

    let after = fs::read_to_string(&current_path).unwrap();
    assert_eq!(
        before, after,
        "Tab must not persist anything to ~/.config/slate/current"
    );
}
