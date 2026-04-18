//! Phase 16 Plan 07 — Task 2: eza truecolor empirical smoke test.
//!
//! Closes the MEDIUM-confidence open question from RESEARCH §Pitfall 3 /
//! Assumption A1: eza's `eza_colors(5)` manpage documents `38;5;nnn` for
//! 256-colour but is silent on `38;2;R;G;B` truecolor. Slate's Phase-16
//! strategy hinges on eza accepting truecolor in `EZA_COLORS` (D-A5 forbids
//! a 256-colour fallback).
//!
//! This test is the one empirical check that cannot be answered by reading
//! source: set `EZA_COLORS="reset:*.rs=38;2;255;0;0"`, invoke `eza
//! --color=always` on a tempdir holding a single `main.rs`, and inspect the
//! raw stdout bytes for the truecolor escape sequence around the filename.
//!
//! Outcomes:
//!   * eza on PATH, truecolor accepted → test passes, A1 confirmed.
//!   * eza on PATH, truecolor rejected → test panics with the
//!     `EZA TRUECOLOR REJECTED` contingency message so the executor
//!     surfaces the fallback path (theme.yml file-type layer, RESEARCH
//!     §Pitfall 3).
//!   * eza absent → test prints a skip line and returns. Not `#[ignore]`d,
//!     because we want it to run silently on dev machines where eza is
//!     installed via slate's own adapter.
//!
//! Isolation: every env var is passed via `Command::env(…)` per the project
//! testing guideline — the host process environment is never mutated.

use std::process::Command;
use tempfile::TempDir;

/// Pure-red truecolor probe. `*.rs=38;2;255;0;0` is unambiguous — if eza
/// accepts it, stdout carries `\x1b[…38;2;255;0;0…m` around `main.rs`.
const EZA_COLORS_PROBE: &str = "reset:*.rs=38;2;255;0;0";

/// Substring we expect to see in eza's output when the probe is accepted.
/// Substring-matching (rather than whole-escape matching) keeps the test
/// robust against eza wrapping the code with additional SGR attributes
/// (`\x1b[00;38;2;255;0;0m` is still a pass).
const EXPECTED_TRUECOLOR_FRAGMENT: &[u8] = b"38;2;255;0;0";

#[test]
fn eza_accepts_truecolor_in_eza_colors_env_var() {
    // Skip cleanly if eza isn't on PATH — project-level integration tests
    // must not be flaky on CI hosts without it.
    let eza_path = match slate_cli::detection::command_path("eza") {
        Some(path) => path,
        None => {
            println!(
                "eza not available on PATH; skipping empirical truecolor smoke \
                 (see RESEARCH §Pitfall 3)"
            );
            return;
        }
    };

    let tempdir = TempDir::new().expect("create tempdir");
    let rs_path = tempdir.path().join("main.rs");
    std::fs::write(&rs_path, b"// slate truecolor smoke fixture\n").expect("write fixture main.rs");

    // Invoke eza with only the env vars we care about. We do NOT mutate the
    // host process environment — all inputs ride on the Command. NO_COLOR is
    // explicitly cleared in case the host exported it: leaving it set would
    // make eza strip all escapes and the probe would falsely appear rejected.
    let output = Command::new(&eza_path)
        .arg("--color=always")
        .arg(tempdir.path())
        .env("EZA_COLORS", EZA_COLORS_PROBE)
        .env_remove("NO_COLOR")
        .output()
        .expect("spawn eza");

    assert!(
        output.status.success(),
        "eza --color=always failed on fixture dir\nstatus: {:?}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr),
    );

    // Inspect raw bytes — ANSI escapes must be matched exactly, not via a
    // lossy UTF-8 round trip (which would fold any malformed sequence to
    // `U+FFFD` and mask the failure).
    let stdout = output.stdout.as_slice();

    let truecolor_present = find_bytes(stdout, EXPECTED_TRUECOLOR_FRAGMENT).is_some();
    let main_rs_present = find_bytes(stdout, b"main.rs").is_some();

    assert!(
        main_rs_present,
        "eza stdout did not list `main.rs` — smoke test cannot validate \
         truecolor handling. stdout:\n{}",
        String::from_utf8_lossy(stdout),
    );

    if !truecolor_present {
        // Contingency trigger — RESEARCH §Pitfall 3 / Assumption A1 landed.
        // The panic message names the pitfall and points the executor /
        // reviewer at the fallback plan: move the file-type layer into
        // `EzaAdapter::render_eza_yaml` (eza theme.yml) and surface the
        // contingency to the user via the Plan 16-07 UAT checkpoint.
        panic!(
            "EZA TRUECOLOR REJECTED — RESEARCH §Pitfall 3 / Assumption A1 landed. \
             Contingency: move the file-type layer into EzaAdapter::render_eza_yaml \
             (eza theme.yml) instead of EZA_COLORS env var. See RESEARCH §Pitfall 3 \
             fallback. Plan 16-07 must surface to user via Task 3 checkpoint.\n\n\
             stdout bytes (lossy):\n{}",
            String::from_utf8_lossy(stdout),
        );
    }

    // Passed — Assumption A1 empirically confirmed on this host.
    println!(
        "eza accepted 38;2;255;0;0 truecolor in EZA_COLORS — Assumption A1 confirmed. \
         eza path: {}",
        eza_path.display()
    );
}

/// Byte-level substring search. `Vec::windows` over `u8` keeps the test
/// dependency-free — we could not rely on `memchr` here without adding a
/// crate, and the haystack is short (< 1 KiB).
fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}
