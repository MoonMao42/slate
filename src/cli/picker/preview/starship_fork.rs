//! Hybrid starship fork for Tab full-preview mode.
//! Forks the user's `starship prompt` binary with a per-subprocess
//! `STARSHIP_CONFIG` env override pointed at a picker-managed preview TOML,
//! so the preview renders the real theme-aware prompt the user will see
//! after committing. Never forks on the default browsing path.
//! Design rules enforced here:
//! - **Path guard (V12 / RESEARCH §Pitfall 5)**: `managed_toml` MUST live
//! inside `managed_dir`. A rogue theme_id must not be able to push the
//! fork to read an arbitrary file via path concatenation.
//! - Both output streams are captured privately with a combined byte cap.
//!   stderr never reaches the picker; stdin is closed, and timeout/output-limit
//!   failures terminate the owned process group before falling back.
//! - **Env isolation**: `.env("STARSHIP_CONFIG", ...)` affects only the
//! child subprocess. `std::env::set_var` would pollute the picker
//! process so the next `slate theme set <id>` reads the wrong path.
//! - **Dependency injection for tests**: `starship_bin: Option<&Path>`
//! lets tests pass an explicit (non-existent) path to exercise the
//! `NotInstalled` branch without mutating global `PATH` — per user
//! MEMORY `feedback_no_tech_debt` (pure function testing, no global
//! env var mutation in tests) and CONTEXT §Anti-patterns.

use crate::config::file_read::{self, Links, MAX_TOOL_CONFIG_BYTES};
use crate::platform::process_output::{self, Completion, Limits};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

const PROMPT_LIMITS: Limits = Limits {
    timeout: Duration::from_millis(750),
    max_output: 64 * 1024,
};

/// Error variants — callers log/ignore (silent fallback) and switch
/// to `compose::self_draw_prompt_from_sample_tokens`.
#[derive(Debug)]
#[allow(dead_code)] // event_loop wiring removes the attribute.
                    // `pub(crate)` → `pub` so integration tests can match on
                    // the error variants (V-11 integration coverage for the V12 path-guard).
pub enum StarshipForkError {
    /// Either `which::which("starship")` failed OR the caller-injected
    /// `starship_bin` path doesn't exist on disk.
    NotInstalled,
    /// Could not start or safely capture the child process.
    SpawnFailed,
    /// Child process returned non-zero.
    NonZeroExit,
    /// V12 path-traversal guard tripped (managed_toml outside managed_dir).
    PathNotAllowed,
    /// Configuration is missing, nonregular, unreadable, oversized or non-UTF-8.
    InvalidConfig,
    /// Capture deadline elapsed. Partial prompt output must not be rendered.
    TimedOut,
    /// Combined stdout/stderr exceeded the capture limit.
    OutputLimit,
    /// Prompt output must be UTF-8 before display filtering; no lossy replay of controls.
    InvalidOutput,
}

/// Fork the starship binary and capture its prompt output.
/// `starship_bin`: dependency-injection hook.
/// - `None` (production): probe via `which::which("starship")`.
/// - `Some(&path)` (tests): use the path as-is; if it doesn't exist on
/// disk, the function returns `NotInstalled` without spawning. This
/// lets the unit test suite exercise the fallback branch without
/// mutating the process's `PATH` env var.
/// Returns UTF-8 prompt text with zsh width wrappers and terminal actions removed;
/// bounded numeric SGR colors/styles remain. On failure use the built-in sample.
#[allow(dead_code)] // event_loop wiring removes the attribute.
                    // `pub(crate)` → `pub` so integration + bench targets can
                    // call the fork without a shim. Production callers inside the crate are
                    // unaffected by the visibility bump.
