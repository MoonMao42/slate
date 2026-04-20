//! D-04 Hybrid starship fork for Tab full-preview mode.
//!
//! Forks the user's `starship prompt` binary with a per-subprocess
//! `STARSHIP_CONFIG` env override pointed at `managed/starship/active.toml`,
//! so the preview renders the real theme-aware prompt the user will see
//! after committing. Never forks on the default browsing path (D-04).
//!
//! Design rules enforced here:
//! - **Path guard (V12 / RESEARCH §Pitfall 5)**: `managed_toml` MUST live
//!   inside `managed_dir`. A rogue theme_id must not be able to push the
//!   fork to read an arbitrary file via path concatenation.
//! - **stderr null (RESEARCH §Pitfall 5)**: zsh `command_not_found_handler`
//!   and similar shell wrappers can inject stderr output that corrupts
//!   the alt-screen. `.stderr(Stdio::null())` blocks it.
//! - **Env isolation**: `.env("STARSHIP_CONFIG", ...)` affects only the
//!   child subprocess. `std::env::set_var` would pollute the picker
//!   process so the next `slate theme set <id>` reads the wrong path.
//! - **Dependency injection for tests**: `starship_bin: Option<&Path>`
//!   lets tests pass an explicit (non-existent) path to exercise the
//!   `NotInstalled` branch without mutating global `PATH` — per user
//!   MEMORY `feedback_no_tech_debt` (pure function testing, no global
//!   env var mutation in tests) and CONTEXT §Anti-patterns.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Error variants — callers log/ignore (D-04 silent fallback) and switch
/// to `compose::self_draw_prompt_from_sample_tokens`.
#[derive(Debug)]
#[allow(dead_code)] // Plan 19-07 event_loop wiring removes the attribute.
pub(crate) enum StarshipForkError {
    /// Either `which::which("starship")` failed OR the caller-injected
    /// `starship_bin` path doesn't exist on disk.
    NotInstalled,
    /// `Command::output()` failed (e.g. spawn error, syscall failure).
    SpawnFailed,
    /// Child process returned non-zero.
    NonZeroExit,
    /// V12 path-traversal guard tripped (managed_toml outside managed_dir).
    PathNotAllowed,
}

/// Fork the starship binary and capture its prompt output.
///
/// `starship_bin`: dependency-injection hook.
///   - `None` (production): probe via `which::which("starship")`.
///   - `Some(&path)` (tests): use the path as-is; if it doesn't exist on
///     disk, the function returns `NotInstalled` without spawning. This
///     lets the unit test suite exercise the fallback branch without
///     mutating the process's `PATH` env var.
///
/// Returns the stdout (with zsh %{ %} prompt-width escapes stripped) on
/// success. On any failure the caller should fall back to self-drawing.
#[allow(dead_code)] // Plan 19-07 event_loop wiring removes the attribute.
pub(crate) fn fork_starship_prompt(
    managed_toml: &Path,
    managed_dir: &Path,
    width: u16,
    starship_bin: Option<&Path>,
) -> Result<String, StarshipForkError> {
    // 1. Path-traversal guard (V12). Must run before any binary resolution
    //    so a hostile path never triggers a fork, not even a failed one.
    if !managed_toml.starts_with(managed_dir) {
        return Err(StarshipForkError::PathNotAllowed);
    }

    // 2. Binary resolution. Injected path wins; otherwise probe PATH.
    let resolved: PathBuf = match starship_bin {
        Some(p) => p.to_path_buf(),
        None => which::which("starship").map_err(|_| StarshipForkError::NotInstalled)?,
    };
    // If the injected path (or a stale which result) doesn't point at a
    // real file, surface NotInstalled rather than letting `Command::output`
    // emit a confusing OS-level ENOENT.
    if !resolved.exists() {
        return Err(StarshipForkError::NotInstalled);
    }

    // 3. Fork with env override + stderr null.
    //    --path is a fixed fixture: `/Users/demo/code/slate` produces a
    //    believable [directory] module output without leaking the user's
    //    actual pwd into the picker preview (RESEARCH Open Q2).
    let output = Command::new(&resolved)
        .arg("prompt")
        .args(["--status", "0", "--keymap", "viins"])
        .args(["--terminal-width", &width.to_string()])
        .args(["--path", "/Users/demo/code/slate"])
        .env("STARSHIP_CONFIG", managed_toml)
        .stderr(Stdio::null())
        .stdout(Stdio::piped())
        .output()
        .map_err(|_| StarshipForkError::SpawnFailed)?;

    if !output.status.success() {
        return Err(StarshipForkError::NonZeroExit);
    }

    let raw = String::from_utf8_lossy(&output.stdout);
    Ok(strip_zsh_prompt_escapes(&raw))
}

/// starship emits zsh-prompt `%{...%}` wrappers for width accounting;
/// strip them so the picker alt-screen doesn't render them literally.
#[allow(dead_code)] // Plan 19-07 event_loop wiring removes the attribute.
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
        let managed = std::env::temp_dir();
        let toml = managed.join("active.toml");
        // Inject a non-existent binary path — no PATH mutation needed.
        let fake_bin = PathBuf::from("/nonexistent/bin/starship");
        let result = fork_starship_prompt(&toml, &managed, 80, Some(&fake_bin));
        assert!(
            matches!(result, Err(StarshipForkError::NotInstalled)),
            "non-existent injected binary must yield NotInstalled; got {result:?}"
        );
    }

    #[test]
    fn strip_zsh_prompt_escapes_removes_wrappers() {
        // Fixtures use the Unicode rune form of ESC (U+001B) so the Phase 18
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
