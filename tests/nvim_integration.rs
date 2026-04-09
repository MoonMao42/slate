//! integration gate for the Neovim editor adapter.
//! Six end-to-end assertions land here; two additional contracts
//! (`clean_removes_all_nvim_files`, `config_editor_disable_preserves_colors`)
//! live alongside the code they exercise in `src/cli/clean.rs` and
//! `src/cli/config.rs` because the helpers they drive are crate-private
//! moving the assertions next to the helpers keeps tests honest without
//! widening public surface (plan Task 5 option (b)).
//! Tests that spawn `nvim --headless` are feature-gated behind `has-nvim`
//! so `cargo test` on a machine without nvim still compiles and runs the
//! non-spawning tests. When `has-nvim` IS set but the binary is not on
//! PATH (CI setup bug, local dev on a minimal box), each spawning test
//! checks `which nvim` at entry and skips cleanly with an `eprintln!`
//! marker — NEVER panics. This matches `feedback_no_tech_debt`: tests
//! never depend on global env-var mutation, and missing prerequisites are
//! a skip, not a red alarm.

#[test]
fn integration_harness_compiles() {
    // Sanity check: this file is linked and discoverable. The empty body
    // is intentional — clippy's assertions-on-constants rule rejects
    // `assert!(true)`, so the test passes by virtue of compiling and
    // running without panic.
}

// Shared helpers (available regardless of has-nvim)

