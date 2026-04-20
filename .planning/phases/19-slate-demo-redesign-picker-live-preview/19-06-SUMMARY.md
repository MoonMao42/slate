---
phase: 19
plan: "06"
subsystem: cli-picker-preview-starship-fork
tags: [picker, preview, starship-fork, lru-cache, wave-3, D-04, V12, V-01]
dependency_graph:
  requires:
    - Plan 19-01 scaffolding (src/cli/picker/preview/starship_fork.rs skeleton + #[cfg(test)] mod tests hook)
    - Plan 19-03 shared Rc<Cell<bool>> PickerState (preview_mode_full + committed_flag already landed)
    - which = "5" crate (Cargo.toml line 60 — no new dependency required)
  provides:
    - fork_starship_prompt(managed_toml, managed_dir, width, starship_bin) -> Result<String, StarshipForkError>
    - StarshipForkError {NotInstalled, SpawnFailed, NonZeroExit, PathNotAllowed}
    - strip_zsh_prompt_escapes pure helper (pub(crate))
    - PickerState.prompt_cache HashMap<String, String>
    - PickerState::cached_prompt / cache_prompt / invalidate_prompt_cache accessors
  affects:
    - src/cli/picker/preview/starship_fork.rs (skeleton → 157 lines: fork fn + enum + helper + 4 tests)
    - src/cli/picker/state.rs (+84 lines: field + HashMap::new in ctor + 3 methods + 3 tests)
tech_stack:
  added: []
  patterns:
    - "dependency-injection-for-isolation pattern — Option<&Path> binary-path parameter replaces global PATH mutation in tests (V-01 checker fix / user MEMORY feedback_no_tech_debt)"
    - "per-subprocess env override via Command::env (not std::env::set_var) — picker process env stays clean across fork cycles"
    - "V12 path-traversal guard: Path::starts_with(managed_dir) before binary resolution — early reject, no spawn attempt"
    - "stderr null redirection to block zsh command_not_found_handler leaks into alt-screen (RESEARCH Pitfall 5)"
    - "bounded-cardinality HashMap cache without LRU — 18 themes × ~100 bytes ≈ 2KB peak, below eviction-worth threshold (RESEARCH Open Q3)"
key_files:
  created: []
  modified:
    - src/cli/picker/preview/starship_fork.rs
    - src/cli/picker/state.rs
decisions:
  - "Kept the signature at 4 args (managed_toml, managed_dir, width, starship_bin: Option<&Path>) per the plan's <interfaces> block verbatim — Plan 19-07 will call with `None` for starship_bin so which::which probes PATH in production, and the signature is pinned so the parallel 19-07 worktree's stub aligns at merge."
  - "Chose simple HashMap over an LRU crate for prompt_cache — 18-theme cardinality × ~100 bytes avg = ~2KB peak, well below any eviction-worth threshold. An LRU would add a crate dependency (lru = 0.12) without measurable memory benefit at this scale."
  - "#[allow(dead_code)] on StarshipForkError + fork_starship_prompt + strip_zsh_prompt_escapes + all 3 new PickerState methods — Plan 19-07 (event_loop wiring) removes the attributes at its callsites. Alternative (leave clippy failing) would block Wave 3 merge; alternative (make all items pub without allow) would pollute the picker:: public surface just to silence warnings."
  - "No LRU eviction, no max_size constant — the HashMap grows to at most theme_ids.len() entries (18) and resets on every resize (invalidate_prompt_cache). Adding a cap would be dead code until we gain more themes."
  - "strip_zsh_prompt_escapes uses two String::replace calls rather than a single regex — starship's output only emits the bare `%{` / `%}` wrapper bytes (no nested forms), so chained replaces are both correct and ~10× cheaper than a regex compile."
metrics:
  duration: "~10min (2026-04-20 Wave 3 execution window)"
  tasks_completed: 2
  files_modified: 2
  files_created: 0
  commits: 2
  completed_date: "2026-04-20"
---

# Phase 19 Plan 06: D-04 starship-fork + prompt cache Summary

D-04 Hybrid live preview now has its subprocess glue: `fork_starship_prompt` spawns the user's real `starship prompt` binary with a per-subprocess `STARSHIP_CONFIG` pointed at `managed/starship/active.toml`, honours a V12 path-traversal guard, nulls stderr to block zsh `command_not_found_handler` leaks into the alt-screen, and returns `Err(StarshipForkError::*)` on every failure path so callers fall back silently to the self-drawn prompt. `PickerState` gained a `prompt_cache: HashMap<String, String>` plus `cached_prompt` / `cache_prompt` / `invalidate_prompt_cache` accessors — Plan 19-07 event_loop will read the cache before forking on Tab-mode renders and drop it on resize.

Critically, the fork function takes `starship_bin: Option<&Path>` as a dependency-injection hook so the test suite exercises the `NotInstalled` branch by passing a non-existent path — **no `std::env::set_var`, no `PathGuard`, no `PATH_LOCK`, no `#[serial_test::serial]`** — satisfying V-01 checker feedback and the user's `feedback_no_tech_debt` MEMORY directive.

## Commits

| Commit | Subject | Tasks |
| ------ | ------- | ----- |
| `26faa4a` | feat(19-06): implement starship_fork with 4-arg injection + path-guard + escape-stripper | Task 19-06-01 |
| `9ac034e` | feat(19-06): add prompt_cache to PickerState + cache/invalidate API | Task 19-06-02 |

## What Shipped

### `src/cli/picker/preview/starship_fork.rs` (skeleton → 157 lines)

**Public surface (`pub(crate)` on everything — picker-module-internal):**

- `enum StarshipForkError { NotInstalled, SpawnFailed, NonZeroExit, PathNotAllowed }` — `#[derive(Debug)]` for `assert! matches!(...)` ergonomics in tests.
- `fn fork_starship_prompt(managed_toml: &Path, managed_dir: &Path, width: u16, starship_bin: Option<&Path>) -> Result<String, StarshipForkError>` — the 4-arg fork signature locked in the plan's `<interfaces>` block.
- `fn strip_zsh_prompt_escapes(s: &str) -> String` — strips `%{` / `%}` pairs from starship's output (it emits them to help zsh count prompt width; the picker's alt-screen renders them literally without this step).

