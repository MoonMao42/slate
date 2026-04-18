---
phase: 16
plan: 07
subsystem: integration-testing
tags: [integration-test, ls-colors, eza-colors, truecolor, phase-gate, uat]
dependency_graph:
  requires:
    - 16-02 (LS_COLORS / EZA_COLORS rendering module)
    - 16-04 (SharedShellModel wiring of LS / EZA exports into env.{zsh,bash,fish})
    - 16-06 (new-shell reminder plumbing in CLI handlers)
  provides:
    - end-to-end regression guard for the LS_COLORS / EZA_COLORS pipeline
    - empirical confirmation of Assumption A1 (eza honours 38;2;R;G;B in EZA_COLORS)
    - shell-syntax parse guard for generated env.zsh (Pitfall 5)
  affects:
    - tests/ (two new integration crates)
tech_stack:
  added:
    - regex-based multi-line assertion helpers for shell-quoted env vars
    - byte-level substring matching for ANSI escape verification
  patterns:
    - ConfigManager::write_shell_integration_file driven via SlateEnv::with_home + TempDir
      for full-pipeline integration tests (no global env mutation)
    - Command::env / Command::env_remove for per-process env var isolation in
      external-binary smoke tests
key_files:
  created:
    - tests/ls_colors_integration.rs
    - tests/eza_truecolor_smoke.rs
  modified: []
decisions:
  - "Integration tests target the public ConfigManager pipeline, not the `pub(crate)` render functions, to catch wiring + shell-quoting regressions that unit tests miss."
  - "Empirical eza smoke test runs by default (no `#[ignore]`) and skips gracefully when eza is absent — project guideline: tests must not be flaky, but must actually run where the tool is available."
  - "NO_COLOR is explicitly cleared in the eza Command env so an inherited NO_COLOR on the host cannot mask a rejected truecolor code as a false negative."
metrics:
  duration: ~25 min
  tasks_completed: 2 (of 3; Task 3 is a human-UAT checkpoint)
  files_created: 2
  tests_added: 5
  lines_added: 408
  completed_date: 2026-04-18
---

# Phase 16 Plan 07: LS/EZA Integration Tests + UAT Summary

End-to-end integration coverage for the `LS_COLORS` / `EZA_COLORS` shell-integration pipeline and the empirical verification of `eza`'s truecolor acceptance (RESEARCH §Pitfall 3 / Assumption A1).

## What shipped

Two new integration-test crates under `tests/`, exercising the managed shell-integration pipeline end-to-end:

- **`tests/ls_colors_integration.rs`** — four tests, all passing:
  - `env_zsh_contains_ls_colors_and_eza_colors_exports` — POSIX `export LS_COLORS='…'` and `export EZA_COLORS='…'` lines both present with `rs=0:no=0:` / `reset:` prefixes and at least one `38;2;R;G;B` truecolor escape; no `38;5;` regression.
  - `env_fish_uses_set_gx_for_ls_eza_colors` — fish uses `set -gx LS_COLORS 'rs=0:no=0:…'` and `set -gx EZA_COLORS 'reset:…'`; file contains zero `export LS_COLORS` / `export EZA_COLORS` strings (POSIX syntax forbidden in fish file).
  - `env_zsh_passes_shell_syntax_check` — `zsh -n` parses the generated `env.zsh` cleanly (exit 0); guards Pitfall 5 (shell-quoting regressions). Skips gracefully when `zsh` is absent.
  - `env_zsh_round_trips_classifier` — for every entry in `extension_map()`, the ANSI code inside `LS_COLORS` equals `PaletteRenderer::rgb_to_ansi_24bit(palette.resolve(classify("fixture.{ext}", FileKind::Regular)))` — strongest possible guarantee the classifier and env-var pipeline cannot drift.

- **`tests/eza_truecolor_smoke.rs`** — one test, passing:
  - `eza_accepts_truecolor_in_eza_colors_env_var` — spawns `eza --color=always` against a tempdir holding `main.rs` with `EZA_COLORS=reset:*.rs=38;2;255;0;0`, byte-level substring-matches the stdout for `38;2;255;0;0`. On an absent `eza`, prints a skip line and returns (not `#[ignore]`d). On a rejected outcome, panics with the `EZA TRUECOLOR REJECTED — RESEARCH §Pitfall 3 / Assumption A1 landed` contingency message to trigger the theme.yml fallback plan.

## eza truecolor empirical result — Assumption A1

**PASSED.** `eza` v0.23.4 on macOS 26.4 (Darwin 25.4.0) accepts `38;2;R;G;B` directly in `EZA_COLORS`, with `reset:*.rs=38;2;255;0;0` yielding the expected truecolor escape around `main.rs` in `--color=always` output.

- Binary path: `/opt/homebrew/bin/eza`
- Version: `v0.23.4 [+git]`
- Platform: macOS 26.4, build 25E246 (Darwin 25.4.0, arm64)
- Probe output: contained byte sequence `38;2;255;0;0` as expected.