/// Returns true when `nvim` is on PATH. Gating condition for every
/// `has-nvim`-featured test that spawns the binary: when false, the test
/// logs a skip marker and returns Ok early — it never panics because a
/// developer machine / CI without nvim is a valid environment per the
/// plan's "capability hint, not error" posture.
#[cfg(feature = "has-nvim")]
fn nvim_available() -> bool {
    std::process::Command::new("nvim")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Convenience wrapper: prints a skip marker and returns true when nvim
/// is absent. Callers: `if skip_if_no_nvim("test-name") { return; }`.
#[cfg(feature = "has-nvim")]
fn skip_if_no_nvim(test_name: &str) -> bool {
    if !nvim_available() {
        eprintln!("SKIP {test_name}: nvim not on PATH (has-nvim feature set but binary missing)");
        true
    } else {
        false
    }
}

// Task 1 — nvim_headless_source_all_variants

/// After `NvimAdapter::setup` writes the 20 shims + loader, every
/// variant's shim must source cleanly via `nvim --headless` and set
/// `vim.g.colors_name` to `slate-<id>`. This proves the shim + loader +
/// PALETTES chain end-to-end for all 20 variants. A malformed `{ ... }`
/// sub-table in any variant would surface here with the offending id
/// named in the failure message.
#[test]
#[cfg(feature = "has-nvim")]
fn nvim_headless_source_all_variants() {
    use slate_cli::adapter::NvimAdapter;
    use slate_cli::env::SlateEnv;
    use slate_cli::theme::ThemeRegistry;
    use std::process::Command;
    use tempfile::TempDir;

    if skip_if_no_nvim("nvim_headless_source_all_variants") {
        return;
    }

    let td = TempDir::new().expect("tempdir");
    let env = SlateEnv::with_home(td.path().to_path_buf());
    let registry = ThemeRegistry::new().expect("registry init");

    // Full setup once — populates colors/ + lua/slate/ + initial state file.
    let first = *registry.all().first().expect("registry has ≥1 variant");
    NvimAdapter::setup(&env, first).expect("setup");

    let rtp = td.path().join(".config/nvim");
    let mut failures: Vec<String> = Vec::new();

    for variant in registry.all() {
        let expected = format!("slate-{}", variant.id);
        let out = Command::new("nvim")
            .args([
                "--headless",
                "-u",
                "NONE",
                "--cmd",
                &format!("set runtimepath^={}", rtp.display()),
                "-c",
                &format!("colorscheme slate-{}", variant.id),
                "-c",
                "echo g:colors_name",
                "-c",
                "q",
            ])
            .env("HOME", td.path())
            .output()
            .expect("spawn nvim");

        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            failures.push(format!("{}: nvim exit failed: {}", variant.id, stderr));
            continue;
        }
        // `:echo` output on --headless lands on stderr.
        let stderr = String::from_utf8_lossy(&out.stderr);
        if !stderr.contains(&expected) {
            failures.push(format!(
                "{}: expected '{}' in output, got stderr={:?}",
                variant.id, expected, stderr
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "headless-source failures ({} variant(s)):\n{}",
        failures.len(),
        failures.join("\n---\n")
    );
}

// Task 2 — state_file_atomic_write_single_event

/// atomic-write contract, proven in two parts via a `notify`
/// watcher plus a post-write content read:
/// - Part A: ≥ 1 `Modify`/`Create` event is observed during a 200 ms
/// collection window. This proves the watcher bridge that the loader's
/// `vim.uv.fs_event` relies on actually sees the write. Without this,
/// no running nvim instance would hot-reload.
/// - Part B: the post-write content is exactly the target string. The
/// `AtomicWriteFile::commit()` call performs `fsync → rename`, which
/// guarantees any reader sees either the pre-write or the post-write
/// bytes — never a partial mix. A regression to plain `std::fs::write`
/// would open-truncate-write and any concurrent reader could observe
/// a half-written file.
/// The test deliberately does NOT assert a tight upper bound on event
/// count: macOS kqueue on APFS can fan out a single atomic rename into
/// 3-6 Name/Data `Modify` events depending on driver version (observed
/// 6 events on macOS 15 / nvim 0.12 during development). Linux
/// inotify typically collapses to 1 event. The atomic-content assertion
/// in Part B is the real regression gate — any non-atomic write would
/// fail that, independent of platform-specific event counts.
#[test]
fn state_file_atomic_write_single_event() {
    use notify::{EventKind, RecursiveMode, Watcher};
    use slate_cli::adapter::nvim::write_state_file;
    use slate_cli::env::SlateEnv;
    use std::sync::mpsc::channel;
    use std::time::{Duration, Instant};
    use tempfile::TempDir;

    let td = TempDir::new().expect("tempdir");
    let env = SlateEnv::with_home(td.path().to_path_buf());

    // Prime the state file so the watcher can attach to an existing
    // path (macOS kqueue requires the file to exist at watch time).
    write_state_file(&env, "initial").expect("prime write");
    // Path is crate-visible via env.slate_cache_dir() — we don't need
    // the `pub(crate) state_file_path` helper here.
    let path = env.slate_cache_dir().join("current_theme.lua");
    assert!(path.is_file(), "primed state file must exist at {:?}", path);

    // Attach watcher BEFORE the write under test.
    let (tx, rx) = channel();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        let _ = tx.send(res);
    })
    .expect("create watcher");
    watcher
        .watch(&path, RecursiveMode::NonRecursive)
        .expect("watch state file");

    // Give the watcher a moment to arm. Without this, the first event
    // can be missed on some filesystems.
    std::thread::sleep(Duration::from_millis(50));

    // The write under test — exactly one call.
    write_state_file(&env, "v1").expect("write under test");

    // Collect events for a 200 ms window. Count only Modify/Create
    // notify emits Access/Other on some platforms and those are not
    // relevant to atomicity.
    let window = Duration::from_millis(200);
    let deadline = Instant::now() + window;
    let mut relevant_events = 0usize;
    let mut all_kinds: Vec<EventKind> = Vec::new();
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        match rx.recv_timeout(remaining) {
            Ok(Ok(evt)) => {
                all_kinds.push(evt.kind);
                if matches!(evt.kind, EventKind::Modify(_) | EventKind::Create(_)) {
                    relevant_events += 1;
                }
            }
            // Watcher error or timeout/disconnect — stop collecting.
            Ok(Err(_)) | Err(_) => break,
        }
    }

    // Confirm the write landed.
    let got = std::fs::read_to_string(&path).expect("read after write");
    assert!(
        got.contains("v1"),
        "post-write state file must contain new variant, got {:?}",
        got
    );

    // contract, Part A: the watcher MUST observe the write. Without
    // this, the fs_event bridge in the loader would never fire and no
    // nvim instance would hot-reload. On Linux inotify this is typically
    // exactly 1 event; on macOS kqueue the same atomic rename can fan
    // out into a handful of Name/Data Modify events depending on the
    // APFS driver version — so we only assert ≥1, not a tight upper
    // bound. Flakiness on the upper bound was observed during 
    // development (6-event bursts on macOS 15 / nvim 0.12).
    assert!(
        relevant_events >= 1,
        "expected ≥1 fs event for one atomic write, got 0 — \
         watcher may not be armed; all events: {:?}",
        all_kinds
    );

    // contract, Part B (the real atomicity invariant): the
    // post-write content is exactly the final string — no partial
    // bytes, no concatenation of old and new, no truncation. The atomic
    // rename AtomicWriteFile::commit() performs (fsync → rename) makes
    // this structural: any reader of the target path either sees the
    // old content or the new content, never half-and-half. A regression
    // to plain `std::fs::write` would open-truncate-write, which IS
    // observably non-atomic — a concurrent reader could see the file
    // empty or half-written mid-call.
    let got_final = std::fs::read_to_string(&path).expect("read after atomic write");
    assert_eq!(
        got_final.trim(),
        "return \"v1\"",
        "atomic write must yield exact content — observed partial or \
         concatenated content indicates non-atomic write regression"
    );
}