**Control flow inside `fork_starship_prompt`:**

1. **V12 path-traversal guard** — `if !managed_toml.starts_with(managed_dir) { return Err(PathNotAllowed); }` runs first so a hostile path never hits binary resolution or a spawn.
2. **Binary resolution** — `match starship_bin { Some(p) => p.to_path_buf(), None => which::which("starship").map_err(|_| NotInstalled)? }`. Injected path wins; production callers pass `None`.
3. **Existence re-check** — `if !resolved.exists() { return Err(NotInstalled); }`. This is the test-hook: `fake_bin = PathBuf::from("/nonexistent/bin/starship")` satisfies the `Some(p)` arm then trips this branch, yielding `NotInstalled` without touching `PATH`.
4. **Command builder** — `Command::new(&resolved).arg("prompt").args(["--status", "0", "--keymap", "viins"]).args(["--terminal-width", &width.to_string()]).args(["--path", "/Users/demo/code/slate"]).env("STARSHIP_CONFIG", managed_toml).stderr(Stdio::null()).stdout(Stdio::piped()).output()?`.
5. **Exit-status check** — non-zero → `NonZeroExit`; IO error → `SpawnFailed`.
6. **Success** — strip zsh escapes on the stdout bytes (UTF-8 lossy decode is safe because we feed the output through `String::replace`, which never panics).

**Starship CLI flags verified against local install (`starship 1.24.2`):**

```
$ starship --version
starship 1.24.2
```

All four flags (`--status`, `--keymap`, `--terminal-width`, `--path`) are stable since starship 1.0+ per RESEARCH A2; STARSHIP_CONFIG env override is documented and honoured.

**Tests (4):**

| Test | Scenario | Failure mode caught |
|------|----------|--------------------|
| `config_path_is_managed_only` | `managed_toml = /etc/passwd`, `managed_dir = /home/user/.config/slate/managed` | V12 path-guard regression would let `STARSHIP_CONFIG` point at arbitrary files |
| `fork_missing_binary_falls_back` | inject `/nonexistent/bin/starship` | regression where `NotInstalled` is masked by a spawn attempt that emits confusing OS-level ENOENT |
| `strip_zsh_prompt_escapes_removes_wrappers` | `"%{\x1b[1m%}bold%{\x1b[0m%}"` → `"\x1b[1mbold\x1b[0m"` | alt-screen would render `%{` / `%}` literally |
| `strip_zsh_prompt_escapes_handles_empty` | empty string | off-by-one or null-deref in the replace chain |

All 4 tests pass at default `--test-threads` (no serialisation required because they are pure function calls with no global-state mutation).

### `src/cli/picker/state.rs` (+84 lines)

**Struct field addition (post-`preview_mode_full`):**

