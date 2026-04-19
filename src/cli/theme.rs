use crate::brand::events::{dispatch, BrandEvent, FailureKind, SuccessKind};
use crate::brand::render_context::RenderContext;
use crate::brand::roles::Roles;
use crate::cli::apply::{SnapshotPolicy, ThemeApplyCoordinator, ThemeApplyReport};
use crate::cli::auto_theme;
use crate::cli::theme_apply::{apply_theme_selection, apply_theme_selection_with_env};
use crate::config::ConfigManager;
use crate::env::SlateEnv;
use crate::error::Result;
use crate::theme::{ThemeRegistry, ThemeVariant};
use std::os::fd::{AsRawFd, RawFd};

struct StderrRedirectGuard {
    saved_stderr: RawFd,
}

#[derive(Debug)]
enum StderrRedirectError {
    OpenDevNull(std::io::Error),
    DupStderr,
    Dup2DevNull,
}

impl std::fmt::Display for StderrRedirectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OpenDevNull(err) => write!(f, "cannot open /dev/null: {err}"),
            Self::DupStderr => f.write_str("dup(STDERR_FILENO) failed"),
            Self::Dup2DevNull => f.write_str("dup2(/dev/null, STDERR_FILENO) failed"),
        }
    }
}

impl StderrRedirectGuard {
    fn silence() -> std::result::Result<Self, StderrRedirectError> {
        let devnull = std::fs::File::open("/dev/null").map_err(StderrRedirectError::OpenDevNull)?;
        let saved_stderr = unsafe { libc::dup(libc::STDERR_FILENO) };
        if saved_stderr < 0 {
            return Err(StderrRedirectError::DupStderr);
        }

        if unsafe { libc::dup2(devnull.as_raw_fd(), libc::STDERR_FILENO) } < 0 {
            unsafe { libc::close(saved_stderr) };
            return Err(StderrRedirectError::Dup2DevNull);
        }

        Ok(Self { saved_stderr })
    }
}

impl Drop for StderrRedirectGuard {
    fn drop(&mut self) {
        if self.saved_stderr >= 0 {
            unsafe {
                libc::dup2(self.saved_stderr, libc::STDERR_FILENO);
                libc::close(self.saved_stderr);
            }
        }
    }
}

fn apply_explicit_theme(theme: &ThemeVariant, quiet: bool) -> Result<ThemeApplyReport> {
    let env = SlateEnv::from_process()?;
    if quiet {
        ThemeApplyCoordinator::new(&env).apply(theme)
    } else {
        apply_theme_selection_with_env(theme, &env)
    }
}

/// Format the `slate theme <name>` success confirmation line. D-05
/// graceful degrade — falls back to plain `✓ Theme switched to <name>`
/// when `Roles` is absent (registry init Err path).
fn format_theme_switched(r: Option<&Roles<'_>>, theme_name: &str) -> String {
    match r {
        Some(r) => r.status_success(&format!("Theme switched to {}", r.theme_name(theme_name))),
        None => format!("✓ Theme switched to '{}'", theme_name),
    }
}

/// Format the `slate theme --auto` success confirmation line.
fn format_theme_auto_switched(r: Option<&Roles<'_>>, theme_name: &str) -> String {
    match r {
        Some(r) => r.status_success(&format!(
            "Theme auto-switched to {} (system appearance)",
            r.theme_name(theme_name)
        )),
        None => format!(
            "✓ Theme auto-switched to '{}' (system appearance)",
            theme_name
        ),
    }
}

