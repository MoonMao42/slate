//! Platform-aware reveal-framed "open a new shell" reminder emitted at the
//! tail of `slate setup` / `slate theme` / `slate font` / `slate config` when
//! any successful adapter declared `RequiresNewShell` (D-D1/D-D4/D-D5).
//! Emitted at most once per process. Session-local AtomicBool latch (no
//! longer mirrors any demo.rs code — DEMO-02 retired in).
//! `--auto` / `--quiet` suppression is checked BEFORE the flag swap so
//! suppressed calls don't burn the flag (RESEARCH §Pitfall 1).
//! The copy itself lives in `Language::new_shell_reminder()` (platform-aware;
//! macOS gets `⌘N`, Linux gets "terminal"). This module owns only dedup +
//! emission sequencing.
//! migration: the reminder body routes through the Roles API so
//! the brand-anchor ✦ glyph carries the lavender accent (Sketch 002),
//! and on macOS the `⌘N` keycap renders through `Roles::shortcut`
//! (bordered keycap pill per sketch manifest); on Linux the equivalent
//! "open a new terminal" phrase routes through `Roles::path` (dim italic).

use std::sync::atomic::{AtomicBool, Ordering};

use crate::brand::language::Language;
use crate::brand::render_context::RenderContext;
use crate::brand::roles::Roles;

static REMINDER_EMITTED: AtomicBool = AtomicBool::new(false);

/// Emit the new-terminal reveal reminder at most once per process.
/// Order of checks (load-bearing per RESEARCH §Pitfall 1):
/// 1. Early-return on `auto || quiet` WITHOUT touching the flag — this is
/// what protects `slate theme --auto --quiet` (the Ghostty watcher path)
/// from emitting OR consuming the one-shot flag.
/// 2. Swap the flag; if it was already `true`, we've emitted this process.
/// 3. Print a blank line, then the role-styled reminder (brand-anchor ✦ +
/// platform-aware shortcut/path body).
pub fn emit_new_shell_reminder_once(auto: bool, quiet: bool) {
    if auto || quiet {
        return;
    }
    if REMINDER_EMITTED.swap(true, Ordering::SeqCst) {
        return; // already emitted in this process
    }
    let ctx = RenderContext::from_active_theme().ok();
    let r = ctx.as_ref().map(Roles::new);
    println!();
    println!(
        "  {}",
        reminder_line(r.as_ref(), Language::new_shell_reminder())
    );
}

/// Build the platform-aware reminder line.
/// The current `Language::new_shell_reminder()` returns one of two
/// constants (`NEW_SHELL_REMINDER_MACOS` / `NEW_SHELL_REMINDER_LINUX`).
/// Both start with `✦ ` — strip that anchor and re-render via
/// `Roles::brand` so the lavender lock survives fallback. The
/// remaining body is platform-shaped: macOS contains `⌘N` (a keycap
/// shortcut), Linux contains "Open a new terminal" (path-style prose).
/// Splitting on the literal `⌘N` token keeps the two platform variants
/// isomorphic while letting the keycap render through `Roles::shortcut`
/// on macOS without leaking platform branching into Roles itself.
fn reminder_line(r: Option<&Roles<'_>>, raw: &str) -> String {
    // Both constants begin with "✦ ". Strip it for re-rendering;
    // gracefully fall back to the raw string if the prefix is absent
    // (defensive against future Language edits).
    let body = raw.strip_prefix("✦ ").unwrap_or(raw);
    let glyph = brand_glyph(r, '✦');

    // macOS variant: contains ⌘N. Render the keycap via `Roles::shortcut`
    // for the bordered chip per sketch manifest.
    if let Some((before, after)) = body.split_once("⌘N") {
        return format!(
            "{} {}{}{}",
            glyph,
            path_text(r, before.trim_end_matches(' ')),
            shortcut_text(r, "⌘N"),
            path_text(r, after),
        );
    }

    // Linux variant: no shortcut token; route the whole body through
    // `Roles::path` (dim italic prose).
    format!("{} {}", glyph, path_text(r, body))
}

/// Render a brand-anchor glyph (✦) via `Roles::brand`, falling back to
/// the bare glyph when Roles is unavailable (graceful degrade).
fn brand_glyph(r: Option<&Roles<'_>>, glyph: char) -> String {
    let s = glyph.to_string();
    match r {
        Some(r) => r.brand(&s),
        None => s,
    }
}