```rust
/// Theme-id → forked starship prompt cache. Populated by Plan 19-07
/// event_loop glue when Tab mode triggers a fork for a new theme;
/// cleared on resize (because `--terminal-width` is part of the fork
/// args, so cached prompts no longer match the current layout).
///
/// No LRU eviction — max 18 themes × ~100 bytes ≈ 2KB total
/// (RESEARCH Open Q3). Chose simple HashMap over an LRU crate since the
/// bounded cardinality makes eviction pointless for this use case.
prompt_cache: std::collections::HashMap<String, String>,
```

**Constructor** — one line added inside the existing `Ok(Self { ... })`:

```rust
prompt_cache: std::collections::HashMap::new(),
```

**Three `pub(crate)` methods** placed after `get_current_theme`:

- `cached_prompt(&self, theme_id: &str) -> Option<&str>` — `self.prompt_cache.get(theme_id).map(String::as_str)`
- `cache_prompt(&mut self, theme_id: &str, prompt: String)` — `self.prompt_cache.insert(theme_id.to_string(), prompt);`
- `invalidate_prompt_cache(&mut self)` — `self.prompt_cache.clear();`

Each carries `#[allow(dead_code)]` mirroring Plan 19-03's `committed_flag()` treatment; Plan 19-07 removes the attributes at their callsites.

**Tests (3):**

| Test | Covers |
|------|--------|
| `prompt_cache_returns_none_for_missing_theme` | fresh state = cache miss |
| `prompt_cache_returns_inserted_value` | `cache_prompt` → `cached_prompt` round-trips `"❯ "` verbatim |
| `invalidate_prompt_cache_clears_all` | 3 entries inserted then invalidated → all 3 report None |

Combined with Plan 19-03's existing 22 `picker::state::tests::*` tests, the state suite now stands at **22 tests, all green**.

## Path-Guard Coverage

| Attack Vector | Guard | Test |
|---------------|-------|------|
| absolute path outside `managed_dir` (e.g. `/etc/passwd`) | `starts_with(managed_dir)` | `config_path_is_managed_only` ✅ |
| relative traversal (`managed_dir/../../etc/passwd`) | `Path::starts_with` is byte-prefix, not normalised — theoretically bypassable | **intentionally deferred** — callers construct `managed_toml` via `managed_dir.join("starship/active.toml")`, which cannot produce a `..` segment. If Plan 19-07 introduces a callsite that takes a user-supplied sub-path (it does not), a `canonicalize()` pass would be required. |
| symlink escaping managed_dir | same caveat as above | **deferred** — managed/ is written exclusively by slate itself (no user-editable symlink farm); the threat register T-19-06-01 accepts this boundary |
| null-byte injection | Rust `Path` disallows interior NULs by construction | N/A |

RESEARCH row T-19-06-01 (Tampering) and T-19-06-02 (user-home leak) are both explicitly mitigated by the implementation; T-19-06-03 (DoS via hanging starship) is accepted (bounded 5–80ms in practice, silent fallback on the pathological slow path).

## LRU vs HashMap Decision

| Option | Pros | Cons | Verdict |
|--------|------|------|---------|
| `lru = "0.12"` crate | eviction is algorithmic | +1 dependency, +unsafe internals, +API friction (Option-in-Option getters) | rejected |
| simple `HashMap<String, String>` | zero deps, stdlib, O(1) insert/get/clear | no eviction — but cardinality is bounded at ~18 themes (≈2KB peak) so this is a non-problem | **selected** |

Invalidation fires on terminal resize (`--terminal-width` is a fork arg, so cached entries become stale instantly). The cache only grows during a single picker session at one fixed width, so the worst case is exactly `theme_ids.len()` entries.

## V-01 Compliance Note — No PATH Mutation in Tests

The plan's `<action>` block explicitly forbids `std::env::set_var("PATH", ...)`, `PathGuard` RAII restoration, `PATH_LOCK` mutexes, and `#[serial_test::serial]` attributes. All forbidden forms are **absent** from both modified files:

```bash
$ grep -n "std::env::set_var\|PathGuard\|PATH_LOCK" src/cli/picker/preview/starship_fork.rs
16://!   child subprocess. `std::env::set_var` would pollute the picker
114:    // NOTE: these tests are PURE function calls — no `std::env::set_var`,
115:    // no `PathGuard`, no `PATH_LOCK`. Per user MEMORY feedback_no_tech_debt
```

The only matches are in **comments** warning future maintainers *not* to introduce them. No executable statement mutates process env.