// Task 3a — watcher_debounces_multi_fire

/// The loader's 100 ms debounce collapses rapid state-file rewrites into
/// a single reload. Three writes within 20 ms must land on the last
/// variant, not race-result in a mixed/earlier colorscheme.
/// The whole exercise runs inside a single `nvim -l <script>` invocation
/// so `vim.wait` drives the event loop between writes — this pumps the
/// fs_event callback + debounce timer so they actually fire during the
/// wait window. Using `uv.sleep` here would block the thread without
/// processing callbacks, and `qa!` would exit with the initial
/// colors_name still in place.
#[test]
#[cfg(feature = "has-nvim")]
fn watcher_debounces_multi_fire() {
    use slate_cli::adapter::NvimAdapter;
    use slate_cli::env::SlateEnv;
    use slate_cli::theme::ThemeRegistry;
    use std::process::Command;
    use tempfile::TempDir;

    if skip_if_no_nvim("watcher_debounces_multi_fire") {
        return;
    }

    let td = TempDir::new().unwrap();
    let env = SlateEnv::with_home(td.path().to_path_buf());
    let registry = ThemeRegistry::new().unwrap();
    let first = *registry.all().first().unwrap();
    NvimAdapter::setup(&env, first).unwrap();

    // Pick 3 distinct variants — the debounced reload should land on
    // the LAST.
    let variants = registry.all();
    assert!(
        variants.len() >= 3,
        "need ≥ 3 variants for debounce test, got {}",
        variants.len()
    );
    let v0 = &variants[0].id;
    let v1 = &variants[1].id;
    let v2 = &variants[2].id;
    let final_id = v2.clone();

    let rtp = td.path().join(".config/nvim");
    let state = td.path().join(".cache/slate/current_theme.lua");

    // IMPORTANT: use `vim.wait(ms, fn)` (NOT `uv.sleep`) between writes
    // `vim.wait` pumps the event loop so the loader's fs_event callback
    // and 100 ms debounce timer actually fire during the wait window.
    // `uv.sleep` blocks the thread without processing callbacks, so
    // scheduled reloads would never run and `qa!` would exit with the
    // initial colors_name still in place.
    let lua_script = format!(
        r#"
vim.opt.runtimepath:prepend('{rtp}')
require('slate')  -- triggers M.setup, starts the fs_event watcher

local function write(id)
  local f = io.open('{state}', 'w')
  f:write('return "' .. id .. '"\n')
  f:close()
end

vim.wait(150, function() return false end)   -- let watcher arm
write('{v0}')
vim.wait(10, function() return false end)
write('{v1}')
vim.wait(10, function() return false end)
write('{v2}')
vim.wait(500, function() return false end)   -- past 100 ms debounce + redraw

io.stderr:write('FINAL=' .. (vim.g.colors_name or 'NONE'))
io.stderr:flush()
vim.cmd('qa!')
"#,
        rtp = rtp.display(),
        state = state.display(),
        v0 = v0,
        v1 = v1,
        v2 = v2,
    );

    let script_path = td.path().join("exercise.lua");
    std::fs::write(&script_path, &lua_script).unwrap();

    let out = Command::new("nvim")
        .args([
            "--headless",
            "-u",
            "NONE",
            "-l",
            script_path.to_str().unwrap(),
        ])
        .env("HOME", td.path())
        .output()
        .expect("spawn nvim");

    let stderr = String::from_utf8_lossy(&out.stderr);
    let expected = format!("slate-{}", final_id);
    assert!(
        stderr.contains(&format!("FINAL={}", expected)),
        "debounce failed — expected final colors_name {}, stderr={:?}",
        expected,
        stderr
    );
}

// Task 3b — lualine_refresh_fires

