//! Phase 19 · Plan 19-08 · Wave-6 integration gate — starship fork
//! fixture suite (VALIDATION rows 12 + 14 integration scope).
//!
//! These tests exercise the `fork_starship_prompt` function at integration
//! scope (separate crate → can only see `pub` items, which is why Plan
//! 19-08 promoted the symbol to `pub` in the preview module).
//!
//! Per user MEMORY `feedback_no_tech_debt` + CONTEXT §Anti-patterns, both
//! tests use the `starship_bin: Option<&Path>` dependency-injection
//! parameter and/or explicit managed_dir paths. NO `std::env::set_var`,
//! NO `PathGuard`, NO `PATH_LOCK`, NO `Command::new` shelling into slate.

use slate_cli::cli::picker::preview::starship_fork::{fork_starship_prompt, StarshipForkError};
use slate_cli::env::SlateEnv;
use std::path::PathBuf;
use tempfile::TempDir;

/// VALIDATION row 12 — integration scope · fork fallback when the
/// starship binary is absent.
///
/// Mirrors the unit test in `starship_fork::tests::fork_missing_binary_falls_back`
/// but lives in the `tests/` tree so we cover the `pub` API surface that
/// integration callers will see. We inject a non-existent binary path via
/// `Some(&PathBuf::from("/nonexistent/bin/starship"))` — no PATH mutation
/// needed, the function returns `NotInstalled` after the existence check.
#[test]
fn fork_missing_binary_falls_back() {
    // Use a valid managed path so the V12 guard doesn't fire first.
    let tmp = TempDir::new().expect("tempdir");
    let managed_dir = tmp.path();
    let managed_toml = managed_dir.join("starship").join("active.toml");
    std::fs::create_dir_all(managed_dir.join("starship")).unwrap();
    std::fs::write(&managed_toml, "# placeholder\n").unwrap();

    // Inject a non-existent binary path — no PATH mutation, no
    // serialization. Pure function call.
    let fake_bin = PathBuf::from("/nonexistent/bin/starship");
    let result = fork_starship_prompt(&managed_toml, managed_dir, 80, Some(&fake_bin));

    assert!(
        matches!(result, Err(StarshipForkError::NotInstalled)),
        "non-existent injected binary must yield NotInstalled; got {result:?}"
    );
}

/// VALIDATION row 14 / V-11 fix — integration scope · V12 path-traversal
/// guard must reject managed_toml paths that don't live under managed_dir.
///
/// The unit test (`config_path_is_managed_only` in `starship_fork`) covers
/// the same branch; the integration companion proves the guard is still
/// enforced when callers import the function across crate boundaries and
/// hands a `SlateEnv`-derived managed_dir.
#[test]
fn fork_rejects_path_outside_managed_dir_integration() {
    let tmp = TempDir::new().expect("tempdir");
    let env = SlateEnv::with_home(tmp.path().to_path_buf());

    // managed_dir points inside tempdir (env's managed subdir); the
    // candidate managed_toml is an absolute path outside that subtree.
    // `starts_with` is lexical, so `/etc/passwd` cannot start with any
    // `<tempdir>/.config/slate/managed` prefix — guard must trip.
    let managed_dir = env.managed_subdir("managed");
    let traversal = PathBuf::from("/etc/passwd");
    let result = fork_starship_prompt(&traversal, &managed_dir, 80, None);

    assert!(
        matches!(result, Err(StarshipForkError::PathNotAllowed)),
        "V12 path-traversal: /etc/passwd must be rejected by the managed_dir guard; got {result:?}"
    );
}
