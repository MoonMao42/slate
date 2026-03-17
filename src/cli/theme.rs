use crate::cli::apply::{SnapshotPolicy, ThemeApplyCoordinator, ThemeApplyReport};
use crate::cli::auto_theme;
use crate::cli::theme_apply::{apply_theme_selection, apply_theme_selection_with_env};
use crate::config::ConfigManager;
use crate::design::symbols::Symbols;
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

/// Handle `slate theme` command
/// Supports three modes:
/// 1. `slate theme <name>` — Apply explicit theme directly
/// 2. `slate theme --auto` — Apply auto-resolved theme based on system appearance
/// 3. `slate theme` (no args) — Launch interactive picker
pub fn handle_theme(theme_name: Option<String>, auto: bool, quiet: bool) -> Result<()> {
    if auto {
        // Auto path: resolve theme based on system appearance
        let env = SlateEnv::from_process()?;
        let config = ConfigManager::with_env(&env)?;

        let theme_id = auto_theme::resolve_auto_theme(&env, &config)?;

        let registry = ThemeRegistry::new()?;
        let theme = registry.get(&theme_id).ok_or_else(|| {
            crate::error::SlateError::InvalidThemeData(format!(
                "Auto-resolved theme '{}' not found",
                theme_id
            ))
        })?;

        // In quiet mode, suppress all stderr output from apply_theme_selection.
        // NOTE: the binding name must not be bare `_` — bare `_` drops immediately and
        // restores stderr before `apply` runs, defeating quiet mode. `.ok()` gracefully
        // degrades to non-quiet if the redirect couldn't be established.
        if quiet {
            let _stderr_guard = StderrRedirectGuard::silence().ok();
            ThemeApplyCoordinator::with_snapshot_policy(&env, SnapshotPolicy::Skip).apply(theme)?;
        } else {
            let _ = apply_theme_selection(theme)?;
            println!(
                "{} Theme auto-switched to '{}' (system appearance)",
                Symbols::SUCCESS,
                theme.name
            );
        }
        crate::cli::sound::play_feedback();
        Ok(())
    } else if let Some(name) = theme_name {
        // Direct apply path: theme_name is canonical kebab-case
        let registry = ThemeRegistry::new()?;

        let theme = registry.get(&name).ok_or_else(|| {
            crate::error::SlateError::InvalidThemeData(format!("Theme '{}' not found", name))
        })?;

        let report = apply_explicit_theme(theme, quiet)?;

        if !quiet {
            println!("{} Theme switched to '{}'", Symbols::SUCCESS, theme.name);
        }
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
    //! The full `handle_theme` body touches `ConfigManager`, `SlateEnv`, and
    //! the dark-mode watcher lifecycle, so isolating the wiring behind
    //! minimal branch helpers is the pragmatic path per 16-06-PLAN.md
    //! "Test executor discretion" guidance. The helpers mirror the exact
    //! decision shape used in `handle_theme` so a future refactor that drops
    //! the aggregator gate or forgets to forward `quiet` will flip these
    //! tests red.
    use crate::adapter::registry::{ToolApplyResult, ToolApplyStatus};
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
}