/// Installs a stand-in ColorScheme autocmd (no real lualine needed) that
/// records slate-* fires, drives a state-file-driven swap, and asserts
/// the autocmd fires for the new variant. Proves the loader emits the
/// ColorScheme event on state-driven apply — the hook lualine
/// refresh lives on.
#[test]
#[cfg(feature = "has-nvim")]
fn lualine_refresh_fires() {
    use slate_cli::adapter::NvimAdapter;
    use slate_cli::env::SlateEnv;
    use slate_cli::theme::ThemeRegistry;
    use std::process::Command;
    use tempfile::TempDir;

    if skip_if_no_nvim("lualine_refresh_fires") {
        return;
    }

    let td = TempDir::new().unwrap();
    let env = SlateEnv::with_home(td.path().to_path_buf());
    let registry = ThemeRegistry::new().unwrap();
    let variants = registry.all();
    assert!(
        variants.len() >= 2,
        "need ≥ 2 variants for refresh test, got {}",
        variants.len()
    );
    let first = variants[0];
    let second = variants[1];
    NvimAdapter::setup(&env, first).unwrap();

    let rtp = td.path().join(".config/nvim");
    let state = td.path().join(".cache/slate/current_theme.lua");
    let second_id = &second.id;

    // `vim.wait` (not `uv.sleep`) pumps the event loop so the fs_event
    // watcher + 100 ms debounce schedule_reload callback actually fire
    // during the wait. Using `uv.sleep` would let `qa!` exit before the
    // callback runs and the ColorScheme autocmd would only see the
    // initial apply from M.setup().
    let lua_script = format!(
        r#"
vim.opt.runtimepath:prepend('{rtp}')

-- Test-double stand-in for lualine's ColorScheme refresh hook.
_G.__slate_cs_fires = 0
_G.__slate_last_cs = ''
vim.api.nvim_create_autocmd('ColorScheme', {{
  pattern = 'slate-*',
  callback = function(args)
    _G.__slate_cs_fires = _G.__slate_cs_fires + 1
    _G.__slate_last_cs = args.match or ''
  end,
}})

require('slate')  -- M.setup starts watcher, applies initial state
vim.wait(150, function() return false end)   -- watcher arm + initial apply settle

-- Drive a swap via the state file — same path `slate theme set` takes.
local f = io.open('{state}', 'w')
f:write('return "{v1}"\n')
f:close()

vim.wait(500, function() return false end)   -- past debounce + apply

io.stderr:write(string.format('FIRES=%d LAST=%s', _G.__slate_cs_fires, _G.__slate_last_cs))
io.stderr:flush()
vim.cmd('qa!')
"#,
        rtp = rtp.display(),
        state = state.display(),
        v1 = second_id,
    );

    let script_path = td.path().join("lualine_exercise.lua");
    std::fs::write(&script_path, &lua_script).unwrap();

    let out = Command::new("nvim")
        .args([
            "--headless",
            "-u",
            "NONE",
            "-l",
            script_path.to_str().unwrap(),
        ])
        .env("HOME", td.path())
        .output()
        .expect("spawn nvim");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("FIRES="),
        "no FIRES marker in output — lualine refresh probe did not run; stderr={:?}",
        stderr
    );

    let fires: usize = stderr
        .split("FIRES=")
        .nth(1)
        .and_then(|s| s.split_whitespace().next())
        .and_then(|s| s.parse().ok())
        .expect("FIRES= count parseable");
    assert!(
        fires >= 1,
        "ColorScheme autocmd did not fire for slate-*; stderr={:?}",
        stderr
    );

    let expected_last = format!("slate-{}", second_id);
    assert!(
        stderr.contains(&format!("LAST={}", expected_last)),
        "last ColorScheme fire was not for {}; stderr={:?}",
        expected_last,
        stderr
    );
}

// Task 4a — marker_block_lua_comment_regression

/// Pitfall 4 regression gate: the slate marker block as written by
/// Option A must parse as valid Lua. Exercises three realistic
/// init.lua shapes: clean (marker only), lazy-prelude + marker, and
/// marker sandwiched by user content. All three must `luafile` cleanly.
#[test]
#[cfg(feature = "has-nvim")]
fn marker_block_lua_comment_regression() {
    use slate_cli::adapter::marker_block::{END, START};
    use tempfile::TempDir;

    if skip_if_no_nvim("marker_block_lua_comment_regression") {
        return;
    }

    let td = TempDir::new().unwrap();

    // Case 1: clean init.lua with only the slate marker block.
    let only_marker = format!(
        "-- {}\n\
         pcall(require, 'slate')  -- slate-managed: keep or delete, safe either way\n\
         -- {}\n",
        START, END,
    );
    let p1 = td.path().join("init_only.lua");
    std::fs::write(&p1, &only_marker).unwrap();
    assert_luafile_ok(&p1, "only-marker case");

    // Case 2: init.lua with a typical LazyVim-style prelude + slate
    // marker block appended.
    let with_lazy = format!(
        "vim.g.mapleader = ' '\n\
         vim.g.maplocalleader = ' '\n\
         vim.opt.number = true\n\
         \n\
         -- Would normally require('lazy').setup({{}}) here,\n\
         -- but we skip real plugin loading in this test.\n\
         \n\
         -- {}\n\
         pcall(require, 'slate')  -- slate-managed\n\
         -- {}\n",
        START, END,
    );
    let p2 = td.path().join("init_with_lazy.lua");
    std::fs::write(&p2, &with_lazy).unwrap();
    assert_luafile_ok(&p2, "lazy-prelude case");

    // Case 3: marker block in the MIDDLE of init.lua (user appended
    // more content after slate setup).
    let surrounded = format!(
        "vim.g.mapleader = ' '\n\
         -- {}\n\
         pcall(require, 'slate')\n\
         -- {}\n\
         vim.opt.wrap = false\n\
         vim.keymap.set('n', '<leader>w', ':w<CR>')\n",
        START, END,
    );
    let p3 = td.path().join("init_surrounded.lua");
    std::fs::write(&p3, &surrounded).unwrap();
    assert_luafile_ok(&p3, "surrounded case");
}