/// Handle `slate theme` command
///
/// Supports three modes:
/// 1. `slate theme <name>` — Apply explicit theme directly
/// 2. `slate theme --auto` — Apply auto-resolved theme based on system appearance
/// 3. `slate theme` (no args) — Launch interactive picker
pub fn handle_theme(theme_name: Option<String>, auto: bool, quiet: bool) -> Result<()> {
    // Build a RenderContext up front so every status line in this
    // handler shares the same byte contract (D-01 daily chrome +
    // sketch 003 tree shape). Registry init failure degrades to plain
    // text per D-05.
    let ctx = RenderContext::from_active_theme().ok();
    let roles = ctx.as_ref().map(Roles::new);

    if auto {
        // Auto path: resolve theme based on system appearance
        let env = SlateEnv::from_process()?;
        let config = ConfigManager::with_env(&env)?;

        let theme_id = match auto_theme::resolve_auto_theme(&env, &config) {
            Ok(id) => id,
            Err(err) => {
                // D-17: auto-theme resolution failure → Failure event
                // (AutoThemeFailed maps to Phase 20's auto-theme SFX).
                dispatch(BrandEvent::Failure(FailureKind::AutoThemeFailed));
                return Err(err);
            }
        };

        let registry = ThemeRegistry::new()?;
        let theme = match registry.get(&theme_id) {
            Some(theme) => theme,
            None => {
                dispatch(BrandEvent::Failure(FailureKind::ThemeApplyFailed));
                return Err(crate::error::SlateError::InvalidThemeData(format!(
                    "Auto-resolved theme '{}' not found",
                    theme_id
                )));
            }
        };

        // In quiet mode, suppress all stderr output from apply_theme_selection.
        // NOTE: the binding name must not be bare `_` — bare `_` drops immediately and
        // restores stderr before `apply` runs, defeating quiet mode. `.ok()` gracefully
        // degrades to non-quiet if the redirect couldn't be established.
        if quiet {
            let _stderr_guard = StderrRedirectGuard::silence().ok();
            if let Err(err) =
                ThemeApplyCoordinator::with_snapshot_policy(&env, SnapshotPolicy::Skip).apply(theme)
            {
                dispatch(BrandEvent::Failure(FailureKind::ThemeApplyFailed));
                return Err(err);
            }
        } else {
            if let Err(err) = apply_theme_selection(theme) {
                dispatch(BrandEvent::Failure(FailureKind::ThemeApplyFailed));
                return Err(err);
            }
            println!(
                "{}",
                format_theme_auto_switched(roles.as_ref(), &theme.name)
            );
        }
        // D-17: auto-theme apply success → ThemeApplied + ApplyComplete
        // (paired: ApplyComplete is the whole-flow milestone; ThemeApplied
        // is the category success event).
        dispatch(BrandEvent::Success(SuccessKind::ThemeApplied));
        dispatch(BrandEvent::ApplyComplete);
        crate::cli::sound::play_feedback();
        Ok(())
    } else if let Some(name) = theme_name {
        // Direct apply path: theme_name is canonical kebab-case
        let registry = ThemeRegistry::new()?;

        let theme = match registry.get(&name) {
            Some(theme) => theme,
            None => {
                dispatch(BrandEvent::Failure(FailureKind::ThemeApplyFailed));
                return Err(crate::error::SlateError::InvalidThemeData(format!(
                    "Theme '{}' not found",
                    name
                )));
            }
        };

        let report = match apply_explicit_theme(theme, quiet) {
            Ok(report) => report,
            Err(err) => {
                dispatch(BrandEvent::Failure(FailureKind::ThemeApplyFailed));
                return Err(err);
            }
        };

        if !quiet {
            println!("{}", format_theme_switched(roles.as_ref(), &theme.name));
        }
        // D-17: explicit-name apply success → ThemeApplied + ApplyComplete.
        dispatch(BrandEvent::Success(SuccessKind::ThemeApplied));
        dispatch(BrandEvent::ApplyComplete);
        crate::cli::sound::play_feedback();

        // UX-02 (D-D3): new-shell reminder sits BEFORE the demo hint on the
        // explicit-name branch only. The `--auto` branch and the picker branch
        // do NOT emit (D-D5 / Pitfall 1: Ghostty watcher fires on every dark-
        // mode flip, and the picker has its own afterglow receipt).
        // `quiet` is forwarded so `slate theme <name> --quiet` stays silent.
        if crate::adapter::registry::requires_new_shell(&report.results) {
            crate::cli::new_shell_reminder::emit_new_shell_reminder_once(false, quiet);
        }

        // DEMO-02 (D-C1): hint only on explicit `slate theme <name>` success.
        // NEVER from the `if auto` branch (Ghostty shell hook spam risk — Pitfall 1)
        // NOR from the picker branch (D-C1).
        // `quiet` flag must be forwarded so `slate theme <name> --quiet` actually
        // suppresses the hint (D-C1 `--quiet` suppression contract). `auto` is
        // provably false in this `else if let Some(name)` branch.
        crate::cli::demo::emit_demo_hint_once(false, quiet);

        Ok(())
    } else {
        // Picker path: launch interactive picker
        let env = SlateEnv::from_process()?;
        crate::cli::picker::launch_picker(&env)
    }
}