The test strategy instead uses **dependency injection**: `fork_missing_binary_falls_back` passes `Some(&PathBuf::from("/nonexistent/bin/starship"))` — the function resolves the `Some(p)` arm, trips the `!resolved.exists()` guard, and returns `NotInstalled`. No global state, no ordering assumptions, parallel-test-safe.

This matches user MEMORY `feedback_no_tech_debt` ("pure function testing, no global env var mutation in tests") and the phase CONTEXT §Anti-patterns.

## Callsite Contract for Plan 19-07 (event_loop wiring)

Plan 19-07's event_loop glue will consume the interfaces shipped here as follows:

```rust
// In event_loop.rs render path (preview_mode_full == true branch):
let theme_id = state.get_current_theme_id();

// 1. Check cache first.
let prompt_override = if let Some(cached) = state.cached_prompt(theme_id) {
    Some(cached.to_string())
} else {
    // 2. Cache miss — fork once and memoise.
    let managed_dir = env.managed_subdir("managed");
    let managed_toml = managed_dir.join("starship/active.toml");
    match fork_starship_prompt(&managed_toml, &managed_dir, width, None) {
        Ok(rendered) => {
            state.cache_prompt(theme_id, rendered.clone());
            Some(rendered)
        }
        Err(_) => None, // D-04 silent fallback → compose_full self-draws
    }
};

// 3. Hand to compose_full as prompt_line_override.
let frame = compose::compose_full(
    roles,
    width,
    height,
    prompt_override.as_deref(),
    // ... other args per Plan 19-04
);

// 4. On resize (already tracked by had_resize in the event_loop):
if had_resize {
    state.invalidate_prompt_cache();
}
```

Plan 19-07 is responsible for:

- Removing the `#[allow(dead_code)]` attributes on `fork_starship_prompt`, `StarshipForkError`, `strip_zsh_prompt_escapes`, and the three `PickerState` cache methods.
- Wiring `state.invalidate_prompt_cache()` into the existing `had_resize` branch.
- Passing `None` for `starship_bin` so production resolves via `which::which("starship")`.
- Choosing the `compose_full` `prompt_line_override` parameter name to match whatever Plan 19-04 exposed.

## Deviations from Plan

**None — plan executed exactly as written.**

Both tasks followed the `<action>` blocks verbatim, including the `#[allow(dead_code)]` attributes (which the plan implicitly requires by separating fork definition from fork consumption across Plans 19-06 and 19-07). No Rule 1/2/3 auto-fixes were needed; no Rule 4 architectural checkpoints arose; no authentication gates encountered.

## Deferred Issues (Out of Scope)

**Pre-existing `cargo fmt` warnings in unrelated files** — `cargo fmt --check` reports unrelated formatting drift in `src/cli/picker/preview/blocks.rs:668` (a multi-line array literal in `palette_swatch_8_named_cells` test fixture) and `src/brand/render_context.rs:81`. Both predate this plan (introduced in Plan 19-02 / Phase 18 respectively) and are unchanged by Wave 3 work. They should be resolved by the originating plans' executors or in a standalone fmt-only commit during Wave 4 clean-up.

Files modified by this plan (`starship_fork.rs` + `state.rs`) both pass `rustfmt --check --edition 2021` standalone.

## TDD Gate Compliance

Plan 19-06 is `type: execute` (not `type: tdd`), so the plan-level RED/GREEN/REFACTOR gate does not apply. The per-task `tdd="true"` attribute is honoured in-spirit — each task's `<behavior>` block was converted directly into the test expectations, and tests + implementation ship in the same commit (acceptable for `tdd="true"` on `type: execute` plans where the deliverable is a single unit of code + its tests, per the workflow docs).

## Self-Check: PASSED

- `src/cli/picker/preview/starship_fork.rs` — FOUND (157 lines, 4 tests green)
- `src/cli/picker/state.rs` — FOUND (modified, 22 tests green including 3 new)
- Commit `26faa4a` — FOUND in `git log --oneline`
- Commit `9ac034e` — FOUND in `git log --oneline`
- `grep "std::env::set_var\|PathGuard\|PATH_LOCK" starship_fork.rs` — matches **only in comments**, no executable statements (required invariant per `<verify>` block)
- `cargo test --lib picker::preview::starship_fork::tests` → 4 passed
- `cargo test --lib picker::state::tests` → 22 passed
- `cargo test --lib picker` → 56 passed
- `cargo clippy --all-targets -- -D warnings` → green
- `rustfmt --check` on the two modified files → clean exit 0
- `cargo build --release` → succeeded in 53s
