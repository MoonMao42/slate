//! · · Wave-6 integration gate — starship fork
//! fixture suite (VALIDATION rows 12 + 14 integration scope).
//! These tests exercise the `fork_starship_prompt` function at integration
//! scope (separate crate → can only see `pub` items, which is why Plan
//! 19-08 promoted the symbol to `pub` in the preview module).
//! Per user MEMORY `feedback_no_tech_debt` + CONTEXT §Anti-patterns, both
//! tests use the `starship_bin: Option<&Path>` dependency-injection
//! parameter and/or explicit managed_dir paths. NO `std::env::set_var`,
//! NO `PathGuard`, NO `PATH_LOCK`, NO `Command::new` shelling into slate.

use slate_cli::cli::picker::preview::starship_fork::{fork_starship_prompt, StarshipForkError};
use slate_cli::env::SlateEnv;
use std::path::PathBuf;
use tempfile::TempDir;

fn program(root: &std::path::Path, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = root.join("starship-fixture");
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[test]
fn preview_starship_output_filters_control_actions_and_rejects_non_utf8() {
    let root = TempDir::new().unwrap();
    let config = root.path().join("preview.toml");
    std::fs::write(&config, "# fixture\n").unwrap();
    let binary = program(root.path(), "printf '%%{\\033[38;2;12;34;56m%%}安全❯\\033]52;c;PRIVATE_CLIPBOARD\\007\\033[999;1H\\033[?1049l\\033PPRIVATE_DCS\\033\\\\\\033[0m\\r\\n'");
    let prompt = fork_starship_prompt(&config, root.path(), 80, Some(&binary)).unwrap();
    assert_eq!(prompt, "\x1b[38;2;12;34;56m安全❯\x1b[0m\n");
    let binary = program(root.path(), "printf '\\377PRIVATE_OUTPUT'");
    assert!(matches!(
        fork_starship_prompt(&config, root.path(), 80, Some(&binary)),
        Err(StarshipForkError::InvalidOutput)
    ));
}

#[test]
fn preview_starship_rejects_oversized_prompt_output() {
    let root = TempDir::new().unwrap();
    let config = root.path().join("preview.toml");
    std::fs::write(&config, "# fixture\n").unwrap();
    let binary = program(
        root.path(),
        "i=0; while [ $i -lt 5000 ]; do printf PRIVATE_PROMPT_NOISE; i=$((i+1)); done",
    );
    assert!(matches!(
        fork_starship_prompt(&config, root.path(), 80, Some(&binary)),
        Err(StarshipForkError::OutputLimit)
    ));
    for (body, success) in [
        ("printf '%32768s' ''; printf '%32768s' '' >&2", true),
        ("printf '%32768s' ''; printf '%32769s' '' >&2", false),
    ] {
        let binary = program(root.path(), body);
        let result = fork_starship_prompt(&config, root.path(), 80, Some(&binary));
        if success {
            assert_eq!(result.unwrap().len(), 32768);
        } else {
            assert!(matches!(result, Err(StarshipForkError::OutputLimit)));
        }
    }
}

#[test]
fn preview_starship_bounds_hangs_and_inherited_pipes_without_returning_partial_output() {
    let root = TempDir::new().unwrap();
    let config = root.path().join("preview.toml");
    std::fs::write(&config, "# fixture\n").unwrap();
    for body in [
        "printf PRIVATE_PARTIAL; exec /bin/sleep 10",
        "printf PRIVATE_PARTIAL; /bin/sleep 10 & exit 0",
    ] {
        let binary = program(root.path(), body);
        let started = std::time::Instant::now();
        let result = fork_starship_prompt(&config, root.path(), 80, Some(&binary));
        assert!(
            matches!(result, Err(StarshipForkError::TimedOut)),
            "{result:?}"
        );
        assert!(started.elapsed() < std::time::Duration::from_secs(4));
    }
}

#[test]
fn preview_starship_preserves_prompt_flags_and_keeps_stderr_private() {
    let root = TempDir::new().unwrap();
    let config = root.path().join("preview.toml");
    std::fs::write(&config, "# fixture\n").unwrap();
    let binary = program(root.path(), "[ \"$1:$2:$3:$4:$5:$6:$7\" = 'prompt:--status:0:--keymap:viins:--terminal-width:123' ] || exit 21\n[ \"$8\" = --path ] && [ -d \"$9\" ] && [ -f \"$STARSHIP_CONFIG\" ] || exit 22\nif read -r unwanted; then exit 23; fi\nprintf PRIVATE_STDERR >&2\nprintf '%%{\\033[31m%%}fixture❯ %%{\\033[0m%%}'");
    let original_env = std::env::var_os("STARSHIP_CONFIG");
    let prompt = fork_starship_prompt(&config, root.path(), 123, Some(&binary)).unwrap();
    assert_eq!(prompt, "\x1b[31mfixture❯ \x1b[0m");
    assert_eq!(std::env::var_os("STARSHIP_CONFIG"), original_env);
    let binary = program(
        root.path(),
        "printf PRIVATE_PARTIAL; printf PRIVATE_ERROR >&2; exit 41",
    );
    assert!(matches!(
        fork_starship_prompt(&config, root.path(), 80, Some(&binary)),
        Err(StarshipForkError::NonZeroExit)
    ));
}

#[test]
fn preview_starship_rejects_resolved_escapes_and_unsafe_inputs_before_spawn() {
    use std::os::unix::fs::symlink;
    let root = TempDir::new().unwrap();
    let managed = root.path().join("managed");
    std::fs::create_dir(&managed).unwrap();
    let outside = root.path().join("outside.toml");
    std::fs::write(&outside, "# private outside\n").unwrap();
    symlink(&outside, managed.join("escape")).unwrap();
    symlink(root.path().join("missing"), managed.join("dangling")).unwrap();
    std::fs::create_dir(managed.join("directory")).unwrap();
    std::fs::write(managed.join("binary"), [0xff, 0]).unwrap();
    std::fs::File::create(managed.join("oversized"))
        .unwrap()
        .set_len(8 * 1024 * 1024 + 1)
        .unwrap();
    let fifo = managed.join("pipe");
    let name = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    let binary = program(root.path(), "printf ran > \"${STARSHIP_CONFIG}.called\"");
    for name in [
        "../outside.toml",
        "escape",
        "dangling",
        "directory",
        "binary",
        "oversized",
        "pipe",
    ] {
        let result = fork_starship_prompt(&managed.join(name), &managed, 80, Some(&binary));
        assert!(
            matches!(
                result,
                Err(StarshipForkError::PathNotAllowed | StarshipForkError::InvalidConfig)
            ),
            "{name}: {result:?}"
        );
        assert!(!managed.join(format!("{name}.called")).exists());
    }
    assert!(!root.path().join("outside.toml.called").exists());
    let config = managed.join("valid.toml");
    std::fs::write(&config, "# linked private preview\n").unwrap();
    symlink(&config, managed.join("valid-link")).unwrap();
    fork_starship_prompt(&managed.join("valid-link"), &managed, 80, Some(&binary)).unwrap();
    assert!(managed.join("valid.toml.called").exists());
}

/// VALIDATION row 12 — integration scope · fork fallback when the
/// starship binary is absent.
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