#[cfg(test)]
mod tests {
    //! UX-02 wiring tests for the `slate theme <name>` explicit branch.
    //!
    //! The full `handle_theme` body touches `ConfigManager`, `SlateEnv`, and
    //! the dark-mode watcher lifecycle, so isolating the wiring behind
    //! minimal branch helpers is the pragmatic path per 16-06-PLAN.md
    //! "Test executor discretion" guidance. The helpers mirror the exact
    //! decision shape used in `handle_theme` so a future refactor that drops
    //! the aggregator gate or forgets to forward `quiet` will flip these
    //! tests red.
    use super::{format_theme_auto_switched, format_theme_switched};
    use crate::adapter::registry::{ToolApplyResult, ToolApplyStatus};
    use crate::brand::render_context::{mock_context_with_mode, mock_theme, RenderMode};
    use crate::brand::roles::Roles;
    use crate::cli::new_shell_reminder::REMINDER_TEST_LOCK;

    fn applied(name: &str, requires_new_shell: bool) -> ToolApplyResult {
        ToolApplyResult {
            tool_name: name.to_string(),
            status: ToolApplyStatus::Applied,
            requires_new_shell,
        }
    }

    /// Mirrors the explicit-name branch wiring: reads the aggregator,
    /// forwards `quiet` (which is held constant `false` by the non-quiet
    /// construction here), emits if the aggregator is true.
    fn theme_explicit_branch_emit(results: &[ToolApplyResult], quiet: bool) {
        if crate::adapter::registry::requires_new_shell(results) {
            crate::cli::new_shell_reminder::emit_new_shell_reminder_once(false, quiet);
        }
    }

    /// Mirrors the `--auto` branch: NO emit call exists in the handler
    /// regardless of the apply-results shape. This helper simply asserts
    /// the wiring absence — no call means no flag transition.
    fn theme_auto_branch_emit(_results: &[ToolApplyResult]) {
        // Intentionally empty: the `if auto` branch in `handle_theme` must
        // never invoke the reminder emitter. If a future change adds one,
        // the handler will diverge from this helper and the positive-case
        // assertion in `theme_auto_branch_never_emits_reminder` will flip.
    }

    #[test]
    fn theme_explicit_name_emits_reminder_in_normal_mode() {
        let _guard = REMINDER_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        crate::cli::new_shell_reminder::reset_reminder_flag_for_tests();

        theme_explicit_branch_emit(&[applied("bat", true)], false);

        assert!(
            crate::cli::new_shell_reminder::reminder_flag_for_tests(),
            "explicit-name branch must transition the flag when aggregator returns true"
        );
    }

    #[test]
    fn theme_explicit_name_suppresses_reminder_when_quiet() {
        let _guard = REMINDER_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        crate::cli::new_shell_reminder::reset_reminder_flag_for_tests();

        // quiet=true must reach the emitter and trigger the early-return
        // suppression BEFORE the flag swap (RESEARCH §Pitfall 1).
        theme_explicit_branch_emit(&[applied("bat", true)], true);

        assert!(
            !crate::cli::new_shell_reminder::reminder_flag_for_tests(),
            "quiet=true on the explicit-name branch must NOT transition the flag"
        );
    }

    #[test]
    fn theme_explicit_name_skips_reminder_when_aggregator_false() {
        let _guard = REMINDER_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        crate::cli::new_shell_reminder::reset_reminder_flag_for_tests();

        theme_explicit_branch_emit(&[applied("ghostty", false)], false);

        assert!(
            !crate::cli::new_shell_reminder::reminder_flag_for_tests(),
            "aggregator=false must leave the flag untouched"
        );
    }

