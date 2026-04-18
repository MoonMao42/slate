use crate::cli::apply::{SnapshotPolicy, ThemeApplyCoordinator};
use crate::cli::auto_theme;
use crate::cli::theme_apply::apply_theme_selection;
use crate::config::ConfigManager;
use crate::design::symbols::Symbols;
use crate::env::SlateEnv;
use crate::error::Result;
use crate::theme::ThemeRegistry;
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

/// Handle `slate theme` command
///
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

        let _ = apply_theme_selection(theme)?;

        println!("{} Theme switched to '{}'", Symbols::SUCCESS, theme.name);
        crate::cli::sound::play_feedback();

        // DEMO-02 (D-C1): hint only on explicit `slate theme <name>` success.
        // NEVER from the `if auto` branch (Ghostty shell hook spam risk — Pitfall 1)
        // NOR from the picker branch (D-C1).
        crate::cli::demo::emit_demo_hint_once(false, false);

        Ok(())
    } else {
        // Picker path: launch interactive picker
        let env = SlateEnv::from_process()?;
        crate::cli::picker::launch_picker(&env)
    }
}