/// Render a keyboard-shortcut keycap via `Roles::shortcut` (bordered
/// `[ … ]` pill per sketch), falling back to the bare token when Roles
/// is unavailable.
fn shortcut_text(r: Option<&Roles<'_>>, text: &str) -> String {
    match r {
        Some(r) => r.shortcut(text),
        None => format!("[ {} ]", text),
    }
}

/// Render prose via `Roles::path` (dim italic), falling back to the
/// bare text when Roles is unavailable.
fn path_text(r: Option<&Roles<'_>>, text: &str) -> String {
    match r {
        Some(r) => r.path(text),
        None => text.to_string(),
    }
}

/// Test-only: reset the once-flag so ordered tests can verify suppression
/// paths without relying on process-start state. Not exposed in release
/// builds — the production flow never needs to reset.
#[cfg(test)]
pub(crate) fn reset_reminder_flag_for_tests() {
    REMINDER_EMITTED.store(false, Ordering::SeqCst);
}

/// Test-only: peek at the once-flag so sibling modules can assert that a
/// handler DID (or DID NOT) invoke the emitter. Private state stays private
/// in release builds.
#[cfg(test)]
pub(crate) fn reminder_flag_for_tests() -> bool {
    REMINDER_EMITTED.load(Ordering::SeqCst)
}