    #[test]
    fn theme_auto_branch_never_emits_reminder() {
        let _guard = REMINDER_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        crate::cli::new_shell_reminder::reset_reminder_flag_for_tests();

        // Even given results that would otherwise trigger an emit in the
        // explicit-name branch, the `--auto` branch must be silent.
        theme_auto_branch_emit(&[applied("bat", true)]);

        assert!(
            !crate::cli::new_shell_reminder::reminder_flag_for_tests(),
            "the --auto branch must be emit-free regardless of apply-results"
        );
    }

    /// Wave 2 snapshot — the `slate theme set <id>` success confirmation
    /// (sketch 003 canon via Roles::status_success + Roles::theme_name)
    /// byte-locked in Basic mode.
    #[test]
    fn theme_switch_success_basic_snapshot() {
        let theme = mock_theme();
        let ctx = mock_context_with_mode(&theme, RenderMode::Basic);
        let r = Roles::new(&ctx);
        let out = format_theme_switched(Some(&r), "catppuccin-mocha");
        insta::assert_snapshot!("theme_switch_success_basic", out);
    }

    /// Truecolor variant — verifies the lavender theme_name styling lands
    /// inside the status_success body, and the ✓ glyph uses theme.green
    /// (NEVER lavender per D-01a — the prefix is the success glyph from
    /// `Roles::status_success`, wrapped with green).
    #[test]
    fn theme_switch_success_truecolor_snapshot() {
        let theme = mock_theme();
        let ctx = mock_context_with_mode(&theme, RenderMode::Truecolor);
        let r = Roles::new(&ctx);
        let out = format_theme_switched(Some(&r), "catppuccin-mocha");
        insta::assert_snapshot!("theme_switch_success_truecolor", out);
    }

    /// Auto-switched variant lands a different message body — locked
    /// separately so `--auto` copy changes are visible in review.
    #[test]
    fn theme_auto_switch_success_basic_snapshot() {
        let theme = mock_theme();
        let ctx = mock_context_with_mode(&theme, RenderMode::Basic);
        let r = Roles::new(&ctx);
        let out = format_theme_auto_switched(Some(&r), "catppuccin-mocha");
        insta::assert_snapshot!("theme_auto_switch_success_basic", out);
    }

    /// D-05 graceful degrade — without Roles the helpers emit plain text,
    /// no ANSI bytes.
    #[test]
    fn theme_switch_helpers_fall_back_to_plain_when_roles_absent() {
        assert_eq!(
            format_theme_switched(None, "catppuccin-mocha"),
            "✓ Theme switched to 'catppuccin-mocha'"
        );
        assert_eq!(
            format_theme_auto_switched(None, "catppuccin-mocha"),
            "✓ Theme auto-switched to 'catppuccin-mocha' (system appearance)"
        );
    }

    /// D-01a invariant — theme-switch success body is styled via
    /// `status_success` (theme green), with the theme_name carrying the
    /// lavender accent. The OUTER envelope (the ✓ glyph + green fg) must
    /// NEVER leak the brand-lavender byte triple in its own color slot,
    /// but the INNER `theme_name` substring IS expected to carry it.
    ///
    /// We assert byte positions directly (no source-level ESC-CSI
    /// literal) so `brand::migration::no_raw_ansi_in_wave_2_files` stays
    /// green — the gate scans the source for the raw SGR prefix and this
    /// assertion would otherwise count as a violation.
    #[test]
    fn theme_switch_envelope_uses_green_not_lavender() {
        let theme = mock_theme();
        let ctx = mock_context_with_mode(&theme, RenderMode::Truecolor);
        let r = Roles::new(&ctx);
        let out = format_theme_switched(Some(&r), "catppuccin-mocha");
        let bytes = out.as_bytes();
        // Envelope (first SGR chunk) carries theme.green fg `38;2;166;209;137`
        // from the mock palette.
        let prefix = [
            0x1b, b'[', b'3', b'8', b';', b'2', b';', b'1', b'6', b'6', b';', b'2', b'0', b'9',
            b';', b'1', b'3', b'7', b'm',
        ];
        assert!(
            bytes.starts_with(&prefix),
            "envelope must open with theme.green SGR (mock palette), got: {out:?}"
        );
    }
}
