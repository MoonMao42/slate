//! D-04 Hybrid starship fork + fallback for Tab full-preview mode.
//!
//! Per-subprocess `.env("STARSHIP_CONFIG", managed_path)` (NOT
//! `std::env::set_var` — V12 security). `which::which` probe first;
//! fallback to self-drawn SAMPLE_TOKENS on any error path (D-04 locked).
//!
//! Plan 19-06 (Wave 3) owns the full implementation + test suite. This
//! file in the Plan 19-07 parallel worktree carries a MINIMAL SIGNATURE
//! STUB so event_loop.rs (Plan 19-07) compiles inside the worktree. The
//! orchestrator's Wave-3 merge will pick up 19-06's authoritative
//! version; the stub signature here is BYTE-IDENTICAL to 19-06's
//! `<interfaces>` block so the merge is a clean replace.
//!
//! Per `<parallel_execution>` directive: "Follow the plan's
//! `<interfaces>` block for the EXACT signature. ... keep the stub
//! signature byte-identical to what 19-06's plan specifies."

use std::path::{Path, PathBuf};

/// Error variants for the starship fork operation.
#[allow(dead_code)] // Populated by Plan 19-06 Wave-3 implementation.
#[derive(Debug)]
pub(crate) enum StarshipForkError {
    /// Binary not on PATH or injected path doesn't exist on disk.
    NotInstalled,
    /// `Command::output()` spawn error.
    SpawnFailed,
    /// Child process returned non-zero exit.
    NonZeroExit,
    /// V12 path-traversal guard: managed_toml outside managed_dir.
    PathNotAllowed,
}

/// Minimal signature stub. Plan 19-06 provides the real implementation
/// (path guard + which probe + `.env("STARSHIP_CONFIG", ...)` + stderr
/// null + escape stripper). This stub returns `NotInstalled`
/// unconditionally so event_loop.rs's Tab → fork call falls back to the
/// self-drawn path per D-04 — picker behavior remains correct (fork
/// failure = self-draw) until the real fork lands.
#[allow(dead_code)] // Wired by Plan 19-07 event_loop Tab branch.
pub(crate) fn fork_starship_prompt(
    managed_toml: &Path,
    managed_dir: &Path,
    _width: u16,
    starship_bin: Option<&Path>,
) -> Result<String, StarshipForkError> {
    // Minimal path-guard so the stub behaves observably like the real fn.
    if !managed_toml.starts_with(managed_dir) {
        return Err(StarshipForkError::PathNotAllowed);
    }
    // Honor the injected-binary contract so 19-07 can test the
    // NotInstalled branch without relying on this stub.
    let resolved: PathBuf = match starship_bin {
        Some(p) => p.to_path_buf(),
        None => return Err(StarshipForkError::NotInstalled),
    };
    if !resolved.exists() {
        return Err(StarshipForkError::NotInstalled);
    }
    // Real fork lives in Plan 19-06. Stub surface is enough for 19-07
    // glue to compile; caller falls back to self-draw on Err.
    Err(StarshipForkError::NotInstalled)
}

#[cfg(test)]
mod tests {
    // Plan 19-06 Wave-3 authoritative tests land on merge. This stub file
    // intentionally has no tests — Plan 19-07 worktree only needs the
    // signature to compile its event_loop Tab branch.
}