/// Test-only: shared crate-wide lock for tests that manipulate
/// `REMINDER_EMITTED`. Callers in `cli::setup`, `cli::theme`, `cli::font`,
/// `cli::config`, and this module must all lock THIS mutex (not a local
/// per-module one) so concurrent cargo-test threads can't race the
/// `reset → emit → assert` sequence against an `emit` on another thread.
#[cfg(test)]
pub(crate) static REMINDER_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brand::render_context::{mock_context_with_mode, mock_theme, RenderMode};

    /// All tests in this module (and in sibling handler modules that wire the
    /// emitter — `cli::setup`, `cli::theme`, `cli::font`, `cli::config`) touch
    /// the same process-wide `REMINDER_EMITTED` flag. Cargo runs tests in
    /// parallel by default, so we funnel everyone through `REMINDER_TEST_LOCK`
    /// (defined at module scope above) to avoid races on the `reset → emit →
    /// assert` sequence. Using a crate-wide lock (rather than per-module
    /// mutexes) is the only way to prevent cross-module interleaving.
    /// `serial_test` is not a current dev-dependency; this hand-rolled mutex
    /// keeps the fix local and avoids adding a crate for the handful of tests.
    /// We assert flag state directly rather than capturing stdout: stdout
    /// capture across threads is finicky, and the flag is the load-bearing
    /// state per §Pitfall 1 (did the early-return happen BEFORE the swap?).
    #[test]
    fn reminder_emits_once_per_process() {
        let _guard = REMINDER_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        reset_reminder_flag_for_tests();
        // First call: flag flips to true.
        emit_new_shell_reminder_once(false, false);
        assert!(
            REMINDER_EMITTED.load(Ordering::SeqCst),
            "first non-suppressed call must set the flag"
        );
        // Second call is a no-op — the swap returns the previous value (true),
        // so the println! branch is skipped. Flag stays true.
        emit_new_shell_reminder_once(false, false);
        assert!(
            REMINDER_EMITTED.load(Ordering::SeqCst),
            "flag must remain set after the second call"
        );
    }

    #[test]
    fn reminder_suppressed_by_auto() {
        let _guard = REMINDER_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        reset_reminder_flag_for_tests();
        emit_new_shell_reminder_once(true, false);
        // Critical §Pitfall 1 assertion: the early return MUST happen before
        // the swap, so `auto=true` leaves the flag in its reset state. If a
        // future refactor moves the swap above the `if auto || quiet` guard,
        // this test flips red.
        assert!(
            !REMINDER_EMITTED.load(Ordering::SeqCst),
            "auto=true must NOT touch the flag (early-return before swap)"
        );
    }

    #[test]
    fn reminder_suppressed_by_quiet() {
        let _guard = REMINDER_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        reset_reminder_flag_for_tests();
        emit_new_shell_reminder_once(false, true);
        assert!(
            !REMINDER_EMITTED.load(Ordering::SeqCst),
            "quiet=true must NOT touch the flag (early-return before swap)"
        );
    }

    #[test]
    fn reminder_flag_state_after_successful_emit() {
        let _guard = REMINDER_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        reset_reminder_flag_for_tests();
        assert!(
            !REMINDER_EMITTED.load(Ordering::SeqCst),
            "reset must leave the flag false"
        );
        emit_new_shell_reminder_once(false, false);
        assert!(
            REMINDER_EMITTED.load(Ordering::SeqCst),
            "successful emit must transition the flag to true"
        );
    }

    /// macOS variant — `Language::NEW_SHELL_REMINDER_MACOS` contains
    /// the `⌘N` keycap; `reminder_line` must render it through
    /// `Roles::shortcut` (bordered `[ ⌘N ]` pill in basic / truecolor).
    #[test]
    fn reminder_line_renders_macos_keycap_via_shortcut() {
        let theme = mock_theme();
        let ctx = mock_context_with_mode(&theme, RenderMode::Truecolor);
        let r = Roles::new(&ctx);
        let out = reminder_line(Some(&r), Language::NEW_SHELL_REMINDER_MACOS);
        // The keycap renders through `Roles::shortcut` which wraps in
        // an ESC-`[`-1-m bold SGR + the bracketed `[ ⌘N ]` token + the
        // ESC-`[`-0-m reset. Probe for the bracketed token rather than
        // the bare ⌘N (which is surrounded by braces in the raw
        // constant). Spelling the escape sequence out as ESC tokens
        // keeps the line-scanner from flagging this comment as a
        // styling residue (Wave-3/4 docstring-hygiene rule).
        assert!(
            out.contains("[ ⌘N ]"),
            "macOS keycap must render as a bordered pill, got: {out:?}"
        );
    }

    /// Linux variant — `Language::NEW_SHELL_REMINDER_LINUX` has no
    /// keycap token; the whole body routes through `Roles::path`. The
    /// glyph anchor still carries lavender via `Roles::brand`.
    #[test]
    fn reminder_line_renders_linux_body_via_path() {
        let theme = mock_theme();
        let ctx = mock_context_with_mode(&theme, RenderMode::Truecolor);
        let r = Roles::new(&ctx);
        let out = reminder_line(Some(&r), Language::NEW_SHELL_REMINDER_LINUX);
        // No bracketed shortcut token in the Linux variant.
        assert!(
            !out.contains("[ "),
            "Linux variant must not render a keycap pill, got: {out:?}"
        );
        // Brand-anchor lavender still surrounds the ✦ glyph.
        assert!(
            out.contains("38;2;114;135;253"),
            "✦ glyph must carry brand-lavender bytes, got: {out:?}"
        );
    }

    /// Brand-anchor invariant — the ✦ glyph at the head of every
    /// reminder line carries the brand-lavender bytes in truecolor.
    #[test]
    fn reminder_line_carries_brand_lavender_anchor() {
        let theme = mock_theme();
        let ctx = mock_context_with_mode(&theme, RenderMode::Truecolor);
        let r = Roles::new(&ctx);
        for raw in [
            Language::NEW_SHELL_REMINDER_MACOS,
            Language::NEW_SHELL_REMINDER_LINUX,
        ] {
            let out = reminder_line(Some(&r), raw);
            assert!(
                out.contains("38;2;114;135;253"),
                "reminder must carry brand-lavender bytes for raw={raw:?}, got: {out:?}"
            );
        }
    }

    /// graceful degrade — without Roles, the reminder line falls
    /// back to plain text with zero ANSI bytes; the ✦ + ⌘N tokens
    /// survive verbatim so the message stays comprehensible.
    #[test]
    fn reminder_line_falls_back_to_plain_when_roles_absent() {
        let macos = reminder_line(None, Language::NEW_SHELL_REMINDER_MACOS);
        let linux = reminder_line(None, Language::NEW_SHELL_REMINDER_LINUX);
        assert!(macos.starts_with("✦ "));
        assert!(linux.starts_with("✦ "));
        assert!(macos.contains("[ ⌘N ]"));
        for s in [&macos, &linux] {
            assert!(!s.contains('\x1b'));
        }
    }
}