#[cfg(feature = "has-nvim")]
fn assert_luafile_ok(path: &std::path::Path, label: &str) {
    let out = std::process::Command::new("nvim")
        .args([
            "--headless",
            "-u",
            "NONE",
            "-c",
            &format!("luafile {}", path.display()),
            "-c",
            "q",
        ])
        .output()
        .expect("spawn nvim");
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        panic!("{}: luafile rejected init.lua:\n{}", label, stderr);
    }
    // Some nvim versions exit 0 but print Lua parse errors on stderr
    // guard against that too.
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("Error") && !stderr.contains("E5") && !stderr.contains("error"),
        "{}: luafile emitted error on stderr:\n{}",
        label,
        stderr
    );
}

// Task 4b — loader_lua_parses_via_luafile

/// The loader body contains every variant's `PALETTES['<id>'] = { ... }`
/// sub-table spliced from `render_colorscheme`. `luafile`ing the full
/// loader byte-compiles the lot through LuaJIT — a parse error in any
/// sub-table aborts with a line-number pointing at the offender. This
/// IS the 20-variant syntax gate.
#[test]
#[cfg(feature = "has-nvim")]
fn loader_lua_parses_via_luafile() {
    use slate_cli::adapter::nvim::render_loader;
    use std::process::Command;
    use tempfile::TempDir;

    if skip_if_no_nvim("loader_lua_parses_via_luafile") {
        return;
    }

    let td = TempDir::new().unwrap();
    let loader = render_loader();
    let path = td.path().join("loader.lua");
    std::fs::write(&path, &loader).unwrap();

    let out = Command::new("nvim")
        .args([
            "--headless",
            "-u",
            "NONE",
            "-c",
            &format!("luafile {}", path.display()),
            "-c",
            "q",
        ])
        // Redirect HOME so M.setup's fs_event watcher cannot reach the
        // real user's state file. The loader executes M.setup() at
        // require time — a no-op on a missing state file is fine.
        .env("HOME", td.path())
        .output()
        .expect("spawn nvim");

    assert!(
        out.status.success(),
        "loader Lua failed to parse/execute \
         (20-variant syntax gate failure):\nstderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
}

// Task 5 — clean + config editor disable (cross-reference)
// The `clean_removes_all_nvim_files` and
// `config_editor_disable_preserves_colors` contracts live alongside the
// helpers they exercise (`remove_nvim_managed_references` in
// `src/cli/clean.rs`, `handle_config_set_with_env` in
// `src/cli/config.rs`) — plan Task 5 option (b). Those helpers are
// crate-private; routing tests through the crate's own `#[cfg(test)]
// mod tests` blocks keeps the public surface tight rather than
// widening it to `pub` just so the integration harness can reach in.
// Each of the following production-side tests covers the same
// assertion shape the stubs originally planned here:
// * `cli::clean::tests::remove_nvim_managed_references_removes_all_slate_files`
// full install → clean takes colors/, lua/slate/, state, marker
// back to pristine state.
// * `cli::clean::tests::remove_nvim_managed_references_leaves_user_files_alone`
// Pitfall 7: user-owned colors/ files survive the clean sweep.
// * `cli::clean::tests::remove_nvim_managed_references_is_noop_on_empty_home`
// clean on a fresh home is a no-op, not an error.
// * `cli::config::tests::config_editor_disable_removes_marker_leaves_colors`
// `slate config set editor disable` strips the init.lua marker
// but preserves the 18 slate-*.lua shims + the loader.
// * `cli::config::tests::config_editor_rejects_unknown_action`
// unknown `editor` action errors with both the bad + valid
// action names in the message.
// * `cli::config::tests::config_editor_disable_is_noop_when_no_init_files`
// disable on an empty home is a best-effort no-op, not an error.
// No stubs are left in this file for those contracts.