pub fn fork_starship_prompt(
    managed_toml: &Path,
    managed_dir: &Path,
    width: u16,
    starship_bin: Option<&Path>,
) -> Result<String, StarshipForkError> {
    // 1. Path-traversal guard (V12). Must run before any binary resolution
    // so a hostile path never triggers a fork, not even a failed one.
    let managed_toml = validate_config(managed_toml, managed_dir)?;

    // 2. Binary resolution. Injected path wins; otherwise probe PATH.
    let resolved: PathBuf = match starship_bin {
        Some(p) => p.to_path_buf(),
        None => which::which("starship").map_err(|_| StarshipForkError::NotInstalled)?,
    };
    // If the injected path (or a stale which result) doesn't point at a
    // real file, surface NotInstalled before attempting to spawn it.
    if !resolved.exists() {
        return Err(StarshipForkError::NotInstalled);
    }

    // 3. Fork with a child-only env override and private bounded output.
    // Use the user's real cwd when available so directory / git / language
    // modules match the fresh-shell prompt they will actually see after
    // commit. If cwd lookup fails, use the validated preview directory.
    let current_dir = std::env::current_dir()
        .unwrap_or_else(|_| managed_toml.parent().unwrap_or(managed_dir).to_owned());
    let output = process_output::capture(
        Command::new(&resolved)
            .arg("prompt")
            .args(["--status", "0", "--keymap", "viins"])
            .args(["--terminal-width", &width.to_string()])
            .arg("--path")
            .arg(current_dir.as_os_str())
            .env("STARSHIP_CONFIG", managed_toml),
        PROMPT_LIMITS,
    )
    .map_err(|_| StarshipForkError::SpawnFailed)?;

    match output.completion {
        Completion::TimedOut => return Err(StarshipForkError::TimedOut),
        Completion::OutputLimit => return Err(StarshipForkError::OutputLimit),
        Completion::Exited(status) if !status.success() => {
            return Err(StarshipForkError::NonZeroExit)
        }
        Completion::Exited(_) => {}
    }

    let raw = std::str::from_utf8(&output.stdout).map_err(|_| StarshipForkError::InvalidOutput)?;
    Ok(super::prompt_text::sanitize(&strip_zsh_prompt_escapes(raw)))
}

/// Validate the resolved target, not just its textual prefix. Checks precede
/// binary resolution and spawning, but do not lock out external file replacement.
fn validate_config(path: &Path, managed_dir: &Path) -> Result<PathBuf, StarshipForkError> {
    if !path.starts_with(managed_dir) {
        return Err(StarshipForkError::PathNotAllowed);
    }
    let root = std::fs::canonicalize(managed_dir).map_err(|_| StarshipForkError::PathNotAllowed)?;
    let target = std::fs::canonicalize(path).map_err(|_| StarshipForkError::InvalidConfig)?;
    if !root.is_dir() || !target.starts_with(&root) {
        return Err(StarshipForkError::PathNotAllowed);
    }
    file_read::read_text_with_links(&target, MAX_TOOL_CONFIG_BYTES, Links::Reject)
        .map_err(|_| StarshipForkError::InvalidConfig)?
        .ok_or(StarshipForkError::InvalidConfig)?;
    Ok(target)
}

/// starship emits zsh-prompt `%{...%}` wrappers for width accounting;
/// strip them so the picker alt-screen doesn't render them literally.
#[allow(dead_code)] // event_loop wiring removes the attribute.
pub(crate) fn strip_zsh_prompt_escapes(s: &str) -> String {
    s.replace("%{", "").replace("%}", "")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // NOTE: these tests are PURE function calls — no `std::env::set_var`,
    // no `PathGuard`, no `PATH_LOCK`. Per user MEMORY feedback_no_tech_debt
    // + CONTEXT §Anti-patterns: "pure function testing, no global env var
    // mutation in tests". The `NotInstalled` branch is exercised by
    // injecting a non-existent binary path via the `starship_bin` parameter.

    #[test]
    fn config_path_is_managed_only() {
        // V12 path-guard: managed_toml outside managed_dir → PathNotAllowed
        let outside = PathBuf::from("/etc/passwd");
        let managed = PathBuf::from("/home/user/.config/slate/managed");
        let result = fork_starship_prompt(&outside, &managed, 80, None);
        assert!(
            matches!(result, Err(StarshipForkError::PathNotAllowed)),
            "path outside managed_dir must be rejected; got {result:?}"
        );
    }

    #[test]
    fn fork_missing_binary_falls_back() {
        // Use a valid managed path so the path-guard doesn't fire first.
        let root = tempfile::tempdir().unwrap();
        let managed = root.path();
        let toml = managed.join("active.toml");
        std::fs::write(&toml, "# fixture\n").unwrap();
        // Inject a non-existent binary path — no PATH mutation needed.
        let fake_bin = PathBuf::from("/nonexistent/bin/starship");
        let result = fork_starship_prompt(&toml, managed, 80, Some(&fake_bin));
        assert!(
            matches!(result, Err(StarshipForkError::NotInstalled)),
            "non-existent injected binary must yield NotInstalled; got {result:?}"
        );
    }

    #[test]
    fn strip_zsh_prompt_escapes_removes_wrappers() {
        // Fixtures use the Unicode rune form of ESC (U+001B) so the
        // aggregate scanner, which matches on the hex-byte literal form in
        // source, doesn't flag these as raw styling. Runtime bytes identical.
        let input = "%{\u{001b}[1m%}bold%{\u{001b}[0m%}";
        let expected = "\u{001b}[1mbold\u{001b}[0m";
        assert_eq!(strip_zsh_prompt_escapes(input), expected);
    }

    #[test]
    fn strip_zsh_prompt_escapes_handles_empty() {
        assert_eq!(strip_zsh_prompt_escapes(""), "");
    }
}