Assumption A1 empirically confirmed for the phase. The theme.yml fallback contingency is **not** required; Phase 16's env-var strategy stands as specified.

## Phase gate

All three phase-gate checks pass at HEAD:

```
cargo fmt --check               # 0
cargo clippy --all-targets -- -D warnings  # 0
cargo test                      # 715 passed, 0 failed, 1 ignored (doc-test)
```

## REQ-ID coverage

All six Phase 16 requirements have landed with tests:

| REQ-ID | Area                        | Coverage source                                                                 |
|--------|-----------------------------|----------------------------------------------------------------------------------|
| LS-01  | `LS_COLORS` from palette    | `src/adapter/ls_colors.rs` (8 unit tests) + `tests/ls_colors_integration.rs` (4) |
| LS-02  | `EZA_COLORS` from palette   | `src/adapter/ls_colors.rs` (4 unit tests) + `tests/eza_truecolor_smoke.rs` (1)   |
| LS-03  | BSD-ls capability message   | `src/cli/preflight.rs` (`preflight_emits_ls_capability_*`, 5+ tests) + `src/config/tracked_state.rs` (4 flag-file tests) |
| UX-01  | `ApplyOutcome` signal       | `src/adapter/mod.rs` + `src/adapter/registry.rs` (`requires_new_shell` aggregator tests, 6+) |
| UX-02  | Reminder wiring in handlers | `src/cli/{setup,theme,font,config}.rs` handler-level tests (10+ `*_emits_reminder_*` tests) |
| UX-03  | Platform-aware copy         | `src/brand/language.rs` (`new_shell_reminder_*`, 5 tests)                        |

Anti-pattern sweep (`grep '38;5;' src/adapter/ls_colors.rs src/config/shell_integration.rs`) returns **two** lines — both inside the `ls_colors` anti-regression unit test that asserts the absence of 256-colour codes. **Zero** production paths emit `38;5;`.

## Deviations from plan

None. The plan executed exactly as written: two integration-test files, one empirical smoke test, one human UAT checkpoint. No Rule 1 / 2 / 3 fixes were needed — the base already had all prerequisites (Plans 16-01 through 16-06) merged.

## UAT: pending orchestrator

Task 3 is a human-verification checkpoint (`type="checkpoint:human-verify"`). This executor stops at that boundary per `autonomous: false`. The orchestrator will:

1. Prompt the user to run the UAT checklist from `16-07-PLAN.md` §Task 3 (steps 1–8: theme switch with reminder ordering, `gls` / `eza` palette check, `--auto --quiet` suppression, picker suppression, `config set` emission, BSD-`ls` one-shot flag, smoke-test replay, phase-gate replay).
2. Capture the user's verbatim response (one of: "approved — phase 16 ships as specified" / "approved with eza contingency — execute fallback plan" / issue descriptions).
3. Append that response and any screenshot / pasted-output evidence under the **UAT evidence** section below.

### UAT evidence

> **Placeholder — to be filled by orchestrator after human UAT.**
>
> Expected content:
> - User's verbatim sign-off line.
> - Pasted output from the `ls` / `gls` / `eza` side-by-side check (or screenshots).
> - Noted platform (macOS tab opened via ⌘N, Ghostty / Terminal.app / iTerm2, shell variant).
> - Any issues raised + follow-up disposition.

## Plan-level metrics

| Metric                       | Value                 |
|------------------------------|-----------------------|
| Files created                | 2                     |
| Tests added (integration)    | 5                     |
| LOC added                    | 408                   |
| Phase-gate test count at HEAD | 715 passing, 0 failing, 1 ignored (pre-existing doc-test) |
| eza smoke result             | PASS (truecolor accepted) |
| Host for empirical smoke     | macOS 26.4 arm64, eza v0.23.4 |

## Commits

| # | Hash     | Subject                                                                  |
|---|----------|--------------------------------------------------------------------------|
| 1 | e5ff3d1  | test(16-07): add end-to-end LS_COLORS / EZA_COLORS integration suite     |
| 2 | 0893928  | test(16-07): add eza truecolor EZA_COLORS empirical smoke                |

## Next milestone

After UAT sign-off, Phase 16 closes. Next milestone per `ROADMAP.md`: **Phase 17 — Editor Adapter Research Spike** (nvim / helix / zed palette bridge scoping) or the v2.2 milestone close if Phase 17 is descoped from this release.

## Self-Check: PASSED

- `tests/ls_colors_integration.rs`: FOUND
- `tests/eza_truecolor_smoke.rs`: FOUND
- `.planning/phases/16-cli-tool-colors-new-terminal-ux/16-07-SUMMARY.md`: FOUND
- Commit `e5ff3d1` (Task 1): FOUND on `worktree-agent-a95e91a8`
- Commit `0893928` (Task 2): FOUND on `worktree-agent-a95e91a8`
- `cargo fmt --check`: PASSED
- `cargo clippy --all-targets -- -D warnings`: PASSED
- `cargo test`: 715 passed / 0 failed / 1 ignored
